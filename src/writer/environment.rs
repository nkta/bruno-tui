//! Écriture de modifications de variables dans le bloc `vars` d'un fichier
//! `.bru` d'environnement.
//!
//! Ne réécrit que les tranches de source correspondant aux variables éditées ;
//! le reste du fichier (autres variables, variables secrètes, commentaires,
//! fins de ligne) est réémis à l'octet près.

use std::collections::HashMap;
use std::path::Path;

use super::edit::Replacement;
use super::error::{EditError, WriteError};
use super::format::{detect_eol, format_entry};
use super::{FileStamp, write_atomic};
use crate::collection::BruFile;

/// Modification de la valeur d'une variable du bloc `vars` d'un environnement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentVarEdit {
    pub index: usize,
    pub value: String,
}

/// Résout les modifications de variables d'un environnement vers les tranches
/// de source à remplacer.
///
/// Ne produit une tranche que si la valeur a effectivement changé.
pub(crate) fn resolve(
    ast: &BruFile,
    edits: &[EnvironmentVarEdit],
) -> Result<Vec<Replacement>, EditError> {
    if edits.is_empty() {
        return Ok(Vec::new());
    }

    let entries = ast
        .dictionary("vars")
        .ok_or(EditError::NoSuchBlock { block: "vars" })?;

    // Si plusieurs modifications ciblent le même indice, la dernière l'emporte.
    let mut latest_edits: HashMap<usize, &str> = HashMap::new();
    for edit in edits {
        if edit.index >= entries.len() {
            return Err(EditError::IndexOutOfRange {
                block: "vars",
                index: edit.index,
            });
        }
        latest_edits.insert(edit.index, &edit.value);
    }

    let eol = detect_eol(ast.raw());
    let mut replacements = Vec::new();

    // On parcourt les entrées dans l'ordre pour produire des remplacements triés.
    for (index, entry) in entries.iter().enumerate() {
        if let Some(&new_value) = latest_edits.get(&index)
            && entry.value != new_value
        {
            let bytes = format_entry(&entry.key, new_value, entry.disabled, eol);
            replacements.push(Replacement {
                span: entry.span.clone(),
                bytes,
            });
        }
    }

    Ok(replacements)
}

/// Sérialise l'AST avec les modifications appliquées.
///
/// Échoue si le fichier produit ne se relit pas (`EditError::Unreadable`).
pub(crate) fn serialize(ast: &BruFile, edits: &[EnvironmentVarEdit]) -> Result<Vec<u8>, EditError> {
    let replacements = resolve(ast, edits)?;
    let raw = ast.raw();
    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0;
    for replacement in &replacements {
        out.push_str(&raw[cursor..replacement.span.start]);
        out.push_str(&replacement.bytes);
        cursor = replacement.span.end;
    }
    out.push_str(&raw[cursor..]);

    let bytes = out.into_bytes();
    let source = String::from_utf8(bytes.clone()).map_err(|_| EditError::Unreadable)?;
    let _ = BruFile::parse(source).map_err(|_| EditError::Unreadable)?;
    Ok(bytes)
}

/// Écrit les modifications de variables dans le fichier d'environnement situé à `path`.
pub fn write_environment(
    path: &Path,
    ast: &BruFile,
    stamp: &FileStamp,
    edits: &[EnvironmentVarEdit],
) -> Result<FileStamp, WriteError> {
    let bytes = serialize(ast, edits)?;
    write_atomic(path, stamp, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn parse(source: &str) -> BruFile {
        BruFile::parse(source.to_owned()).expect("source valide")
    }

    fn workdir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bruno-tui-writer-env-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("répertoire de test");
        dir
    }

    #[test]
    fn resolve_produces_minimal_replacement_and_preserves_rest_of_file() {
        let source = "vars {\n  host: localhost\n  ~debug: true\n}\n\nvars:secret [\n  token\n]\n";
        let file = parse(source);

        let edits = vec![EnvironmentVarEdit {
            index: 0,
            value: "staging.example.com".into(),
        }];

        let replacements = resolve(&file, &edits).expect("résolution réussie");
        assert_eq!(replacements.len(), 1);
        assert_eq!(replacements[0].bytes, "  host: staging.example.com\n");

        let serialized = serialize(&file, &edits).expect("sérialisation");
        let expected =
            "vars {\n  host: staging.example.com\n  ~debug: true\n}\n\nvars:secret [\n  token\n]\n";
        assert_eq!(String::from_utf8(serialized).expect("utf-8"), expected);
    }

    #[test]
    fn resolve_with_unchanged_value_produces_no_replacements() {
        let source = "vars {\n  host: localhost\n}\n";
        let file = parse(source);

        let edits = vec![EnvironmentVarEdit {
            index: 0,
            value: "localhost".into(),
        }];

        let replacements = resolve(&file, &edits).expect("résolution");
        assert!(replacements.is_empty());

        let serialized = serialize(&file, &edits).expect("sérialisation");
        assert_eq!(String::from_utf8(serialized).expect("utf-8"), source);
    }

    #[test]
    fn resolve_errors_on_missing_block_or_out_of_bounds_index() {
        let file_no_vars = parse("vars:secret [\n  token\n]\n");
        let err = resolve(
            &file_no_vars,
            &[EnvironmentVarEdit {
                index: 0,
                value: "x".into(),
            }],
        )
        .expect_err("bloc manquant");
        assert!(matches!(err, EditError::NoSuchBlock { block: "vars" }));

        let file = parse("vars {\n  host: localhost\n}\n");
        let err_oob = resolve(
            &file,
            &[EnvironmentVarEdit {
                index: 2,
                value: "x".into(),
            }],
        )
        .expect_err("indice hors limites");
        assert!(matches!(
            err_oob,
            EditError::IndexOutOfRange {
                block: "vars",
                index: 2
            }
        ));
    }

    #[test]
    fn write_environment_updates_file_and_preserves_stamp() {
        let dir = workdir("write");
        let path = dir.join("env.bru");
        let initial = "vars {\n  host: localhost\n  ~debug: true\n}\n";
        fs::write(&path, initial).expect("écriture initiale");

        let file = BruFile::parse(initial.to_owned()).expect("AST");
        let stamp = FileStamp::capture(&path).expect("stamp");

        let new_stamp = write_environment(
            &path,
            &file,
            &stamp,
            &[EnvironmentVarEdit {
                index: 0,
                value: "staging.example.com".into(),
            }],
        )
        .expect("écriture");

        let rewritten = fs::read_to_string(&path).expect("relecture");
        assert_eq!(
            rewritten,
            "vars {\n  host: staging.example.com\n  ~debug: true\n}\n"
        );
        assert_eq!(new_stamp, FileStamp::capture(&path).expect("stamp frais"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_environment_refuses_stale_stamp() {
        let dir = workdir("stale");
        let path = dir.join("env.bru");
        let initial = "vars {\n  host: localhost\n}\n";
        fs::write(&path, initial).expect("écriture initiale");

        let file = BruFile::parse(initial.to_owned()).expect("AST");
        let stamp = FileStamp::capture(&path).expect("stamp");

        // Modification concurrente
        fs::write(&path, "vars {\n  host: changed-by-third-party\n}\n").expect("tiers");

        let err = write_environment(
            &path,
            &file,
            &stamp,
            &[EnvironmentVarEdit {
                index: 0,
                value: "new-val".into(),
            }],
        )
        .expect_err("refus attendu");

        assert!(matches!(err, WriteError::Stale { .. }));
        assert_eq!(
            fs::read_to_string(&path).expect("relecture"),
            "vars {\n  host: changed-by-third-party\n}\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
