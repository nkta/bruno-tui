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
use bruno_tui::writer::RequestWriter;
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

#[test]
#[ignore = "nécessite node et @usebruno/cli (bru) installés"]
fn bruno_parses_written_text_blocks_including_closing_braces() {
    let script_eval = r#"
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
function langDir() {
  if (process.env.BRUNO_LANG_DIR) return process.env.BRUNO_LANG_DIR;
  const globalRoot = execSync('npm root -g', { encoding: 'utf8' }).trim();
  return path.join(globalRoot, '@usebruno', 'cli', 'node_modules', '@usebruno', 'lang');
}
const { bruToJsonV2 } = require(langDir());
const content = fs.readFileSync(process.argv[2], 'utf8');
const parsed = bruToJsonV2(content);
process.stdout.write(JSON.stringify({
  preRequest: parsed.script?.req || '',
  postResponse: parsed.script?.res || '',
  tests: parsed.tests || '',
  docs: parsed.docs || ''
}));
"#;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp-bruno-lang-test");
    fs::create_dir_all(&dir).expect("dossier créé");
    let script_js = dir.join("parse.js");
    fs::write(&script_js, script_eval).expect("script écrit");

    let source = "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: http://a\n}\n";
    let bru_path = dir.join("test.bru");
    fs::write(&bru_path, source).expect("bru écrit");
    let (file, stamp) = (
        BruFile::parse(source.to_owned()).expect("AST"),
        bruno_tui::writer::FileStamp::capture(&bru_path).expect("stamp"),
    );
    let complex_code = "if (a) {\n  return { b: [1, 2] };\n}\n}\n]\n";
    bruno_tui::writer::BruWriter
        .write_request(
            &bru_path,
            &file,
            &stamp,
            &[
                bruno_tui::writer::FieldEdit::ScriptPreRequest(complex_code.into()),
                bruno_tui::writer::FieldEdit::Tests(
                    "test(\"ok\", () => {\n  expect(1).toBe(1);\n});".into(),
                ),
                bruno_tui::writer::FieldEdit::Docs("# Doc\nDes accolades { } dans la doc".into()),
            ],
        )
        .expect("écrit");

    let output = Command::new("node")
        .arg(&script_js)
        .arg(&bru_path)
        .output()
        .expect("node exécuté");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("json");
    assert_eq!(
        parsed["preRequest"].as_str().unwrap().trim(),
        complex_code.trim()
    );
    assert_eq!(
        parsed["tests"].as_str().unwrap().trim(),
        "test(\"ok\", () => {\n  expect(1).toBe(1);\n});"
    );
    assert_eq!(
        parsed["docs"].as_str().unwrap().trim(),
        "# Doc\nDes accolades { } dans la doc"
    );

    let _ = fs::remove_dir_all(&dir);
}
