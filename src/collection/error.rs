//! Erreurs de chargement d'une collection et de parsing d'un fichier `.bru`.
//!
//! Les messages citent des noms de bloc et des numéros de ligne, jamais le
//! contenu d'une ligne : en-têtes et corps peuvent porter des secrets.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Erreur fatale : la collection n'a pas pu être chargée du tout.
#[derive(Debug, Error)]
pub enum LoadError {
    #[error("`{path}` n'est pas dans une collection Bruno : aucun bruno.json trouvé en remontant")]
    NotACollection { path: PathBuf },

    /// Seule la position est conservée : le message serde brut peut citer
    /// des valeurs du fichier.
    #[error("bruno.json invalide dans `{path}` (ligne {line}, colonne {column})")]
    InvalidConfig {
        path: PathBuf,
        line: usize,
        column: usize,
    },

    #[error("erreur d'entrée/sortie sur `{path}` : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// Erreur propre à un fichier ; portée par un nœud de l'arbre, elle
/// n'interrompt pas le chargement des autres fichiers.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("lecture impossible : {0}")]
    Io(#[source] io::Error),

    #[error("le fichier n'est pas de l'UTF-8 valide")]
    InvalidUtf8,

    #[error("ligne {line} : texte hors de tout bloc")]
    TextOutsideBlock { line: usize },

    #[error("ligne {line} : ouverture de bloc invalide (attendu `nom {{` ou `nom [`)")]
    BadHeader { line: usize },

    #[error("ligne {line} : fermeture de bloc suivie de texte")]
    BadClosing { line: usize },

    #[error("bloc `{name}` ouvert ligne {line} et jamais fermé")]
    UnclosedBlock { name: String, line: usize },

    #[error("aucun bloc de méthode HTTP (get, post, put, ...)")]
    MissingMethod,

    #[error("arborescence trop profonde (plus de {max} niveaux)")]
    TooDeep { max: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_never_quote_line_content() {
        // Fichier malformé dont la ligne fautive porte un secret : les
        // erreurs ne doivent citer que des positions et des noms de bloc.
        let source = "headers {\n  Authorization: Bearer s3cr3t\n";
        let error =
            crate::collection::BruFile::parse(source.to_owned()).expect_err("bloc non fermé");
        assert!(matches!(
            error,
            ParseError::UnclosedBlock { ref name, line: 1 } if name == "headers"
        ));
        assert!(!error.to_string().contains("s3cr3t"), "{error}");
        assert!(!format!("{error:?}").contains("s3cr3t"));

        let outside =
            crate::collection::BruFile::parse("Authorization: Bearer s3cr3t\n".to_owned())
                .expect_err("texte hors bloc");
        assert!(matches!(outside, ParseError::TextOutsideBlock { line: 1 }));
        assert!(!outside.to_string().contains("s3cr3t"));
        assert!(!format!("{outside:?}").contains("s3cr3t"));
    }

    #[test]
    fn not_a_collection_names_the_path() {
        let error = LoadError::NotACollection {
            path: PathBuf::from("/tmp/nowhere"),
        };
        assert!(error.to_string().contains("/tmp/nowhere"));
    }
}
