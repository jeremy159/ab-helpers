use config::Format;
use std::collections::HashMap;

use crate::config::{RawSecret, resolve_secret_file_paths};

#[test]
fn raw_secret_format_puts_trimmed_content_at_its_key() {
    let map = RawSecret("actual.password")
        .parse(None, "hunter2\n")
        .unwrap();

    assert_eq!(
        map.get("actual.password")
            .unwrap()
            .clone()
            .into_string()
            .unwrap(),
        "hunter2"
    );
}

#[test]
fn resolve_secret_file_paths_returns_empty_when_nothing_set() {
    let vars: HashMap<String, String> = HashMap::new();
    let resolved = resolve_secret_file_paths(|k| vars.get(k).cloned()).unwrap();
    assert_eq!(resolved, vec![]);
}

#[test]
fn resolve_secret_file_paths_resolves_a_set_file_var() {
    let vars = HashMap::from([(
        "ABH_ACTUAL__PASSWORD_FILE".to_string(),
        "/path/to/secret".to_string(),
    )]);
    let resolved = resolve_secret_file_paths(|k| vars.get(k).cloned()).unwrap();

    assert_eq!(
        resolved,
        vec![("actual.password", "/path/to/secret".to_string())]
    );
}

#[test]
fn resolve_secret_file_paths_errors_when_both_direct_and_file_var_set() {
    let vars = HashMap::from([
        ("ABH_ACTUAL__PASSWORD".to_string(), "inline".to_string()),
        (
            "ABH_ACTUAL__PASSWORD_FILE".to_string(),
            "/path/to/secret".to_string(),
        ),
    ]);
    let err = resolve_secret_file_paths(|k| vars.get(k).cloned()).unwrap_err();

    assert!(err.to_string().contains("ABH_ACTUAL__PASSWORD"));
    assert!(err.to_string().contains("ABH_ACTUAL__PASSWORD_FILE"));
}
