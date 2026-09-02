//! `strata uninstall` — remove the Strata installation (#2995).
//!
//! A port of the pre-V1 command (`2b3062f5:crates/cli/src/admin.rs`), V1
//! adjusted: a Homebrew-managed binary is redirected to `brew uninstall`, the
//! summary renders through the uniform value pipeline (so `--json` works), and
//! `~/.config/strata/` is deliberately KEPT — `install.sh` documents that
//! databases and the user config are never touched, so a reinstall finds the
//! provider keys and hub settings where it left them. The interactive
//! inventory and confirmation stay on stderr; stdout carries only the summary.
//!
//! Removed: the Strata home (`STRATA_HOME` or `~/.strata` — binary, models,
//! caches), a `STRATA_INSTALL_DIR` outside it, the legacy `~/.stratadb/`
//! model dir, `~/.strata_history`, and the `# Strata` PATH block `install.sh`
//! appends to shell configs (matched by the expanded install-dir marker, the
//! same way `install.sh` detects it). Per-project databases are never touched.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::{init, CliError};

pub(crate) fn run_uninstall(skip_confirm: bool) -> Result<Value, CliError> {
    if let Ok(exe) = std::env::current_exe() {
        if is_homebrew_install(&exe) {
            return Err(CliError::usage(
                "this strata binary is managed by Homebrew; run `brew uninstall strata` instead",
            ));
        }
    }
    let home_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| CliError::usage("could not determine the home directory"))?;
    let strata_dir = init::strata_home()?;
    let legacy_dir = home_dir.join(".stratadb");
    let history_file = home_dir.join(".strata_history");
    let install_dir = std::env::var_os("STRATA_INSTALL_DIR")
        .map(PathBuf::from)
        .filter(|dir| !dir.starts_with(&strata_dir));

    eprintln!("This will remove:");
    eprintln!();
    if strata_dir.exists() {
        eprintln!("  {}   binary, models, and caches", strata_dir.display());
    }
    if let Some(dir) = install_dir.as_deref().filter(|dir| dir.exists()) {
        eprintln!(
            "  {}   installed binary (STRATA_INSTALL_DIR)",
            dir.display()
        );
    }
    if legacy_dir.exists() {
        eprintln!("  {}   legacy model files", legacy_dir.display());
    }
    if history_file.exists() {
        eprintln!("  {}   REPL history", history_file.display());
    }
    eprintln!("  PATH entries        from shell configuration files");
    eprintln!();
    eprintln!("Kept: per-project database directories and ~/.config/strata/");
    eprintln!("(provider keys and hub settings survive a reinstall).");
    eprintln!();

    if !skip_confirm {
        eprint!("Continue? [y/N] ");
        io::stderr().flush()?;
        let mut answer = String::new();
        io::stdin().lock().read_line(&mut answer)?;
        if !answer.trim().eq_ignore_ascii_case("y") {
            eprintln!("Aborted.");
            return Ok(json!({"type": "uninstall", "data": {"aborted": true}}));
        }
    }

    let mut removed: Vec<String> = Vec::new();
    remove_tree(&strata_dir, &mut removed);
    if let Some(dir) = install_dir.as_deref() {
        remove_tree(dir, &mut removed);
    }
    remove_tree(&legacy_dir, &mut removed);
    if history_file.exists() {
        match std::fs::remove_file(&history_file) {
            Ok(()) => removed.push(history_file.display().to_string()),
            Err(error) => eprintln!(
                "  warning: could not remove {}: {error}",
                history_file.display()
            ),
        }
    }

    // The marker is the expanded install dir, exactly what install.sh appends
    // (and greps for) in the `# Strata` PATH block.
    let marker = strata_dir.join("bin").display().to_string();
    let mut cleaned: Vec<String> = Vec::new();
    for config in shell_configs(&home_dir) {
        match clean_path_block(&config, &marker) {
            Ok(true) => cleaned.push(config.display().to_string()),
            Ok(false) => {}
            Err(error) => {
                eprintln!("  warning: could not update {}: {error}", config.display());
            }
        }
    }

    eprintln!();
    eprintln!("Strata has been uninstalled. Restart your shell to apply PATH changes.");
    Ok(json!({
        "type": "uninstall",
        "data": {
            "aborted": false,
            "removed": removed,
            "cleaned_path_from": cleaned,
        }
    }))
}

/// A Homebrew-managed binary belongs to the tap: `brew uninstall` owns it.
/// Shared with `update` (which defers to `brew upgrade` the same way).
pub(crate) fn is_homebrew_install(exe: &Path) -> bool {
    let text = exe.to_string_lossy();
    text.contains("/Cellar/") || text.starts_with("/opt/homebrew/") || text.contains("/linuxbrew/")
}

fn remove_tree(dir: &Path, removed: &mut Vec<String>) {
    if !dir.exists() {
        return;
    }
    match std::fs::remove_dir_all(dir) {
        Ok(()) => removed.push(dir.display().to_string()),
        Err(error) => eprintln!("  warning: could not remove {}: {error}", dir.display()),
    }
}

/// The shell configs `install.sh` may have appended the PATH block to.
fn shell_configs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".zshrc"),
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".profile"),
        home.join(".config/fish/config.fish"),
    ]
}

/// Drop the `# Strata` PATH block from `contents`: the comment line and every
/// line carrying the expanded install-dir marker. `None` when the marker is
/// absent — the file must not be rewritten at all.
fn filter_path_lines(contents: &str, marker: &str) -> Option<String> {
    if !contents.contains(marker) {
        return None;
    }
    let mut kept: Vec<&str> = contents
        .lines()
        .filter(|line| line.trim() != "# Strata" && !line.contains(marker))
        .collect();
    while kept.last() == Some(&"") {
        kept.pop();
    }
    let mut cleaned = kept.join("\n");
    cleaned.push('\n');
    Some(cleaned)
}

/// Returns whether `path` carried (and was cleaned of) the PATH block.
fn clean_path_block(path: &Path, marker: &str) -> io::Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let contents = std::fs::read_to_string(path)?;
    let Some(cleaned) = filter_path_lines(&contents, marker) else {
        return Ok(false);
    };
    std::fs::write(path, cleaned)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{filter_path_lines, is_homebrew_install};
    use std::path::Path;

    #[test]
    fn homebrew_paths_are_detected_and_normal_paths_are_not() {
        assert!(is_homebrew_install(Path::new(
            "/usr/local/Cellar/strata/1.1.0/bin/strata"
        )));
        assert!(is_homebrew_install(Path::new("/opt/homebrew/bin/strata")));
        assert!(is_homebrew_install(Path::new(
            "/home/linuxbrew/.linuxbrew/bin/strata"
        )));
        assert!(!is_homebrew_install(Path::new(
            "/home/user/.strata/bin/strata"
        )));
        assert!(!is_homebrew_install(Path::new("/usr/local/bin/strata")));
    }

    #[test]
    fn filter_removes_exactly_the_strata_block() {
        let marker = "/home/user/.strata/bin";
        let contents = "# my stuff\nexport FOO=bar\n\n# Strata\nexport PATH=\"/home/user/.strata/bin:$PATH\"\n";
        let cleaned = filter_path_lines(contents, marker).expect("marker present");
        assert_eq!(cleaned, "# my stuff\nexport FOO=bar\n");
    }

    #[test]
    fn filter_leaves_marker_free_files_untouched() {
        // `None` means "do not rewrite": a config without the block must not
        // be modified (not even reformatted).
        assert_eq!(
            filter_path_lines("# my stuff\nexport FOO=bar\n", "/home/user/.strata/bin"),
            None
        );
    }

    #[test]
    fn filter_keeps_unrelated_strata_comments_only_when_marker_line_differs() {
        // The fish variant (`fish_add_path <dir>`) is caught by the marker
        // even though the line shape differs from the export form.
        let marker = "/home/user/.strata/bin";
        let contents = "set -x EDITOR vim\n\n# Strata\nfish_add_path /home/user/.strata/bin\n";
        let cleaned = filter_path_lines(contents, marker).expect("marker present");
        assert_eq!(cleaned, "set -x EDITOR vim\n");
    }
}
