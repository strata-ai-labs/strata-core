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
