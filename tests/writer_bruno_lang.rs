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

use bruno_tui::collection::{
    BodyContent, BodyKind, BodyView, BruFile, FileMeta, KeyValue, RequestView,
};
use bruno_tui::writer::RequestWriter;
use serde_json::Value;

/// Même liste que `CASES_NAMES` dans `scripts/gen-writer-bruno-fixtures.sh`.
const ENTRY_CASES: [&str; 23] = [
    "add-header",
    "add-block",
    "add-path-between",
    "remove-entry",
    "rename-disabled",
    "duplicates",
    "query-sync",
    "url-sync",
    "crlf-add",
    "vars-ops",
    "assert-ops",
    "settings-ops",
    "folder-ops",
    "collection-ops",
    "auth-mode-switch",
    "auth-param",
    "auth-apikey",
    "folder-auth",
    "collection-auth",
    "body-mode-ops",
    "body-form-urlencoded-ops",
    "body-multipart-form-ops",
    "body-graphql-vars-ops",
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
        let parsed = BruFile::parse(source).expect("AST");
        let view = RequestView::from_ast(&parsed)
            .unwrap_or_else(|_| FileMeta::from_ast(parsed.clone()).to_request_view());
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

        if let Some(body_mode) = bruno.get("body_mode").and_then(Value::as_str) {
            assert_eq!(view.body_mode(), body_mode, "{name} body_mode");
        }
        if let Some(body_form) = bruno.get("body_form").and_then(Value::as_array) {
            let entries = match &view.body {
                Some(BodyView {
                    kind: BodyKind::FormUrlEncoded,
                    content: BodyContent::Entries(entries),
                }) => entries.as_slice(),
                _ => panic!("{name} : attendu FormUrlEncoded avec Entries"),
            };
            assert_eq!(entries, key_values(body_form.iter()), "{name} body_form");
        }
        if let Some(body_multipart) = bruno.get("body_multipart").and_then(Value::as_array) {
            let entries = match &view.body {
                Some(BodyView {
                    kind: BodyKind::MultipartForm,
                    content: BodyContent::Entries(entries),
                }) => entries.as_slice(),
                _ => panic!("{name} : attendu MultipartForm avec Entries"),
            };
            assert_eq!(entries.len(), body_multipart.len(), "{name} multipart len");
            for (kv, expected) in entries.iter().zip(body_multipart.iter()) {
                assert_eq!(
                    kv.key,
                    expected["name"].as_str().unwrap(),
                    "{name} multipart key"
                );
                assert_eq!(
                    kv.enabled,
                    expected["enabled"].as_bool().unwrap(),
                    "{name} multipart enabled"
                );
                let kind = expected["type"].as_str().unwrap();
                if kind == "text" {
                    assert_eq!(
                        kv.value,
                        expected["value"].as_str().unwrap(),
                        "{name} multipart text value"
                    );
                } else if kind == "file" {
                    let file_val = expected["value"][0].as_str().unwrap();
                    assert!(
                        kv.value.contains(&format!("@file({file_val})")),
                        "{name} multipart file reference"
                    );
                    let ct = expected["contentType"].as_str().unwrap();
                    if !ct.is_empty() {
                        assert!(
                            kv.value.contains(&format!("@contentType({ct})")),
                            "{name} multipart content type"
                        );
                    }
                }
            }
        }
        if let Some(graphql_vars) = bruno.get("body_graphql_vars").and_then(Value::as_str) {
            assert_eq!(
                view.graphql_vars().unwrap_or(""),
                graphql_vars,
                "{name} graphql_vars"
            );
        }

        if let Some(vars_req) = bruno.get("vars_req").and_then(Value::as_array) {
            assert_eq!(
                view.extra.vars_pre_request,
                key_values(vars_req.iter()),
                "{name} vars_req"
            );
        }
        if let Some(vars_res) = bruno.get("vars_res").and_then(Value::as_array) {
            assert_eq!(
                view.extra.vars_post_response,
                key_values(vars_res.iter()),
                "{name} vars_res"
            );
        }
        if let Some(assertions) = bruno.get("assertions").and_then(Value::as_array) {
            assert_eq!(
                view.extra.assertions,
                key_values(assertions.iter()),
                "{name} assertions"
            );
        }
        if let Some(settings) = bruno.get("settings").and_then(Value::as_object) {
            for (key, val) in settings {
                if let Some(kv) = view.extra.settings.iter().find(|s| &s.key == key) {
                    let val_str = match val {
                        Value::Bool(b) => b.to_string(),
                        Value::Number(n) => n.to_string(),
                        Value::String(s) => s.clone(),
                        _ => val.to_string(),
                    };
                    assert_eq!(kv.value, val_str, "{name} setting {key}");
                }
            }
        }
        if let Some(auth_mode) = bruno.get("auth_mode").and_then(Value::as_str) {
            assert_eq!(
                view.auth.as_ref().map(|m| m.as_str()),
                Some(auth_mode),
                "{name} auth_mode"
            );
        }
        if let Some(auth_params) = bruno.get("auth_params").and_then(Value::as_array) {
            assert_eq!(
                view.extra.auth_params,
                key_values(auth_params.iter()),
                "{name} auth_params"
            );
        }
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

#[test]
#[ignore = "nécessite node et @usebruno/cli (bru) installés"]
fn auth_mode_switch_matches_json_to_bru() {
    let script_eval = r#"
const fs = require('fs');
const { execSync } = require('child_process');
const path = require('path');
function langDir() {
  if (process.env.BRUNO_LANG_DIR) return process.env.BRUNO_LANG_DIR;
  const globalRoot = execSync('npm root -g', { encoding: 'utf8' }).trim();
  return path.join(globalRoot, '@usebruno', 'cli', 'node_modules', '@usebruno', 'lang');
}
const { bruToJsonV2, jsonToBruV2 } = require(langDir());
const content = fs.readFileSync(process.argv[2], 'utf8');
const json = bruToJsonV2(content);
const regenerated = jsonToBruV2(json);
process.stdout.write(regenerated);
"#;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp-bruno-lang-auth-test");
    fs::create_dir_all(&dir).expect("dossier créé");
    let script_js = dir.join("json_to_bru.js");
    fs::write(&script_js, script_eval).expect("script écrit");

    let produced_path = cases().join("auth-mode-switch.after.bru");
    let produced_content = fs::read_to_string(&produced_path).expect("fixture after");

    let output = Command::new("node")
        .arg(&script_js)
        .arg(&produced_path)
        .output()
        .expect("node exécuté");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bruno_generated = String::from_utf8(output.stdout).expect("utf8");

    // Le bloc auth:bearer produit par bru-writer a les mêmes clés, dans le même ordre, à la même place
    assert_eq!(
        produced_content, bruno_generated,
        "Le fichier produit par bru-writer pour auth-mode-switch doit correspondre à celui écrit par jsonToBru"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// Références des opérations de fichiers (`docs/edition-phase3.md`) :
/// `scripts/bruno-new-items.js` les régénère avec le sérialiseur officiel ;
/// elles doivent être identiques aux fichiers versionnés.
#[test]
#[ignore = "nécessite node et @usebruno/cli (bru) installés"]
fn file_ops_references_are_still_produced_by_bruno() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp-bruno-new-items");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).expect("dossier créé");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/bruno-new-items.js");
    let output = Command::new("node")
        .arg(&script)
        .arg(&out)
        .output()
        .expect("node lancé");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut count = 0;
    for entry in fs::read_dir(&out).expect("sortie") {
        let entry = entry.expect("entrée");
        let fresh = fs::read_to_string(entry.path()).expect("régénéré");
        let versioned = fs::read_to_string(cases().join("files").join(entry.file_name()))
            .expect("référence versionnée");
        assert_eq!(fresh, versioned, "{:?}", entry.file_name());
        count += 1;
    }
    assert_eq!(count, 10);

    // Table des paramètres d'URL, recalculée par le code de Bruno.
    let table = out.join("bruno-url-params.json");
    let output = Command::new("node")
        .arg(&script)
        .arg("--url-params")
        .arg(&table)
        .output()
        .expect("node lancé");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let versioned = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/url-params/bruno-url-params.json");
    assert_eq!(
        fs::read_to_string(&table).expect("table régénérée"),
        fs::read_to_string(versioned).expect("table versionnée")
    );
    let _ = fs::remove_dir_all(&out);
}

/// Les fichiers créés, renommés, clonés ou réordonnés par le TUI se relisent
/// avec le parser officiel, avec le nom, le `seq` et l'URL attendus.
#[test]
#[ignore = "nécessite node et @usebruno/cli (bru) installés"]
fn bruno_reads_files_written_by_file_operations() {
    use bruno_tui::writer::files::{self, NewRequest, RequestKind, SeqChange};

    let script_eval = r#"
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');
function langDir() {
  if (process.env.BRUNO_LANG_DIR) return process.env.BRUNO_LANG_DIR;
  const globalRoot = execSync('npm root -g', { encoding: 'utf8' }).trim();
  return path.join(globalRoot, '@usebruno', 'cli', 'node_modules', '@usebruno', 'lang');
}
const { bruToJsonV2, collectionBruToJson } = require(langDir());
const file = process.argv[2];
const content = fs.readFileSync(file, 'utf8');
const json = path.basename(file) === 'folder.bru' ? collectionBruToJson(content) : bruToJsonV2(content);
process.stdout.write(JSON.stringify({
  name: json.meta && json.meta.name,
  seq: json.meta && json.meta.seq,
  type: json.meta && json.meta.type,
  url: json.http ? json.http.url : null,
  params: (json.params || []).map((p) => [p.type, p.name, p.value, p.enabled]),
  method: json.http ? json.http.method : null,
  auth: json.auth && json.auth.mode
}));
"#;
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp-bruno-file-ops");
    let _ = fs::remove_dir_all(&base);
    let root = base.join("collection");
    copy_tree(&cases().join("files/collection"), &root);
    let script_js = base.join("read.js");
    fs::write(&script_js, script_eval).expect("script écrit");

    let created = files::create_request(
        &root,
        Path::new("Posts"),
        &NewRequest {
            name: "Lire: un article".into(),
            filename: "Lire un article".into(),
            kind: RequestKind::Graphql,
            method: "post".into(),
            url: "{{base_url}}/graphql".into(),
            seq: 2,
        },
    )
    .expect("création");
    let folder =
        files::create_folder(&root, Path::new(""), "Nouveau", "Nouveau", 5).expect("dossier");
    let with_params = files::create_request(
        &root,
        Path::new(""),
        &NewRequest {
            name: "Paramètres".into(),
            filename: "params".into(),
            kind: RequestKind::Http,
            method: "get".into(),
            url: "{{base_url}}/odata/Items(id=:key)/:id?full=1&flag&a=1=2".into(),
            seq: 9,
        },
    )
    .expect("création avec paramètres");
    let get = fs::read_to_string(root.join("get-post.bru")).expect("source");
    let cloned = files::clone_request(&root, Path::new("get-post.bru"), &get, "Copie", "Copie", 7)
        .expect("clone");
    let renamed = files::rename_request(
        &root,
        Path::new("get-post.bru"),
        &get,
        "Renommée",
        Some("renommee"),
    )
    .expect("renommage");
    files::resequence(
        &root,
        &[SeqChange {
            path: "NoMeta".into(),
            is_folder: true,
            seq: 4,
            expected: None,
        }],
    )
    .expect("seq");

    let read = |relative: &Path| -> Value {
        let output = Command::new("node")
            .arg(&script_js)
            .arg(root.join(relative))
            .output()
            .expect("node lancé");
        assert!(
            output.status.success(),
            "{} : {}",
            relative.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON valide")
    };
    // `meta.seq` est relu en texte par `bruToJsonV2`, en nombre ailleurs.
    let seq = |value: &Value| match &value["seq"] {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let value = read(&created);
    assert_eq!(value["name"], "Lire: un article");
    assert_eq!(seq(&value), "2");
    assert_eq!(value["type"], "graphql");
    assert_eq!(value["url"], "{{base_url}}/graphql");
    assert_eq!(value["method"], "post");
    let value = read(&with_params);
    assert_eq!(
        value["params"],
        serde_json::json!([
            ["query", "full", "1", true],
            ["query", "flag", "", true],
            ["query", "a", "1=2", true],
            ["path", "key", "", true],
            ["path", "id", "", true]
        ])
    );
    let value = read(&folder.join("folder.bru"));
    assert_eq!(value["name"], "Nouveau");
    assert_eq!(seq(&value), "5");
    assert_eq!(value["auth"], "inherit");
    let value = read(&cloned);
    assert_eq!(
        (value["name"].as_str(), seq(&value)),
        (Some("Copie"), "7".to_owned())
    );
    let value = read(&renamed);
    assert_eq!(
        (value["name"].as_str(), seq(&value)),
        (Some("Renommée"), "1".to_owned())
    );
    let value = read(Path::new("NoMeta/folder.bru"));
    assert_eq!(
        (value["name"].as_str(), seq(&value)),
        (Some("NoMeta"), "4".to_owned())
    );

    let _ = fs::remove_dir_all(&base);
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("copie");
    for entry in fs::read_dir(from).expect("lecture") {
        let entry = entry.expect("entrée");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copie fichier");
        }
    }
}
