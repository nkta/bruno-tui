//! Brouillon ordonné d'une requête : application séquentielle des
//! modifications, synchronisation `params:query` ↔ URL, puis comparaison
//! à l'AST d'origine pour ne produire que les tranches à réécrire
//! (`add-entry-management`, design D2 et D3).
//!
//! Tout est pur : aucune I/O, aucun effet de bord en cas d'erreur.

use std::ops::Range;

use super::edit::{EntrySection, FieldEdit, Replacement};
use super::error::{EditError, EntryProblem};
use super::format;
use super::query;
use crate::collection::ast::METHODS;
use crate::collection::{BlockBody, BruFile, Entry, NodeKind};

const TEXT_BODY_BLOCKS: [(&str, &str); 5] = [
    ("json", "body:json"),
    ("text", "body:text"),
    ("xml", "body:xml"),
    ("sparql", "body:sparql"),
    ("graphql", "body:graphql"),
];
const FORM_BODY_KINDS: [&str; 3] = ["formUrlEncoded", "multipartForm", "file"];

/// Entrée courante d'une section ; `origin` est l'indice de l'`Entry`
/// d'origine dans le bloc, `None` pour une entrée ajoutée.
#[derive(Debug, Clone)]
struct DraftEntry {
    origin: Option<usize>,
    key: String,
    value: String,
    disabled: bool,
}

impl DraftEntry {
    fn from_entry(index: usize, entry: &Entry) -> Self {
        Self {
            origin: Some(index),
            key: entry.key.clone(),
            value: entry.value.clone(),
            disabled: entry.disabled,
        }
    }
}

/// Entrée `url` du bloc de méthode, valeur d'origine et valeur courante.
struct UrlState {
    span: Range<usize>,
    key: String,
    disabled: bool,
    original: String,
    current: String,
}

/// Contenu d'un bloc de corps texte ciblé.
struct BodyState {
    span: Range<usize>,
    text: String,
}

struct Section {
    entries: Vec<DraftEntry>,
    /// Le bloc existe dans le fichier chargé.
    block_present: bool,
    /// Une modification structurelle ou une synchronisation a touché la
    /// section : un bloc resté vide est alors retiré.
    touched: bool,
}

pub(crate) struct Draft<'a> {
    ast: &'a BruFile,
    url: Result<UrlState, EditError>,
    sections: [Section; 3],
    body: Option<BodyState>,
}

impl<'a> Draft<'a> {
    pub(crate) fn from_ast(ast: &'a BruFile) -> Self {
        let section = |section: EntrySection| {
            let entries = ast.dictionary(section.block_name());
            Section {
                entries: entries
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| DraftEntry::from_entry(index, entry))
                    .collect(),
                block_present: entries.is_some(),
                touched: false,
            }
        };
        Self {
            ast,
            url: url_state(ast),
            sections: EntrySection::ALL.map(section),
            body: None,
        }
    }

    /// Applique une modification ; l'indice porté désigne la position dans
    /// l'état courant de la section.
    pub(crate) fn apply(&mut self, edit: &FieldEdit) -> Result<(), EditError> {
        use EntrySection::{Headers, PathParams, QueryParams};
        match edit {
            FieldEdit::Url(value) => {
                let url = self.url.as_mut().map_err(|error| error.clone())?;
                url.current = value.clone();
                self.sync_params_from_url()?;
            }
            FieldEdit::HeaderValue { index, value } => {
                self.entry_mut(Headers, *index)?.value = value.clone();
            }
            FieldEdit::PathParamValue { index, value } => {
                self.entry_mut(PathParams, *index)?.value = value.clone();
            }
            FieldEdit::QueryParamValue { index, value } => {
                check_query_value(value, Some(*index))?;
                self.entry_mut(QueryParams, *index)?.value = value.clone();
                self.sync_url_from_params();
            }
            FieldEdit::HeaderEnabled { index, enabled } => {
                self.entry_mut(Headers, *index)?.disabled = !enabled;
            }
            FieldEdit::PathParamEnabled { index, enabled } => {
                self.entry_mut(PathParams, *index)?.disabled = !enabled;
            }
            FieldEdit::QueryParamEnabled { index, enabled } => {
                self.entry_mut(QueryParams, *index)?.disabled = !enabled;
                self.sync_url_from_params();
            }
            FieldEdit::BodyText(text) => {
                if self.body.is_none() {
                    self.body = Some(body_state(self.ast)?);
                }
                if let Some(body) = &mut self.body {
                    body.text = text.clone();
                }
            }
            FieldEdit::AddEntry {
                section,
                key,
                value,
                enabled,
            } => {
                check_key(*section, key, None)?;
                if *section == QueryParams {
                    check_query_value(value, None)?;
                }
                let target = &mut self.sections[section.position()];
                target.entries.push(DraftEntry {
                    origin: None,
                    key: key.clone(),
                    value: value.clone(),
                    disabled: !enabled,
                });
                target.touched = true;
                if *section == QueryParams {
                    self.sync_url_from_params();
                }
            }
            FieldEdit::RemoveEntry { section, index } => {
                self.entry_mut(*section, *index)?;
                let target = &mut self.sections[section.position()];
                target.entries.remove(*index);
                target.touched = true;
                if *section == QueryParams {
                    self.sync_url_from_params();
                }
            }
            FieldEdit::RenameKey {
                section,
                index,
                key,
            } => {
                self.entry_mut(*section, *index)?;
                check_key(*section, key, Some(*index))?;
                self.entry_mut(*section, *index)?.key = key.clone();
                if *section == QueryParams {
                    self.sync_url_from_params();
                }
            }
        }
        Ok(())
    }

    fn entry_mut(
        &mut self,
        section: EntrySection,
        index: usize,
    ) -> Result<&mut DraftEntry, EditError> {
        let block = section.block_name();
        let target = &mut self.sections[section.position()];
        if !target.block_present && target.entries.is_empty() {
            return Err(EditError::NoSuchBlock { block });
        }
        target
            .entries
            .get_mut(index)
            .ok_or(EditError::IndexOutOfRange { block, index })
    }

    /// Reconstruit la chaîne de requête de l'URL à partir des paramètres de
    /// requête activés. Sans entrée `url`, rien à synchroniser.
    fn sync_url_from_params(&mut self) {
        let Ok(url) = &mut self.url else {
            return;
        };
        let enabled: Vec<(&str, &str)> = self.sections[EntrySection::QueryParams.position()]
            .entries
            .iter()
            .filter(|entry| !entry.disabled)
            .map(|entry| (entry.key.as_str(), entry.value.as_str()))
            .collect();
        let rebuilt = query::build_url(&query::split_url(&url.current), &enabled);
        url.current = rebuilt;
    }

    /// Recalcule les paramètres de requête activés à partir de l'URL : la
    /// i-ème entrée activée prend la i-ème paire, les paires excédentaires
    /// sont ajoutées en fin, les activées excédentaires retirées ; les
    /// désactivées restent en place.
    fn sync_params_from_url(&mut self) -> Result<(), EditError> {
        let Ok(url) = &self.url else {
            return Ok(());
        };
        let pairs = query::split_url(&url.current)
            .query
            .map(query::parse_query)
            .unwrap_or_default();
        for (key, value) in &pairs {
            check_key(EntrySection::QueryParams, key, None)?;
            check_query_value(value, None)?;
        }
        let section = &mut self.sections[EntrySection::QueryParams.position()];
        let enabled: Vec<usize> = section
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| !entry.disabled)
            .map(|(index, _)| index)
            .collect();
        let mut pairs = pairs.into_iter();
        let mut surplus = Vec::new();
        for &position in &enabled {
            match pairs.next() {
                Some((key, value)) => {
                    let entry = &mut section.entries[position];
                    entry.key = key;
                    entry.value = value;
                }
                None => surplus.push(position),
            }
        }
        for position in surplus.into_iter().rev() {
            section.entries.remove(position);
        }
        for (key, value) in pairs {
            section.entries.push(DraftEntry {
                origin: None,
                key,
                value,
                disabled: false,
            });
        }
        section.touched = true;
        Ok(())
    }

    /// Tranches à réécrire pour passer du fichier chargé à l'état courant,
    /// triées par position.
    pub(crate) fn replacements(&self) -> Result<Vec<Replacement>, EditError> {
        let raw = self.ast.raw();
        let eol = format::detect_eol(raw);
        let mut replacements = Vec::new();

        if let Ok(url) = &self.url
            && url.current != url.original
        {
            replacements.push(Replacement {
                span: url.span.clone(),
                bytes: format::format_entry(&url.key, &url.current, url.disabled, eol),
            });
        }
        if let Some(body) = &self.body {
            replacements.push(Replacement {
                span: body.span.clone(),
                bytes: format::format_body_content(&body.text, eol),
            });
        }

        let mut prefixed_anchors: Vec<usize> = Vec::new();
        for section in EntrySection::ALL {
            let state = &self.sections[section.position()];
            match block_node(self.ast, section.block_name()) {
                Some((span, entries)) => {
                    if state.touched && state.entries.is_empty() {
                        let start = span.start - preceding_blank_line_len(raw, span.start);
                        replacements.push(Replacement {
                            span: start..span.end,
                            bytes: String::new(),
                        });
                        continue;
                    }
                    for (index, original) in entries.iter().enumerate() {
                        let current = state.entries.iter().find(|e| e.origin == Some(index));
                        match current {
                            None => replacements.push(Replacement {
                                span: original.span.clone(),
                                bytes: String::new(),
                            }),
                            Some(entry)
                                if entry.key != original.key
                                    || entry.value != original.value
                                    || entry.disabled != original.disabled =>
                            {
                                replacements.push(Replacement {
                                    span: original.span.clone(),
                                    bytes: format::format_entry(
                                        &entry.key,
                                        &entry.value,
                                        entry.disabled,
                                        eol,
                                    ),
                                });
                            }
                            Some(_) => {}
                        }
                    }
                    let added = format_added(state, eol);
                    if !added.is_empty() {
                        let anchor = entries
                            .last()
                            .map_or_else(|| line_end(raw, span.start), |e| e.span.end);
                        replacements.push(Replacement {
                            span: anchor..anchor,
                            bytes: added,
                        });
                    }
                }
                None if !state.entries.is_empty() => {
                    let anchor = creation_anchor(self.ast, section);
                    let mut bytes = String::new();
                    if !raw[..anchor].is_empty()
                        && !raw[..anchor].ends_with('\n')
                        && !prefixed_anchors.contains(&anchor)
                    {
                        bytes.push_str(eol);
                        prefixed_anchors.push(anchor);
                    }
                    bytes.push_str(eol);
                    bytes.push_str(section.block_name());
                    bytes.push_str(" {");
                    bytes.push_str(eol);
                    bytes.push_str(&format_added(state, eol));
                    bytes.push('}');
                    bytes.push_str(eol);
                    replacements.push(Replacement {
                        span: anchor..anchor,
                        bytes,
                    });
                }
                None => {}
            }
        }

        // Une insertion (tranche vide) passe avant un remplacement qui
        // commence au même endroit ; le tri stable garde l'ordre d'émission
        // entre insertions au même point.
        replacements.sort_by_key(|replacement| (replacement.span.start, replacement.span.end));
        for pair in replacements.windows(2) {
            if pair[0].span.end > pair[1].span.start {
                return Err(EditError::Overlap);
            }
        }
        Ok(replacements)
    }
}

fn format_added(section: &Section, eol: &str) -> String {
    section
        .entries
        .iter()
        .filter(|entry| entry.origin.is_none())
        .map(|entry| format::format_entry(&entry.key, &entry.value, entry.disabled, eol))
        .collect()
}

fn check_key(section: EntrySection, key: &str, index: Option<usize>) -> Result<(), EditError> {
    let problem = if key.is_empty() {
        Some(EntryProblem::EmptyKey)
    } else if key.contains([' ', '\t', '\n', '\r']) {
        Some(EntryProblem::KeyWhitespace)
    } else if key.contains(':') {
        Some(EntryProblem::KeyColon)
    } else if key.starts_with(['~', '"']) {
        Some(EntryProblem::KeyLeadingMarker)
    } else if section == EntrySection::QueryParams && key.contains(['&', '=', '#']) {
        Some(EntryProblem::QueryKeyReserved)
    } else {
        None
    };
    match problem {
        Some(problem) => Err(EditError::InvalidEntry {
            block: section.block_name(),
            index,
            problem,
        }),
        None => Ok(()),
    }
}

fn check_query_value(value: &str, index: Option<usize>) -> Result<(), EditError> {
    if value.contains(['&', '#', '\n', '\r']) {
        return Err(EditError::InvalidEntry {
            block: EntrySection::QueryParams.block_name(),
            index,
            problem: EntryProblem::QueryValueReserved,
        });
    }
    Ok(())
}

/// Tranche et entrées du premier bloc dictionnaire portant ce nom.
fn block_node<'f>(ast: &'f BruFile, name: &str) -> Option<(Range<usize>, &'f [Entry])> {
    ast.nodes().iter().find_map(|node| match &node.kind {
        NodeKind::Block(block) if block.name == name => match &block.body {
            BlockBody::Dictionary(entries) => Some((node.span.clone(), entries.as_slice())),
            _ => None,
        },
        _ => None,
    })
}

/// Tranche du premier bloc de méthode.
fn method_node_span(ast: &BruFile) -> Option<Range<usize>> {
    ast.nodes().iter().find_map(|node| match &node.kind {
        NodeKind::Block(block) if METHODS.contains(&block.name.as_str()) => Some(node.span.clone()),
        _ => None,
    })
}

/// Point d'insertion d'un bloc absent : fin du dernier bloc présent parmi
/// ses prédécesseurs dans l'ordre d'écriture de Bruno (méthode,
/// `params:query`, `params:path`, `headers`), sinon fin du fichier.
fn creation_anchor(ast: &BruFile, section: EntrySection) -> usize {
    let mut anchor = method_node_span(ast).map(|span| span.end);
    for previous in EntrySection::ALL
        .iter()
        .take_while(|candidate| **candidate != section)
    {
        if let Some((span, _)) = block_node(ast, previous.block_name()) {
            anchor = Some(span.end);
        }
    }
    anchor.unwrap_or(ast.raw().len())
}

/// Fin de la ligne commençant à `start` (fin de ligne comprise).
fn line_end(raw: &str, start: usize) -> usize {
    raw[start..].find('\n').map_or(raw.len(), |i| start + i + 1)
}

/// Longueur de la ligne vide qui se termine juste avant `start`, zéro s'il
/// n'y en a pas.
fn preceding_blank_line_len(raw: &str, start: usize) -> usize {
    let before = &raw[..start];
    let eol_len = if before.ends_with("\r\n") {
        2
    } else if before.ends_with('\n') {
        1
    } else {
        return 0;
    };
    let rest = &before[..before.len() - eol_len];
    if rest.is_empty() || rest.ends_with('\n') {
        eol_len
    } else {
        0
    }
}

fn url_state(ast: &BruFile) -> Result<UrlState, EditError> {
    let method = ast
        .blocks()
        .find(|block| METHODS.contains(&block.name.as_str()))
        .ok_or(EditError::MissingField { field: "method" })?;
    let BlockBody::Dictionary(entries) = &method.body else {
        return Err(EditError::MissingField { field: "url" });
    };
    let entry = entries
        .iter()
        .find(|entry| entry.key == "url")
        .ok_or(EditError::MissingField { field: "url" })?;
    Ok(UrlState {
        span: entry.span.clone(),
        key: entry.key.clone(),
        disabled: entry.disabled,
        original: entry.value.clone(),
        current: entry.value.clone(),
    })
}

fn body_state(ast: &BruFile) -> Result<BodyState, EditError> {
    let method = ast
        .blocks()
        .find(|block| METHODS.contains(&block.name.as_str()))
        .ok_or(EditError::MissingField { field: "method" })?;
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
    Ok(BodyState {
        span: content.clone(),
        text: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::edit::resolve;
    use super::super::format::serialize;
    use super::*;

    fn parse(source: &str) -> BruFile {
        BruFile::parse(source.to_owned()).expect("source valide")
    }

    fn apply(source: &str, edits: &[FieldEdit]) -> String {
        let file = parse(source);
        String::from_utf8(serialize(&file, edits).expect("sérialisation")).expect("utf-8")
    }

    fn add(section: EntrySection, key: &str, value: &str) -> FieldEdit {
        FieldEdit::AddEntry {
            section,
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    const GET: &str = "get {\n  url: https://h/items\n}\n";

    #[test]
    fn remove_then_edit_targets_the_former_next_entry() {
        let source = "get {\n  url: http://a\n}\n\nheaders {\n  A: 1\n  B: 2\n  C: 3\n}\n";
        let out = apply(
            source,
            &[
                FieldEdit::RemoveEntry {
                    section: EntrySection::Headers,
                    index: 0,
                },
                FieldEdit::HeaderValue {
                    index: 0,
                    value: "20".into(),
                },
            ],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nheaders {\n  B: 20\n  C: 3\n}\n"
        );
    }

    #[test]
    fn add_then_disable_the_added_index() {
        let source = "get {\n  url: http://a\n}\n\nheaders {\n  A: 1\n  B: 2\n}\n";
        let out = apply(
            source,
            &[
                add(EntrySection::Headers, "X-Id", "1"),
                FieldEdit::HeaderEnabled {
                    index: 2,
                    enabled: false,
                },
            ],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nheaders {\n  A: 1\n  B: 2\n  ~X-Id: 1\n}\n"
        );
    }

    #[test]
    fn key_problems_are_detected() {
        let cases = [
            ("", EntryProblem::EmptyKey),
            ("X Trace", EntryProblem::KeyWhitespace),
            ("X\tTrace", EntryProblem::KeyWhitespace),
            ("a:b", EntryProblem::KeyColon),
            ("~a", EntryProblem::KeyLeadingMarker),
            ("\"a", EntryProblem::KeyLeadingMarker),
        ];
        for (key, problem) in cases {
            let error = resolve(&parse(GET), &[add(EntrySection::Headers, key, "v")])
                .expect_err("clé refusée");
            assert_eq!(
                error,
                EditError::InvalidEntry {
                    block: "headers",
                    index: None,
                    problem
                },
                "{key:?}"
            );
        }
        for key in ["a&b", "a=b", "a#b"] {
            let error = resolve(&parse(GET), &[add(EntrySection::QueryParams, key, "v")])
                .expect_err("clé refusée");
            assert!(matches!(
                error,
                EditError::InvalidEntry {
                    problem: EntryProblem::QueryKeyReserved,
                    ..
                }
            ));
        }
        // `=` et `&` restent permis dans un en-tête.
        assert!(resolve(&parse(GET), &[add(EntrySection::Headers, "a=b&c", "v")]).is_ok());
    }

    #[test]
    fn query_value_problems_are_detected() {
        for value in ["a&b", "a#b", "a\nb"] {
            let error = resolve(&parse(GET), &[add(EntrySection::QueryParams, "q", value)])
                .expect_err("valeur refusée");
            assert!(matches!(
                error,
                EditError::InvalidEntry {
                    block: "params:query",
                    problem: EntryProblem::QueryValueReserved,
                    ..
                }
            ));
        }
        let source = "get {\n  url: https://h/items?q=1\n}\n\nparams:query {\n  q: 1\n}\n";
        let error = resolve(
            &parse(source),
            &[FieldEdit::QueryParamValue {
                index: 0,
                value: "a&b".into(),
            }],
        )
        .expect_err("valeur refusée");
        assert_eq!(
            error,
            EditError::InvalidEntry {
                block: "params:query",
                index: Some(0),
                problem: EntryProblem::QueryValueReserved
            }
        );
    }

    #[test]
    fn duplicate_key_is_accepted() {
        let source = "get {\n  url: http://a\n}\n\nheaders {\n  Accept: text/plain\n}\n";
        let out = apply(
            source,
            &[add(EntrySection::Headers, "Accept", "application/json")],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nheaders {\n  Accept: text/plain\n  Accept: application/json\n}\n"
        );
    }

    #[test]
    fn url_edit_recomputes_enabled_params_and_keeps_disabled_in_place() {
        let source = "get {\n  url: https://h/items?page=2&size=10\n}\n\nparams:query {\n  page: 2\n  ~debug: 1\n  size: 10\n}\n";
        let out = apply(source, &[FieldEdit::Url("https://h/items?page=3".into())]);
        assert_eq!(
            out,
            "get {\n  url: https://h/items?page=3\n}\n\nparams:query {\n  page: 3\n  ~debug: 1\n}\n"
        );
    }

    #[test]
    fn removing_last_enabled_param_drops_query_and_block() {
        let source =
            "get {\n  url: https://h/items?page=2#top\n}\n\nparams:query {\n  page: 2\n}\n";
        let out = apply(
            source,
            &[FieldEdit::RemoveEntry {
                section: EntrySection::QueryParams,
                index: 0,
            }],
        );
        assert_eq!(out, "get {\n  url: https://h/items#top\n}\n");
    }

    #[test]
    fn header_edit_does_not_touch_inconsistent_url() {
        let source = "get {\n  url: https://h/items?a=1\n}\n\nparams:query {\n  b: 2\n}\n";
        let out = apply(source, &[add(EntrySection::Headers, "X", "1")]);
        assert_eq!(
            out,
            "get {\n  url: https://h/items?a=1\n}\n\nparams:query {\n  b: 2\n}\n\nheaders {\n  X: 1\n}\n"
        );
    }

    #[test]
    fn url_query_with_invalid_key_is_refused() {
        let error = resolve(
            &parse(GET),
            &[FieldEdit::Url("https://h/items?~a=1".into())],
        )
        .expect_err("clé refusée");
        assert!(matches!(
            error,
            EditError::InvalidEntry {
                block: "params:query",
                problem: EntryProblem::KeyLeadingMarker,
                ..
            }
        ));
    }

    #[test]
    fn block_created_at_end_of_file_without_final_newline() {
        let source = "get {\n  url: http://a\n}";
        let out = apply(source, &[add(EntrySection::Headers, "A", "1")]);
        assert_eq!(out, "get {\n  url: http://a\n}\n\nheaders {\n  A: 1\n}\n");
    }

    #[test]
    fn two_blocks_created_at_the_same_anchor_follow_bruno_order() {
        let source = "get {\n  url: http://a\n}";
        let out = apply(
            source,
            &[
                add(EntrySection::Headers, "A", "1"),
                add(EntrySection::PathParams, "id", "2"),
            ],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nparams:path {\n  id: 2\n}\n\nheaders {\n  A: 1\n}\n"
        );
    }

    #[test]
    fn block_inserted_between_two_existing_blocks() {
        let source =
            "get {\n  url: http://a?q=1\n}\n\nparams:query {\n  q: 1\n}\n\nheaders {\n  A: 1\n}\n";
        let out = apply(source, &[add(EntrySection::PathParams, "id", "42")]);
        assert_eq!(
            out,
            "get {\n  url: http://a?q=1\n}\n\nparams:query {\n  q: 1\n}\n\nparams:path {\n  id: 42\n}\n\nheaders {\n  A: 1\n}\n"
        );
    }

    #[test]
    fn untouched_empty_block_is_kept_and_filled_on_add() {
        let source = "get {\n  url: http://a\n}\n\nheaders {\n}\n";
        assert_eq!(
            apply(source, &[FieldEdit::Url("http://b".into())]),
            "get {\n  url: http://b\n}\n\nheaders {\n}\n"
        );
        assert_eq!(
            apply(source, &[add(EntrySection::Headers, "A", "1")]),
            "get {\n  url: http://a\n}\n\nheaders {\n  A: 1\n}\n"
        );
    }

    #[test]
    fn add_then_remove_is_identity() {
        let source = "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n\ntests {\n  x\n}\n";
        let out = apply(
            source,
            &[
                add(EntrySection::PathParams, "id", "1"),
                FieldEdit::RemoveEntry {
                    section: EntrySection::PathParams,
                    index: 0,
                },
            ],
        );
        assert_eq!(out, source);
    }

    #[test]
    fn removing_a_block_and_creating_the_next_one() {
        let source = "get {\n  url: http://a\n}\n\nparams:path {\n  id: 1\n}\n\ntests {\n  x\n}\n";
        let out = apply(
            source,
            &[
                FieldEdit::RemoveEntry {
                    section: EntrySection::PathParams,
                    index: 0,
                },
                add(EntrySection::Headers, "A", "1"),
            ],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nheaders {\n  A: 1\n}\n\ntests {\n  x\n}\n"
        );
    }

    #[test]
    fn crlf_is_used_for_added_lines_and_blocks() {
        let source = "get {\r\n  url: http://a\r\n}\r\n";
        let out = apply(source, &[add(EntrySection::Headers, "A", "1")]);
        assert_eq!(
            out,
            "get {\r\n  url: http://a\r\n}\r\n\r\nheaders {\r\n  A: 1\r\n}\r\n"
        );
    }
}
