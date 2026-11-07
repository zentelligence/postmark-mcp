# Memory and Knowledge Rules

Keep these stores distinct:

1. `docs/` holds in-repo technical documentation, which is the authority on how this project works.
2. mempalace (globally configured MCP) holds cross-session findings, learnings, and decisions with rationale, under the wing `postmark-mcp`.
3. The ContextOS vault holds non-technical information.

Anything a future contributor needs in order to work on this repository belongs in `docs/` or `.claude/`, not only in a personal memory store.

Local additions belong in `memory.local.md`, which is not committed.
