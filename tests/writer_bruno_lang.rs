//! Accord entre les fichiers écrits par `bru-writer` et le parser officiel
//! de Bruno (`@usebruno/lang`).
//!
//! Pour chaque fixture d'ajout, de suppression ou de renommage,
//! `<nom>.after.bruno.json` est la relecture de `<nom>.after.bru` par Bruno,
//! générée par `scripts/gen-writer-bruno-fixtures.sh`. Le test par défaut la
//! compare à la vue de `bru-parser`, sans Node ; le test ignoré relance le
//! parser de Bruno et vérifie que la relecture versionnée est toujours
//! exacte (`cargo test --test writer_bruno_lang -- --ignored`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bruno_tui::collection::{BruFile, KeyValue, RequestView};
use serde_json::Value;

/// Même liste que `CASES_NAMES` dans `scripts/gen-writer-bruno-fixtures.sh`.
const ENTRY_CASES: [&str; 9] = [
    "add-header",
    "add-block",
    "add-path-between",
    "remove-entry",
    "rename-disabled",
    "duplicates",
    "query-sync",
    "url-sync",
    "crlf-add",
];

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases")
}

fn bruno_json(name: &str) -> Value {
    let text = fs::read_to_string(cases().join(format!("{name}.after.bruno.json")))
        .expect("relecture Bruno versionnée");
    serde_json::from_str(&text).expect("JSON valide")
}

/// Entrées Bruno `{ name, value, enabled }` sous la forme de `bru-parser`.
fn key_values<'a>(items: impl Iterator<Item = &'a Value>) -> Vec<KeyValue> {
    items
        .map(|item| KeyValue {
            key: item["name"].as_str().expect("name").to_owned(),
            value: item["value"].as_str().expect("value").to_owned(),
            enabled: item["enabled"].as_bool().expect("enabled"),
        })
        .collect()
}

#[test]
fn bru_parser_agrees_with_recorded_bruno_reading() {
    for name in ENTRY_CASES {
        let source =
            fs::read_to_string(cases().join(format!("{name}.after.bru"))).expect("fixture after");
        let view = RequestView::from_ast(&BruFile::parse(source).expect("AST")).expect("vue");
        let bruno = bruno_json(name);

        assert_eq!(view.url, bruno["url"].as_str().expect("url"), "{name}");
        let headers = bruno["headers"].as_array().expect("headers");
        assert_eq!(view.headers, key_values(headers.iter()), "{name}");
        let params = bruno["params"].as_array().expect("params");
        let of_type = |kind: &str| {
            key_values(
                params
                    .iter()
                    .filter(move |item| item["type"].as_str() == Some(kind)),
            )
        };
        assert_eq!(view.query_params, of_type("query"), "{name}");
        assert_eq!(view.path_params, of_type("path"), "{name}");
    }
}

#[test]
#[ignore = "nécessite node et @usebruno/cli (bru) installés"]
fn bruno_still_reads_after_fixtures_as_recorded() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/bruno-lang-dump.js");
    for name in ENTRY_CASES {
        let output = Command::new("node")
            .arg(&script)
            .arg(cases().join(format!("{name}.after.bru")))
            .output()
            .expect("node lancé");
        assert!(
            output.status.success(),
            "{name} : {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let fresh: Value = serde_json::from_slice(&output.stdout).expect("JSON valide");
        assert_eq!(fresh, bruno_json(name), "{name}");
    }
}
