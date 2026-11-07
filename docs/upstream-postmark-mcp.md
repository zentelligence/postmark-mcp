# Upstream Postmark MCP server: behaviour reference

This document records how the official Postmark MCP server (npm package `@activecampaign/postmark-mcp`) behaves, so that a reimplementation does not need to re-read its Node.js source.

## 1. Version and scope

The version read is 3.0.0, taken from `package.json`. The server reports that version in the `X-Postmark-Client-Version` request header and as its MCP server version. The header comment in `index.js` still says 1.0.0, but that comment is not used at runtime.

Everything below describes upstream's behaviour at 3.0.0 as found in `index.js`, `lib/log.js`, `lib/url.js`, `package.json`, `.env.example`, `README.md`, `CLAUDE.md` and `CHANGELOG.md`.

The MCP SDK (`@modelcontextprotocol/sdk` 1.29.0) and Zod (3.25.76) were not read. Where behaviour depends on them, this document says so.

The test files and smoke-test examples were not read.

Upstream is MIT licensed. This document paraphrases behaviour and contains no upstream source. Line references are to `index.js` unless another file is named.

## 2. Configuration

`index.js` imports `dotenv/config` first, so a `.env` file in the current working directory is loaded when the process starts. `index.js` does not pass a path to dotenv. The behaviour described here is dotenv's default; it was not checked against the installed dotenv 16.6.1 source.

Only the eight environment variables below are read by `index.js` or `lib/`. No other variable is read.

Required variables are checked in the order listed. An empty string counts as missing. The first missing variable prints an error block to stderr and exits the process with status 1, so later checks do not run (index.js:121 to 140).

- `POSTMARK_SERVER_TOKEN` is sent as the `X-Postmark-Server-Token` header and is only ever sent to `https://api.postmarkapp.com`. No log line writes its value.
- `DEFAULT_SENDER_EMAIL` is used as `From` when a tool call does not supply `from`. index.js does not validate its format at startup. The startup message says it must be a verified sender signature in Postmark.
- `DEFAULT_MESSAGE_STREAM` is used as `MessageStream` on the four send tools. It is also the default stream for `listSuppressions`, `createSuppressions`, `deleteSuppressions` and `diagnoseDelivery`. It is not a default for `listWebhooks`, `createWebhook`, `searchOutboundMessages`, `searchBounces` or `getDeliveryStats`. In those tools `messageStream` is an optional filter or value, and omitting it sends nothing.

Optional variables:

- `AGENT_LABEL`: unset or empty means no label. When set, it is sent as the `X-Agent-Label` header on every Postmark API request and printed to stderr at startup as "Agent label:".
- `WEBHOOK_URL_ALLOWLIST`: unset or empty means no allowlist. Otherwise the value is split on commas, each entry is trimmed, and empty entries are dropped (index.js:36 to 38). If no entries remain, a WARN line is printed at startup (index.js:40 to 42) and every `createWebhook` call is rejected. The matching rules are in section 5.3.
- `LOG_FILE`: unset or empty means stderr only. Otherwise every log line is also appended to this path with a synchronous append, and the file is created if missing. A failed append prints a warn JSON line to stderr and does not fail the tool call. No rotation or size limit is applied.
- `LOG_EMAIL_FULL`: only the exact string `true` disables email masking. Any other value, including `TRUE` or `1`, leaves masking on.
- `POSTMARK_SKIP_VERIFY`: only the exact string `true` skips the startup check described below. When skipping with a token other than the literal string `POSTMARK_API_TEST`, a WARN line is printed saying not to use the setting in production (index.js:149 to 152). `.env.example` does not list this variable. The CHANGELOG describes it as intended for tests and offline use.

Startup check. Unless `POSTMARK_SKIP_VERIFY` is `true`, the server issues a GET to `/server` before it registers tools (index.js:153 to 155). If that call fails, the error is mapped as follows (index.js:163 to 179):

- If the error text contains `401`, or contains `unauthorized` in any letter case, or contains `ErrorCode 10`, the server throws an "Initialization failed" error saying the token was rejected and that it must be a Server Token rather than an Account Token.
- If the error cause code is `ENOTFOUND`, or the message contains `fetch failed` or `ENOTFOUND`, the server throws an error saying the Postmark API could not be reached.
- Anything else, including a timeout, throws "Initialization failed:" followed by the error code if one exists and the message.

`main()` prints "Server initialization failed:" with the message and exits with status 1 (index.js:215 to 218).

Startup sequence. The server creates an `McpServer` named `postmark-mcp` with the package version, registers all 24 tools, connects a `StdioServerTransport`, and then prints a ready line that lists the tool names (index.js:183 to 211). Nothing is written to stdout by `index.js`. The MCP client identity is captured later, when the SDK fires `oninitialized` (index.js:194 to 200). The code does not wait for that event, so tool calls made before it fires log a `null` client name.

Shutdown. SIGTERM and SIGINT close the server and exit with status 0. If closing throws, the process exits with status 1 (index.js:222 to 233). Uncaught exceptions and unhandled rejections print a message and exit with status 1 (index.js:236 to 244).

## 3. Request client

All Postmark calls go through `postmarkRequest` (index.js:76). It uses the global `fetch`. There is no SDK.

Base URL: `https://api.postmarkapp.com` (index.js:20).

Path resolution is delegated to `resolvePostmarkUrl` in `lib/url.js`. A string that begins with a single `/` is joined to the base. A string that begins with the base followed by `/` is used as an absolute URL. Anything else is refused with an error before any request is made. That covers non-string values, other hosts, and protocol-relative `//host` paths. Every current caller passes a literal path beginning with `/`.

Headers, in the order they are built:

- `Accept: application/json` is always sent.
- `X-Postmark-Server-Token` carries `POSTMARK_SERVER_TOKEN`.
- `X-Postmark-Client` is always `postmark-mcp`.
- `X-Postmark-Client-Version` is the package version.
- `X-Postmark-MCP-Client` is sent only when the MCP client name is known. Its value is `name/version`, or just `name` when the client gave no version.
- `X-Agent-Label` is sent only when `AGENT_LABEL` is set.
- Any caller-supplied `options.headers` is spread last and could override the above. No caller in `index.js` supplies it.
- `Content-Type: application/json` is added when a body is passed and no content type is already set.

No `X-Postmark-Correlation-Id` header is sent. `CLAUDE.md` describes one, but the code does not set it, and the 2.1.0 CHANGELOG entry says the correlation header was replaced.

Timeout. Each request is governed by an `AbortController` with a 60 second limit (index.js:21, 92 to 93). The timer is cleared as soon as `fetch` resolves with response headers, so reading the response body is not covered by the timeout. An abort is reported as "Postmark request timed out after 60s: " followed by the path. Other `fetch` failures are re-thrown unchanged.

Redirects. The request is made with `redirect: 'error'` (index.js:96). Any redirect response therefore causes `fetch` to reject. That rejection is not an abort, so it is re-thrown unchanged, and its exact text depends on the `fetch` implementation, which was not checked. The 3.0.0 CHANGELOG entry says such calls fail rather than being followed.

Error mapping for non-2xx responses (index.js:104 to 108). The body is read as text. The message starts as the HTTP status text. If the body parses as JSON, its `Message` field replaces the message when present, and its `ErrorCode` is captured when it is not null or undefined, so a value of 0 is captured. The thrown error reads `Postmark API <status>`, then ` (ErrorCode <n>)` if an error code was captured, then `: <message>`. A body that is not JSON leaves the status text as the message.

Success handling (index.js:109). An empty 2xx body returns `null`. Otherwise the body is parsed as JSON. A non-JSON 2xx body throws the parser's error unchanged. Handlers that read a property from the result do not check for `null`, so an empty body makes them throw a TypeError.

Methods used: GET (no method set), POST, PUT and DELETE. Bodies are JSON strings.

Query string helper `qs` (index.js:113 to 116). It builds `?key=value` pairs with `encodeURIComponent` on values only. Keys are written as given. Entries whose value is `undefined`, `null` or the empty string are skipped. `false` and `0` are kept, so a `false` boolean is sent. If nothing remains, the result is an empty string. Key casing differs between tools, so the code sends `fromdate`, `messagestream` and `EmailAddress` exactly as shown in section 5.

## 4. Logging

There are two logging channels. Both write to stderr. stdout is never used.

Channel one is a structured JSON line. Every tool handler is wrapped (index.js:414 to 436). After each call, whether it succeeds or throws, one line is written with these fields: `timestamp` (ISO 8601), `tool`, `clientName`, `clientVersion` (both from the MCP handshake, or `null`), `args` (the sanitised arguments), `status` (`ok` or `error`), `error` (the message, on error only) and `durationMs`. Errors are re-thrown after logging. The duration covers the handler only.

The field is named `args`, as the README and code both show. The 2.1.0 CHANGELOG names it `sanitizedArgs`, which is not what the code emits.

Schema validation. The wrapper wraps only the handler. Zod validation happens in the MCP SDK before the handler runs. Whether a call rejected by the schema produces a wrapper log line cannot be determined from `index.js`.

Channel two is plain `console.error` output from the handlers and the startup code. These lines are not passed through the sanitiser (section 4.2 lists them).

### 4.1 Sanitiser rules

The sanitiser is `sanitizeArgs` in `lib/log.js`. It processes only the top-level keys of the argument object. Nested objects are not walked. For each key the rules are applied in this order.

1. If the key matches `password`, `secret`, `token`, `apikey` or `api_key` in any letter case, the value becomes the string `[redacted]`, whatever its type. No tool argument name in `index.js` matches this pattern.
2. If the key contains `htmlBody` or `textBody` in any letter case and the value is a string, the value becomes `[N ch]`, where N is the JavaScript string length (UTF-16 code units). The README calls this a byte count, but the code does not count bytes.
3. If the value is a string longer than 300 characters, it becomes `[truncated N ch]`, where N is the string length. This applies before the email checks, so a long email-shaped string is truncated rather than masked.
4. If the value is a string matching the email shape (no whitespace, one `@`, and a dot in the domain), it is masked as described in 4.1.1.
5. If the value is a string containing commas where every comma-separated part, once trimmed, is an email address, each part is masked and the parts are re-joined with `, `. Separators without spaces are therefore changed to `, `.
6. If the value is an array that is empty, the result is an empty array.
7. If the value is an array whose first element is an object, each element contributes the first truthy of its `to`, `email` or `EmailAddress` fields, masked. The result is `{ _count, _recipients }`, where `_recipients` is capped at the first 20 masked addresses. If no element has an address, the result is `{ _count }` only. This applies to `messages` in `sendBatch` and `recipients` in `sendBatchWithTemplate`. Their `templateModel` and body fields are therefore not logged at all.
8. If the value is an array of other items, the first 20 items are kept. If there are more, a string `…+N more` is appended after them. Email-shaped string items are masked. This applies to the `to` array in `sendEmail` and to `emailAddresses` in `createSuppressions` and `deleteSuppressions`.
9. If the value is any other non-null object, the result is `{ _keys }`, listing the key names only. This applies to `templateModel` and `testRenderModel`.
10. Every other value, including numbers, booleans, `null` and strings that match none of the rules above, is kept unchanged. Those strings include `subject`, `tag`, `name`, `alias`, `messageId`, `url` and non-email text. Keys that are `undefined` are dropped when the line is serialised.

#### 4.1.1 Email masking

Masking applies to the mailbox part only. The domain is kept in full. The code splits at the last `@`. If the mailbox is one character or less, the address is unchanged. Otherwise the result is the first mailbox character, then the greater of one and (mailbox length minus two) asterisks, then the last mailbox character, then `@` and the domain. For example, a five-character mailbox keeps its first and last characters and has three asterisks in between. When `LOG_EMAIL_FULL` is `true`, the address is returned unchanged.

### 4.2 Plain stderr lines that are not sanitised

These `console.error` calls pass raw values, so they can contain unmasked email addresses, full webhook URLs and message identifiers. The CHANGELOG and README say that addresses are masked and that sensitive information is kept out of logs, but these lines do not go through the sanitiser.

- `searchOutboundMessages` prints its whole filter object, which includes `recipient`, `fromemail` and `subject`, unmasked (index.js:954).
- `searchBounces` prints its whole filter object, which includes `emailFilter` and `messageID`, unmasked (index.js:1175).
- `listSuppressions` prints the stream and filter, which includes `EmailAddress`, unmasked (index.js:1266).
- `listWebhooks` prints its filter (index.js:1451).
- `createWebhook` prints the full URL (index.js:1522) and then the new webhook ID (index.js:1524).
- `diagnoseDelivery` prints `messageId` and the stream, but not the recipient (index.js:1030).
- `getMessageDetails`, `getBounceDump`, `activateBounce` and `deleteWebhook` print their identifier (index.js:983, 1204, 1227, 1555).
- `getDeliveryStats` prints the stat and filter (index.js:1374).
- Template tools print template names, aliases and IDs (index.js:722, 780, 782, 828, 830, 854). `sendEmailWithTemplate` and `sendBatchWithTemplate` print the template identifier (index.js:533, 671).
- Send tools print message IDs and counts. `sendEmail` prints the message ID (index.js:479).
- Startup prints `DEFAULT_SENDER_EMAIL` and `AGENT_LABEL` in full (index.js:143, 145).
- The startup `[WARN]` and `[ERROR]` lines name environment variables but never print their values, except the sender and label lines above.

### 4.3 Summary of what is logged

Logged in full: tool name, timestamp, duration, client name and version, status and error message, `subject`, `tag`, `name`, `alias`, `status`, `messageStream`, dates, identifiers, and numeric values.

Logged as lengths, counts or key names: `htmlBody`, `textBody`, `templateModel`, `testRenderModel`, and the messages and recipients arrays.

Logged with masked mailboxes: string addresses, comma-separated address lists, and address arrays, within the sanitised argument object.

Not logged by any line: the server token value.

## 5. Tools

The server registers 24 tools. The count is also printed at startup.

Every tool handler returns a single text content item and sets no structured content. `index.js` never sets `isError` itself. Errors thrown in a handler are not caught by `index.js`, and no tool entry says otherwise. How the SDK turns a thrown error into a client-visible tool error is in the SDK, which was not read.

Most handlers print one or two lines through `console.error`, as listed in section 4.2. When a handler reads a field from a response that is missing, it throws a TypeError. No handler provides a fallback for a missing response object unless the tool entry says so.

Annotation presets (index.js:66 to 68):

- `READ_ONLY`: `readOnlyHint` true, `destructiveHint` false, `idempotentHint` true, `openWorldHint` true.
- `MUTATING`: `readOnlyHint` false, `destructiveHint` false, `idempotentHint` false, `openWorldHint` true.
- `DESTRUCTIVE`: `readOnlyHint` false, `destructiveHint` true, `idempotentHint` false, `openWorldHint` true.

Common argument types. Email fields use Zod `z.string().email()` (the exact acceptance rules of Zod's email check were not read). Date fields use the regular expression `^\d{4}-\d{2}-\d{2}$`, which checks format only. The code does not check that the date is real or that a start date comes before an end date. Integer fields use `z.number().int()`. Message stream, tag and other free-text fields are plain strings with no length limit in the schema.

Only `diagnoseDelivery` has a default date window. The search tools and `getDeliveryStats` send no date parameter when none is given.

### 5.1 Index

- `sendEmail` (index.js:440): MUTATING, POST `/email`.
- `sendEmailWithTemplate` (index.js:490): MUTATING, POST `/email/withTemplate`.
- `sendBatch` (index.js:580): MUTATING, POST `/email/batch`.
- `sendBatchWithTemplate` (index.js:625): MUTATING, POST `/email/batchWithTemplates`.
- `listTemplates` (index.js:682): READ_ONLY, GET `/templates`.
- `getTemplate` (index.js:714): READ_ONLY, GET `/templates/{id}`.
- `createTemplate` (index.js:744): MUTATING, POST `/templates`.
- `editTemplate` (index.js:798): DESTRUCTIVE, PUT `/templates/{id}`.
- `deleteTemplate` (index.js:846): DESTRUCTIVE, DELETE `/templates/{id}`.
- `validateTemplate` (index.js:867): READ_ONLY, POST `/templates/validate`.
- `searchOutboundMessages` (index.js:925): READ_ONLY, GET `/messages/outbound`.
- `getMessageDetails` (index.js:975): READ_ONLY, GET `/messages/outbound/{id}/details`.
- `diagnoseDelivery` (index.js:1012): READ_ONLY, composite of three GETs and one conditional GET.
- `searchBounces` (index.js:1139): READ_ONLY, GET `/bounces`.
- `getBounceDump` (index.js:1196): READ_ONLY, GET `/bounces/{id}/dump`.
- `activateBounce` (index.js:1219): MUTATING, PUT `/bounces/{id}/activate`.
- `listSuppressions` (index.js:1245): READ_ONLY, GET `/message-streams/{stream}/suppressions/dump`.
- `createSuppressions` (index.js:1287): MUTATING, POST `/message-streams/{stream}/suppressions`.
- `deleteSuppressions` (index.js:1318): DESTRUCTIVE, POST `/message-streams/{stream}/suppressions/delete`.
- `getDeliveryStats` (index.js:1351): READ_ONLY, GET `/stats/outbound` and related paths.
- `getServerInfo` (index.js:1404): READ_ONLY, GET `/server`.
- `listWebhooks` (index.js:1440): READ_ONLY, GET `/webhooks`.
- `createWebhook` (index.js:1480): MUTATING, POST `/webhooks`.
- `deleteWebhook` (index.js:1547): DESTRUCTIVE, DELETE `/webhooks/{id}`.

### 5.2 Shared helper: formatBatchResults (index.js:555)

`formatBatchResults` is defined inside `registerTools` and used by `sendBatch` and `sendBatchWithTemplate`. It receives the array returned by Postmark.

A result counts as a success only when its `ErrorCode` is exactly 0. Any other value, including a missing `ErrorCode`, counts as a failure.

The first line is `Sent S/T successfully`, where S is the success count and T is the total. When there are failures, ` (F failed)` is appended.

If there are failures, a blank line and `Failures:` follow. Up to 20 failures are listed, each as a line with the recipient (`To`, or `(unknown)` when absent), then the `ErrorCode` and the `Message`. If there are more than 20, a line `... and K more` follows.

If there are successes, a blank line and a heading follow. The heading is `Successes:`, or `Successes (first 10 shown):` when there are more than 10. Up to 10 are listed, each as the recipient and the `MessageID`. The recipient is printed directly and has no `(unknown)` fallback, so a missing value prints as `undefined`. If there are more than 10, a line `... and K more successful sends` follows.

Failures are always listed before successes. An empty result array produces `Sent 0/0 successfully`. The helper does not check that its input is an array. A non-array input causes a TypeError when `filter` is called.

Stderr receives a separate count line from each batch handler (index.js:619 and 674).

### 5.3 Webhook URL allowlist

The allowlist is active only when `WEBHOOK_URL_ALLOWLIST` yields a list, including an empty list (index.js:51 to 62). When it is unset or empty, the function returns true for every URL.

The check runs inside `createWebhook`, after the schema check and the trigger check (section 5.14). The schema has already required the `url` to be a string that passes Zod's URL check and begins with `https://`. That prefix check is case-sensitive.

When a list is active, the rules are:

- The URL must parse with the WHATWG URL constructor. A parse failure rejects it.
- The URL must not contain a username or password. Any userinfo rejects it, which blocks forms such as `https://allowed.host@other.host/`.
- Each entry is parsed with the same constructor. An entry that fails to parse is skipped silently.
- A URL is accepted when, for some entry, the URL origin equals the entry origin and the URL pathname begins with the entry pathname as a plain string prefix.

Consequences of those rules, derived from the code:

- Origin means scheme, host and port as the URL parser normalises them. Host matching is therefore case-insensitive, and default ports are normalised.
- The path test has no segment boundary. An entry of `https://h.example/hooks` also accepts `https://h.example/hooks-other`.
- An entry without a path has pathname `/`, so it accepts every path on that origin.
- Query strings and fragments on the URL or the entry are ignored.
- The entry's scheme is not checked on its own. An entry beginning with `http://` can never match, because the URL must begin with `https://` and origins must then be equal.
- Entries are trimmed when the variable is parsed.

The rejection error reads `Webhook URL rejected: does not match any allowed origin in WEBHOOK_URL_ALLOWLIST.` followed by `Allowed entries:` and the entries joined with `, `. The entries are returned to the MCP client verbatim. When the list is empty, the code tests the array for truthiness, which is true for an empty array, so the text `(none)` is never produced. The line ends with `Allowed entries: ` followed by nothing.

### 5.4 Email tools

#### sendEmail (index.js:440)

Postmark call: POST `/email`.

Arguments:

- `to` is required. It is either one email-shaped string, or an array of 1 to 50 email-shaped strings.
- `subject` is a required string. The schema places no length or emptiness rule on it.
- `textBody` is a required string. An empty string passes the schema.
- `htmlBody` is an optional string.
- `from` is an optional email-shaped string.
- `cc` and `bcc` are optional plain strings. They are passed through without validation, and the description says they are comma-separated.
- `replyTo` is an optional email-shaped string.
- `tag` is an optional string.

Values set automatically: `From` is `from`, or `DEFAULT_SENDER_EMAIL` when `from` is absent. `To` is the string as given, or the array joined with `, `. `MessageStream` is `DEFAULT_MESSAGE_STREAM`. `TrackOpens` is true. `TrackLinks` is `HtmlAndText`. `HtmlBody`, `Cc`, `Bcc`, `ReplyTo` and `Tag` are included only when their arguments are truthy, so an empty `htmlBody` or `tag` is omitted.

Guards before the API call: none beyond the schema.

Failure handling: errors from `postmarkRequest` propagate. A 2xx response whose `ErrorCode` is not 0 throws `Postmark returned ErrorCode N: Message`. A `null` body makes the `ErrorCode` read throw a TypeError.

Result: a text block beginning `Email sent successfully!`, followed by `MessageID`, `To` and `Subject`. `To` is the argument as received. An array is rendered by JavaScript string conversion, so its items appear joined by commas with no spaces. The `MessageID` is also printed to stderr.

#### sendEmailWithTemplate (index.js:490)

Postmark call: POST `/email/withTemplate`.

Arguments:

- `to` is a required single email-shaped string. Arrays are not accepted here.
- `templateId` is an optional number. The schema does not require an integer.
- `templateAlias` is an optional string.
- `templateModel` is required by the schema. It is an object with any keys, and it may be empty. Its contents are not checked.
- `from` and `replyTo` are optional email-shaped strings. `cc` and `bcc` are optional plain strings. `tag` is an optional string.

Guards, run in the handler before any request:

- If neither `templateId` nor `templateAlias` is truthy, it throws `Either templateId or templateAlias must be provided`.
- If both are truthy, it throws `Provide only one of templateId or templateAlias, not both`.

Truthiness matters here. A `templateId` of 0, or an empty alias, counts as not supplied.

Values set automatically: `From`, `MessageStream`, `TrackOpens` and `TrackLinks` as in `sendEmail`. `TemplateId` is set when `templateId` is truthy, otherwise `TemplateAlias` is set. `TemplateModel` is the object as given. `Cc`, `Bcc`, `ReplyTo` and `Tag` are included only when truthy.

Failure handling: as in `sendEmail`, including the `ErrorCode` check.

Result: `Template email sent successfully!`, then `MessageID`, `To` and `Template`, where `Template` is the `templateId` or the alias.

#### sendBatch (index.js:580)

Postmark call: POST `/email/batch`. The body is a bare JSON array of message objects, not an object with a wrapper key. The code treats the response as one result per message.

Arguments: `messages` is a required array of 1 to 500 objects. Each object has these fields:

- `to` (email-shaped), `subject` and `textBody`, all required.
- `htmlBody` (string), `from` (email-shaped), `cc` and `bcc` (plain strings), `replyTo` (email-shaped) and `tag` (string), all optional.

Values set automatically, per message: `From` is the message `from` or `DEFAULT_SENDER_EMAIL`. `MessageStream` is `DEFAULT_MESSAGE_STREAM`. `TrackOpens` is true and `TrackLinks` is `HtmlAndText`. `HtmlBody`, `Cc`, `Bcc`, `ReplyTo` and `Tag` are included only when truthy.

Guards: none beyond the schema. There is no duplicate-recipient check and no check that the messages are distinct.

Failure handling: an HTTP-level error is thrown as usual. A failed message does not throw. Its `ErrorCode` is checked only in `formatBatchResults`, so the handler returns normal content, with the counts in the text, even when every message failed. A non-array response throws a TypeError.

Result: the output of `formatBatchResults` (section 5.2).

#### sendBatchWithTemplate (index.js:625)

Postmark call: POST `/email/batchWithTemplates`. The body is an object with a `Messages` array (index.js:672).

Arguments:

- `templateId` is an optional integer.
- `templateAlias` is an optional string.
- `from` is an optional email-shaped default sender for every message.
- `tag` is an optional default tag for every message.
- `recipients` is a required array of 1 to 500 objects. Each has `to` (email-shaped, required), `templateModel` (object, required by the schema), and optional `from` (email-shaped), `cc`, `bcc`, `replyTo` and `tag`.

Guards, run before any request: the same neither-or-both checks as `sendEmailWithTemplate`, using truthiness, with the same messages.

Values set automatically, per recipient: `From` is the recipient `from`, else the top-level `from`, else `DEFAULT_SENDER_EMAIL`. `TemplateId` is set when `templateId` is truthy, otherwise `TemplateAlias` is set. `TemplateModel` is the recipient's model. `MessageStream` is `DEFAULT_MESSAGE_STREAM`. `TrackOpens` is true and `TrackLinks` is `HtmlAndText`. `Cc`, `Bcc` and `ReplyTo` are included when truthy. The tag is the recipient `tag` when it is not `null` or `undefined`, otherwise the top-level `tag`. The `??` operator is used, so an empty recipient tag overrides the top-level tag and then sends no `Tag` at all, because an empty string is falsy.

Failure handling: as in `sendBatch`.

Result: the output of `formatBatchResults`.

### 5.5 Template tools

#### listTemplates (index.js:682)

Postmark call: GET `/templates?count=100&offset=0`. The query is always sent. There are no arguments.

Result: a heading `Found N templates:`, where N is the length of the returned list. Each template is a bullet line with the name in bold markup, then indented lines for `ID`, `Alias` (or `none`), `Subject` (or `none`), and `Type` and `Layout` when those fields are present. Templates are separated by blank lines.

If exactly 100 templates are returned, the heading gains ` (first 100 shown, server may have more; pagination not yet supported)`. No pagination is implemented. A server with zero templates produces the heading with an empty list.

#### getTemplate (index.js:714)

Postmark call: GET `/templates/{id}`, with the identifier encoded by `encodeURIComponent`.

Argument: `templateIdOrAlias` is a number or a string. Neither is validated further.

Result: `Template: <Name>`, then `ID`, `Alias` (or `none`), `Subject` (with no fallback, so an absent subject prints `undefined`), `Type`, `Layout` (or `none`), `Active`, and `Associated Server` from `AssociatedServerId`. Then an HTML Body section and a Text Body section, each showing the content or `(empty)`.

#### createTemplate (index.js:744)

Postmark call: POST `/templates`.

Arguments:

- `name` is a required string.
- `subject`, `htmlBody`, `textBody`, `alias` are optional strings. The `alias` description says it may contain letters, numbers, dots, hyphens and underscores, but the code applies no pattern to it.
- `templateType` is an optional enum of `Standard` or `Layout`.
- `layoutTemplate` is an optional string, the alias of a Layout. The code does not check that it exists.

Guards, in this order, each thrown before any request:

1. Neither `htmlBody` nor `textBody` is truthy: `At least one of htmlBody or textBody must be provided`.
2. The template is not a Layout and `subject` is not truthy: `Subject is required for Standard templates`. A missing `templateType` counts as Standard.
3. The template is a Layout and `subject` is truthy: an error stating that Subject must not be provided for Layout templates because Postmark rejects that field on Layouts.
4. `layoutTemplate` is truthy and the template is a Layout: an error stating that only Standard templates can wrap a layout.

Body sent: `Name` always. `Subject`, `HtmlBody`, `TextBody`, `Alias`, `TemplateType` and `LayoutTemplate` are included only when truthy. The server never sends `TemplateType` unless the caller supplied it.

Failure handling: errors from `postmarkRequest` propagate.

Result: `Template created successfully!`, then `ID`, `Name`, `Alias` (or `none`), `Layout` (or `none`) and `Active`, all from the response.

#### editTemplate (index.js:798)

Postmark call: PUT `/templates/{id}`. The identifier is encoded.

Arguments: `templateIdOrAlias` is required. `name`, `subject`, `htmlBody`, `textBody` and `alias` are optional strings. `layoutTemplate` is an optional nullable string.

Body construction. `Name`, `Subject`, `HtmlBody`, `TextBody` and `Alias` are included only when truthy, so an empty string cannot clear a field. When `layoutTemplate` is not `undefined`, it is sent as `LayoutTemplate`. `null` becomes an empty string. Any other value, including an empty string, is sent as given. The code comment says Postmark treats JSON `null` as no change and an empty string as removing the binding. The code does not fetch the existing template first, and fields left out of the body are simply not sent.

Guard: if the body has no keys, it throws `Provide at least one field to update (name, subject, htmlBody, textBody, alias, or layoutTemplate)`. A call that supplies only empty strings reaches the same error. No check is made of whether the template is a Layout, so a Layout can receive `subject` or `layoutTemplate` without a client-side error.

Failure handling: errors from `postmarkRequest` propagate.

Result: `Template updated successfully!`, then `ID`, `Name`, `Alias` (or `none`), `Layout` (or `none`) and `Active`, all from the response.

#### deleteTemplate (index.js:846)

Postmark call: DELETE `/templates/{id}`. The identifier is encoded.

Argument: `templateIdOrAlias`, a number or string.

Guards: none. The layout constraint in the description (a Layout cannot be deleted while Standard templates are bound to it) is not checked by the code. Postmark's response decides, and any refusal propagates as a `postmarkRequest` error.

Response: the body is ignored. Any 2xx response counts as success.

Result: `Template "<id>" deleted successfully.`, using the identifier as given.

#### validateTemplate (index.js:867)

Postmark call: POST `/templates/validate`. Nothing is saved.

Arguments: `subject`, `htmlBody`, `textBody` and `layoutTemplate` are optional strings. `testRenderModel` is an optional object. `templateType` is an optional enum of `Standard` or `Layout`.

Guard: if none of `subject`, `htmlBody` or `textBody` is truthy, it throws `At least one of subject, htmlBody, or textBody must be provided`.

Body: `Subject`, `HtmlBody`, `TextBody`, `TestRenderModel`, `TemplateType` and `LayoutTemplate` are included only when truthy.

Result: a heading `Template Validation: ALL VALID` when the response's `AllContentIsValid` is truthy, or `Template Validation: HAS ERRORS` otherwise. Then one section for each of `Subject`, `HtmlBody` and `TextBody` that is present in the response. Each section shows `Valid` or `INVALID`, then `Errors:` with the messages joined by `, ` when there are any. The subject section also shows `Rendered:` with the rendered subject when present. The HTML and text sections do not show rendered output. Sections are separated by blank lines. If none is present, the heading stands alone.

### 5.6 Message tools

#### searchOutboundMessages (index.js:925)

Postmark call: GET `/messages/outbound` with a query string.

Arguments, all optional:

- `recipient` is a plain string, not email-validated. It is sent as `recipient`.
- `fromEmail` is a plain string, sent as `fromemail`.
- `tag` is sent as `tag`.
- `subject` is sent as `subject`.
- `status` is an enum of `queued`, `sent` or `processed`, sent as `status`.
- `messageStream` is sent as `messagestream`.
- `fromDate` and `toDate` are date strings, sent as `fromdate` and `todate`.
- `count` is an integer from 1 to 500. It defaults to 50.
- `offset` is an integer of 0 or more. It defaults to 0.

`count` and `offset` are always sent. The description says their sum cannot exceed 10,000. The code does not check this, and Postmark's response decides.

Guards: none.

Result: when `Messages` is empty, `No messages found matching your criteria.` Otherwise a heading `Found <TotalCount> messages (showing <n>):`, then one bullet per message. Each bullet has the subject in bold markup, then `MessageID`, `To` (the `Recipients` joined with `, `), `From`, `Status`, `Date` (from `ReceivedAt`) and `Tag` (or `none`). A message with no `Recipients` field throws a TypeError.

#### getMessageDetails (index.js:975)

Postmark call: GET `/messages/outbound/{messageId}/details`, with the identifier encoded.

Argument: `messageId`, a string, not validated further.

Result: `Message Details`, then `MessageID`, `Subject`, `From`, `To` (the `Recipients` joined with `, `), `Status`, `Date` (from `ReceivedAt`) and `Tag` (or `none`). If `MessageEvents` has entries, a blank line and `Events:` follow, with one indented line per event: the `Type`, `at`, the `ReceivedAt` value, and the `Details.Summary` in parentheses when present. Otherwise the text reads `No events recorded.`

### 5.7 Diagnostics: diagnoseDelivery (index.js:1012)

This tool makes only read calls. It is a composite of several GETs. It has no guard and returns one text block.

Arguments:

- `recipient` is a required email-shaped string.
- `messageId` is an optional string.
- `fromDate` and `toDate` are optional date strings in the format above.
- `messageStream` is an optional string.

Derived values, computed first (index.js:1024 to 1028):

- `stream` is `messageStream` when given, otherwise `DEFAULT_MESSAGE_STREAM`.
- `today` is the current UTC date, as `YYYY-MM-DD`.
- `weekAgo` is the UTC date of the instant seven days (604,800,000 ms) before now.
- `windowFrom` is `fromDate` when given, otherwise `weekAgo`.
- `windowTo` is `toDate` when given, otherwise `today`. The two window values are not checked against each other.

Lookups, started together (index.js:1034 to 1048). Each lookup has its own catch, so none of them rejects, and a failure yields an empty list:

1. Message lookup, branch A, used when `messageId` is given: GET `/messages/outbound/{messageId}/details`. On success the result is wrapped in a one-element list. On failure the list is empty. The date window is not used in this branch.
2. Message lookup, branch B, used when no `messageId` is given: GET `/messages/outbound` with `recipient`, `fromdate` set to `windowFrom`, `todate` set to `windowTo`, and `count` set to 5. On success the list is the response's `Messages` array, or empty when it is missing. On failure the list is empty.
3. Suppression lookup: GET `/message-streams/{stream}/suppressions/dump` with `EmailAddress` set to the recipient. The list is the response's `Suppressions` array, or empty on failure or a missing array.
4. Bounce lookup: GET `/bounces` with `emailFilter` set to the recipient and `count` set to 10. The list is the response's `Bounces` array, or empty on failure or a missing array. The lookup is not filtered by stream.

A failed lookup is indistinguishable from an empty result in every later step, and the report contains no error text. Only the suppression lookup is filtered by message stream. The message search and the bounce lookup are not.

The most recent message, called `latest`, is then chosen (index.js:1051 to 1056):

- If the message list is empty, `latest` is `null`.
- Otherwise the first item is used, in the order Postmark returned it. The code does not sort.
- If that item has a `MessageEvents` field, it becomes `latest`.
- If not, a GET of its details by `MessageID` is made. If that call fails, the search item itself becomes `latest`.

Report text, built as lines in this order (index.js:1058 to 1131):

1. Heading `Delivery Diagnosis: <recipient>`, a rule of 48 box-drawing characters, and a blank line.
2. Suppression section. If the suppression list is non-empty, only its first item is used: `Suppression: SUPPRESSED on stream "<stream>"`, then indented `Reason`, `Origin` and `Since` (from `CreatedAt`). Otherwise the line `Suppression: not suppressed on stream "<stream>"`. Then a blank line.
3. Most recent message section. If `latest` exists: `Most recent message:`, then indented `MessageID`, `Subject` (or `(none)`), `Sent` (from `ReceivedAt`), and `Status`. If `latest` has events, an `Events:` line follows. It lists each distinct event type in order of first appearance. A type that occurs more than once is shown as the type, the multiplication sign and the count. Types are joined by `, `. If `latest` is `null`, the line reads `No messages found for <recipient> between <windowFrom> and <windowTo>.` This wording appears even when `messageId` was given, and the dates shown are the window values, which that branch does not use. Then a blank line.
4. Bounce section. If the bounce list is non-empty: `Bounce history (<n> recent):`, where n is the length of the list (at most 10 requested), then the first three bounces as indented lines of `<BouncedAt> <Type>: <Description>`. A bounce with a truthy `CanActivate` has ` [can reactivate]` appended. If there are more than three, a line `... and <n minus 3> more` follows. If the list is empty: `Bounce history: none`. Then a blank line.
5. Recommended action. The branches below are tested in this order, and the first one that applies is written.

Recommended action branches, in order:

- Branch 1, a suppression exists. The first suppression item decides the text by its `SuppressionReason`:
  - `SpamComplaint`: the text says the recipient marked previous mail as spam, that the suppression is permanent and cannot be lifted through the API, and that the caller should not retry.
  - `HardBounce`: the code looks for the first bounce in the bounce list with a truthy `CanActivate`. If one exists, the text says to run `activateBounce` with that bounce's `ID`, then resend. If none exists, the text says the address hard-bounced, to verify the address before retrying, and that if the address is confirmed valid, to run `deleteSuppressions` and then resend.
  - Any other reason, including `ManualSuppression` and any unlisted value: the text says to run `deleteSuppressions` with the address and then resend.
- Branch 2, no suppression, and `latest` has a `MessageEvents` entry whose `Type` is exactly `Delivered`. The text says the email was delivered, and suggests checking the spam folder or asking the recipient to allow the sender domain. A `latest` taken from a search item that lacks events does not match this branch.
- Branch 3, no suppression, and `latest` has `Status` exactly `Queued`. The text says the most recent message is still queued and to re-run in a few minutes.
- Branch 4, no suppression, and `latest` is `null`. The text says there was no recent send to this address and suggests `sendEmail` or a wider date range.
- Branch 5, anything else. The text is `Message status is "<Status>".` followed by a request to review the events above.

The branch tests are case-sensitive. Only the first suppression is examined, and the bounce list is used only in branch 1. The result is a single text content item, and no error response is produced by this tool.

### 5.8 Bounce tools

#### searchBounces (index.js:1139)

Postmark call: GET `/bounces` with a query string.

Arguments, all optional:

- `type` is an enum of 22 values: AddressChange, AutoResponder, BadEmailAddress, Blocked, ChallengeVerification, DMARCPolicy, DnsError, HardBounce, InboundError, ManuallyDeactivated, OpenRelayTest, SMTPApiError, SoftBounce, SpamComplaint, SpamNotification, Subscribe, TemplateRenderingFailed, Transient, Unconfirmed, Unknown, Unsubscribe and VirusNotification. It is sent as `type`.
- `inactive` is a boolean sent as `inactive`. A `false` value is sent, because the query helper keeps `false`.
- `emailFilter` is a plain string, sent as `emailFilter`.
- `tag` is sent as `tag`.
- `messageID` is sent as `messageID`, with that exact casing.
- `messageStream` is sent as `messagestream`.
- `fromDate` and `toDate` are sent as `fromdate` and `todate`.
- `count` is an integer from 1 to 500, defaulting to 50, always sent.
- `offset` is an integer of 0 or more, defaulting to 0, always sent. The description says the sum with `count` cannot exceed 10,000. The code does not check this.

Guards: none.

Result: when `Bounces` is empty, `No bounces found matching your criteria.` Otherwise a heading `Found <TotalCount> bounces (showing <n>):`, then one bullet per bounce. Each bullet has the email in bold markup, then `BounceID`, `Type` with `TypeCode` in parentheses, `Description`, `Date` (from `BouncedAt`), `Inactive`, `Can Activate` (from `CanActivate`), `Subject` (or `N/A`), and `Tag` (or `none`).

#### getBounceDump (index.js:1196)

Postmark call: GET `/bounces/{bounceId}/dump`.

Argument: `bounceId`, an integer.

Result: when the response's `Body` is truthy, the text `SMTP Bounce Dump (Bounce ID: <id>):`, a blank line, then the body verbatim. Otherwise `No SMTP dump available for bounce <id>. Dumps are retained for 30 days.` The 30-day statement is therefore produced only in the no-dump case.

#### activateBounce (index.js:1219)

Postmark call: PUT `/bounces/{bounceId}/activate`.

Argument: `bounceId`, an integer.

Guards: none in the code. The description says the call works only where `CanActivate` is true, and that SpamComplaint bounces cannot be reactivated. That is left to Postmark, so a refusal from Postmark propagates as a `postmarkRequest` error.

Result: `Bounce reactivated successfully!`, then `Bounce ID` (from `result.Bounce.ID`), `Email` (from `result.Bounce.Email`) and `Message` (from `result.Message`). The success wording is produced for any 2xx response. A response without a `Bounce` object throws a TypeError after Postmark has already acted.

### 5.9 Suppression tools

#### listSuppressions (index.js:1245)

Postmark call: GET `/message-streams/{stream}/suppressions/dump` with a query string. The stream is encoded and defaults to `DEFAULT_MESSAGE_STREAM`.

Arguments, all optional:

- `suppressionReason` is an enum of `HardBounce`, `SpamComplaint` or `ManualSuppression`, sent as `SuppressionReason`.
- `origin` is an enum of `Recipient`, `Customer` or `Admin`, sent as `Origin`.
- `emailAddress` is a plain string, sent as `EmailAddress`.
- `fromDate` and `toDate` are sent as `fromdate` and `todate`.

No `count` or `offset` is sent, and the code does no paging.

Result: when the list is empty, `No suppressions found on stream "<stream>" matching your criteria.` Otherwise the heading `Found <n> suppressions (stream: <stream>):`, where n is the length of the returned list, then one bullet per suppression with `EmailAddress` in bold markup, then `Reason`, `Origin` and `Created` (from `CreatedAt`).

#### createSuppressions (index.js:1287)

Postmark call: POST `/message-streams/{stream}/suppressions`. The body is `{ Suppressions: [ { EmailAddress } ... ] }`.

Arguments: `emailAddresses` is a required array of 1 to 50 email-shaped strings. `messageStream` is optional and defaults to `DEFAULT_MESSAGE_STREAM`.

Guards: none. Duplicates are not removed. The code does not check the list against existing suppressions.

Result: `Suppression results (stream: <stream>):`, then one line per item in Postmark's response order. Each line is a bullet with `EmailAddress`, a colon, `Status`, and then the `Message` when one is present. The code does not interpret `Status`. A response without a `Suppressions` array throws a TypeError.

#### deleteSuppressions (index.js:1318)

Postmark call: POST `/message-streams/{stream}/suppressions/delete`. This is a POST, not a DELETE. The body has the same shape as `createSuppressions`.

Arguments and limits: the same as `createSuppressions`. The description says SpamComplaint suppressions cannot be deleted through the API. The code does not check for that and leaves the outcome to the per-address `Status` in Postmark's response. The description also says that deleting a HardBounce suppression has the same effect as reactivating that bounce.

Result: `Suppression deletion results (stream: <stream>):`, followed by the same per-address lines as `createSuppressions`.

### 5.10 Stats tool: getDeliveryStats (index.js:1351)

Postmark call: GET, with the path chosen by the `stat` argument (table below). The query may contain `tag`, `fromdate`, `todate` and `messagestream`, each sent only when supplied. There is no default stream. When no filter is given, the path is sent with no query string.

Stat values and paths (index.js:1376 to 1391):

- `summary` (default): `/stats/outbound`.
- `overview`: `/stats/outbound`.
- `sent`: `/stats/outbound/sends`.
- `bounces`: `/stats/outbound/bounces`.
- `spam`: `/stats/outbound/spam`.
- `tracked`: `/stats/outbound/tracked`.
- `opens`: `/stats/outbound/opens`.
- `openPlatforms`: `/stats/outbound/opens/platforms`.
- `openClients`: `/stats/outbound/opens/emailClients`.
- `openReadTimes`: `/stats/outbound/opens/readTimes`.
- `clicks`: `/stats/outbound/clicks`.
- `clickBrowsers`: `/stats/outbound/clicks/browserFamilies`.
- `clickPlatforms`: `/stats/outbound/clicks/platforms`.
- `clickLocation`: `/stats/outbound/clicks/location`.

Guards: none.

Summary output (`summary` only), built by `fmtDeliverySummary` (index.js:283 to 313). Percentages use one decimal place and are `0.0%` when the denominator is zero. Numbers use the `en-US` thousands separator whatever the host locale. Lines are:

- `Sent`, from `Sent`.
- `Tracked`, from `Tracked`, with its percentage of `Sent`.
- `Open rate`, `UniqueOpens` over `Tracked`, with the counts shown.
- `Click rate`, `UniqueLinksClicked` over `TotalTrackedLinksSent`, with the counts shown.
- `Bounced`, `Bounced` over `Sent`. The percentage is computed locally and is not the API's `BounceRate`.
- `Spam`, `SpamComplaints` over `Sent`. Computed locally, not from `SpamComplaintsRate`.

Filter lines follow a blank line. They appear only when a date or tag or stream was given: `Period:` with the start (or `start`) and end (or `now`) joined by a right arrow, `Tag:`, and `Stream:`. The period text uses the caller's values as given.

Other stats output, built by `fmtStatResponse` (index.js:316 to 403). The code reads the field names shown; the response shapes were not checked against the API.

- `overview`: a list of `Sent`, `Bounced` (with `BounceRate` to two decimals, default 0), `SMTP API errors` (`SMTPApiErrors`), `Spam complaints` (`SpamComplaints`, with `SpamComplaintsRate` to two decimals), `Tracked`, `Opens (total)`, `Opens (unique)`, `Tracked links sent`, `Total clicks`, `Unique link clicks`, `With open tracking`, and `With link tracking`.
- `sent`: `Sent`, then `Total` from `Sent`.
- `bounces`: every numeric field except `Days` and `Total` that is greater than zero, sorted descending, each with its percentage of the sum. A zero sum prints `(no bounces in range)`.
- `spam`: `Spam Complaints`, then `Total` from the field `SpamComplaint` (singular).
- `tracked`: `Tracked Emails`, then `Total` from `Tracked`.
- `opens`: `Total opens` from `Opens` and `Unique opens` from `Unique`.
- `openPlatforms`: the four buckets Desktop, Mobile, WebMail and Unknown, each with its count and percentage of their own total. A zero total prints `(no data in range)`.
- `openClients`: heading `Email Client Usage (top 10)`. Every numeric field except `Days` is a row. The top ten by count are shown, with a percentage of the total of all rows, then `... and N more` when there are more.
- `openReadTimes`: heading `Open Read Times`, then the same generic breakdown with the same ten-row cap and `... and N more` line.
- `clicks`: `Total clicks` from `Clicks` and `Unique clicks` from `Unique`.
- `clickBrowsers`: heading `Click Browser Usage (top 10)` with the same generic top-ten breakdown.
- `clickPlatforms`: the four platform buckets, as for `openPlatforms`, under `Click Platform Usage`.
- `clickLocation`: `HTML` and `Text` counts from fields `HTML` and `Text`, each with a percentage of their sum.

The code's default branch returns the raw JSON. No `stat` value reaches it, because the schema limits the enum to the fourteen values above.

### 5.11 Server tool: getServerInfo (index.js:1404)

Postmark call: GET `/server`. No arguments and no guards.

Result: `Server: <Name>`, then `ID`, `Color`, `SMTP Activated` (from `SmtpApiActivated`), `Inbound Address` (or `none`), `Inbound Domain` (or `none`), then a `Tracking` block with `Open Tracking` (`TrackOpens`), `Link Tracking` (`TrackLinks`) and `First Open Only` (`PostFirstOpenOnly`), then a `Webhooks` block with legacy server-level `Bounce`, `Open`, `Delivery`, `Click` and `Inbound` URLs, each shown as `none` when absent.

### 5.12 Webhook tools

#### listWebhooks (index.js:1440)

Postmark call: GET `/webhooks`. The `messageStream` argument is sent as `messagestream` when given. It has no default.

Guards: none.

Result: when the list is empty, `No webhooks configured.` Otherwise `Found <n> webhooks:`, then one bullet per webhook with its `Url` in bold markup, then `ID`, `Stream` (from `MessageStream`, or `all`), and `Triggers`. The triggers are the names of enabled entries among `Open`, `Click`, `Delivery`, `Bounce`, `SpamComplaint` and `SubscriptionChange`, or `none`.

#### createWebhook (index.js:1480)

Postmark call: POST `/webhooks`.

Arguments:

- `url` is a required string. The schema requires Zod's URL check and a case-sensitive `https://` prefix.
- `messageStream` is an optional string. It has no default, so it is omitted when not given.
- Six optional booleans: `openEnabled`, `clickEnabled`, `deliveryEnabled`, `bounceEnabled`, `spamComplaintEnabled` and `subscriptionChangeEnabled`.

Guards, in this order, before any request:

1. If none of the six booleans is truthy, it throws `At least one trigger must be enabled` followed by the six argument names.
2. The URL allowlist check in section 5.3 runs. A rejection throws the message described there.

Body: `Url`, plus `Triggers` with six entries, `Open`, `Click`, `Delivery`, `Bounce`, `SpamComplaint` and `SubscriptionChange`. Each has `Enabled` set to the argument or `false`. `MessageStream` is included only when `messageStream` is given.

Failure handling: errors from `postmarkRequest` propagate.

Result: `Webhook created successfully!`, then `ID`, `URL` (from the response's `Url`), `Stream` (the response's `MessageStream`, or `all`), and `Triggers`, which lists the enabled trigger names from the response.

The webhook is persistent on the Postmark side until deleted. The tool description says this and says to register only URLs the caller controls. Those are upstream's words, quoted in section 6.

#### deleteWebhook (index.js:1547)

Postmark call: DELETE `/webhooks/{id}`.

Argument: `webhookId`, an integer.

Guards: none. The response body is ignored.

Result: `Webhook <id> deleted successfully.`, using the identifier as given.

## 6. Gotchas upstream records

These come from `CLAUDE.md`, `README.md`, `CHANGELOG.md` and the tool descriptions. They are upstream's own statements, paraphrased.

Email and sending:

- Batch endpoints are synchronous. `sendBatch` and `sendBatchWithTemplate` wrap `/email/batch` and `/email/batchWithTemplates`, accept up to 500 messages, and return per-message results (CLAUDE.md, README, CHANGELOG 2.0.0).
- Postmark's separate asynchronous bulk API (`/email/bulk`, submit then poll, with no count cap and a 50 MB payload limit) is not wrapped. CLAUDE.md says adding it needs two tools, `submitBulk` and `getBulkStatus`, and records it as a follow-up.
- Every outgoing message is set to open tracking and `HtmlAndText` link tracking (README, CLAUDE.md).
- A failed message inside a batch does not fail the HTTP call. README says so, and the batch tool descriptions say the same.
- Per-recipient tags in `sendBatchWithTemplate` override the top-level tag, as CLAUDE.md states. Section 5.4 notes that an empty override removes the tag.

Templates:

- `listTemplates` returns at most 100 templates. The endpoint has no pagination in this server, and a truncation note is printed when exactly 100 are returned (README, CHANGELOG 2.1.0).
- - A Layout template cannot be deleted while Standard templates are bound to it, and Layouts cannot carry a subject (`deleteTemplate` description, `createTemplate` description, README).
- `editTemplate` replaces content in place, and the description says this cannot be undone. Passing `layoutTemplate: null` unbinds a template (README, description).

Messages and bounces:

- `searchOutboundMessages` and `searchBounces` accept `count` and `offset`, and their descriptions say `count` plus `offset` cannot exceed 10,000 (CHANGELOG 2.1.0, descriptions). The code does not enforce this.
- Bounce records are retained for 45 days (`searchBounces` description). Bounce SMTP dumps are retained for 30 days (`getBounceDump` description, README).
- `activateBounce` works only where `CanActivate` is true, which is typically `HardBounce`. SpamComplaint bounces cannot be reactivated (`activateBounce` description).
- `diagnoseDelivery` uses a default window of the last 7 days (README, description). Its recommendations depend on the three lookups, as described in section 5.7.

Suppressions:

- The suppression list endpoint (`/message-streams/{stream}/suppressions/dump`) is eventually consistent. A suppression created moments earlier may not appear in the next `listSuppressions` call. CLAUDE.md says tests should not assume read-after-write consistency for it.
- SpamComplaint suppressions cannot be deleted through the API (`deleteSuppressions` description, README).
- Deleting a HardBounce suppression has the same effect as reactivating that bounce (`deleteSuppressions` description).
- `deleteSuppressions` is a POST to a `/delete` path, not a DELETE request.

Stats and server:

- CLAUDE.md notes that `summary` and `overview` both use `/stats/outbound`.
- The `/stats/outbound` response contains `Opens` and `UniqueOpens`, which is broader than its name suggests (CLAUDE.md).
- Some stats paths are not obvious. CLAUDE.md names `/stats/outbound/opens/emailClients`, `/stats/outbound/opens/platforms`, `/stats/outbound/clicks/browserFamilies` and `/stats/outbound/clicks/location`.
- `getServerInfo` reads `PostFirstOpenOnly` for the first-open-only setting. `EnableSmtpApiErrorHooks` is a different flag. The 2.0.0 CHANGELOG records a fix for reading the wrong field.
- `getServerInfo` shows legacy server-level webhook URLs. `listWebhooks` lists webhooks managed through the Webhooks API (`getServerInfo` description).

Webhooks:

- Webhooks are persistent and Postmark keeps calling the URL until the webhook is deleted (`createWebhook` description, README).
- Register only URLs the operator controls. To secure the receiving endpoint, whitelist Postmark's sending IPs at the network level (`createWebhook` description, README). The README links to Postmark's published IP list.
- `WEBHOOK_URL_ALLOWLIST` restricts `createWebhook` to listed prefixes. When unset, any HTTPS URL is accepted (README, CHANGELOG 2.1.0).
- The `createWebhook` `postFirstOpenOnly`, `bounceIncludeContent` and `spamIncludeContent` options were deferred and are not implemented (CHANGELOG, Unreleased).
- `getWebhook` and `editWebhook` were deferred and are not implemented. Without `editWebhook`, changing a webhook means deleting it and creating a new one, which the CHANGELOG says leaves a delivery gap (CHANGELOG, Unreleased).

Access and security:

- A Server Token gives full access to every operation on its server. Neither token type supports scoped permissions (README, Security Considerations).
- The README suggests a dedicated server for MCP traffic, limited streams and sender signatures, and token rotation if exposure is suspected. It also describes prompt injection as a risk, given that the server can send mail and register webhooks.
- Tool annotations are advisory hints for MCP clients. The README says some clients use them to auto-approve reads and prompt for writes (README).

Runtime and packaging:

- The minimum Node.js version is 22 (`engines.node` `>=22.0.0`). Node 20 receives `EBADENGINE` warnings or install failures under `engine-strict` (CHANGELOG 3.0.0).
- Redirects are refused for every request, so a redirect from Postmark now fails the call (CHANGELOG 3.0.0).
- `LOG_FILE` has no rotation or size limit. The README suggests an external tool such as logrotate (README, `.env.example`).
- The 2.1.0 CHANGELOG describes email masking as pseudonymisation rather than anonymisation, because the domain and the first and last mailbox characters are kept (CHANGELOG 2.1.0).
- `POSTMARK_SKIP_VERIFY` is described as intended for tests and offline use. The code prints a warning when it is used with a non-test token (CHANGELOG 2.1.0, index.js:151).

### Discrepancies between upstream documents and the code

- `CLAUDE.md` says `X-Postmark-Correlation-Id` is sent on requests. The code does not send it (section 3).
- `CLAUDE.md` names `getOutboundMessages`, `getOutboundMessageDetails`, `getSuppressions` and `getBounces` as the lookups used by `diagnoseDelivery`. The code uses direct paths, and none of those names is a function or a registered tool.
- The README says `htmlBody` and `textBody` are logged as a byte count. The code logs the JavaScript string length (section 4.1).
- The README's error-handling section says there is no exposure of sensitive information in logs. Plain `console.error` lines in section 4.2 print unmasked addresses and URLs.
- The 2.1.0 CHANGELOG names the log field `sanitizedArgs`. The code and README use `args` (section 4).
- `index.js` header says version 1.0.0. The runtime version is 3.0.0 from `package.json`.
- The `alias` description for `createTemplate` describes allowed characters. The code applies no pattern.
- The allowlist is described as HTTPS URL prefixes. The code matches origin and path prefix, and does not check the scheme of the entries (section 5.3).
