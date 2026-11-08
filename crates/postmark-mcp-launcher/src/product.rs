//! What the updater needs to know about this product, in one place.

/// The program the launcher starts, and the one command a release declares.
pub(crate) const SERVER_NAME: &str = "postmark-mcp";

/// The name the launcher manages the installation under.
pub(crate) const LAUNCHER_NAME: &str = "postmark-mcp-launcher";

/// The product name in every release's metadata and signed manifest. A release
/// for any other product is refused.
pub(crate) const PRODUCT: &str = "postmark-mcp";

/// The folder the release sets are kept in, under `~/.local/share` or
/// `%LOCALAPPDATA%`.
pub(crate) const INSTALL_FOLDER: &str = "postmark-mcp";

/// The prefix of the launcher's variables: `POSTMARK_MCP_INSTALL_ROOT` and
/// `POSTMARK_MCP_UPDATE_URL`.
pub(crate) const ENV_PREFIX: &str = "POSTMARK_MCP";

/// The name people read, which also marks the line added to a shell profile.
pub(crate) const PRODUCT_LABEL: &str = "Postmark MCP";

/// The signed manifest of the latest release. GitHub serves `releases/latest`
/// only once a release is promoted, after it has been installed on every
/// platform.
pub(crate) const DEFAULT_UPDATE_URL: &str =
    "https://github.com/zentelligence/postmark-mcp/releases/latest/download/latest.json";

/// How many release sets to keep: the active one and the one before it, for
/// rollback and for any server still running from it.
pub(crate) const RETAINED_VERSIONS: usize = 2;

/// A public key that release manifests may be signed with.
pub(crate) struct TrustedPublicKey {
    /// The identifier a signature names its key by.
    pub(crate) id: &'static str,
    /// The Ed25519 public key, as 64 hexadecimal digits.
    pub(crate) hex: &'static str,
}

/// The keys this launcher trusts. To rotate, add the new key here and sign with
/// it only once a release carrying it is out, so that launchers already
/// installed can verify the release that brings them the new key. Remove the old
/// key in a later release.
pub(crate) const TRUSTED_KEYS: [TrustedPublicKey; 1] = [TrustedPublicKey {
    id: "release-2026",
    hex: "f1e6f3f72b4a5dd96098651d1890b94ec632f2b8f0c9bea74169137b390fc352",
}];
