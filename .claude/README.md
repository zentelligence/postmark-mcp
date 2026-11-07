# Claude Development Kit

This directory is the project's operating handbook. The root [AGENTS.md](../AGENTS.md) is the mandatory engineering contract for every coding agent; everything here supplies the focused rules, workflows, review lenses, and templates it references, plus the Claude Code mechanisms that apply them during a session.

## Layout

```text
.claude/
├── README.md
├── settings.json
├── agents/code-reviewer.md
├── commands/{brief,gate,review-code,tdd}.md
├── hooks/{markdown-check,rustfmt-check}.sh
├── reviewers/code-review.md
├── rules/{00-index,architecture,mcp-contracts,memory,rust-quality,security,testing}.md
├── scripts/check.sh
├── templates/change-brief.md
└── workflows/{change-loop,quality-gate,tdd}.md
```

## How Claude Code uses this

- **Commands** (`commands/`) are `/brief`, `/gate`, `/review-code`, and `/tdd`. Each loads the relevant rules, workflow, reviewer, or template before acting.
- **Subagent** (`agents/`): `code-reviewer` applies the review lens to a diff and never modifies code.
- **Hooks** (`hooks/`, wired in `settings.json`) run after every `Edit` or `Write`, reporting Rust formatting drift and Markdown trailing whitespace straight away.
- **`scripts/check.sh`** is the completion gate: guidance checks (required files, trailing whitespace, local Markdown links, shellcheck), followed by `just ci`.

## Session entry point

1. Read `AGENTS.md` at the repository root.
2. Read [`rules/00-index.md`](rules/00-index.md).
3. Inspect the working tree and `docs/roadmap.md`.
4. Load only the rules and workflow needed for the task.
5. Use the review lens before reporting completion.

## Maintenance policy

- Put rules shared by every coding agent in `AGENTS.md`.
- Put focused, task-scoped execution guidance here.
- Put product and operator documentation in the root `README.md` or `docs/`.
- Change guidance in the same change as the behaviour or failure mode that made it necessary.
