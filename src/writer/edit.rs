//! Modifications de champ et résolution vers des tranches de source à
//! remplacer.
//!
//! La résolution est pure : elle ne touche jamais le disque et ne produit
//! aucun effet de bord en cas d'erreur.

use std::ops::Range;

use super::error::EditError;
use super::format;
use crate::collection::ast::METHODS;
use crate::collection::{BlockBody, BruFile};

/// Modification d'un champ déjà présent dans une requête chargée.
///
/// `index` désigne la position dans `RequestView.headers` / `.query_params`
/// / `.path_params`, qui correspond 1:1 à l'entrée de même indice dans le
/// bloc AST correspondant. `Url` et `BodyText` sont singletons : une seule
/// URL, un seul corps texte par requête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldEdit {
    Url(String),
    HeaderValue { index: usize, value: String },
    HeaderEnabled { index: usize, enabled: bool },
    QueryParamValue { index: usize, value: String },
    QueryParamEnabled { index: usize, enabled: bool },
    PathParamValue { index: usize, value: String },
    PathParamEnabled { index: usize, enabled: bool },
    BodyText(String),
}

impl FieldEdit {
    fn target(&self) -> TargetKey {
        match self {
            Self::Url(_) => TargetKey::Url,
            Self::HeaderValue { index, .. } | Self::HeaderEnabled { index, .. } => {
                TargetKey::Header(*index)
            }
            Self::QueryParamValue { index, .. } | Self::QueryParamEnabled { index, .. } => {
                TargetKey::QueryParam(*index)
            }
            Self::PathParamValue { index, .. } | Self::PathParamEnabled { index, .. } => {
                TargetKey::PathParam(*index)
            }
            Self::BodyText(_) => TargetKey::Body,
        }
    }
}

/// Identité d'une cible dans l'AST, pour regrouper les éditions qui portent
/// sur la même entrée avant de produire un seul remplacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKey {
    Url,
    Header(usize),
    QueryParam(usize),
    PathParam(usize),
    Body,
}

/// Remplacement résolu : une tranche du source et son texte de
/// substitution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Replacement {
    pub span: Range<usize>,
    pub bytes: String,
}

/// État courant d'une entrée dictionnaire ciblée (URL, en-tête, paramètre),
/// mis à jour au fil des éditions qui la concernent.
struct EntryState {
    span: Range<usize>,
    key: String,
    value: String,
    disabled: bool,
}

/// État courant d'un bloc de corps texte ciblé.
struct BodyState {
    span: Range<usize>,
    text: String,
}

enum Snapshot {
    Entry(EntryState),
    Body(BodyState),
}

const TEXT_BODY_BLOCKS: [(&str, &str); 5] = [
    ("json", "body:json"),
    ("text", "body:text"),
    ("xml", "body:xml"),
    ("sparql", "body:sparql"),
    ("graphql", "body:graphql"),
];
const FORM_BODY_KINDS: [&str; 3] = ["formUrlEncoded", "multipartForm", "file"];

/// Résout une liste d'éditions vers les remplacements à appliquer, fusionnant
/// celles qui ciblent la même entrée et triant le résultat par position dans
/// le fichier. Sans effet de bord : une cible introuvable retourne une
/// erreur sans avoir modifié quoi que ce soit.
pub(crate) fn resolve(ast: &BruFile, edits: &[FieldEdit]) -> Result<Vec<Replacement>, EditError> {
    let mut order: Vec<TargetKey> = Vec::new();
    let mut entries: Vec<(TargetKey, EntryState)> = Vec::new();
    let mut bodies: Vec<(TargetKey, BodyState)> = Vec::new();

    for edit in edits {
        let key = edit.target();
        if !order.contains(&key) {
            order.push(key);
            match snapshot_for(ast, key)? {
                Snapshot::Entry(state) => entries.push((key, state)),
                Snapshot::Body(state) => bodies.push((key, state)),
            }
        }
        if let Some((_, state)) = entries.iter_mut().find(|(found, _)| *found == key) {
            apply_entry(state, edit);
        } else if let Some((_, state)) = bodies.iter_mut().find(|(found, _)| *found == key) {
            apply_body(state, edit);
        }
    }

    let eol = format::detect_eol(ast.raw());
    let mut replacements: Vec<Replacement> = Vec::new();
    for (_, state) in &entries {
        replacements.push(Replacement {
            span: state.span.clone(),
            bytes: format::format_entry(&state.key, &state.value, state.disabled, eol),
        });
    }
    for (_, state) in &bodies {
        replacements.push(Replacement {
            span: state.span.clone(),
            bytes: format::format_body_content(&state.text, eol),
        });
    }

    replacements.sort_by_key(|replacement| replacement.span.start);
    for pair in replacements.windows(2) {
        if pair[0].span.end > pair[1].span.start {
            return Err(EditError::Overlap);
        }
    }
    Ok(replacements)
}

fn apply_entry(entry: &mut EntryState, edit: &FieldEdit) {
    match edit {
        FieldEdit::Url(value) => entry.value = value.clone(),
        FieldEdit::HeaderValue { value, .. }
        | FieldEdit::QueryParamValue { value, .. }
        | FieldEdit::PathParamValue { value, .. } => entry.value = value.clone(),
        FieldEdit::HeaderEnabled { enabled, .. }
        | FieldEdit::QueryParamEnabled { enabled, .. }
        | FieldEdit::PathParamEnabled { enabled, .. } => entry.disabled = !enabled,
        FieldEdit::BodyText(_) => unreachable!("BodyText ne cible jamais une entrée"),
    }
}

fn apply_body(body: &mut BodyState, edit: &FieldEdit) {
    match edit {
        FieldEdit::BodyText(text) => body.text = text.clone(),
        _ => unreachable!("seul BodyText cible un corps"),
    }
}

fn snapshot_for(ast: &BruFile, key: TargetKey) -> Result<Snapshot, EditError> {
    match key {
        TargetKey::Url => url_snapshot(ast),
        TargetKey::Header(index) => entry_snapshot(ast, "headers", index),
        TargetKey::QueryParam(index) => entry_snapshot(ast, "params:query", index),
        TargetKey::PathParam(index) => entry_snapshot(ast, "params:path", index),
        TargetKey::Body => body_snapshot(ast),
    }
}

fn method_block(ast: &BruFile) -> Result<&crate::collection::Block, EditError> {
    ast.blocks()
        .find(|block| METHODS.contains(&block.name.as_str()))
        .ok_or(EditError::MissingField { field: "method" })
}

fn url_snapshot(ast: &BruFile) -> Result<Snapshot, EditError> {
    let method = method_block(ast)?;
    let BlockBody::Dictionary(entries) = &method.body else {
        return Err(EditError::MissingField { field: "url" });
    };
    let entry = entries
        .iter()
        .find(|entry| entry.key == "url")
        .ok_or(EditError::MissingField { field: "url" })?;
    Ok(Snapshot::Entry(EntryState {
        span: entry.span.clone(),
        key: entry.key.clone(),
        value: entry.value.clone(),
        disabled: entry.disabled,
    }))
}

fn entry_snapshot(ast: &BruFile, block: &'static str, index: usize) -> Result<Snapshot, EditError> {
    let entries = ast
        .dictionary(block)
        .ok_or(EditError::NoSuchBlock { block })?;
    let entry = entries
        .get(index)
        .ok_or(EditError::IndexOutOfRange { block, index })?;
    Ok(Snapshot::Entry(EntryState {
        span: entry.span.clone(),
        key: entry.key.clone(),
        value: entry.value.clone(),
        disabled: entry.disabled,
    }))
}

fn body_snapshot(ast: &BruFile) -> Result<Snapshot, EditError> {
    let method = method_block(ast)?;
    let BlockBody::Dictionary(entries) = &method.body else {
        return Err(EditError::MissingField { field: "body" });
    };
    let declared = entries
        .iter()
        .find(|entry| entry.key == "body")
        .map(|entry| entry.value.as_str())
        .unwrap_or_default();

    if FORM_BODY_KINDS.contains(&declared) {
        return Err(EditError::WrongBodyForm {
            expected: "body:text",
        });
    }
    let block_name = TEXT_BODY_BLOCKS
        .iter()
        .find(|(kind, _)| *kind == declared)
        .map(|(_, block)| *block)
        .ok_or(EditError::NoSuchBlock { block: "body" })?;

    let block = ast
        .block(block_name)
        .ok_or(EditError::NoSuchBlock { block: block_name })?;
    let BlockBody::Text { content } = &block.body else {
        return Err(EditError::NoSuchBlock { block: block_name });
    };
    Ok(Snapshot::Body(BodyState {
        span: content.clone(),
        text: String::new(),
    }))
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
        assert_eq!(replacements[0].bytes, "  q: y\n");

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
