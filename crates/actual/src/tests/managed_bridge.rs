use super::super::managed_bridge::{
    INDEX_JS, LOCKFILE_HASH_STAMP, PACKAGE_JSON, PACKAGE_LOCK_JSON, current_lockfile_hash,
    ensure_managed_bridge_script_at, needs_npm_install, npm_binary_for, write_if_changed,
};
use std::path::{Path, PathBuf};

const BARE_NODE: &str = "node";

#[test]
fn writes_embedded_files_to_target_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bridge");

    // Pre-create node_modules and a matching stamp so npm is never
    // invoked - this test must not touch the network.
    std::fs::create_dir_all(dir.join("node_modules")).unwrap();
    std::fs::write(dir.join(LOCKFILE_HASH_STAMP), current_lockfile_hash()).unwrap();

    let index_js = ensure_managed_bridge_script_at(&dir, Path::new(BARE_NODE)).unwrap();

    assert_eq!(index_js, dir.join("index.js"));
    assert_eq!(std::fs::read_to_string(&index_js).unwrap(), INDEX_JS);
    assert_eq!(
        std::fs::read_to_string(dir.join("package.json")).unwrap(),
        PACKAGE_JSON
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("package-lock.json")).unwrap(),
        PACKAGE_LOCK_JSON
    );
}

#[test]
fn overwrites_stale_embedded_files() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bridge");
    std::fs::create_dir_all(dir.join("node_modules")).unwrap();
    std::fs::write(dir.join("index.js"), "stale content").unwrap();
    // Make node_modules look up-to-date so we don't shell out to npm.
    std::fs::write(dir.join(LOCKFILE_HASH_STAMP), current_lockfile_hash()).unwrap();

    ensure_managed_bridge_script_at(&dir, Path::new(BARE_NODE)).unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.join("index.js")).unwrap(),
        INDEX_JS
    );
}

#[test]
fn write_if_changed_skips_write_when_content_matches() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("file.txt");
    std::fs::write(&path, "hello").unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();

    // Sleep briefly so a real rewrite would be observable via mtime on
    // filesystems with coarse mtime resolution.
    std::thread::sleep(std::time::Duration::from_millis(10));
    write_if_changed(&path, "hello").unwrap();

    let after = std::fs::metadata(&path).unwrap().modified().unwrap();
    assert_eq!(before, after, "file should not have been rewritten");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
}

#[test]
fn write_if_changed_atomically_replaces_differing_content() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("file.txt");
    std::fs::write(&path, "old").unwrap();

    write_if_changed(&path, "new").unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    // No leftover temp files.
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");
}

#[test]
fn needs_npm_install_true_without_node_modules() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(needs_npm_install(tmp.path()));
}

#[test]
fn needs_npm_install_false_with_node_modules_and_matching_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("node_modules")).unwrap();
    std::fs::write(
        tmp.path().join(LOCKFILE_HASH_STAMP),
        current_lockfile_hash(),
    )
    .unwrap();
    assert!(!needs_npm_install(tmp.path()));
}

#[test]
fn needs_npm_install_true_with_node_modules_but_missing_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("node_modules")).unwrap();
    assert!(needs_npm_install(tmp.path()));
}

#[test]
fn needs_npm_install_true_with_node_modules_but_stale_stamp() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("node_modules")).unwrap();
    std::fs::write(tmp.path().join(LOCKFILE_HASH_STAMP), "not-a-real-hash").unwrap();
    assert!(needs_npm_install(tmp.path()));
}

#[test]
fn npm_binary_for_bare_node_uses_ambient_npm() {
    assert_eq!(npm_binary_for(Path::new(BARE_NODE)), PathBuf::from("npm"));
}

#[test]
fn npm_binary_for_pathed_node_prefers_sibling_npm_if_present() {
    let tmp = tempfile::tempdir().unwrap();
    let node_bin = tmp.path().join("node");
    std::fs::write(&node_bin, "").unwrap();
    let npm_bin = tmp.path().join("npm");
    std::fs::write(&npm_bin, "").unwrap();

    assert_eq!(npm_binary_for(&node_bin), npm_bin);
}

#[test]
fn npm_binary_for_pathed_node_falls_back_when_sibling_npm_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let node_bin = tmp.path().join("node");
    std::fs::write(&node_bin, "").unwrap();
    // No sibling `npm` written.

    assert_eq!(npm_binary_for(&node_bin), PathBuf::from("npm"));
}

/// Manual verification only: actually shells out to `npm ci`.
/// Requires network access and `npm` on PATH, so it's `#[ignore]`d and
/// must be run explicitly with `cargo test -- --ignored`.
#[test]
#[ignore]
fn real_npm_install_succeeds() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("bridge");

    let index_js = ensure_managed_bridge_script_at(&dir, Path::new(BARE_NODE)).unwrap();

    assert!(index_js.is_file());
    assert!(dir.join("node_modules").is_dir());
}
