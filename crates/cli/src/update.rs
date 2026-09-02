//! `strata update` — channel-aware self-update (#3038).
//!
//! Updates the curl-installed binary in place: resolve the target release,
//! download the target-triple tarball and `checksums-sha256.txt`, verify the
//! SHA-256 **before** touching anything, then atomically replace the running
//! binary. A Homebrew-managed binary is redirected to `brew upgrade` (it belongs
//! to the tap, exactly as `uninstall` defers to `brew uninstall`).
//!
//! Downloads shell out to the same tools `install.sh` uses (`curl`, `tar`) so the
//! CLI carries no HTTP/TLS stack; the machine already has them, since the binary
//! was curl-installed. The checksum is computed in-process (`sha2`), so a
//! tampered download is caught without trusting an external tool. Explicit-invoke
//! only — there is no update-on-startup.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::uninstall::is_homebrew_install;
use crate::CliError;

const REPO: &str = "stratalab/strata-core";
const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub(crate) fn run_update(
    check_only: bool,
    target_version: Option<String>,
) -> Result<Value, CliError> {
    let exe = std::env::current_exe().map_err(|error| {
        CliError::usage(format!("could not locate the running binary: {error}"))
    })?;
    if is_homebrew_install(&exe) {
        return Err(CliError::usage(
            "this strata binary is managed by Homebrew; run `brew upgrade strata` instead",
        ));
    }
    let triple = target_triple()?;

    // The version we would move to: an explicit --version, else the latest release.
    let explicit = target_version.is_some();
    let wanted = match target_version {
        Some(v) => v.trim_start_matches('v').to_owned(),
        None => resolve_latest_version()?,
    };
    let up_to_date = !is_newer(&wanted, CURRENT);

    match decide(check_only, up_to_date, explicit) {
        Action::Report => {
            if up_to_date {
                eprintln!("strata is up to date ({CURRENT}).");
            } else {
                eprintln!("an update is available: {CURRENT} -> {wanted}  (run `strata update`)");
            }
            return Ok(json!({
                "type": "update",
                "data": { "current": CURRENT, "latest": wanted, "update_available": !up_to_date, "changed": false }
            }));
        }
        Action::AlreadyCurrent => {
            eprintln!("strata is already up to date ({CURRENT}).");
            return Ok(json!({
                "type": "update",
                "data": { "current": CURRENT, "latest": wanted, "update_available": false, "changed": false }
            }));
        }
        Action::Install => {}
    }

    // --- download into a scratch dir that is cleaned up on drop ---
    let scratch = TempDir::new()?;
    let asset = asset_name(&wanted, triple);
    let base = format!("https://github.com/{REPO}/releases/download/v{wanted}");
    let tarball = scratch.path().join(&asset);
    let sums = scratch.path().join("checksums-sha256.txt");
    eprintln!("downloading strata {wanted} ({triple}) ...");
    download(&format!("{base}/{asset}"), &tarball)?;
    download(&format!("{base}/checksums-sha256.txt"), &sums)?;

    // --- verify sha256 BEFORE replacing anything ---
    let sums_text = std::fs::read_to_string(&sums)
        .map_err(|e| CliError::usage(format!("could not read checksums: {e}")))?;
    let expected = expected_sha(&sums_text, &asset).ok_or_else(|| {
        CliError::usage(format!(
            "release {wanted} has no checksum entry for {asset}"
        ))
    })?;
    let got = sha256_file(&tarball)?;
    if !got.eq_ignore_ascii_case(expected) {
        return Err(CliError::usage(format!(
            "checksum mismatch for {asset} — refusing to install (expected {expected}, got {got})"
        )));
    }

    // --- extract, then atomically replace the running binary ---
    extract(&tarball, scratch.path())?;
    let staged = scratch.path().join("strata");
    if !staged.exists() {
        return Err(CliError::usage(
            "downloaded archive did not contain a `strata` binary",
        ));
    }
    replace_binary(&exe, &staged)?;

    eprintln!("updated strata {CURRENT} -> {wanted}");
    Ok(json!({
        "type": "update",
        "data": { "current": CURRENT, "latest": wanted, "update_available": false, "changed": true }
    }))
}

/// The release asset triple for the host, or an error on an unsupported target.
/// The release ships exactly three: the two Linux glibc targets and Apple aarch64.
fn target_triple() -> Result<&'static str, CliError> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        (os, arch) => Err(CliError::usage(format!(
            "`strata update` has no release build for {os}/{arch}; reinstall from https://stratadb.org/install.sh"
        ))),
    }
}

fn asset_name(version: &str, triple: &str) -> String {
    format!("strata-v{version}-{triple}.tar.gz")
}

/// What `run_update` should do, decided from the flags alone (pure).
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// `--check`: report status, install nothing.
    Report,
    /// No `--check`, already current, and not an explicit version: nothing to do.
    AlreadyCurrent,
    /// Download and replace (behind, or an explicit `--version` re/install).
    Install,
}

fn decide(check_only: bool, up_to_date: bool, explicit: bool) -> Action {
    if check_only {
        Action::Report
    } else if up_to_date && !explicit {
        Action::AlreadyCurrent
    } else {
        Action::Install
    }
}

/// Parse an `X.Y.Z` version, ignoring any pre-release/build suffix.
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut it = core.split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next()?.parse().ok()?;
    let c = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((a, b, c))
}

/// Whether `candidate` is a strictly newer version than `current`. An unparseable
/// candidate is treated as newer (an explicit user request we don't second-guess).
fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(new), Some(cur)) => new > cur,
        _ => true,
    }
}

/// The SHA-256 for `asset` from a `checksums-sha256.txt` (`<sha>␠␠<name>` lines).
fn expected_sha<'a>(checksums: &'a str, asset: &str) -> Option<&'a str> {
    checksums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let sha = parts.next()?;
        let name = parts.next()?;
        (name == asset).then_some(sha)
    })
}

fn sha256_file(path: &Path) -> Result<String, CliError> {
    let bytes = std::fs::read(path)
        .map_err(|e| CliError::usage(format!("could not read download: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn resolve_latest_version() -> Result<String, CliError> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let out = Command::new("curl")
        .args(["-fsSL", "-A", "strata-update", &url])
        .output()
        .map_err(|e| CliError::usage(format!("could not run curl: {e}")))?;
    if !out.status.success() {
        return Err(CliError::usage(
            "could not reach the release API to resolve the latest version",
        ));
    }
    let body: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| CliError::usage(format!("could not parse the release API response: {e}")))?;
    let tag = body
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| CliError::usage("the release API response had no tag_name"))?;
    Ok(tag.trim_start_matches('v').to_owned())
}

fn download(url: &str, dest: &Path) -> Result<(), CliError> {
    let status = Command::new("curl")
        .args(["-fSL", "--proto", "=https", "-o"])
        .arg(dest)
        .arg(url)
        .status()
        .map_err(|e| CliError::usage(format!("could not run curl: {e}")))?;
    if !status.success() {
        return Err(CliError::usage(format!("download failed: {url}")));
    }
    Ok(())
}

fn extract(tarball: &Path, into: &Path) -> Result<(), CliError> {
    let status = Command::new("tar")
        .arg("xzf")
        .arg(tarball)
        .arg("-C")
        .arg(into)
        .status()
        .map_err(|e| CliError::usage(format!("could not run tar: {e}")))?;
    if !status.success() {
        return Err(CliError::usage("failed to extract the downloaded archive"));
    }
    Ok(())
}

/// Atomically replace `current` with `staged`: copy into a temp file in the SAME
/// directory (so the rename is atomic on one filesystem), make it executable, and
/// rename over the running binary. Unix permits renaming over a live executable.
fn replace_binary(current: &Path, staged: &Path) -> Result<(), CliError> {
    let dir = current.parent().ok_or_else(|| {
        CliError::usage("could not determine the installation directory of the running binary")
    })?;
    let tmp = dir.join(".strata.update.tmp");
    std::fs::copy(staged, &tmp).map_err(|e| {
        CliError::usage(format!(
            "cannot write to {} ({e}) — is this a system install? reinstall via https://stratadb.org/install.sh",
            dir.display()
        ))
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| CliError::usage(format!("could not set permissions: {e}")))?;
    }
    std::fs::rename(&tmp, current).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        CliError::usage(format!("could not replace the running binary: {e}"))
    })?;
    Ok(())
}

/// A scratch directory removed on drop (best-effort).
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Result<Self, CliError> {
        let base = std::env::temp_dir().join(format!("strata-update-{}", std::process::id()));
        std::fs::create_dir_all(&base)
            .map_err(|e| CliError::usage(format!("could not create a scratch directory: {e}")))?;
        Ok(Self(base))
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{asset_name, decide, expected_sha, is_newer, parse_version, Action};

    #[test]
    fn decide_covers_the_flag_truth_table() {
        // --check always reports, regardless of state.
        assert_eq!(decide(true, true, false), Action::Report);
        assert_eq!(decide(true, false, false), Action::Report);
        assert_eq!(decide(true, true, true), Action::Report);
        // Not --check, up to date, no explicit version → nothing to do.
        assert_eq!(decide(false, true, false), Action::AlreadyCurrent);
        // Not --check, behind → install.
        assert_eq!(decide(false, false, false), Action::Install);
        // An explicit --version installs even when "up to date" (reinstall/rollback).
        assert_eq!(decide(false, true, true), Action::Install);
    }

    #[test]
    fn version_parsing_ignores_v_prefix_and_suffix() {
        assert_eq!(parse_version("1.2.0"), Some((1, 2, 0)));
        assert_eq!(parse_version("v1.2.0"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.2.0-rc1"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.0.1"), None);
        assert_eq!(parse_version("nonsense"), None);
    }

    #[test]
    fn newer_compares_numerically_and_defaults_to_installing_on_garbage() {
        assert!(is_newer("1.2.0", "1.1.1"));
        assert!(is_newer("1.10.0", "1.9.9")); // numeric, not lexical
        assert!(!is_newer("1.1.1", "1.1.1"));
        assert!(!is_newer("1.1.0", "1.1.1"));
        // An unparseable explicit target is honored (treated as newer).
        assert!(is_newer("weird-tag", "1.1.1"));
    }

    #[test]
    fn asset_name_matches_the_release_convention() {
        assert_eq!(
            asset_name("1.2.0", "aarch64-apple-darwin"),
            "strata-v1.2.0-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn expected_sha_finds_the_matching_asset_line() {
        let sums = "\
abc123  strata-v1.2.0-x86_64-unknown-linux-gnu.tar.gz
def456  strata-v1.2.0-aarch64-apple-darwin.tar.gz
";
        assert_eq!(
            expected_sha(sums, "strata-v1.2.0-aarch64-apple-darwin.tar.gz"),
            Some("def456")
        );
        assert_eq!(expected_sha(sums, "strata-v1.2.0-nope.tar.gz"), None);
    }
}
