# Quality Gate

Verification is proportional during development and comprehensive before a
change is declared complete.

Passing commands is necessary but not sufficient. Completion also requires a
holistic review of correctness, security, data integrity, architecture,
cross-platform behaviour, interoperability, performance, operability,
maintainability, and documentation wherever the change can affect them.

## During the loop

Run the smallest target that demonstrates red or green, for example:

```sh
cargo test -p postmark-mcp-core server_token_rejects_values_that_cannot_be_sent_as_a_header -- --exact
cargo test -p postmark-mcp-client --test transport_contract redirects
```

Use the actual test target and name; do not cargo-cult these examples.

## Before completion

If the repository supplies `just ci`, it is the authoritative completion gate. Otherwise run:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo audit
```

Also run any applicable contract suite, platform suite, or manual acceptance
procedure. Documentation-only work
must at least run `.claude/scripts/check.sh` and inspect rendered links.

## Versioning

Bump `[workspace.package] version` in the root `Cargo.toml` as part of closing any change that adds, removes, or alters a tool, schema, config field, or other operator-visible behaviour; a documentation-only or purely internal refactor does not need a bump. Record the change in `CHANGELOG.md` under `[Unreleased]`.

## Completion evidence

The final report distinguishes:

- checks that passed;
- checks that failed and why;
- checks not run and why; and
- manual or platform-specific validation still required.

Warnings, flaky reruns, ignored tests, and unavailable audit tooling are not a
clean pass. Never claim that a gate passed from a narrower unit suite.
