//! Écriture de modifications de champ sur les fixtures écrites à la main de
//! `tests/fixtures/collections/writer-cases/`, comparées à l'octet près à
//! des fichiers `*.after.bru` versionnés à côté de chaque fixture.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bruno_tui::collection::{BruFile, RequestView};
use bruno_tui::writer::{BruWriter, EditError, FieldEdit, FileStamp, RequestWriter, WriteError};

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
            &[FieldEdit::QueryParamValue {
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
    assert_eq!(view.query_params[0].value, value);

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

    let real_stamp = apply_url_edit(&BruWriter, &path, &ast, &stamp, "https://{{host}}/pong")
        .expect("écriture réelle");
    assert_ne!(real_stamp, stamp);
    let expected = fs::read(cases().join("simple.after.bru")).expect("fixture after");
    assert_eq!(fs::read(&path).expect("relecture"), expected);

    let _ = fs::remove_dir_all(&dir);
}
