//! Self-installing copy of the Node bridge for environments that don't have
//! a full git checkout of this repo (e.g. `cargo install`, or Docker images
//! built from just the compiled binary).
//!
//! The three small text files the bridge needs (`index.js`, `package.json`,
//! `package-lock.json`) are embedded into the compiled binary via
//! `include_str!` and written out to a managed directory on first use. The
//! `node_modules` directory (which contains platform-specific native
//! binaries pulled in by `@actual-app/api`) is deliberately *not* embedded -
//! it's installed with `npm ci` the first time it's missing or stale.
//!
//! Note: the functions in this module do blocking file I/O and shell out to
//! `npm` synchronously. Callers running inside an async runtime (e.g. the
//! daemon) should run them via `tokio::task::spawn_blocking` rather than
//! calling them directly on an async worker thread.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ActualResult;
use crate::error::Error;

pub(crate) const INDEX_JS: &str = include_str!("../bridge/index.js");
pub(crate) const PACKAGE_JSON: &str = include_str!("../bridge/package.json");
pub(crate) const PACKAGE_LOCK_JSON: &str = include_str!("../bridge/package-lock.json");

/// Name of the stamp file (in the managed bridge dir) that records a hash of
/// the `package-lock.json` that `node_modules` was last installed from.
pub(crate) const LOCKFILE_HASH_STAMP: &str = ".lockfile-hash";

/// Ensures the managed copy of the bridge is installed under
/// `$XDG_DATA_HOME/ab-helpers/bridge` (or `~/.local/share/ab-helpers/bridge`
/// if `XDG_DATA_HOME` is unset), installing `node_modules` via `npm ci` if
/// it's missing or stale (i.e. the embedded `package-lock.json` has changed
/// since the last install, e.g. after a binary upgrade), and returns the
/// path to the managed `index.js`.
///
/// `node_bin` is the configured Node binary (`ActualSettings::node_bin`);
/// when it points at a specific non-`PATH` Node install, the sibling `npm`
/// in the same directory is used instead of ambient `npm` on `PATH`, to
/// avoid installing native modules against the wrong Node ABI.
///
/// This is the fallback used when no `bridge_script` is explicitly
/// configured and the process isn't running from inside a full checkout of
/// this repo (see `ActualSettings::bridge_config` in `ab-helpers-server`).
pub fn ensure_managed_bridge_script(node_bin: &Path) -> ActualResult<PathBuf> {
    ensure_managed_bridge_script_at(&managed_bridge_dir()?, node_bin)
}

/// Core logic, taking the target directory explicitly so it's testable
/// without touching the real XDG data dir or requiring network access.
pub(crate) fn ensure_managed_bridge_script_at(
    dir: &Path,
    node_bin: &Path,
) -> ActualResult<PathBuf> {
    std::fs::create_dir_all(dir).map_err(|e| {
        Error::BridgeInstall(format!(
            "failed to create managed bridge directory `{}`: {e}",
            dir.display()
        ))
    })?;

    write_embedded_files(dir)?;

    if needs_npm_install(dir) {
        run_npm_install(dir, node_bin)?;
        write_lockfile_stamp(dir)?;
    }

    Ok(dir.join("index.js"))
}

/// Keep the three small embedded files in sync with whatever version of the
/// binary is running - but only touch disk when the content actually
/// differs, and do so via write-to-temp-then-rename so a concurrent reader
/// (e.g. a long-running daemon's `node` process) never observes a
/// partially-written file.
fn write_embedded_files(dir: &Path) -> ActualResult<()> {
    for (name, contents) in [
        ("index.js", INDEX_JS),
        ("package.json", PACKAGE_JSON),
        ("package-lock.json", PACKAGE_LOCK_JSON),
    ] {
        write_if_changed(&dir.join(name), contents)?;
    }
    Ok(())
}

/// Writes `contents` to `path` only if it differs from what's already
/// there, doing so atomically (write to a temp file in the same directory,
/// then `rename` into place) so readers never see a truncated file.
pub(crate) fn write_if_changed(path: &Path, contents: &str) -> ActualResult<()> {
    if let Ok(existing) = std::fs::read_to_string(path)
        && existing == contents
    {
        return Ok(());
    }

    let dir = path.parent().ok_or_else(|| {
        Error::BridgeInstall(format!(
            "managed bridge file path `{}` has no parent directory",
            path.display()
        ))
    })?;

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let tmp_path = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));

    std::fs::write(&tmp_path, contents).map_err(|e| {
        Error::BridgeInstall(format!(
            "failed to write temporary file `{}`: {e}",
            tmp_path.display()
        ))
    })?;

    std::fs::rename(&tmp_path, path).map_err(|e| {
        Error::BridgeInstall(format!(
            "failed to atomically install `{}`: {e}",
            path.display()
        ))
    })?;

    Ok(())
}

/// Whether `npm ci` needs to run: either `node_modules` isn't present, or
/// the lockfile-hash stamp is missing/stale relative to the embedded
/// `package-lock.json` (meaning the binary was upgraded since the last
/// install). A small pure predicate so it can be unit-tested without
/// touching the network.
pub(crate) fn needs_npm_install(dir: &Path) -> bool {
    if !dir.join("node_modules").is_dir() {
        return true;
    }

    match std::fs::read_to_string(dir.join(LOCKFILE_HASH_STAMP)) {
        Ok(stamp) => stamp.trim() != current_lockfile_hash(),
        Err(_) => true,
    }
}

/// Writes the current embedded lockfile's hash to the stamp file, recording
/// that `node_modules` now matches it.
fn write_lockfile_stamp(dir: &Path) -> ActualResult<()> {
    write_if_changed(&dir.join(LOCKFILE_HASH_STAMP), &current_lockfile_hash())
}

/// A stable hash of the embedded `package-lock.json`, as a hex string.
pub(crate) fn current_lockfile_hash() -> String {
    let mut hasher = DefaultHasher::new();
    PACKAGE_LOCK_JSON.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Derives the `npm` binary to invoke from the configured `node_bin`: if
/// `node_bin` points at a specific path (not just the bare command name),
/// look for `npm` alongside it and use that; otherwise fall back to `npm`
/// resolved from `PATH` (correct for the default `node_bin = "node"` case).
pub(crate) fn npm_binary_for(node_bin: &Path) -> PathBuf {
    match node_bin.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => {
            let candidate = dir.join("npm");
            if candidate.is_file() {
                return candidate;
            }
            PathBuf::from("npm")
        }
        _ => PathBuf::from("npm"),
    }
}

fn run_npm_install(dir: &Path, node_bin: &Path) -> ActualResult<()> {
    let npm_bin = npm_binary_for(node_bin);

    tracing::info!(
        dir = %dir.display(),
        npm = %npm_bin.display(),
        "installing Actual bridge dependencies via `npm ci`"
    );

    let status = Command::new(&npm_bin)
        .args(["ci", "--omit=dev", "--no-audit", "--no-fund"])
        .current_dir(dir)
        .status()
        .map_err(|e| {
            Error::BridgeInstall(format!(
                "failed to run `{} ci` in `{}`: {e}. Make sure Node.js \
                 (which provides `npm`) is installed and on PATH - it's required \
                 to set up the Actual bridge on first run.",
                npm_bin.display(),
                dir.display()
            ))
        })?;

    if !status.success() {
        return Err(Error::BridgeInstall(format!(
            "`{} ci` in `{}` exited with {status}. Check the npm output \
             above for details; Node.js and npm must be installed and on PATH.",
            npm_bin.display(),
            dir.display()
        )));
    }

    Ok(())
}

/// `$XDG_DATA_HOME/ab-helpers/bridge` or `~/.local/share/ab-helpers/bridge`.
fn managed_bridge_dir() -> ActualResult<PathBuf> {
    if let Some(x) = std::env::var_os("XDG_DATA_HOME")
        && !x.is_empty()
    {
        return Ok(PathBuf::from(x).join("ab-helpers").join("bridge"));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| {
        Error::BridgeInstall(
            "could not determine a home directory (HOME and XDG_DATA_HOME are both unset) \
             to install the managed Actual bridge into"
                .to_string(),
        )
    })?;
    Ok(PathBuf::from(home)
        .join(".local/share")
        .join("ab-helpers")
        .join("bridge"))
}
