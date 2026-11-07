# Postmark MCP Agent Contract

This file governs all work in this repository. More specific `AGENTS.md` files may add constraints for their subtree, but may not weaken this contract.

## Mission and current state

Build `postmark-mcp`, a Rust MCP server for the Postmark email API. The read-only tools, delivery diagnosis, and sending are delivered; [`docs/roadmap.md`](docs/roadmap.md) records what is delivered and what comes next.

Behaviour is grounded in three sources: the roadmap, the [Postmark API documentation](https://postmarkapp.com/developer), and ActiveCampaign's `postmark-mcp` (Node.js), which informs the tool surface and configuration. If a behaviour is not settled by those, record the uncertainty and ask. Never invent missing semantics.

## Quality mandate

**Holistic quality trumps everything else.** Optimise for the integrity of the whole product and its lifecycle, including correctness, security, architecture, interoperability, test strength, operability, maintainability, performance, accessibility of diagnostics, and documentation.

Delivery speed, diff size, convenience, token cost, and passing a narrow test are never sufficient reasons to weaken the result. Scope discipline still applies: solve the requested problem completely and coherently without adding unrequested product surface.

Prior existence of an error, deviation, or violation is no excuse for perpetuating it. Identified issues are not "raised for later"; the only exception is a change with significant architectural implications that needs its own analysis and design.

## Start every substantive task

1. Read this file and [`.claude/rules/00-index.md`](.claude/rules/00-index.md).
2. Inspect the working tree and preserve unrelated or user-authored changes.
3. Identify the governing roadmap entry, and read the matching upstream behaviour.
4. Read only the `.claude` rules relevant to the task.
5. Define the smallest observable behaviour that can be driven by a failing test.

## Non-negotiable engineering rules

- Follow trait-first development.
- Use test-driven delivery for every behaviour change: red, green, then refactor. See [`.claude/workflows/tdd.md`](.claude/workflows/tdd.md).
- Do not add production behaviour until a focused test has failed for the expected reason. Bug fixes require a regression test.
- Keep dependency direction hexagonal and strictly towards `postmark-mcp-core`. Library crates never depend on `postmark-mcp`.
- All Postmark requests go through `HardenedTransport`. Never construct another HTTP client for Postmark.
- The server token never appears in a log, error, tool result, or debug output. Tool results never include Postmark `ApiTokens`, webhook passwords or headers, or the credentials, query, or fragment of a URL from Postmark.
- Production code contains no `unsafe`, `unwrap`, or `expect`. Use typed errors and preserve stable machine-readable error codes.
- Implement every project type conversion with `From<T>` when infallible or `TryFrom<T>` when fallible. Never create free-form conversion functions.
- Use `tracing` for runtime diagnostics, to stderr only. Stdout is reserved for MCP traffic, and for the output of CLI subcommands.
- Reject malformed or unknown input. Never silently repair it.
- Use Australian English spelling, grammar, and punctuation for operator-facing text and documentation, including the Oxford comma. Never use em dashes.
- Maximum 1000 lines per code file.
- Do not hard-wrap Markdown prose: one line per paragraph.
- Keep changes scoped. Do not commit, push, rewrite history, or alter releases unless the user explicitly asks.
- Never add attribution to commit messages.

## Test standards

Tests are part of the design, not a closing activity. See [`.claude/rules/testing.md`](.claude/rules/testing.md).

- Tests must be deterministic. Never depend on network access beyond loopback, wall-clock sleeps, the operator's environment, or test order.
- Do not weaken, delete, ignore, or over-broaden a test merely to make it pass.
- Name tests for the behaviour they prove. Comments explain the why in terms anyone can verify against the code.

## Rust quality gate

Run the narrowest relevant test during the red/green loop. Before declaring a code change complete, run:

```sh
.claude/scripts/check.sh
```

This runs the repository guidance checks and then `just ci`. Never claim a check passed unless it ran successfully. Report any skipped check and its reason.

## Definition of done

A change is complete only when:

- the governing roadmap entry is identified and updated;
- the failing test was observed and the minimal implementation makes it pass;
- the change has been assessed holistically, not only against its focused test;
- relevant negative and security cases are covered;
- formatting, linting, tests, and dependency checks pass;
- operator-facing errors are actionable and secrets remain redacted;
- documentation, including `docs/mcp-tools.md` and `CHANGELOG.md`, matches behaviour; and
- the final report lists changed files, verification performed, and residual risks or deliberate follow-ups.
