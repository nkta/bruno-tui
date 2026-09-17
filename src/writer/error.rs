//! Erreurs de résolution d'une édition et d'écriture d'un fichier `.bru`.
//!
//! Les messages ne citent jamais une valeur de champ : seulement des noms
//! de bloc, des indices et des chemins. Un en-tête ou un corps de requête
//! peut porter un secret.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Nature du refus d'une clé ou d'une valeur d'entrée. Ne porte jamais le
/// texte refusé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryProblem {
    /// Clé vide.
    EmptyKey,
    /// Clé contenant un espace, une tabulation ou un saut de ligne.
    KeyWhitespace,
    /// Clé contenant `:`.
    KeyColon,
    /// Clé commençant par `~` ou `"`.
    KeyLeadingMarker,
    /// Clé de paramètre de requête contenant `&`, `=` ou `#`.
    QueryKeyReserved,
    /// Valeur de paramètre de requête contenant `&`, `#` ou un saut de ligne.
    QueryValueReserved,
}

impl std::fmt::Display for EntryProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::EmptyKey => "clé vide",
            Self::KeyWhitespace => "clé contenant un espace ou un saut de ligne",
            Self::KeyColon => "clé contenant `:`",
            Self::KeyLeadingMarker => "clé commençant par `~` ou `\"`",
            Self::QueryKeyReserved => "clé de paramètre de requête contenant `&`, `=` ou `#`",
            Self::QueryValueReserved => {
                "valeur de paramètre de requête contenant `&`, `#` ou un saut de ligne"
            }
        })
    }
}

/// Position facultative d'une entrée dans un message d'erreur.
fn at_index(index: &Option<usize>) -> String {
    index.map_or_else(String::new, |index| format!(", indice {index}"))
}

/// Erreur de résolution d'une modification, sans effet de bord.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    #[error("bloc `{block}` absent")]
    NoSuchBlock { block: &'static str },

    #[error("bloc `{block}` : indice {index} hors limites")]
    IndexOutOfRange { block: &'static str, index: usize },

    #[error("champ `{field}` absent du fichier")]
    MissingField { field: &'static str },

    #[error("corps de forme formulaire : seul un corps `{expected}` est éditable")]
    WrongBodyForm { expected: &'static str },

    #[error("bloc `{block}`{} : {problem}", at_index(.index))]
    InvalidEntry {
        block: &'static str,
        index: Option<usize>,
        problem: EntryProblem,
    },

    /// Le fichier produit ne se relit pas : bogue interne du writer, sans
    /// citer le contenu produit.
    #[error("le fichier produit ne se relit pas")]
    Unreadable,

    /// Chevauchement de deux remplacements résolus : bogue interne, jamais
    /// atteint par construction (blocs et entrées disjoints).
    #[error("chevauchement interne entre deux modifications résolues")]
    Overlap,
}

/// Erreur d'écriture, enveloppant une erreur de résolution ou d'I/O.
#[derive(Debug, Error)]
pub enum WriteError {
    #[error("`{path}` a été modifié depuis son chargement")]
    Stale { path: PathBuf },

    #[error("{0}")]
    Edit(#[from] EditError),

    #[error("erreur d'entrée/sortie sur `{path}` : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_never_quote_field_values() {
        let secret = "Bearer s3cr3t";
        let errors: Vec<EditError> = vec![
            EditError::NoSuchBlock { block: "headers" },
            EditError::IndexOutOfRange {
                block: "headers",
                index: 3,
            },
            EditError::MissingField { field: "url" },
            EditError::WrongBodyForm {
                expected: "body:text",
            },
            EditError::Overlap,
            EditError::Unreadable,
            EditError::InvalidEntry {
                block: "params:query",
                index: Some(2),
                problem: EntryProblem::QueryValueReserved,
            },
            EditError::InvalidEntry {
                block: "headers",
                index: None,
                problem: EntryProblem::KeyWhitespace,
            },
        ];
        for error in errors {
            assert!(!error.to_string().contains(secret), "{error}");
            assert!(!format!("{error:?}").contains(secret));
        }

        let write_error = WriteError::Stale {
            path: PathBuf::from("/tmp/headers.bru"),
        };
        assert!(!write_error.to_string().contains(secret));
        assert!(!format!("{write_error:?}").contains(secret));
    }
}
