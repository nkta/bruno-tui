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
    VarsPreRequest,
    VarsPostResponse,
    Assert,
    Settings,
    FormUrlEncoded,
    MultipartForm,
}

impl EntrySection {
    /// Ordre d'écriture de Bruno après le bloc de méthode.
    pub const ALL: [Self; 9] = [
        Self::QueryParams,
        Self::PathParams,
        Self::Headers,
        Self::FormUrlEncoded,
        Self::MultipartForm,
        Self::VarsPreRequest,
        Self::VarsPostResponse,
        Self::Assert,
        Self::Settings,
    ];

    /// Nom du bloc `.bru` de la section.
    pub fn block_name(self) -> &'static str {
        match self {
            Self::QueryParams => "params:query",
            Self::PathParams => "params:path",
            Self::Headers => "headers",
            Self::FormUrlEncoded => "body:form-urlencoded",
            Self::MultipartForm => "body:multipart-form",
            Self::VarsPreRequest => "vars:pre-request",
            Self::VarsPostResponse => "vars:post-response",
            Self::Assert => "assert",
            Self::Settings => "settings",
        }
    }

    /// Position de la section dans `ALL`.
    pub(crate) fn position(self) -> usize {
        match self {
            Self::QueryParams => 0,
            Self::PathParams => 1,
            Self::Headers => 2,
            Self::FormUrlEncoded => 3,
            Self::MultipartForm => 4,
            Self::VarsPreRequest => 5,
            Self::VarsPostResponse => 6,
            Self::Assert => 7,
            Self::Settings => 8,
        }
    }
}

/// Modes du corps autorisés pour une requête HTTP (`docs/edition-phase2.md` § 6).
pub const BODY_MODES: [&str; 8] = [
    "none",
    "json",
    "text",
    "xml",
    "sparql",
    "formUrlEncoded",
    "multipartForm",
    "file",
];

/// Modification d'une requête chargée.
///
/// Les modifications d'une liste s'appliquent dans l'ordre : `index`
/// désigne la position dans `RequestView.headers` / `.query_params` /
/// `.path_params` / etc. telle qu'elle résulte des modifications précédentes
/// de la même liste. Sans ajout ni suppression, il coïncide avec l'indice
/// exposé par `bru-parser`. `Url`, `Method`, `BodyMode` et `BodyText` sont singletons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldEdit {
    Url(String),
    /// Nom du bloc de méthode, en minuscules, l'une des 9 valeurs de
    /// `crate::collection::ast::METHODS` (`add-method-editing`).
    Method(String),
    /// Mode du corps dans le bloc de méthode (`BODY_MODES`).
    BodyMode(String),
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
    VarsPreRequestValue {
        index: usize,
        value: String,
    },
    VarsPreRequestEnabled {
        index: usize,
        enabled: bool,
    },
    VarsPostResponseValue {
        index: usize,
        value: String,
    },
    VarsPostResponseEnabled {
        index: usize,
        enabled: bool,
    },
    AssertValue {
        index: usize,
        value: String,
    },
    AssertEnabled {
        index: usize,
        enabled: bool,
    },
    SettingsValue {
        index: usize,
        value: String,
    },
    FormUrlEncodedValue {
        index: usize,
        value: String,
    },
    FormUrlEncodedEnabled {
        index: usize,
        enabled: bool,
    },
    MultipartFormValue {
        index: usize,
        value: String,
    },
    MultipartFormEnabled {
        index: usize,
        enabled: bool,
    },
    BodyText(String),
    /// Contenu du bloc body:graphql:vars ; vide pour retirer le bloc.
    GraphqlVars(String),
    /// Contenu du bloc script:pre-request ; vide pour retirer le bloc.
    ScriptPreRequest(String),
    /// Contenu du bloc script:post-response ; vide pour retirer le bloc.
    ScriptPostResponse(String),
    /// Contenu du bloc tests ; vide pour retirer le bloc.
    Tests(String),
    /// Contenu du bloc docs ; vide pour retirer le bloc.
    Docs(String),
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

    #[test]
    fn resolves_vars_assert_settings() {
        let source = "get {\n  url: http://a\n}\n\nvars:pre-request {\n  v1: 1\n}\n\nvars:post-response {\n  v2: 2\n}\n\nassert {\n  res.status: 200\n}\n\nsettings {\n  encodeUrl: true\n}\n";
        let file = parse(source);
        let replacements = resolve(
            &file,
            &[
                FieldEdit::VarsPreRequestValue {
                    index: 0,
                    value: "10".into(),
                },
                FieldEdit::VarsPostResponseValue {
                    index: 0,
                    value: "20".into(),
                },
                FieldEdit::AssertValue {
                    index: 0,
                    value: "eq 200".into(),
                },
                FieldEdit::SettingsValue {
                    index: 0,
                    value: "false".into(),
                },
            ],
        )
        .expect("résolu");
        assert_eq!(replacements.len(), 4);
        assert_eq!(replacements[0].bytes, "  v1: 10\n");
        assert_eq!(replacements[1].bytes, "  v2: 20\n");
        assert_eq!(replacements[2].bytes, "  res.status: eq 200\n");
        assert_eq!(replacements[3].bytes, "  encodeUrl: false\n");
    }
}
