set shell := ["bash", "-euo", "pipefail", "-c"]

fmt:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    cargo test --workspace --all-features --no-fail-fast

audit:
    cargo audit

deny:
    cargo deny check

licences:
    cargo about generate about.hbs -o THIRD_PARTY_LICENSES.md --workspace

ci: fmt clippy test audit deny
    shellcheck .claude/scripts/check.sh .claude/hooks/rustfmt-check.sh .claude/hooks/markdown-check.sh
