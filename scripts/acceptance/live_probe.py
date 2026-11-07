"""Live acceptance check for postmark-mcp.

Drives the real binary over stdio against a real Postmark server, and prints only
shapes, counts, codes, and derived yes or no answers: never addresses, subjects,
bodies, or other account content. The token is passed through from the
environment and never printed. See docs/acceptance.md.

Usage: python3 -I scripts/acceptance/live_probe.py <binary> [send] [templates]

Environment:
  POSTMARK_SERVER_TOKEN   required
  DEFAULT_SENDER_EMAIL    required, a verified sender on that server
  DEFAULT_MESSAGE_STREAM  optional, defaults to outbound

With `send`, exactly one email is sent, to Postmark's black hole address.

With `templates`, a throwaway layout and template are created, read, edited,
used for one email to the black hole address, and deleted. Deletion is attempted
even if an earlier step fails.
"""

import datetime
import json
import os
import subprocess
import sys
import time

BINARY = sys.argv[1]
SEND = "send" in sys.argv[2:]
TEMPLATES = "templates" in sys.argv[2:]
SENDER = os.environ["DEFAULT_SENDER_EMAIL"]
STREAM = os.environ.get("DEFAULT_MESSAGE_STREAM") or "outbound"
BLACKHOLE = "test@blackhole.postmarkapp.com"


class Server:
    def __init__(self):
        env = {
            "PATH": os.environ.get("PATH", ""),
            "POSTMARK_SERVER_TOKEN": os.environ["POSTMARK_SERVER_TOKEN"],
            "DEFAULT_SENDER_EMAIL": SENDER,
            "DEFAULT_MESSAGE_STREAM": STREAM,
        }
        # The server's stderr, kept for checking the tool log by hand. It holds no
        # token, but it does hold masked addresses, so it is not printed.
        self.stderr = open(os.environ.get("ACCEPTANCE_STDERR", os.devnull), "w")
        self.proc = subprocess.Popen(
            [BINARY], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr, env=env, text=True
        )
        self.next_id = 0
        result = self.request(
            "initialize",
            {"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "probe", "version": "1"}},
        )
        print("protocol:", result["result"]["protocolVersion"])
        self.notify("notifications/initialized")

    def notify(self, method):
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method}) + "\n")
        self.proc.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self.proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}) + "\n")
        self.proc.stdin.flush()
        while True:
            message = json.loads(self.proc.stdout.readline())
            if message.get("id") == self.next_id:
                return message

    def call(self, tool, arguments=None):
        response = self.request("tools/call", {"name": tool, "arguments": arguments or {}})
        if "error" in response:
            return {"protocol_error": response["error"].get("message")}
        result = response["result"]
        content = result.get("structuredContent") or {}
        content["_is_error"] = result.get("isError")
        return content

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def shape(value, depth=0):
    """Describes a value's structure without its content."""
    if isinstance(value, dict):
        if depth > 1:
            return "{...}"
        return {key: shape(item, depth + 1) for key, item in sorted(value.items())}
    if isinstance(value, list):
        return [f"{len(value)} items"] + ([shape(value[0], depth + 1)] if value else [])
    return type(value).__name__


def report(tool, result, keys_only=True):
    status = "ERROR " + str(result.get("code") or result.get("protocol_error")) if result.get("_is_error") or "protocol_error" in result else "ok"
    print(f"\n== {tool}: {status}")
    if status == "ok":
        print("   keys:", sorted(key for key in result if key != "_is_error"))
    else:
        print("   message:", str(result.get("message"))[:160])


server = Server()

info = server.call("get_server_info")
report("get_server_info", info)
print("   fields present:", sorted(k for k in info if not k.startswith("_")))

templates = server.call("list_templates")
report("list_templates", templates)
print("   total_count:", templates.get("total_count"), "| page:", len(templates.get("templates", [])))
print("   item fields seen:", sorted({k for t in templates.get("templates", []) for k in t}))
if templates.get("templates"):
    first = templates["templates"][0]
    template = server.call("get_template", {"template_id_or_alias": first["template_id"]})
    report("get_template", template)
    print("   has html/text/subject:", [k in template for k in ("html_body", "text_body", "subject")])

validation = server.call("validate_template", {"subject": "Hi {{name}}", "text_body": "Hello {{name}}", "test_render_model": {"name": "x"}})
report("validate_template", validation)
print("   all_content_is_valid:", validation.get("all_content_is_valid"), "| parts:", [k for k in ("subject", "html_body", "text_body") if k in validation])

search = server.call("search_outbound_messages", {"count": 100})
report("search_outbound_messages", search)
messages = search.get("messages", [])
print("   total_count:", search.get("total_count"), "| page:", len(messages))
print("   item fields seen:", sorted({k for m in messages for k in m}))
print("   metadata types:", sorted({type(m.get("metadata")).__name__ for m in messages}))
print("   recipients per message (min/max):", (min((len(m["recipients"]) for m in messages), default=None), max((len(m["recipients"]) for m in messages), default=None)))
times = [m.get("received_at") for m in messages if m.get("received_at")]
parsed = [datetime.datetime.fromisoformat(t) for t in times]
print("   newest first:", all(a >= b for a, b in zip(parsed, parsed[1:])), f"({len(parsed)} timestamps)")
print("   display names in recipients:", any("<" in r for m in messages for r in m["recipients"]))

if messages:
    newest = messages[0]
    details = server.call("get_message_details", {"message_id": newest["message_id"]})
    report("get_message_details", details)
    events = details.get("events", [])
    print("   event types:", sorted({e.get("type") for e in events}))
    print("   detail keys kept:", sorted({k for e in events for k in (e.get("details") or {})}))
    print("   events with no recipient:", sum(1 for e in events if "recipient" not in e))

    # Spaces in a query value: search by an existing subject that contains one.
    spaced = next((m for m in messages if " " in (m.get("subject") or "")), None)
    if spaced:
        hit = server.call("search_outbound_messages", {"subject": spaced["subject"], "count": 50})
        found = any(m["message_id"] == spaced["message_id"] for m in hit.get("messages", []))
        print("\n== subject with spaces finds its own message (+ encoding works):", found, "| errors:", hit.get("code"))

    # Whether to_date covers its whole final day, in Eastern Time.
    eastern = datetime.timezone(datetime.timedelta(hours=-5))
    newest_day = datetime.datetime.fromisoformat(newest["received_at"]).astimezone(eastern).date().isoformat()
    day = server.call("search_outbound_messages", {"from_date": newest_day, "to_date": newest_day, "count": 500})
    ids = {m["message_id"] for m in day.get("messages", [])}
    print("== to_date includes a message sent later that same day:", newest["message_id"] in ids, f"(day {newest_day} ET)", "| errors:", day.get("code"))

bounces = server.call("search_bounces", {"count": 100})
report("search_bounces", bounces)
records = bounces.get("bounces", [])
print("   total_count:", bounces.get("total_count"), "| page:", len(records))
print("   types seen:", sorted({b.get("type") for b in records}))
print("   item fields seen:", sorted({k for b in records for k in b}))
if records:
    dump = server.call("get_bounce_dump", {"bounce_id": records[0]["bounce_id"]})
    report("get_bounce_dump", dump)
    print("   available:", dump.get("available"), "| body chars:", len(dump.get("body") or ""))
    bounced_ids = {b.get("message_id") for b in records if b.get("message_id")}
    for message_id in list(bounced_ids)[:3]:
        d = server.call("get_message_details", {"message_id": message_id})
        linked = [e for e in d.get("events", []) if e.get("type") == "Bounced"]
        print("   bounced message: Bounced events:", len(linked), "| BounceID present:", any("BounceID" in (e.get("details") or {}) for e in linked),
              "| BounceID matches a record:", any(str((e.get("details") or {}).get("BounceID")) in {str(b["bounce_id"]) for b in records} for e in linked))

suppressions = server.call("list_suppressions")
report("list_suppressions", suppressions)
print("   count:", len(suppressions.get("suppressions", [])), "| reasons:", sorted({s.get("suppression_reason") for s in suppressions.get("suppressions", [])}))

for stat in ["summary", "overview", "sent", "bounces", "spam", "tracked", "opens", "open_platforms", "open_clients", "open_read_times", "clicks", "click_browsers", "click_platforms", "click_location"]:
    result = server.call("get_delivery_stats", {"stat": stat})
    status = result.get("code") if result.get("_is_error") else "ok"
    print(f"== get_delivery_stats {stat}: {status} | totals keys: {len(result.get('totals', {}))} | rates: {sorted(result.get('rates', {}))}")

webhooks = server.call("list_webhooks")
report("list_webhooks", webhooks)
hooks = webhooks.get("webhooks", [])
print("   count:", len(hooks), "| streams:", sorted({h.get("message_stream") for h in hooks}))
for stream in sorted({h.get("message_stream") for h in hooks if h.get("message_stream")}):
    filtered = server.call("list_webhooks", {"message_stream": stream})
    expected = sum(1 for h in hooks if h.get("message_stream") == stream)
    print(f"   MessageStream filter on one stream returns {len(filtered.get('webhooks', []))}, expected {expected}")
print("   any url with query or credentials:", any("?" in (h.get("url") or "") or "@" in (h.get("url") or "") for h in hooks))

if messages:
    any_recipient = next((r for m in messages for r in m["recipients"] if "<" not in r), None)
    if any_recipient:
        diagnosis = server.call("diagnose_delivery", {"recipient": any_recipient})
        report("diagnose_delivery (a recent recipient)", diagnosis)
        print("   verdict:", diagnosis.get("verdict"), "| incomplete codes:", [i.get("code") for i in diagnosis.get("incomplete", [])])

if SEND:
    print("\n==== LIVE SEND ====")
    receipt = server.call("send_email", {
        "to": BLACKHOLE,
        "subject": "postmark-mcp live acceptance test",
        "text_body": "Sent by the postmark-mcp acceptance probe to Postmark's black hole address.",
        "tag": "postmark-mcp-acceptance",
    })
    report("send_email", receipt)
    print("   receipt fields:", sorted(k for k in receipt if not k.startswith("_")))
    message_id = receipt.get("message_id")
    if message_id:
        for attempt in range(6):
            time.sleep(5)
            details = server.call("get_message_details", {"message_id": message_id})
            events = sorted({e.get("type") for e in details.get("events", [])})
            print(f"   after {5 * (attempt + 1)}s: status {details.get('status')} | events {events} | track_opens {details.get('track_opens')} | track_links {details.get('track_links')} | errors {details.get('code')}")
            if "Delivered" in events:
                break
        diagnosis = server.call("diagnose_delivery", {"recipient": BLACKHOLE})
        report("diagnose_delivery (black hole)", diagnosis)
        print("   verdict:", diagnosis.get("verdict"), "| investigated:", (diagnosis.get("investigated_message") or {}).get("message_id") == message_id,
              "| stream:", diagnosis.get("message_stream"), "| incomplete:", [i.get("code") for i in diagnosis.get("incomplete", [])])

if TEMPLATES:
    print("\n==== TEMPLATE LIFECYCLE ====")
    stamp = int(time.time())
    layout_alias = f"pmcp-acceptance-layout-{stamp}"
    template_alias = f"pmcp-acceptance-{stamp}"
    created = []

    def check(label, condition):
        print(f"   {'PASS' if condition else 'FAIL'}: {label}")

    try:
        layout = server.call("create_template", {
            "name": "postmark-mcp acceptance layout",
            "alias": layout_alias,
            "template_type": "Layout",
            "html_body": "<html><body>{{{@content}}}</body></html>",
            "text_body": "{{{@content}}}",
        })
        report("create_template (layout)", layout)
        if layout.get("template_id"):
            created.append(layout_alias)

        template = server.call("create_template", {
            "name": "postmark-mcp acceptance template",
            "alias": template_alias,
            "subject": "Acceptance {{name}}",
            "html_body": "<p>Hello {{name}}</p>",
            "text_body": "Hello {{name}}",
            "layout_template": layout_alias,
        })
        report("create_template (standard)", template)
        if template.get("template_id"):
            created.append(template_alias)

        before = server.call("get_template", {"template_id_or_alias": template_alias})
        report("get_template", before)
        check("uses the layout", before.get("layout_template") == layout_alias)

        listed = server.call("list_templates", {"count": 500})
        aliases = {t.get("alias") for t in listed.get("templates", [])}
        check("list_templates shows both", {layout_alias, template_alias} <= aliases)
        check("list_templates reports no subject (as documented)", all("subject" not in t for t in listed.get("templates", [])))

        edited = server.call("edit_template", {"template_id_or_alias": template_alias, "subject": "Edited {{name}}"})
        report("edit_template (subject only)", edited)
        after = server.call("get_template", {"template_id_or_alias": template_alias})
        check("subject changed", after.get("subject") == "Edited {{name}}")
        for field in ("name", "html_body", "text_body", "alias", "layout_template", "template_type"):
            check(f"{field} kept", after.get(field) == before.get(field))

        receipt = server.call("send_email_with_template", {
            "to": BLACKHOLE,
            "template_alias": template_alias,
            "template_model": {"name": "Acceptance"},
            "tag": "postmark-mcp-acceptance",
        })
        report("send_email_with_template", receipt)
        if receipt.get("message_id"):
            time.sleep(5)
            details = server.call("get_message_details", {"message_id": receipt["message_id"]})
            check("rendered subject used", details.get("subject") == "Edited Acceptance")
            print("   events:", sorted({e.get("type") for e in details.get("events", [])}))

        detached = server.call("edit_template", {"template_id_or_alias": template_alias, "detach_layout": True})
        report("edit_template (detach_layout)", detached)
        detached_state = server.call("get_template", {"template_id_or_alias": template_alias})
        check("layout detached", "layout_template" not in detached_state)
        check("subject kept after detaching", detached_state.get("subject") == "Edited {{name}}")
    finally:
        for alias in reversed(created):
            deleted = server.call("delete_template", {"template_id_or_alias": alias})
            report(f"delete_template ({'layout' if 'layout' in alias else 'standard'})", deleted)
            gone = server.call("get_template", {"template_id_or_alias": alias})
            check("now gone", gone.get("_is_error") and gone.get("code") == "postmark/rejected")

server.close()
