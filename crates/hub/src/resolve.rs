//! Hub-URL resolution — the 5-layer precedence chain from stratahub's
//! `strata-cli-hub-resolution-config.md` §2.
//!
//! First source that yields a value wins: explicit flag, environment,
//! per-project `.strata/config.toml` (walking up from the working
//! directory, stopping at a `.git` boundary or the filesystem root),
//! global user config, then a structured refusal. A malformed source
//! never falls through silently — it aborts naming the source.
//!
//! Hub-neutrality (§5, Q8): this module is strata-core's single
//! designated defaults surface, and it carries **no** default hub URL.
//! A fresh install with nothing configured refuses hub commands.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use url::Url;

/// Which layer produced the resolved URL (surfaced by `config show`
/// style diagnostics).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HubUrlSource {
    /// The `--hub` flag (or an explicit per-call override).
    Flag,
    /// The `STRATA_HUB_URL` environment variable.
    Environment,
    /// A per-project `.strata/config.toml`, at the recorded path.
    ProjectConfig(PathBuf),
    /// The global user config, at the recorded path.
    GlobalConfig(PathBuf),
}

impl fmt::Display for HubUrlSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Flag => formatter.write_str("--hub flag"),
            Self::Environment => formatter.write_str("STRATA_HUB_URL"),
            Self::ProjectConfig(path) | Self::GlobalConfig(path) => {
                write!(formatter, "{}", path.display())
            }
        }
    }
}

/// A resolved hub URL plus the layer it came from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedHubUrl {
    /// The parsed base URL.
    pub url: Url,
    /// The layer that supplied it.
    pub source: HubUrlSource,
}

/// Resolution failure modes.
#[derive(Debug)]
#[non_exhaustive]
pub enum HubUrlError {
    /// A source supplied a value that does not parse as a URL, or a
    /// config file is malformed. Never falls through to lower layers.
    MalformedSource {
        /// The offending source, by name.
        source: String,
        /// Parse failure detail.
        detail: String,
    },
    /// No layer supplied a URL (§2 layer 5): the structured refusal.
    NotConfigured,
}

impl fmt::Display for HubUrlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedSource { source, detail } => {
                write!(formatter, "{source}: {detail}")
            }
            Self::NotConfigured => formatter.write_str(
                "no hub URL configured\n\n\
                 set one of the following, in any order of preference:\n  \
                 --hub <url>                  (one-off, this command only)\n  \
                 STRATA_HUB_URL=<url>         (shell session, e.g., CI)\n  \
                 ./.strata/config.toml        (per-project, in or above CWD)\n  \
                 the global strata config     (run `strata config set hub.url <url>`)",
            ),
        }
    }
}

impl Error for HubUrlError {}

/// Inputs to resolution, parameterized so callers own process state
/// (argv, environment, CWD, platform config paths) and tests inject it.
#[derive(Clone, Debug, Default)]
pub struct HubUrlInputs {
    /// Layer 1: the `--hub` flag value (or per-call override), verbatim.
    pub flag: Option<String>,
    /// Layer 2: the `STRATA_HUB_URL` value, verbatim. Empty string is
    /// treated as unset; whitespace-only is a parse error.
    pub environment: Option<String>,
    /// Layer 3 anchor: the working directory the project-config walk
    /// starts from.
    pub working_dir: Option<PathBuf>,
    /// Layer 4: the platform's global config file path.
    pub global_config: Option<PathBuf>,
}

/// Resolves the hub URL by the §2 precedence chain.
///
/// # Errors
///
/// [`HubUrlError::MalformedSource`] when the winning source is invalid;
/// [`HubUrlError::NotConfigured`] when no layer supplies a value.
pub fn resolve_hub_url(inputs: &HubUrlInputs) -> Result<ResolvedHubUrl, HubUrlError> {
    if let Some(flag) = &inputs.flag {
        return parse_layer(flag, HubUrlSource::Flag, "--hub");
    }

    if let Some(environment) = &inputs.environment {
        if environment.is_empty() {
            // Empty means unset; fall through.
        } else if environment.trim().is_empty() {
            return Err(HubUrlError::MalformedSource {
                source: "STRATA_HUB_URL".to_owned(),
                detail: "value is whitespace-only".to_owned(),
            });
        } else {
            return parse_layer(environment, HubUrlSource::Environment, "STRATA_HUB_URL");
        }
    }

    if let Some(working_dir) = &inputs.working_dir {
        if let Some(config_path) = find_project_config(working_dir) {
            let url = read_config_hub_url(&config_path)?;
            return Ok(ResolvedHubUrl {
                url,
                source: HubUrlSource::ProjectConfig(config_path),
            });
        }
    }

    if let Some(global) = &inputs.global_config {
        if global.is_file() {
            let url = read_config_hub_url(global)?;
            return Ok(ResolvedHubUrl {
                url,
                source: HubUrlSource::GlobalConfig(global.clone()),
            });
        }
    }

    Err(HubUrlError::NotConfigured)
}

fn parse_layer(
    value: &str,
    source: HubUrlSource,
    source_name: &str,
) -> Result<ResolvedHubUrl, HubUrlError> {
    let url = Url::parse(value).map_err(|error| HubUrlError::MalformedSource {
        source: source_name.to_owned(),
        detail: format!("not a valid URL: {error}"),
    })?;
    Ok(ResolvedHubUrl { url, source })
}

/// Walks up from `working_dir` looking for `.strata/config.toml`,
/// stopping past a `.git` boundary or at the filesystem root.
fn find_project_config(working_dir: &Path) -> Option<PathBuf> {
    let mut current = Some(working_dir);
    while let Some(dir) = current {
        let candidate = dir.join(".strata/config.toml");
        if candidate.is_file() {
            return Some(candidate);
        }
        if dir.join(".git").exists() {
            return None;
        }
        current = dir.parent();
    }
    None
}

fn read_config_hub_url(path: &Path) -> Result<Url, HubUrlError> {
    let source = path.display().to_string();
    let text = std::fs::read_to_string(path).map_err(|error| HubUrlError::MalformedSource {
        source: source.clone(),
        detail: format!("unreadable: {error}"),
    })?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|error| HubUrlError::MalformedSource {
            source: source.clone(),
            detail: format!("malformed TOML: {error}"),
        })?;
    let url = value
        .get("hub")
        .and_then(|hub| hub.get("url"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| HubUrlError::MalformedSource {
            source: source.clone(),
            detail: "missing [hub].url".to_owned(),
        })?;
    Url::parse(url).map_err(|error| HubUrlError::MalformedSource {
        source,
        detail: format!("[hub].url is not a valid URL: {error}"),
    })
}

impl HubUrlInputs {
    /// Gathers resolution inputs from the process environment: the
    /// caller's explicit flag, `STRATA_HUB_URL`, the working directory
    /// (for the project-config walk), and the platform's global config
    /// path. This is the entry frontends use so resolution behavior is
    /// identical everywhere.
    #[must_use]
    pub fn from_process(flag: Option<String>) -> Self {
        Self {
            flag,
            environment: std::env::var("STRATA_HUB_URL").ok(),
            working_dir: std::env::current_dir().ok(),
            global_config: global_config_path(),
        }
    }
}

/// The platform's global strata config file path
/// (`<config dir>/strata/config.toml`), when the platform exposes one.
#[must_use]
pub fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("strata/config.toml"))
}

/// Reads `hub.url` from the global config; `Ok(None)` when the file or
/// key is absent.
///
/// # Errors
///
/// [`HubUrlError::MalformedSource`] when the file exists but is invalid.
pub fn read_global_hub_url() -> Result<Option<Url>, HubUrlError> {
    let Some(path) = global_config_path() else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    match read_config_hub_url(&path) {
        Ok(url) => Ok(Some(url)),
        Err(HubUrlError::MalformedSource { detail, .. }) if detail == "missing [hub].url" => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Writes `hub.url` into the global config, preserving other keys and
/// creating the file (`0600` on Unix) and parent directories on first
/// use. Returns the file path written.
///
/// # Errors
///
/// [`HubUrlError::MalformedSource`] when `url` is not a valid URL or the
/// existing file is unreadable/unwritable.
pub fn write_global_hub_url(url: &str) -> Result<PathBuf, HubUrlError> {
    let parsed = Url::parse(url).map_err(|error| HubUrlError::MalformedSource {
        source: "hub.url".to_owned(),
        detail: format!("not a valid URL: {error}"),
    })?;
    let path = global_config_path().ok_or_else(|| HubUrlError::MalformedSource {
        source: "global config".to_owned(),
        detail: "the platform exposes no user config directory".to_owned(),
    })?;
    edit_global_config(&path, |hub| {
        hub.insert(
            "url".to_owned(),
            toml::Value::String(parsed.as_str().to_owned()),
        );
    })?;
    Ok(path)
}

/// Removes `hub.url` from the global config, leaving other keys. Returns
/// the file path when the file existed.
///
/// # Errors
///
/// [`HubUrlError::MalformedSource`] on unreadable/unwritable state.
pub fn unset_global_hub_url() -> Result<Option<PathBuf>, HubUrlError> {
    let Some(path) = global_config_path() else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    edit_global_config(&path, |hub| {
        hub.remove("url");
    })?;
    Ok(Some(path))
}

fn edit_global_config(
    path: &Path,
    edit: impl FnOnce(&mut toml::map::Map<String, toml::Value>),
) -> Result<(), HubUrlError> {
    let source = path.display().to_string();
    let malformed = |detail: String| HubUrlError::MalformedSource {
        source: source.clone(),
        detail,
    };
    let mut root: toml::Value = if path.is_file() {
        let text = std::fs::read_to_string(path)
            .map_err(|error| malformed(format!("unreadable: {error}")))?;
        toml::from_str(&text).map_err(|error| malformed(format!("malformed TOML: {error}")))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };
    let table = root
        .as_table_mut()
        .ok_or_else(|| malformed("config root is not a table".to_owned()))?;
    let hub = table
        .entry("hub")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let hub = hub
        .as_table_mut()
        .ok_or_else(|| malformed("[hub] is not a table".to_owned()))?;
    edit(hub);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| malformed(format!("config directory: {error}")))?;
    }
    let rendered = toml::to_string_pretty(&root)
        .map_err(|error| malformed(format!("config serialization: {error}")))?;
    std::fs::write(path, rendered).map_err(|error| malformed(format!("write failed: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}
