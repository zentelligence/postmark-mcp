---
description: Draft a change brief before implementation
argument-hint: <roadmap entry or feature description>
---

Draft a change brief for: $ARGUMENTS

1. Read `.claude/templates/change-brief.md` for the required structure, and
   `.claude/workflows/change-loop.md` for the discovery steps.
2. Identify the governing `docs/roadmap.md` entry, and read the matching upstream behaviour in `postmark-mcp` where it exists, and the Postmark API documentation.
3. Inspect the working tree and the code and tests nearest the change so the brief reflects the actual baseline, not an assumed one.
4. Fill every section of the template: roadmap entry, behaviour (given, when, then, must not), scope, holistic quality assessment, planned TDD evidence, verification commands, and risks.
5. Surface contract ambiguities explicitly per the source-of-truth precedence in `.claude/rules/00-index.md` rather than resolving them silently.
6. Present the brief in the conversation.

Stop after the brief. Do not begin implementation until the user confirms scope, or the task explicitly authorised implementation.
