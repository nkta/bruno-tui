//! Écriture de modifications de champ sur les fixtures écrites à la main de
//! `tests/fixtures/collections/writer-cases/`, comparées à l'octet près à
//! des fichiers `*.after.bru` versionnés à côté de chaque fixture.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bruno_tui::collection::{
    BruFile, BruLoader, CollectionLoader, FileMeta, RequestView, TreeNode,
};
use bruno_tui::writer::{
    BruWriter, EditError, EntryProblem, EntrySection, EnvironmentVarEdit, FieldEdit, FileStamp,
    RequestWriter, WriteError, preview, write_environment,
};

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases")
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bruno-tui-writer-fixtures-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("répertoire de test");
    dir
}

fn copy_case(name: &str, dir: &Path) -> PathBuf {
    let target = dir.join(name);
    fs::copy(cases().join(name), &target).expect("copie de fixture");
    target
}

fn load(path: &Path) -> (BruFile, FileStamp) {
    let source = fs::read_to_string(path).expect("lecture");
    let ast = BruFile::parse(source).expect("AST");
    let stamp = FileStamp::capture(path).expect("instantané");
    (ast, stamp)
}

fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("répertoire")
        .map(|entry| {
            entry
                .expect("entrée")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

// --- Fidélité ------------------------------------------------------------

#[test]
fn simple_url_edit_matches_after_fixture() {
    let dir = workdir("simple");
    let path = copy_case("simple.bru", &dir);
    let (ast, stamp) = load(&path);

    let new_stamp = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::Url("https://{{host}}/pong".into())],
        )
        .expect("écriture");

    let expected = fs::read(cases().join("simple.after.bru")).expect("fixture after");
    assert_eq!(fs::read(&path).expect("relecture"), expected);
    assert_eq!(new_stamp, FileStamp::capture(&path).expect("instantané"));
    assert_eq!(dir_entries(&dir), ["simple.bru"]);

    // Round-trip : l'URL modifiée se relit à l'identique.
    let reloaded = BruFile::parse(fs::read_to_string(&path).expect("relecture")).expect("AST");
    let view = RequestView::from_ast(&reloaded).expect("vue");
    assert_eq!(view.url, "https://{{host}}/pong");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn method_edit_matches_after_fixture() {
    let dir = workdir("method");
    let path = copy_case("method.bru", &dir);
    let (ast, stamp) = load(&path);

    let new_stamp = BruWriter
        .write_request(&path, &ast, &stamp, &[FieldEdit::Method("post".into())])
        .expect("écriture");

    let expected = fs::read(cases().join("method.after.bru")).expect("fixture after");
    assert_eq!(fs::read(&path).expect("relecture"), expected);
    assert_eq!(new_stamp, FileStamp::capture(&path).expect("instantané"));
    assert_eq!(dir_entries(&dir), ["method.bru"]);

    // Round-trip : la méthode modifiée se relit à l'identique, le reste du
    // bloc (URL, body, auth) est inchangé.
    let reloaded = BruFile::parse(fs::read_to_string(&path).expect("relecture")).expect("AST");
    let view = RequestView::from_ast(&reloaded).expect("vue");
    assert_eq!(view.method, "POST");
    assert_eq!(view.url, "https://{{host}}/items");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unknown_method_is_refused_without_writing() {
    let dir = workdir("method-unknown");
    let path = copy_case("method.bru", &dir);
    let original = fs::read(&path).expect("lecture");
    let (ast, stamp) = load(&path);

    let error = BruWriter
        .write_request(&path, &ast, &stamp, &[FieldEdit::Method("fetch".into())])
        .expect_err("refus attendu");
    assert!(matches!(error, WriteError::Edit(EditError::UnknownMethod)));
    assert!(!error.to_string().contains("fetch"));
    assert_eq!(fs::read(&path).expect("relecture"), original);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unchanged_method_does_not_rewrite_the_block_keyword() {
    let dir = workdir("method-unchanged");
    let path = copy_case("method.bru", &dir);
    let original = fs::read(&path).expect("lecture");
    let (ast, stamp) = load(&path);

    BruWriter
        .write_request(&path, &ast, &stamp, &[FieldEdit::Method("get".into())])
        .expect("écriture");
    assert_eq!(fs::read(&path).expect("relecture"), original);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn headers_edit_preserves_untouched_headers_byte_for_byte() {
    let dir = workdir("headers");
    let path = copy_case("headers.bru", &dir);
    let original = fs::read_to_string(&path).expect("lecture");
    let (ast, stamp) = load(&path);

    BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::HeaderValue {
                index: 1,
                value: "xyz789".into(),
            }],
        )
        .expect("écriture");

    let expected = fs::read(cases().join("headers.after.bru")).expect("fixture after");
    let rewritten = fs::read(&path).expect("relecture");
    assert_eq!(rewritten, expected);
    assert_eq!(dir_entries(&dir), ["headers.bru"]);

    let rewritten_text = String::from_utf8_lossy(&rewritten);
    for line in original
        .split_inclusive('\n')
        .filter(|line| line.contains("Accept:") || line.contains("X-Debug:"))
    {
        assert!(
            rewritten_text.contains(line),
            "ligne non préservée à l'octet près : {line:?}"
        );
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn json_body_edit_preserves_unknown_block() {
    let dir = workdir("json-body");
    let path = copy_case("json-body.bru", &dir);
    let (ast, stamp) = load(&path);

    BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::BodyText(
                "{\n  \"a\": 2,\n  \"b\": true\n}".into(),
            )],
        )
        .expect("écriture");

    let expected = fs::read(cases().join("json-body.after.bru")).expect("fixture after");
    let rewritten = fs::read(&path).expect("relecture");
    assert_eq!(rewritten, expected);
    assert_eq!(dir_entries(&dir), ["json-body.bru"]);
    assert!(
        String::from_utf8_lossy(&rewritten).contains("future:thing {\n  mode: experimental\n}")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn crlf_edit_preserves_line_endings() {
    let dir = workdir("crlf");
    let path = copy_case("crlf.bru", &dir);
    let (ast, stamp) = load(&path);

    BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::HeaderValue {
                index: 1,
                value: "xyz789".into(),
            }],
        )
        .expect("écriture");

    let expected = fs::read(cases().join("crlf.after.bru")).expect("fixture after");
    let rewritten = fs::read(&path).expect("relecture");
    assert_eq!(rewritten, expected);
    assert_eq!(dir_entries(&dir), ["crlf.bru"]);
    let lf_count = rewritten.iter().filter(|&&b| b == b'\n').count();
    let crlf_count = rewritten.windows(2).filter(|w| *w == b"\r\n").count();
    assert_eq!(lf_count, crlf_count, "chaque LF doit être précédé d'un CR");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn multiline_target_edit_reloads_with_expected_value() {
    let dir = workdir("multiline");
    let path = copy_case("multiline-target.bru", &dir);
    let (ast, stamp) = load(&path);

    let value = "line one\nline two";
    BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::HeaderValue {
                index: 0,
                value: value.into(),
            }],
        )
        .expect("écriture");

    let expected = fs::read(cases().join("multiline-target.after.bru")).expect("fixture after");
    assert_eq!(fs::read(&path).expect("relecture"), expected);
    assert_eq!(dir_entries(&dir), ["multiline-target.bru"]);

    let reloaded = BruFile::parse(fs::read_to_string(&path).expect("relecture")).expect("AST");
    let view = RequestView::from_ast(&reloaded).expect("vue");
    assert_eq!(view.headers[0].value, value);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn no_edits_leaves_file_byte_identical() {
    let dir = workdir("noop");
    let path = copy_case("headers.bru", &dir);
    let original = fs::read(&path).expect("lecture");
    let (ast, stamp) = load(&path);

    BruWriter
        .write_request(&path, &ast, &stamp, &[])
        .expect("écriture");

    assert_eq!(fs::read(&path).expect("relecture"), original);

    let _ = fs::remove_dir_all(&dir);
}

// --- Refus sans écriture ---------------------------------------------------

#[test]
fn stale_file_is_refused_and_third_party_content_kept() {
    let dir = workdir("stale");
    let path = copy_case("simple.bru", &dir);
    let (ast, stamp) = load(&path);

    let third_party = "get {\n  url: https://third-party\n}\n";
    fs::write(&path, third_party).expect("modification tierce");

    let error = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::Url("https://{{host}}/pong".into())],
        )
        .expect_err("refus attendu");
    assert!(matches!(error, WriteError::Stale { .. }));
    assert_eq!(fs::read_to_string(&path).expect("relecture"), third_party);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn resolution_errors_leave_target_file_untouched() {
    struct Case {
        fixture: &'static str,
        edits: Vec<FieldEdit>,
        tag: &'static str,
    }
    let cases_to_try = [
        Case {
            fixture: "headers.bru",
            edits: vec![FieldEdit::HeaderValue {
                index: 5,
                value: "x".into(),
            }],
            tag: "index-out-of-range",
        },
        Case {
            fixture: "simple.bru",
            edits: vec![FieldEdit::HeaderValue {
                index: 0,
                value: "x".into(),
            }],
            tag: "no-such-block",
        },
        Case {
            fixture: "form-body.bru",
            edits: vec![FieldEdit::BodyText("x".into())],
            tag: "wrong-body-form",
        },
    ];

    for case in cases_to_try {
        let dir = workdir(&format!("resolve-err-{}", case.tag));
        let path = copy_case(case.fixture, &dir);
        let original = fs::read(&path).expect("lecture");
        let (ast, stamp) = load(&path);

        let error = BruWriter
            .write_request(&path, &ast, &stamp, &case.edits)
            .expect_err("erreur de résolution attendue");
        match case.tag {
            "index-out-of-range" => assert!(matches!(
                error,
                WriteError::Edit(EditError::IndexOutOfRange { .. })
            )),
            "no-such-block" => {
                assert!(matches!(
                    error,
                    WriteError::Edit(EditError::NoSuchBlock { .. })
                ))
            }
            "wrong-body-form" => assert!(matches!(
                error,
                WriteError::Edit(EditError::WrongBodyForm { .. })
            )),
            other => unreachable!("cas de test inconnu : {other}"),
        }
        assert_eq!(fs::read(&path).expect("relecture"), original);
        assert_eq!(dir_entries(&dir), [case.fixture]);

        let _ = fs::remove_dir_all(&dir);
    }
}

// --- Permissions et fichiers temporaires (Unix) ---------------------------

#[cfg(unix)]
mod unix_only {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn original_permissions_are_preserved() {
        let dir = workdir("perms");
        let path = copy_case("simple.bru", &dir);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("chmod");
        let (ast, stamp) = load(&path);

        BruWriter
            .write_request(
                &path,
                &ast,
                &stamp,
                &[FieldEdit::Url("https://{{host}}/pong".into())],
            )
            .expect("écriture");

        let mode = fs::metadata(&path)
            .expect("métadonnées")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o640);
        assert_eq!(dir_entries(&dir), ["simple.bru"]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_temp_file_survives_a_simulated_write_failure() {
        let dir = workdir("no-residue");
        let path = copy_case("simple.bru", &dir);
        let original = fs::read(&path).expect("lecture");
        let (ast, stamp) = load(&path);

        // Répertoire lecture/exécution seules : la création du fichier
        // temporaire doit échouer avant tout renommage.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o500)).expect("chmod répertoire");
        let result = BruWriter.write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::Url("https://{{host}}/pong".into())],
        );
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755))
            .expect("restauration des permissions");

        assert!(matches!(result, Err(WriteError::Io { .. })));
        assert_eq!(dir_entries(&dir), ["simple.bru"]);
        assert_eq!(fs::read(&path).expect("relecture"), original);

        let _ = fs::remove_dir_all(&dir);
    }
}

// --- Abstraction RequestWriter ---------------------------------------------

/// Écrivain de substitution : enregistre les appels sans toucher au disque.
#[derive(Default)]
struct FakeWriter {
    calls: Mutex<Vec<(PathBuf, Vec<FieldEdit>)>>,
}

impl RequestWriter for FakeWriter {
    fn write_request(
        &self,
        path: &Path,
        _ast: &BruFile,
        stamp: &FileStamp,
        edits: &[FieldEdit],
    ) -> Result<FileStamp, WriteError> {
        self.calls
            .lock()
            .expect("verrou")
            .push((path.to_path_buf(), edits.to_vec()));
        Ok(*stamp)
    }
}

/// Code appelant générique, indépendant de l'implémentation.
fn apply_url_edit(
    writer: &dyn RequestWriter,
    path: &Path,
    ast: &BruFile,
    stamp: &FileStamp,
    url: &str,
) -> Result<FileStamp, WriteError> {
    writer.write_request(path, ast, stamp, &[FieldEdit::Url(url.into())])
}

#[test]
fn callers_are_independent_of_the_writer() {
    let dir = workdir("fake-vs-real");
    let path = copy_case("simple.bru", &dir);
    let original = fs::read(&path).expect("lecture");
    let (ast, stamp) = load(&path);

    let fake = FakeWriter::default();
    let returned = apply_url_edit(&fake, &path, &ast, &stamp, "https://{{host}}/pong")
        .expect("écriture simulée");
    assert_eq!(returned, stamp);
    assert_eq!(
        fs::read(&path).expect("relecture"),
        original,
        "FakeWriter ne doit pas toucher au disque"
    );
    assert_eq!(fake.calls.lock().expect("verrou").len(), 1);

    // Assure un saut d'horloge pour que l'instantané change même si la taille est identique.
    std::thread::sleep(std::time::Duration::from_millis(10));
    let real_stamp = apply_url_edit(&BruWriter, &path, &ast, &stamp, "https://{{host}}/pong")
        .expect("écriture réelle");
    assert_ne!(real_stamp, stamp);
    let expected = fs::read(cases().join("simple.after.bru")).expect("fixture after");
    assert_eq!(fs::read(&path).expect("relecture"), expected);

    let _ = fs::remove_dir_all(&dir);
}

// --- Ajout, suppression, renommage (add-entry-management) ----------------

fn add(section: EntrySection, key: &str, value: &str) -> FieldEdit {
    FieldEdit::AddEntry {
        section,
        key: key.into(),
        value: value.into(),
        enabled: true,
    }
}

fn remove(section: EntrySection, index: usize) -> FieldEdit {
    FieldEdit::RemoveEntry { section, index }
}

/// Modifications attendues pour chaque fixture d'ajout, de suppression ou
/// de renommage, dont le résultat est versionné dans `<nom>.after.bru`.
fn entry_cases() -> Vec<(&'static str, Vec<FieldEdit>)> {
    use EntrySection::{
        Assert, Headers, PathParams, QueryParams, VarsPostResponse, VarsPreRequest,
    };
    vec![
        (
            "add-header",
            vec![add(Headers, "X-Request-Id", "{{requestId}}")],
        ),
        ("add-block", vec![add(Headers, "Accept", "text/plain")]),
        ("add-path-between", vec![add(PathParams, "id", "42")]),
        (
            "remove-entry",
            vec![remove(Headers, 1), remove(PathParams, 0)],
        ),
        (
            "rename-disabled",
            vec![FieldEdit::RenameKey {
                section: Headers,
                index: 1,
                key: "X-Verbose".into(),
            }],
        ),
        (
            "duplicates",
            vec![
                add(Headers, "Accept", "application/json"),
                add(QueryParams, "tag", "b"),
            ],
        ),
        (
            "query-sync",
            vec![
                add(QueryParams, "sort", "asc"),
                FieldEdit::QueryParamEnabled {
                    index: 1,
                    enabled: false,
                },
                remove(QueryParams, 0),
            ],
        ),
        (
            "url-sync",
            vec![FieldEdit::Url(
                "https://{{host}}/items?page=3&limit=5&flag".into(),
            )],
        ),
        (
            "crlf-add",
            vec![
                add(Headers, "C", "3"),
                remove(Headers, 0),
                add(PathParams, "id", "1"),
            ],
        ),
        (
            "vars-ops",
            vec![
                FieldEdit::VarsPreRequestValue {
                    index: 0,
                    value: "updated_token".into(),
                },
                remove(VarsPreRequest, 1),
                add(VarsPostResponse, "res_id", "res.body.id"),
            ],
        ),
        (
            "assert-ops",
            vec![
                FieldEdit::AssertValue {
                    index: 0,
                    value: "eq 201".into(),
                },
                FieldEdit::AssertEnabled {
                    index: 1,
                    enabled: false,
                },
                add(Assert, "res.headers['content-type']", "matches json"),
            ],
        ),
        (
            "settings-ops",
            vec![
                FieldEdit::SettingsValue {
                    index: 0,
                    value: "false".into(),
                },
                FieldEdit::SettingsValue {
                    index: 1,
                    value: "5000".into(),
                },
            ],
        ),
        (
            "folder-ops",
            vec![
                FieldEdit::HeaderValue {
                    index: 0,
                    value: "v2".into(),
                },
                add(VarsPreRequest, "folder_var", "hello"),
            ],
        ),
        (
            "collection-ops",
            vec![
                FieldEdit::HeaderValue {
                    index: 0,
                    value: "Bearer changed".into(),
                },
                add(VarsPreRequest, "coll_var", "world"),
            ],
        ),
        ("body-mode-ops", vec![FieldEdit::BodyMode("json".into())]),
        (
            "body-form-urlencoded-ops",
            vec![
                FieldEdit::FormUrlEncodedValue {
                    index: 0,
                    value: "bob".into(),
                },
                FieldEdit::RenameKey {
                    section: EntrySection::FormUrlEncoded,
                    index: 1,
                    key: "auth_token".into(),
                },
                FieldEdit::FormUrlEncodedEnabled {
                    index: 3,
                    enabled: true,
                },
                FieldEdit::RemoveEntry {
                    section: EntrySection::FormUrlEncoded,
                    index: 2,
                },
                add(EntrySection::FormUrlEncoded, "grant_type", "password"),
            ],
        ),
        (
            "body-multipart-form-ops",
            vec![
                FieldEdit::MultipartFormValue {
                    index: 0,
                    value: "updated note".into(),
                },
                FieldEdit::MultipartFormEnabled {
                    index: 1,
                    enabled: false,
                },
            ],
        ),
        (
            "body-graphql-vars-ops",
            vec![FieldEdit::GraphqlVars(
                "{\n  \"limit\": 10,\n  \"offset\": 0\n}".into(),
            )],
        ),
    ]
}

#[test]
fn body_graphql_vars_ops_created_then_modified() {
    let dir = workdir("entry-body-graphql-vars-ops-flow");
    let file_name = "body-graphql-vars-ops.bru";
    let path = copy_case(file_name, &dir);
    let (ast, stamp) = load(&path);

    // 1. Création du bloc
    let intermediate_vars = "{\n  \"initial\": true\n}";
    let stamp2 = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::GraphqlVars(intermediate_vars.into())],
        )
        .expect("création");

    // 2. Modification du bloc vers l'état attendu
    let (ast2, _) = load(&path);
    let final_vars = "{\n  \"limit\": 10,\n  \"offset\": 0\n}";
    BruWriter
        .write_request(
            &path,
            &ast2,
            &stamp2,
            &[FieldEdit::GraphqlVars(final_vars.into())],
        )
        .expect("modification");

    let expected = fs::read(cases().join("body-graphql-vars-ops.after.bru")).expect("after");
    let written = fs::read(&path).expect("relecture");
    assert_eq!(
        String::from_utf8_lossy(&written),
        String::from_utf8_lossy(&expected)
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn entry_cases_match_after_fixtures_and_preview() {
    for (name, edits) in entry_cases() {
        let dir = workdir(&format!("entry-{name}"));
        let file_name = format!("{name}.bru");
        let path = copy_case(&file_name, &dir);
        let (ast, stamp) = load(&path);

        let previewed = preview(&ast, &edits).unwrap_or_else(|e| panic!("{name} : {e}"));
        BruWriter
            .write_request(&path, &ast, &stamp, &edits)
            .unwrap_or_else(|e| panic!("{name} : {e}"));

        let expected = fs::read(cases().join(format!("{name}.after.bru"))).expect("fixture after");
        let written = fs::read(&path).expect("relecture");
        assert_eq!(
            String::from_utf8_lossy(&written),
            String::from_utf8_lossy(&expected),
            "{name}"
        );
        assert_eq!(
            dir_entries(&dir),
            std::slice::from_ref(&file_name),
            "{name}"
        );

        let reloaded = BruFile::parse(String::from_utf8(written).expect("utf-8")).expect("AST");
        let reloaded_view = RequestView::from_ast(&reloaded)
            .unwrap_or_else(|_| FileMeta::from_ast(reloaded.clone()).to_request_view());
        assert_eq!(previewed, reloaded_view, "{name}");

        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn writer_cases_load_without_errors() {
    fn errors(nodes: &[TreeNode]) -> Vec<String> {
        nodes
            .iter()
            .flat_map(|node| match node {
                TreeNode::Error(error) => vec![error.path.display().to_string()],
                TreeNode::Folder(folder) => errors(&folder.children),
                TreeNode::Request(_) => Vec::new(),
            })
            .collect()
    }
    let collection = BruLoader.load(&cases()).expect("collection chargée");
    assert!(
        errors(&collection.tree).is_empty(),
        "{:?}",
        errors(&collection.tree)
    );
}

#[test]
fn add_is_refused_on_a_file_changed_since_the_stamp() {
    let dir = workdir("entry-stale");
    let path = copy_case("add-header.bru", &dir);
    let (ast, stamp) = load(&path);

    std::thread::sleep(std::time::Duration::from_millis(10));
    let third_party = "get {\n  url: http://third-party\n}\n";
    fs::write(&path, third_party).expect("tiers");

    let error = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[add(EntrySection::Headers, "X-Trace", "abc")],
        )
        .expect_err("refus attendu");
    assert!(matches!(error, WriteError::Stale { .. }));
    assert_eq!(fs::read_to_string(&path).expect("relecture"), third_party);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn one_refused_edit_cancels_the_whole_list() {
    let dir = workdir("entry-atomic");
    let path = copy_case("duplicates.bru", &dir);
    let original = fs::read(&path).expect("lecture");
    let (ast, stamp) = load(&path);

    let error = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[
                add(EntrySection::Headers, "X-Trace", "abc"),
                FieldEdit::RenameKey {
                    section: EntrySection::QueryParams,
                    index: 0,
                    key: "bad key".into(),
                },
            ],
        )
        .expect_err("refus attendu");
    assert!(matches!(
        error,
        WriteError::Edit(EditError::InvalidEntry {
            block: "params:query",
            index: Some(0),
            problem: EntryProblem::KeyWhitespace,
        })
    ));
    assert_eq!(fs::read(&path).expect("relecture"), original);
    assert_eq!(dir_entries(&dir), ["duplicates.bru"]);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn environment_vars_edit_matches_after_fixture() {
    let dir = workdir("env-vars");
    let path = copy_case("environment-vars.bru", &dir);
    let (ast, stamp) = load(&path);

    let new_stamp = write_environment(
        &path,
        &ast,
        &stamp,
        &[EnvironmentVarEdit::Value {
            index: 0,
            value: "staging.example.com:443".into(),
        }],
    )
    .expect("écriture");

    let expected = fs::read(cases().join("environment-vars.after.bru")).expect("fixture after");
    let rewritten = fs::read(&path).expect("relecture");
    assert_eq!(rewritten, expected);
    assert_eq!(new_stamp, FileStamp::capture(&path).expect("instantané"));
    assert_eq!(dir_entries(&dir), ["environment-vars.bru"]);

    // Vérifier que la variable désactivée est préservée à l'octet près
    let rewritten_text = String::from_utf8_lossy(&rewritten);
    assert!(rewritten_text.contains("  ~debug: true\n"));
    assert!(rewritten_text.contains("vars:secret [\n  token\n]\n"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn text_blocks_edit_creation_and_removal() {
    let dir = workdir("text-blocks");
    let path = dir.join("test-req.bru");
    let initial = "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://example.com\n}\n\nscript:pre-request {\n  console.log(\"old\");\n}\n";
    fs::write(&path, initial).expect("écriture initiale");
    let (ast, stamp) = load(&path);

    // 1. Modification de script:pre-request, création de tests et de docs
    let stamp2 = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[
                FieldEdit::ScriptPreRequest("console.log(\"new\");".into()),
                FieldEdit::Tests("test(\"ok\", () => {});".into()),
                FieldEdit::Docs("# Title\nDocumentation".into()),
            ],
        )
        .expect("écriture");

    let rewritten = fs::read_to_string(&path).expect("relecture");
    assert_eq!(
        rewritten,
        "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://example.com\n}\n\nscript:pre-request {\n  console.log(\"new\");\n}\n\ntests {\n  test(\"ok\", () => {});\n}\n\ndocs {\n  # Title\n  Documentation\n}\n"
    );

    // 2. Retrait de script:pre-request et de tests
    let (ast2, _) = load(&path);
    BruWriter
        .write_request(
            &path,
            &ast2,
            &stamp2,
            &[
                FieldEdit::ScriptPreRequest("".into()),
                FieldEdit::Tests("   \n".into()),
            ],
        )
        .expect("suppression");

    let rewritten2 = fs::read_to_string(&path).expect("relecture 2");
    assert_eq!(
        rewritten2,
        "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://example.com\n}\n\ndocs {\n  # Title\n  Documentation\n}\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn folder_bru_text_blocks_and_headers_edit() {
    let dir = workdir("folder-bru-edit");
    let path = dir.join("folder.bru");
    let initial = "meta {\n  name: Mon Dossier\n}\n";
    fs::write(&path, initial).expect("écriture initiale");
    let (ast, stamp) = load(&path);

    BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[
                add(EntrySection::Headers, "Authorization", "Bearer tok"),
                FieldEdit::Docs("# Docs du dossier\nTexte".into()),
            ],
        )
        .expect("écriture folder");

    let rewritten = fs::read_to_string(&path).expect("relecture");
    assert_eq!(
        rewritten,
        "meta {\n  name: Mon Dossier\n}\n\nheaders {\n  Authorization: Bearer tok\n}\n\ndocs {\n  # Docs du dossier\n  Texte\n}\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn body_mode_edit_and_body_text_block_creation_and_removal() {
    let dir = workdir("body-mode-edit");
    let path = dir.join("req.bru");
    let initial = "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://example.com\n  body: none\n  auth: none\n}\n";
    fs::write(&path, initial).expect("écriture initiale");
    let (ast, stamp) = load(&path);

    // 1. Passage en mode json et création du bloc body:json
    let stamp2 = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[
                FieldEdit::BodyMode("json".into()),
                FieldEdit::BodyText("{\n  \"hello\": \"world\"\n}".into()),
            ],
        )
        .expect("écriture json");

    let rewritten = fs::read_to_string(&path).expect("relecture");
    assert_eq!(
        rewritten,
        "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://example.com\n  body: json\n  auth: none\n}\n\nbody:json {\n  {\n    \"hello\": \"world\"\n  }\n}\n"
    );

    // 2. Vidage du corps : le bloc body:json est supprimé mais body: json reste
    let (ast2, _) = load(&path);
    BruWriter
        .write_request(&path, &ast2, &stamp2, &[FieldEdit::BodyText("".into())])
        .expect("vidage corps");

    let rewritten2 = fs::read_to_string(&path).expect("relecture 2");
    assert_eq!(
        rewritten2,
        "meta {\n  name: test\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://example.com\n  body: json\n  auth: none\n}\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn body_form_urlencoded_and_multipart_form_round_trip() {
    let dir = workdir("forms-round-trip");
    let path = dir.join("form.bru");
    let initial = "meta {\n  name: form\n  type: http\n  seq: 1\n}\n\npost {\n  url: https://example.com\n  body: formUrlEncoded\n  auth: none\n}\n\nbody:form-urlencoded {\n  user: alice\n}\n";
    fs::write(&path, initial).expect("écriture initiale");
    let (ast, stamp) = load(&path);

    // 1. Ajout et bascule dans form-urlencoded
    let stamp2 = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[
                add(EntrySection::FormUrlEncoded, "role", "admin"),
                FieldEdit::FormUrlEncodedEnabled {
                    index: 0,
                    enabled: false,
                },
            ],
        )
        .expect("écriture form");

    let rewritten = fs::read_to_string(&path).expect("relecture");
    assert_eq!(
        rewritten,
        "meta {\n  name: form\n  type: http\n  seq: 1\n}\n\npost {\n  url: https://example.com\n  body: formUrlEncoded\n  auth: none\n}\n\nbody:form-urlencoded {\n  ~user: alice\n  role: admin\n}\n"
    );

    // 2. Passage en multipartForm avec annotations et @file
    let (ast2, _) = load(&path);
    BruWriter
        .write_request(
            &path,
            &ast2,
            &stamp2,
            &[
                FieldEdit::BodyMode("multipartForm".into()),
                add(
                    EntrySection::MultipartForm,
                    "avatar",
                    "@file(photo.jpg) @contentType(image/jpeg)",
                ),
                add(EntrySection::MultipartForm, "@customKey", "value"),
            ],
        )
        .expect("écriture multipart");

    let rewritten2 = fs::read_to_string(&path).expect("relecture 2");
    assert_eq!(
        rewritten2,
        "meta {\n  name: form\n  type: http\n  seq: 1\n}\n\npost {\n  url: https://example.com\n  body: multipartForm\n  auth: none\n}\n\nbody:form-urlencoded {\n  ~user: alice\n  role: admin\n}\n\nbody:multipart-form {\n  avatar: @file(photo.jpg) @contentType(image/jpeg)\n  @customKey: value\n}\n"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn body_graphql_vars_creation_edit_and_removal() {
    let dir = workdir("gql-vars");
    let path = dir.join("gql.bru");
    let initial = "meta {\n  name: gql\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query { ping }\n}\n";
    fs::write(&path, initial).expect("écriture initiale");
    let (ast, stamp) = load(&path);

    // 1. Création du bloc body:graphql:vars
    let stamp2 = BruWriter
        .write_request(
            &path,
            &ast,
            &stamp,
            &[FieldEdit::GraphqlVars("{\n  \"limit\": 5\n}".into())],
        )
        .expect("création gql vars");

    let rewritten = fs::read_to_string(&path).expect("relecture");
    assert_eq!(
        rewritten,
        "meta {\n  name: gql\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query { ping }\n}\n\nbody:graphql:vars {\n  {\n    \"limit\": 5\n  }\n}\n"
    );

    // 2. Vidage : suppression du bloc body:graphql:vars
    let (ast2, _) = load(&path);
    BruWriter
        .write_request(&path, &ast2, &stamp2, &[FieldEdit::GraphqlVars("".into())])
        .expect("vidage gql vars");

    let rewritten2 = fs::read_to_string(&path).expect("relecture 2");
    assert_eq!(
        rewritten2,
        "meta {\n  name: gql\n  type: graphql\n  seq: 1\n}\n\npost {\n  url: https://example.com/graphql\n  body: graphql\n  auth: none\n}\n\nbody:graphql {\n  query { ping }\n}\n"
    );

    let _ = fs::remove_dir_all(&dir);
}
