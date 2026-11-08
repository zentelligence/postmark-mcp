//! Signed releases served from memory, for tests that drive the whole lifecycle
//! through the real updater without a network or a real server binary.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use updater_core::{ActiveVersionReader, ProductVersion};
use updater_engine::{
    Clock, Ed25519ManifestVerifier, EngineError, ExecutableVersionProbe, HttpTransport, TrustedKey, detect_target,
};

use crate::host::Host;
use crate::installation::Installation;

pub(crate) type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const MANIFEST_URL: &str = "https://releases.test/latest.json";
const EXE: &str = std::env::consts::EXE_SUFFIX;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// The release server: one archive per published version, and a manifest that
/// names whichever version is current, signed by `key`.
pub(crate) struct Releases {
    key: SigningKey,
    archives: RefCell<HashMap<String, Vec<u8>>>,
    /// The versions successive manifest requests are given. The last stays.
    current: RefCell<VecDeque<String>>,
    /// The manifest most recently served, which its signature request signs.
    served: RefCell<Vec<u8>>,
    downloads: Cell<usize>,
}

impl Releases {
    fn new(key: SigningKey) -> Self {
        Self {
            key,
            archives: RefCell::default(),
            current: RefCell::default(),
            served: RefCell::default(),
            downloads: Cell::new(0),
        }
    }

    /// Publishes `version` and makes it the current release.
    pub(crate) fn publish(&self, version: &str) -> TestResult {
        self.publish_quietly(version)?;
        *self.current.borrow_mut() = VecDeque::from([version.to_owned()]);
        Ok(())
    }

    /// Makes successive manifest requests see each of `versions` in turn, as if
    /// releases were published between them.
    pub(crate) fn publish_in_turn(&self, versions: &[&str]) -> TestResult {
        for version in versions {
            self.publish_quietly(version)?;
        }
        *self.current.borrow_mut() = versions.iter().map(|version| (*version).to_owned()).collect();
        Ok(())
    }

    fn publish_quietly(&self, version: &str) -> TestResult {
        let archive = release_archive(version)?;
        self.archives.borrow_mut().insert(version.to_owned(), archive);
        Ok(())
    }

    /// How many archives have been downloaded.
    pub(crate) fn downloads(&self) -> usize {
        self.downloads.get()
    }

    fn archive_url(version: &str) -> TestResult<String> {
        Ok(format!(
            "https://releases.test/postmark-mcp-{version}-{}.tar.gz",
            detect_target()?
        ))
    }

    fn manifest(&self) -> TestResult<Vec<u8>> {
        let version = {
            let mut current = self.current.borrow_mut();
            if current.len() > 1 {
                current.pop_front()
            } else {
                current.front().cloned()
            }
        }
        .ok_or("nothing is published")?;
        let archive = self.archives.borrow().get(&version).cloned().ok_or("no archive")?;
        let manifest = serde_json::json!({
            "schema_version": 1,
            "product": "postmark-mcp",
            "channel": "stable",
            "version": version,
            "released_at": "2026-11-08T00:00:00Z",
            "minimum_supported_version": version,
            "minimum_launcher_version": "0.1.0",
            "release_notes_url": "https://releases.test/notes",
            "assets": [{
                "target": detect_target()?.to_string(),
                "url": Self::archive_url(&version)?,
                "sha256": hex(&Sha256::digest(&archive)),
                "size": archive.len(),
            }],
        });
        Ok(serde_json::to_vec(&manifest)?)
    }

    fn respond(&self, url: &str) -> TestResult<Vec<u8>> {
        if url == MANIFEST_URL {
            let manifest = self.manifest()?;
            self.served.replace(manifest.clone());
            return Ok(manifest);
        }
        if url == format!("{MANIFEST_URL}.sig") {
            let signature = self.key.sign(&self.served.borrow());
            let envelope = serde_json::json!({ "key_id": "test", "signature": hex(&signature.to_bytes()) });
            return Ok(serde_json::to_vec(&envelope)?);
        }
        for (version, archive) in self.archives.borrow().iter() {
            if url == Self::archive_url(version)? {
                self.downloads.set(self.downloads.get() + 1);
                return Ok(archive.clone());
            }
        }
        Err(format!("nothing is served at {url}").into())
    }
}

impl HttpTransport for Releases {
    fn fetch(&self, url: &str, _max_bytes: u64) -> Result<Vec<u8>, EngineError> {
        self.respond(url).map_err(|error| EngineError::Transport {
            url: url.to_owned(),
            reason: error.to_string(),
        })
    }

    fn stream(&self, url: &str, writer: &mut dyn std::io::Write) -> Result<u64, EngineError> {
        let failed = |reason: String| EngineError::Transport {
            url: url.to_owned(),
            reason,
        };
        let bytes = self.respond(url).map_err(|error| failed(error.to_string()))?;
        writer.write_all(&bytes).map_err(|error| failed(error.to_string()))?;
        u64::try_from(bytes.len()).map_err(|error| failed(error.to_string()))
    }
}

/// A release set: the server, which here is a text file holding what its
/// `--version` would print, the launcher, and `release.json`.
fn release_archive(version: &str) -> TestResult<Vec<u8>> {
    let metadata = serde_json::json!({
        "schema_version": 1,
        "product": "postmark-mcp",
        "version": version,
        "target": detect_target()?.to_string(),
        "published_at": "2026-11-08T00:00:00Z",
        "minimum_launcher_version": "0.1.0",
        "commands": [{ "name": "postmark-mcp", "path": format!("bin/postmark-mcp{EXE}") }],
    });
    let mut archive = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
    let entries = [
        ("release.json".to_owned(), serde_json::to_vec(&metadata)?),
        (
            format!("bin/postmark-mcp{EXE}"),
            format!("postmark-mcp {version}\n").into_bytes(),
        ),
        (
            format!("bin/postmark-mcp-launcher{EXE}"),
            format!("launcher {version}\n").into_bytes(),
        ),
    ];
    for (path, contents) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(u64::try_from(contents.len())?);
        header.set_mode(0o755);
        header.set_cksum();
        archive.append_data(&mut header, path, contents.as_slice())?;
    }
    Ok(archive.into_inner()?.finish()?)
}

/// Reads the fake server's version from its contents, as the real probe reads
/// the last word of `--version`.
struct ReadsVersion;

impl ExecutableVersionProbe for ReadsVersion {
    fn probe_version(&self, command_name: &str, executable: &Path) -> Result<ProductVersion, EngineError> {
        let failed = |reason: String| EngineError::ExecutableProbeFailed {
            name: command_name.to_owned(),
            reason,
        };
        let text = std::fs::read_to_string(executable).map_err(|error| failed(error.to_string()))?;
        let word = text
            .split_whitespace()
            .next_back()
            .ok_or_else(|| failed("empty".to_owned()))?;
        Ok(ProductVersion::try_from(word)?)
    }
}

struct FixedClock;

impl Clock for FixedClock {
    fn now_rfc3339(&self) -> String {
        "2026-11-08T00:00:00Z".to_owned()
    }
}

/// A computer with no installation yet, a release server, and a person who
/// answers every question with `answer`.
pub(crate) struct World {
    directory: TempDir,
    pub(crate) releases: Releases,
    verifier: Ed25519ManifestVerifier,
    answer: Box<dyn Fn(&str) -> bool>,
}

impl World {
    pub(crate) fn new() -> TestResult<Self> {
        Self::signed_by(SigningKey::from_bytes(&[7; 32]))
    }

    /// A release server whose manifests are signed by a key the launcher does
    /// not trust.
    pub(crate) fn signed_by_a_stranger() -> TestResult<Self> {
        Self::signed_by(SigningKey::from_bytes(&[9; 32]))
    }

    fn signed_by(key: SigningKey) -> TestResult<Self> {
        let trusted = SigningKey::from_bytes(&[7; 32]).verifying_key().to_bytes();
        Ok(Self {
            directory: tempfile::tempdir()?,
            releases: Releases::new(key),
            verifier: Ed25519ManifestVerifier::new(vec![TrustedKey::new("test", &trusted)?]),
            answer: Box::new(|_| true),
        })
    }

    /// Answers every question by calling `answer`, which can stand in for
    /// another launcher acting while the question is open.
    pub(crate) fn answering(mut self, answer: impl Fn(&str) -> bool + 'static) -> Self {
        self.answer = Box::new(answer);
        self
    }

    /// No home folder can be found.
    pub(crate) fn homeless_host(&self) -> Host<'_> {
        Host {
            home: None,
            ..self.host()
        }
    }

    pub(crate) fn declining(mut self) -> Self {
        self.answer = Box::new(|_| false);
        self
    }

    pub(crate) fn root(&self) -> PathBuf {
        self.directory.path().join("root")
    }

    pub(crate) fn home(&self) -> PathBuf {
        self.directory.path().join("home")
    }

    /// Where the launcher places the commands.
    pub(crate) fn commands(&self) -> PathBuf {
        if cfg!(windows) {
            self.root().join("bin")
        } else {
            self.home().join(".local").join("bin")
        }
    }

    pub(crate) fn installation(&self) -> TestResult<Installation> {
        Ok(Installation::at(self.root())?)
    }

    pub(crate) fn active(&self) -> TestResult<Option<String>> {
        Ok(self
            .installation()?
            .activation()
            .active_version()?
            .map(|version| version.to_string()))
    }

    /// The host, with the commands' folder already on `PATH`, so no shell
    /// profile or user `Path` is ever changed.
    pub(crate) fn host(&self) -> Host<'_> {
        Host {
            transport: &self.releases,
            verifier: &self.verifier,
            probe: &ReadsVersion,
            clock: &FixedClock,
            confirm: self.answer.as_ref(),
            update_url: Some(OsString::from(MANIFEST_URL)),
            home: Some(self.home()),
            path: Some(self.commands().into_os_string()),
            shell: None,
        }
    }
}
