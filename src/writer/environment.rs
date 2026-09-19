//! Écriture de modifications de variables dans le bloc `vars` d'un fichier
//! `.bru` d'environnement.
//!
//! Ne réécrit que les tranches de source correspondant aux variables
//! touchées (valeur modifiée, ajoutée ou supprimée) ; le reste du fichier
//! (autres variables, variables secrètes, commentaires, fins de ligne) est
//! réémis à l'octet près. Diffusion d'un ajout ou d'une suppression
//! déléguée à `dictionary::dictionary_replacements`, partagée avec le
//! writer de requête (`add-environment-entry-management`, design D1).

use std::path::Path;

use super::dictionary::{
    DictEntry, DictSection, block_node, check_key_general, dictionary_replacements,
};
use super::edit::Replacement;
use super::error::{EditError, WriteError};
use super::format::detect_eol;
use super::{FileStamp, write_atomic};
use crate::collection::BruFile;

/// Position d'insertion du bloc `vars` s'il n'existe pas encore : tout
/// début de fichier, cohérent avec les fichiers d'environnement réels où
/// `vars` précède toujours `vars:secret` (design D2).
const VARS_CREATION_ANCHOR: usize = 0;

/// Modification des variables du bloc `vars` d'un environnement.
///
/// Les indices s'appliquent dans l'ordre de la liste : un indice désigne
/// la position dans les variables telle qu'elle résulte des modifications
/// précédentes de la même liste (même principe que `FieldEdit` pour une
/// requête).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentVarEdit {
    /// Nouvelle valeur d'une variable déjà présente.
    Value { index: usize, value: String },
    /// Ajout d'une variable en fin de bloc.
    Add {
        key: String,
        value: String,
        enabled: bool,
    },
    /// Suppression d'une variable.
    Remove { index: usize },
}

/// Résout les modifications de variables d'un environnement vers les
/// tranches de source à remplacer.
pub(crate) fn resolve(
    ast: &BruFile,
    edits: &[EnvironmentVarEdit],
) -> Result<Vec<Replacement>, EditError> {
    if edits.is_empty() {
        return Ok(Vec::new());
    }

    let block = block_node(ast, "vars");
    let mut section = DictSection {
        entries: block
            .as_ref()
            .map_or(&[][..], |(_, entries)| entries)
            .iter()
            .enumerate()
            .map(|(index, entry)| DictEntry::from_entry(index, entry))
            .collect(),
        block_present: block.is_some(),
        touched: false,
    };

    for edit in edits {
        match edit {
            EnvironmentVarEdit::Value { index, value } => {
                let entry = section
                    .entries
                    .get_mut(*index)
                    .ok_or(EditError::IndexOutOfRange {
                        block: "vars",
                        index: *index,
                    })?;
                entry.value = value.clone();
            }
            EnvironmentVarEdit::Add {
                key,
                value,
                enabled,
            } => {
                check_key_general(key, "vars", None)?;
                section.entries.push(DictEntry {
                    origin: None,
                    key: key.clone(),
                    value: value.clone(),
                    disabled: !enabled,
                });
                section.touched = true;
            }
            EnvironmentVarEdit::Remove { index } => {
                if *index >= section.entries.len() {
                    return Err(EditError::IndexOutOfRange {
                        block: "vars",
                        index: *index,
                    });
                }
                section.entries.remove(*index);
                section.touched = true;
            }
        }
    }

    let raw = ast.raw();
    let eol = detect_eol(raw);
    Ok(dictionary_replacements(
        raw,
        eol,
        "vars",
        block,
        &section,
        VARS_CREATION_ANCHOR,
        true,
    ))
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

        let edits = vec![EnvironmentVarEdit::Value {
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

        let edits = vec![EnvironmentVarEdit::Value {
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
            &[EnvironmentVarEdit::Value {
                index: 0,
                value: "x".into(),
            }],
        )
        .expect_err("indice hors limites (aucune entrée)");
        assert!(matches!(
            err,
            EditError::IndexOutOfRange {
                block: "vars",
                index: 0
            }
        ));

        let file = parse("vars {\n  host: localhost\n}\n");
        let err_oob = resolve(
            &file,
            &[EnvironmentVarEdit::Value {
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
    fn resolve_adds_a_variable_to_an_existing_block() {
        let file = parse("vars {\n  host: localhost\n}\n");
        let edits = vec![EnvironmentVarEdit::Add {
            key: "region".into(),
            value: "eu-west".into(),
            enabled: true,
        }];
        let serialized = serialize(&file, &edits).expect("sérialisation");
        assert_eq!(
            String::from_utf8(serialized).expect("utf-8"),
            "vars {\n  host: localhost\n  region: eu-west\n}\n"
        );
    }

    #[test]
    fn resolve_creates_the_vars_block_when_absent() {
        let file = parse("vars:secret [\n  token\n]\n");
        let edits = vec![EnvironmentVarEdit::Add {
            key: "host".into(),
            value: "localhost".into(),
            enabled: true,
        }];
        let serialized = serialize(&file, &edits).expect("sérialisation");
        assert_eq!(
            String::from_utf8(serialized).expect("utf-8"),
            "vars {\n  host: localhost\n}\n\nvars:secret [\n  token\n]\n"
        );
    }

    #[test]
    fn resolve_refuses_an_invalid_key_on_add() {
        let file = parse("vars {\n  host: localhost\n}\n");
        let err = resolve(
            &file,
            &[EnvironmentVarEdit::Add {
                key: "in valid".into(),
                value: "x".into(),
                enabled: true,
            }],
        )
        .expect_err("clé invalide");
        assert!(matches!(
            err,
            EditError::InvalidEntry {
                block: "vars",
                problem: crate::writer::EntryProblem::KeyWhitespace,
                ..
            }
        ));
    }

    #[test]
    fn resolve_removes_a_variable_among_others() {
        let file = parse("vars {\n  host: localhost\n  debug: true\n  region: eu\n}\n");
        let edits = vec![EnvironmentVarEdit::Remove { index: 1 }];
        let serialized = serialize(&file, &edits).expect("sérialisation");
        assert_eq!(
            String::from_utf8(serialized).expect("utf-8"),
            "vars {\n  host: localhost\n  region: eu\n}\n"
        );
    }

    #[test]
    fn resolve_removing_the_last_variable_drops_the_block() {
        let file = parse("vars {\n  host: localhost\n}\n\nvars:secret [\n  token\n]\n");
        let edits = vec![EnvironmentVarEdit::Remove { index: 0 }];
        let serialized = serialize(&file, &edits).expect("sérialisation");
        assert_eq!(
            String::from_utf8(serialized).expect("utf-8"),
            "vars:secret [\n  token\n]\n"
        );
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
            &[EnvironmentVarEdit::Value {
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
            &[EnvironmentVarEdit::Value {
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
