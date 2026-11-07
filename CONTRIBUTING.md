# Contributing to Postmark MCP

Thank you for your interest in contributing. This document is the entry point for human and AI-agent contributors; it summarises the workflow enforced by [`AGENTS.md`](AGENTS.md), which remains the binding contract for every substantive change.

## Code of Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). By participating, you are expected to uphold it.

## Before you start

1. Read [`AGENTS.md`](AGENTS.md) in full.
2. Read [`.claude/rules/00-index.md`](.claude/rules/00-index.md), and load only the guidance relevant to the area you are changing.
3. Check [`docs/roadmap.md`](docs/roadmap.md) for the planned shape of the work.

## Reporting issues

Open an issue describing the problem or proposal before starting non-trivial work, so the approach can be agreed before code is written. Never include a Postmark token in an issue, log, or test.

## Development workflow

1. Select a roadmap entry and define its observable contract.
2. Add the smallest failing test and confirm it fails for the intended reason (red).
3. Implement only enough behaviour to make it pass (green).
4. Refactor with the suite green.
5. Run the affected crate's tests, then the quality gate.

Bug fixes require a regression test that fails before the fix and passes after it.

## Quality gate

Run before opening a pull request:

```sh
.claude/scripts/check.sh
```

This needs `just`, `ripgrep`, `shellcheck`, `cargo-audit`, and `cargo-deny`. After any dependency change, also run `just licences`, which needs `cargo-about`.

Never claim a check passed unless it actually ran and succeeded; note any skipped check and why in your pull request description.

## Commit and pull request expectations

- Keep changes scoped to the problem described in your issue or pull request.
- Do not rewrite history on shared branches.
- Do not add commit attributions such as `Co-Authored-By` trailers.
- Describe what changed, what verification you ran, and any residual risk or deliberate follow-up.

## Licensing

By submitting a contribution, you agree that it is licensed under the terms in [`LICENSE`](LICENSE) (Apache License, Version 2.0).

## Getting help

For questions about the contribution process, open an issue or contact [support@zentelligence.com.au](mailto:support@zentelligence.com.au).
