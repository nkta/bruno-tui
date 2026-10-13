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
    #[error(
        "`{path}` n'est pas dans une collection Bruno : aucun bruno.json ni opencollection.yml trouvé en remontant"
    )]
    NotACollection { path: PathBuf },

    /// Membre d'un workspace qui ne désigne pas la racine d'une collection.
    #[error("`{path}` n'est pas une collection Bruno : ni bruno.json ni opencollection.yml")]
    NotACollectionRoot { path: PathBuf },

    /// Seule la position est conservée : le message serde brut peut citer
    /// des valeurs du fichier.
    #[error("bruno.json invalide dans `{path}` (ligne {line}, colonne {column})")]
    InvalidConfig {
        path: PathBuf,
        line: usize,
        column: usize,
    },

    /// Seule la position est conservée : le message YAML brut peut citer
    /// des valeurs du fichier. Ligne et colonne valent 0 si inconnues.
    #[error("opencollection.yml invalide dans `{path}` (ligne {line}, colonne {column})")]
    InvalidOpenCollection {
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

    /// Fichier YAML mal formé ; seule la position est conservée.
    #[error("YAML invalide{}", yaml_position(*.line, *.column))]
    InvalidYaml {
        line: Option<usize>,
        column: Option<usize>,
    },

    #[error("le document YAML n'est pas un dictionnaire")]
    NotAMapping,

    #[error("aucun type de requête (`info.type`)")]
    MissingItemType,

    /// `kind` est un type connu de Bruno (`grpc`, `websocket`, ...) ou
    /// `inconnu` : jamais une valeur arbitraire du fichier.
    #[error("type de requête `{kind}` non pris en charge par le TUI")]
    UnsupportedItemType { kind: String },

    #[error("arborescence trop profonde (plus de {max} niveaux)")]
    TooDeep { max: usize },
}

fn yaml_position(line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!(" (ligne {line}, colonne {column})"),
        (Some(line), None) => format!(" (ligne {line})"),
        _ => String::new(),
    }
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
