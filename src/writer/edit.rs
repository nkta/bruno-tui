//! Modifications de champ et résolution vers des tranches de source à
//! remplacer.
//!
//! La résolution est pure : elle ne touche jamais le disque et ne produit
//! aucun effet de bord en cas d'erreur.

use std::ops::Range;

use super::draft::Draft;
use super::error::EditError;
use crate::collection::BruFile;

/// Section d'entrées où ajouter, supprimer ou renommer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntrySection {
    Headers,
    QueryParams,
    PathParams,
}

impl EntrySection {
    /// Ordre d'écriture de Bruno après le bloc de méthode.
    pub const ALL: [Self; 3] = [Self::QueryParams, Self::PathParams, Self::Headers];

    /// Nom du bloc `.bru` de la section.
    pub fn block_name(self) -> &'static str {
        match self {
            Self::Headers => "headers",
            Self::QueryParams => "params:query",
            Self::PathParams => "params:path",
        }
    }

    /// Position de la section dans `ALL`.
    pub(crate) fn position(self) -> usize {
        match self {
            Self::QueryParams => 0,
            Self::PathParams => 1,
            Self::Headers => 2,
        }
    }
}

/// Modification d'une requête chargée.
///
/// Les modifications d'une liste s'appliquent dans l'ordre : `index`
/// désigne la position dans `RequestView.headers` / `.query_params` /
/// `.path_params` telle qu'elle résulte des modifications précédentes de la
/// même liste. Sans ajout ni suppression, il coïncide avec l'indice exposé
/// par `bru-parser`. `Url` et `BodyText` sont singletons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldEdit {
    Url(String),
    HeaderValue {
        index: usize,
        value: String,
    },
    HeaderEnabled {
        index: usize,
        enabled: bool,
    },
    QueryParamValue {
        index: usize,
        value: String,
    },
    QueryParamEnabled {
        index: usize,
        enabled: bool,
    },
    PathParamValue {
        index: usize,
        value: String,
    },
    PathParamEnabled {
        index: usize,
        enabled: bool,
    },
    BodyText(String),
    /// Ajout d'une entrée en fin de section.
    AddEntry {
        section: EntrySection,
        key: String,
        value: String,
        enabled: bool,
    },
    RemoveEntry {
        section: EntrySection,
        index: usize,
    },
    /// Renommage de la clé, valeur et état conservés.
    RenameKey {
        section: EntrySection,
        index: usize,
        key: String,
    },
}

/// Remplacement résolu : une tranche du source et son texte de
/// substitution. Une tranche vide est une insertion, un texte vide une
/// suppression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub span: Range<usize>,
    pub bytes: String,
}

/// Applique `edits` dans l'ordre sur un brouillon de la requête, puis
/// produit les remplacements triés par position. Sans effet de bord : une
/// modification refusée retourne une erreur sans rien produire.
pub(crate) fn resolve(ast: &BruFile, edits: &[FieldEdit]) -> Result<Vec<Replacement>, EditError> {
    let mut draft = Draft::from_ast(ast);
    for edit in edits {
        draft.apply(edit)?;
    }
    draft.replacements()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> BruFile {
        BruFile::parse(source.to_owned()).expect("source valide")
    }

    #[test]
    fn resolves_url() {
        let file = parse("get {\n  url: http://a\n}\n");
        let replacements = resolve(&file, &[FieldEdit::Url("http://b".into())]).expect("résolu");
        assert_eq!(replacements.len(), 1);
        assert_eq!(replacements[0].bytes, "  url: http://b\n");
    }

    #[test]
    fn resolves_header_value_and_enabled() {
        let file = parse(
            "get {\n  url: http://a\n}\n\nheaders {\n  Accept: text/plain\n  ~X-Debug: 1\n}\n",
        );
        let replacements = resolve(
            &file,
            &[FieldEdit::HeaderValue {
                index: 0,
                value: "application/json".into(),
            }],
        )
        .expect("résolu");
        assert_eq!(replacements[0].bytes, "  Accept: application/json\n");

        let replacements = resolve(
            &file,
            &[FieldEdit::HeaderEnabled {
                index: 1,
                enabled: true,
            }],
        )
        .expect("résolu");
        assert_eq!(replacements[0].bytes, "  X-Debug: 1\n");
    }

    #[test]
    fn resolves_query_and_path_params() {
        let file = parse(
            "get {\n  url: http://a\n}\n\nparams:query {\n  q: x\n}\n\nparams:path {\n  id: 1\n}\n",
        );
        let replacements = resolve(
            &file,
            &[FieldEdit::QueryParamValue {
                index: 0,
                value: "y".into(),
            }],
        )
        .expect("résolu");
        // L'URL est reconstruite depuis les paramètres activés (sync Bruno).
        assert_eq!(replacements.len(), 2);
        assert_eq!(replacements[0].bytes, "  url: http://a?q=y\n");
        assert_eq!(replacements[1].bytes, "  q: y\n");

        let replacements = resolve(
            &file,
            &[FieldEdit::PathParamEnabled {
                index: 0,
                enabled: false,
            }],
        )
        .expect("résolu");
        assert_eq!(replacements[0].bytes, "  ~id: 1\n");
    }

    #[test]
    fn merges_value_and_enabled_edits_on_same_index() {
        let file = parse("get {\n  url: http://a\n}\n\nheaders {\n  Accept: text/plain\n}\n");
        let replacements = resolve(
            &file,
            &[
                FieldEdit::HeaderValue {
                    index: 0,
                    value: "application/json".into(),
                },
                FieldEdit::HeaderEnabled {
                    index: 0,
                    enabled: false,
                },
            ],
        )
        .expect("résolu");
        assert_eq!(replacements.len(), 1);
        assert_eq!(replacements[0].bytes, "  ~Accept: application/json\n");
    }

    #[test]
    fn index_out_of_range() {
        let file = parse("get {\n  url: http://a\n}\n\nheaders {\n  Accept: text/plain\n}\n");
        let error = resolve(
            &file,
            &[FieldEdit::HeaderValue {
                index: 3,
                value: "x".into(),
            }],
        )
        .expect_err("hors limites");
        assert!(matches!(
            error,
            EditError::IndexOutOfRange {
                block: "headers",
                index: 3
            }
        ));
    }

    #[test]
    fn missing_block() {
        let file = parse("get {\n  url: http://a\n}\n");
        let error = resolve(
            &file,
            &[FieldEdit::HeaderValue {
                index: 0,
                value: "x".into(),
            }],
        )
        .expect_err("bloc absent");
        assert!(matches!(error, EditError::NoSuchBlock { block: "headers" }));
    }

    #[test]
    fn body_text_on_form_body_is_rejected() {
        let file = parse(
            "put {\n  url: http://a\n  body: formUrlEncoded\n}\n\nbody:form-urlencoded {\n  a: 1\n}\n",
        );
        let error = resolve(&file, &[FieldEdit::BodyText("x".into())]).expect_err("refusé");
        assert!(matches!(
            error,
            EditError::WrongBodyForm {
                expected: "body:text"
            }
        ));
    }

    #[test]
    fn body_text_on_json_body() {
        let file = parse("post {\n  url: http://a\n  body: json\n}\n\nbody:json {\n  {}\n}\n");
        let replacements =
            resolve(&file, &[FieldEdit::BodyText("{\n  \"a\": 1\n}".into())]).expect("résolu");
        assert_eq!(replacements[0].bytes, "  {\n    \"a\": 1\n  }\n");
    }

    #[test]
    fn error_does_not_quote_secret_header_value() {
        let secret = "Bearer s3cr3t";
        let source =
            format!("get {{\n  url: http://a\n}}\n\nheaders {{\n  Authorization: {secret}\n}}\n");
        let file = parse(&source);
        let error = resolve(
            &file,
            &[FieldEdit::HeaderValue {
                index: 5,
                value: "irrelevant".into(),
            }],
        )
        .expect_err("hors limites");
        assert!(!error.to_string().contains(secret), "{error}");
        assert!(!format!("{error:?}").contains(secret));
    }

    #[test]
    fn no_edits_produces_no_replacements() {
        let file = parse("get {\n  url: http://a\n}\n");
        assert!(resolve(&file, &[]).expect("résolu").is_empty());
    }
}
