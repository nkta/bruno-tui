//! Erreurs de résolution d'une édition et d'écriture d'un fichier `.bru`.
//!
//! Les messages ne citent jamais une valeur de champ : seulement des noms
//! de bloc, des indices et des chemins. Un en-tête ou un corps de requête
//! peut porter un secret.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Erreur de résolution d'une modification, sans effet de bord.
#[derive(Debug, Error)]
pub enum EditError {
    #[error("bloc `{block}` absent")]
    NoSuchBlock { block: &'static str },

    #[error("bloc `{block}` : indice {index} hors limites")]
    IndexOutOfRange { block: &'static str, index: usize },

    #[error("champ `{field}` absent du fichier")]
    MissingField { field: &'static str },

    #[error("corps de forme formulaire : seul un corps `{expected}` est éditable")]
    WrongBodyForm { expected: &'static str },

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
