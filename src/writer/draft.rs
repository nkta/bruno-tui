//! Brouillon ordonné d'une requête : application séquentielle des
//! modifications, synchronisation `params:query` ↔ URL, puis comparaison
//! à l'AST d'origine pour ne produire que les tranches à réécrire
//! (`add-entry-management`, design D2 et D3).
//!
//! Tout est pur : aucune I/O, aucun effet de bord en cas d'erreur.

use std::ops::Range;

use super::dictionary::{
    DictEntry as DraftEntry, DictSection as Section, check_key_general, dictionary_replacements,
    following_blank_line_len, preceding_blank_line_len, section_block_node,
};
use super::edit::{BODY_MODES, EntrySection, FieldEdit, Replacement};
use super::error::{EditError, EntryProblem};
use super::format;
use super::query;
use crate::collection::ast::METHODS;
use crate::collection::{BlockBody, BruFile, NodeKind};

const TEXT_BODY_BLOCKS: [(&str, &str); 5] = [
    ("json", "body:json"),
    ("text", "body:text"),
    ("xml", "body:xml"),
    ("sparql", "body:sparql"),
    ("graphql", "body:graphql"),
];
const FORM_BODY_KINDS: [&str; 3] = ["formUrlEncoded", "multipartForm", "file"];

/// Entrée `url` du bloc de méthode, valeur d'origine et valeur courante.
struct UrlState {
    span: Range<usize>,
    key: String,
    disabled: bool,
    original: String,
    current: String,
}

/// Clé `body` du bloc de méthode (`docs/edition-phase2.md` § 6).
struct BodyModeState {
    span: Option<Range<usize>>,
    insert_anchor: usize,
    original: String,
    current: String,
}

/// Nom du bloc de méthode (ex. `"get"`), tranche de ce seul mot-clé dans
/// le source (`add-method-editing`) : le bloc de méthode s'écrit toujours
/// en colonne 0, donc cette tranche est
/// `node.span.start..node.span.start + block.name.len()`.
struct MethodState {
    span: Range<usize>,
    original: String,
    current: String,
}

/// Contenu d'un bloc de corps texte ciblé.
struct BodyState {
    block_name: &'static str,
    block_span: Option<Range<usize>>,
    content_span: Option<Range<usize>>,
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextBlockKind {
    ScriptPreRequest,
    ScriptPostResponse,
    Tests,
    Docs,
    GraphqlVars,
}

impl TextBlockKind {
    pub const ALL: [Self; 5] = [
        Self::ScriptPreRequest,
        Self::ScriptPostResponse,
        Self::Tests,
        Self::Docs,
        Self::GraphqlVars,
    ];

    pub fn block_name(&self) -> &'static str {
        match self {
            Self::ScriptPreRequest => "script:pre-request",
            Self::ScriptPostResponse => "script:post-response",
            Self::Tests => "tests",
            Self::Docs => "docs",
            Self::GraphqlVars => "body:graphql:vars",
        }
    }

    pub fn position(&self) -> usize {
        match self {
            Self::ScriptPreRequest => 0,
            Self::ScriptPostResponse => 1,
            Self::Tests => 2,
            Self::Docs => 3,
            Self::GraphqlVars => 4,
        }
    }
}

/// État d'un bloc de texte libre (`script:*`, `tests`, `docs`, `body:graphql:vars`).
struct TextBlockState {
    block_span: Option<Range<usize>>,
    content_span: Option<Range<usize>>,
    new_text: Option<String>,
}

pub(crate) struct Draft<'a> {
    ast: &'a BruFile,
    url: Result<UrlState, EditError>,
    method: Result<MethodState, EditError>,
    body_mode: Result<BodyModeState, EditError>,
    sections: [Section; EntrySection::ALL.len()],
    body: Option<BodyState>,
    text_blocks: [TextBlockState; TextBlockKind::ALL.len()],
}

impl<'a> Draft<'a> {
    pub(crate) fn from_ast(ast: &'a BruFile) -> Self {
        let section = |section: EntrySection| {
            let entries = section_block_node(ast, section).map(|(_, entries)| entries);
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
        let text_block = |kind: TextBlockKind| {
            let found = ast.nodes().iter().find_map(|node| match &node.kind {
                NodeKind::Block(block) if block.name == kind.block_name() => match &block.body {
                    BlockBody::Text { content } => Some((node.span.clone(), content.clone())),
                    _ => None,
                },
                _ => None,
            });
            match found {
                Some((block_span, content_span)) => TextBlockState {
                    block_span: Some(block_span),
                    content_span: Some(content_span),
                    new_text: None,
                },
                None => TextBlockState {
                    block_span: None,
                    content_span: None,
                    new_text: None,
                },
            }
        };
        Self {
            ast,
            url: url_state(ast),
            method: method_state(ast),
            body_mode: body_mode_state(ast),
            sections: EntrySection::ALL.map(section),
            body: None,
            text_blocks: TextBlockKind::ALL.map(text_block),
        }
    }

    /// Applique une modification ; l'indice porté désigne la position dans
    /// l'état courant de la section.
    pub(crate) fn apply(&mut self, edit: &FieldEdit) -> Result<(), EditError> {
        use EntrySection::{
            Assert, Headers, PathParams, QueryParams, Settings, VarsPostResponse, VarsPreRequest,
        };
        match edit {
            FieldEdit::Url(value) => {
                let url = self.url.as_mut().map_err(|error| error.clone())?;
                url.current = value.clone();
                self.sync_params_from_url()?;
            }
            FieldEdit::Method(value) => {
                if !METHODS.contains(&value.as_str()) {
                    return Err(EditError::UnknownMethod);
                }
                let method = self.method.as_mut().map_err(|error| error.clone())?;
                method.current = value.clone();
            }
            FieldEdit::BodyMode(value) => {
                if !BODY_MODES.contains(&value.as_str()) {
                    return Err(EditError::UnknownBodyMode);
                }
                let body_mode = self.body_mode.as_mut().map_err(|error| error.clone())?;
                body_mode.current = value.clone();
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
            FieldEdit::VarsPreRequestValue { index, value } => {
                self.entry_mut(VarsPreRequest, *index)?.value = value.clone();
            }
            FieldEdit::VarsPostResponseValue { index, value } => {
                self.entry_mut(VarsPostResponse, *index)?.value = value.clone();
            }
            FieldEdit::AssertValue { index, value } => {
                self.entry_mut(Assert, *index)?.value = value.clone();
            }
            FieldEdit::SettingsValue { index, value } => {
                self.entry_mut(Settings, *index)?.value = value.clone();
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
            FieldEdit::VarsPreRequestEnabled { index, enabled } => {
                self.entry_mut(VarsPreRequest, *index)?.disabled = !enabled;
            }
            FieldEdit::VarsPostResponseEnabled { index, enabled } => {
                self.entry_mut(VarsPostResponse, *index)?.disabled = !enabled;
            }
            FieldEdit::AssertEnabled { index, enabled } => {
                self.entry_mut(Assert, *index)?.disabled = !enabled;
            }
            FieldEdit::BodyText(text) => {
                if self.body.is_none() {
                    let declared = self
                        .body_mode
                        .as_ref()
                        .map(|m| m.current.as_str())
                        .unwrap_or("none");
                    self.body = Some(body_state(self.ast, declared)?);
                }
                if let Some(body) = &mut self.body {
                    body.text = text.clone();
                }
            }
            FieldEdit::ScriptPreRequest(text) => {
                self.text_blocks[TextBlockKind::ScriptPreRequest.position()].new_text =
                    Some(text.clone());
            }
            FieldEdit::ScriptPostResponse(text) => {
                self.text_blocks[TextBlockKind::ScriptPostResponse.position()].new_text =
                    Some(text.clone());
            }
            FieldEdit::Tests(text) => {
                self.text_blocks[TextBlockKind::Tests.position()].new_text = Some(text.clone());
            }
            FieldEdit::Docs(text) => {
                self.text_blocks[TextBlockKind::Docs.position()].new_text = Some(text.clone());
            }
            FieldEdit::GraphqlVars(text) => {
                self.text_blocks[TextBlockKind::GraphqlVars.position()].new_text =
                    Some(text.clone());
            }
            FieldEdit::AddEntry {
                section,
                key,
                value,
                enabled,
            } => {
                if *section == Settings {
                    return Err(EditError::ImmutableBlock {
                        block: Settings.block_name(),
                    });
                }
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
                if *section == Settings {
                    return Err(EditError::ImmutableBlock {
                        block: Settings.block_name(),
                    });
                }
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
                if *section == Settings {
                    return Err(EditError::ImmutableBlock {
                        block: Settings.block_name(),
                    });
                }
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
        if let Ok(method) = &self.method
            && method.current != method.original
        {
            replacements.push(Replacement {
                span: method.span.clone(),
                bytes: method.current.clone(),
            });
        }
        if let Ok(body_mode) = &self.body_mode
            && body_mode.current != body_mode.original
        {
            match &body_mode.span {
                Some(span) => {
                    replacements.push(Replacement {
                        span: span.clone(),
                        bytes: format!("  body: {}{eol}", body_mode.current),
                    });
                }
                None => {
                    if body_mode.current != "none" {
                        replacements.push(Replacement {
                            span: body_mode.insert_anchor..body_mode.insert_anchor,
                            bytes: format!("  body: {}{eol}", body_mode.current),
                        });
                    }
                }
            }
        }
        let mut prefixed_anchors: Vec<usize> = Vec::new();

        if let Some(body) = &self.body {
            let anchor = if body.block_span.is_none() && !body.text.trim().is_empty() {
                canonical_creation_anchor(self.ast, body.block_name)
            } else {
                0
            };
            let separate = !prefixed_anchors.contains(&anchor);
            if body.block_span.is_none() && !body.text.trim().is_empty() && separate {
                prefixed_anchors.push(anchor);
            }
            replacements.extend(render_text_block(
                raw,
                eol,
                TextBlockTarget {
                    block_name: body.block_name,
                    block_span: body.block_span.as_ref(),
                    content_span: body.content_span.as_ref(),
                },
                Some(&body.text),
                anchor,
                separate,
            ));
        }

        for section in EntrySection::ALL {
            let state = &self.sections[section.position()];
            let block = section_block_node(self.ast, section);
            let anchor = if block.is_none() && !state.entries.is_empty() {
                canonical_creation_anchor(self.ast, section.block_name())
            } else {
                0
            };
            let separate = !prefixed_anchors.contains(&anchor);
            if block.is_none() && !state.entries.is_empty() && separate {
                prefixed_anchors.push(anchor);
            }
            replacements.extend(dictionary_replacements(
                raw,
                eol,
                section.block_name(),
                block,
                state,
                anchor,
                separate,
            ));
        }

        for kind in TextBlockKind::ALL {
            let state = &self.text_blocks[kind.position()];
            let anchor = if state.block_span.is_none()
                && state
                    .new_text
                    .as_ref()
                    .is_some_and(|t| !t.trim().is_empty())
            {
                canonical_creation_anchor(self.ast, kind.block_name())
            } else {
                0
            };
            let separate = !prefixed_anchors.contains(&anchor);
            if state.block_span.is_none()
                && state
                    .new_text
                    .as_ref()
                    .is_some_and(|t| !t.trim().is_empty())
                && separate
            {
                prefixed_anchors.push(anchor);
            }
            replacements.extend(text_block_replacements(
                raw, eol, kind, state, anchor, separate,
            ));
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

/// Cible d'un bloc de texte (`body:*`, `script:*`, `tests`, `docs`).
struct TextBlockTarget<'a> {
    block_name: &'a str,
    block_span: Option<&'a Range<usize>>,
    content_span: Option<&'a Range<usize>>,
}

/// Émet les remplacements pour un bloc de texte libre (`script:*`, `tests`, `docs`) :
/// modification, suppression ou création.
fn text_block_replacements(
    raw: &str,
    eol: &str,
    kind: TextBlockKind,
    state: &TextBlockState,
    create_anchor: usize,
    separate_from_previous: bool,
) -> Vec<Replacement> {
    render_text_block(
        raw,
        eol,
        TextBlockTarget {
            block_name: kind.block_name(),
            block_span: state.block_span.as_ref(),
            content_span: state.content_span.as_ref(),
        },
        state.new_text.as_deref(),
        create_anchor,
        separate_from_previous,
    )
}

/// Émet les remplacements pour un bloc de texte (`body:*`, `script:*`, `tests`, `docs`) :
/// modification, suppression ou création.
fn render_text_block(
    raw: &str,
    eol: &str,
    target: TextBlockTarget<'_>,
    new_text: Option<&str>,
    create_anchor: usize,
    separate_from_previous: bool,
) -> Vec<Replacement> {
    let mut replacements = Vec::new();
    let Some(new_text) = new_text else {
        return replacements;
    };

    let is_empty = new_text.trim().is_empty();

    match target.block_span {
        Some(span) => {
            if is_empty {
                let preceding = preceding_blank_line_len(raw, span.start);
                let start = span.start - preceding;
                let end = if preceding == 0 && start == 0 {
                    span.end + following_blank_line_len(raw, span.end)
                } else {
                    span.end
                };
                replacements.push(Replacement {
                    span: start..end,
                    bytes: String::new(),
                });
            } else if let Some(content_span) = target.content_span {
                replacements.push(Replacement {
                    span: content_span.clone(),
                    bytes: format::format_body_content(new_text, eol),
                });
            }
        }
        None => {
            if !is_empty {
                let mut bytes = String::new();
                let anything_before = !raw[..create_anchor].is_empty();
                if separate_from_previous
                    && anything_before
                    && !raw[..create_anchor].ends_with('\n')
                {
                    bytes.push_str(eol);
                }
                if anything_before {
                    bytes.push_str(eol);
                }
                bytes.push_str(target.block_name);
                bytes.push_str(" {");
                bytes.push_str(eol);
                bytes.push_str(&format::format_body_content(new_text, eol));
                bytes.push('}');
                bytes.push_str(eol);
                if !raw[create_anchor..].is_empty()
                    && following_blank_line_len(raw, create_anchor) == 0
                {
                    bytes.push_str(eol);
                }
                replacements.push(Replacement {
                    span: create_anchor..create_anchor,
                    bytes,
                });
            }
        }
    }
    replacements
}

/// Rang d'ordre canonique des blocs de Bruno (`jsonToBru.js`).
fn canonical_block_rank(name: &str) -> Option<usize> {
    if name == "meta" {
        return Some(0);
    }
    if METHODS.contains(&name) || name == "http" || name == "grpc" || name == "ws" {
        return Some(1);
    }
    if name == "params:query" {
        return Some(2);
    }
    if name == "params:path" {
        return Some(3);
    }
    if name == "headers" {
        return Some(4);
    }
    if name == "metadata" {
        return Some(5);
    }
    if name.starts_with("auth:") || name == "auth" {
        return Some(6);
    }
    if name == "body:json" {
        return Some(7);
    }
    if name == "body:text" {
        return Some(8);
    }
    if name == "body:xml" {
        return Some(9);
    }
    if name == "body:sparql" {
        return Some(10);
    }
    if name == "body:form-urlencoded" {
        return Some(11);
    }
    if name == "body:multipart-form" {
        return Some(12);
    }
    if name == "body:file" {
        return Some(13);
    }
    if name == "body:graphql" {
        return Some(14);
    }
    if name == "body:graphql:vars" {
        return Some(15);
    }
    if name.starts_with("body:grpc") {
        return Some(16);
    }
    if name.starts_with("body:ws") {
        return Some(17);
    }
    if name.starts_with("body:") || name == "body" {
        return Some(18);
    }
    // `vars` est l'ancien nom de `vars:pre-request`, conservé tel quel.
    if name == "vars:pre-request" || name == "vars" {
        return Some(19);
    }
    if name == "vars:post-response" {
        return Some(20);
    }
    if name == "assert" {
        return Some(21);
    }
    if name == "script:pre-request" {
        return Some(22);
    }
    if name == "script:post-response" {
        return Some(23);
    }
    if name.starts_with("script:grpc:") || name == "script:grpc" {
        return Some(24);
    }
    if name == "tests" {
        return Some(25);
    }
    if name == "app" {
        return Some(26);
    }
    if name == "settings" {
        return Some(27);
    }
    if name == "docs" {
        return Some(28);
    }
    if name == "example" {
        return Some(29);
    }
    None
}

/// Point d'insertion d'un bloc absent selon l'ordre canonique de Bruno :
/// fin du dernier bloc présent parmi ses prédécesseurs, sinon début du
/// premier successeur présent, sinon fin du fichier.
fn canonical_creation_anchor(ast: &BruFile, target_name: &str) -> usize {
    let target_rank = match canonical_block_rank(target_name) {
        Some(rank) => rank,
        None => return ast.raw().len(),
    };

    let mut last_predecessor_end = None;
    let mut first_successor_start = None;

    for node in ast.nodes() {
        if let NodeKind::Block(block) = &node.kind
            && let Some(rank) = canonical_block_rank(&block.name)
        {
            if rank <= target_rank {
                last_predecessor_end = Some(node.span.end);
            } else if first_successor_start.is_none() {
                first_successor_start = Some(node.span.start);
            }
        }
    }

    if let Some(end) = last_predecessor_end {
        end
    } else if let Some(start) = first_successor_start {
        start
    } else {
        ast.raw().len()
    }
}
/// Règles générales (`check_key_general`, `dictionary.rs`) plus la règle
/// propre aux paramètres de requête (caractères réservés d'une chaîne de
/// requête dans la clé) et à assert (espaces autorisés).
fn check_key(section: EntrySection, key: &str, index: Option<usize>) -> Result<(), EditError> {
    if section == EntrySection::Assert {
        let problem = if key.is_empty() {
            Some(EntryProblem::EmptyKey)
        } else if key.contains(['\n', '\r']) {
            Some(EntryProblem::KeyWhitespace)
        } else if key.contains(':') {
            Some(EntryProblem::KeyColon)
        } else if key.starts_with(['~', '"']) {
            Some(EntryProblem::KeyLeadingMarker)
        } else {
            None
        };
        if let Some(problem) = problem {
            return Err(EditError::InvalidEntry {
                block: section.block_name(),
                index,
                problem,
            });
        }
        return Ok(());
    }

    check_key_general(key, section.block_name(), index)?;
    if section == EntrySection::QueryParams && key.contains(['&', '=', '#']) {
        return Err(EditError::InvalidEntry {
            block: section.block_name(),
            index,
            problem: EntryProblem::QueryKeyReserved,
        });
    }
    Ok(())
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

fn method_state(ast: &BruFile) -> Result<MethodState, EditError> {
    let (name, start) = ast
        .nodes()
        .iter()
        .find_map(|node| match &node.kind {
            NodeKind::Block(block) if METHODS.contains(&block.name.as_str()) => {
                Some((block.name.clone(), node.span.start))
            }
            _ => None,
        })
        .ok_or(EditError::MissingField { field: "method" })?;
    let span = start..start + name.len();
    Ok(MethodState {
        span,
        original: name.clone(),
        current: name,
    })
}

fn body_mode_state(ast: &BruFile) -> Result<BodyModeState, EditError> {
    let method = ast
        .blocks()
        .find(|block| METHODS.contains(&block.name.as_str()))
        .ok_or(EditError::MissingField { field: "method" })?;
    let BlockBody::Dictionary(entries) = &method.body else {
        return Err(EditError::MissingField { field: "body" });
    };
    let insert_anchor = entries
        .iter()
        .find(|entry| entry.key == "url")
        .map(|entry| entry.span.end)
        .unwrap_or(0);
    let existing = entries.iter().find(|entry| entry.key == "body");
    let (span, original, current) = match existing {
        Some(entry) => (
            Some(entry.span.clone()),
            entry.value.clone(),
            entry.value.clone(),
        ),
        None => (None, "none".to_owned(), "none".to_owned()),
    };
    Ok(BodyModeState {
        span,
        insert_anchor,
        original,
        current,
    })
}

fn body_state(ast: &BruFile, declared: &str) -> Result<BodyState, EditError> {
    if FORM_BODY_KINDS.contains(&declared) {
        return Err(EditError::WrongBodyForm {
            expected: "body:text",
        });
    }
    let block_name = TEXT_BODY_BLOCKS
        .iter()
        .find(|(kind, _)| *kind == declared)
        .map(|(_, block)| *block)
        .or_else(|| {
            TEXT_BODY_BLOCKS.iter().find_map(|(_, block)| {
                if ast.block(block).is_some() {
                    Some(*block)
                } else {
                    None
                }
            })
        })
        .ok_or(EditError::NoSuchBlock { block: "body" })?;
    let found = ast.nodes().iter().find_map(|node| match &node.kind {
        NodeKind::Block(block) if block.name == block_name => match &block.body {
            BlockBody::Text { content } => Some((node.span.clone(), content.clone())),
            _ => None,
        },
        _ => None,
    });
    let (block_span, content_span) = match found {
        Some((b, c)) => (Some(b), Some(c)),
        None => (None, None),
    };
    Ok(BodyState {
        block_name,
        block_span,
        content_span,
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
    fn method_rename_keeps_the_rest_of_the_block_untouched() {
        let source = "get {\n  url: https://h/items\n  auth: none\n}\n";
        let out = apply(source, &[FieldEdit::Method("post".into())]);
        assert_eq!(out, "post {\n  url: https://h/items\n  auth: none\n}\n");
    }

    #[test]
    fn unknown_method_is_refused() {
        let file = parse(GET);
        let error = resolve(&file, &[FieldEdit::Method("fetch".into())]).unwrap_err();
        assert_eq!(error, EditError::UnknownMethod);
    }

    #[test]
    fn method_state_span_covers_only_the_block_keyword() {
        let file = parse(GET);
        let state = method_state(&file).expect("état de méthode");
        assert_eq!(state.original, "get");
        assert_eq!(&file.raw()[state.span.clone()], "get");
    }

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

    #[test]
    fn vars_pre_and_post_edits_and_block_creation_and_removal() {
        let source = "get {\n  url: http://a\n}\n";
        // Ajout vars:pre-request et vars:post-response
        let with_vars = apply(
            source,
            &[
                add(EntrySection::VarsPreRequest, "v1", "foo"),
                add(EntrySection::VarsPostResponse, "v2", "bar"),
            ],
        );
        assert_eq!(
            with_vars,
            "get {\n  url: http://a\n}\n\nvars:pre-request {\n  v1: foo\n}\n\nvars:post-response {\n  v2: bar\n}\n"
        );

        // Modification de valeur et désactivation
        let modified = apply(
            &with_vars,
            &[
                FieldEdit::VarsPreRequestValue {
                    index: 0,
                    value: "foo2".into(),
                },
                FieldEdit::VarsPostResponseEnabled {
                    index: 0,
                    enabled: false,
                },
            ],
        );
        assert_eq!(
            modified,
            "get {\n  url: http://a\n}\n\nvars:pre-request {\n  v1: foo2\n}\n\nvars:post-response {\n  ~v2: bar\n}\n"
        );

        // Suppression de l'entrée -> le bloc vide est retiré
        let removed = apply(
            &modified,
            &[FieldEdit::RemoveEntry {
                section: EntrySection::VarsPreRequest,
                index: 0,
            }],
        );
        assert_eq!(
            removed,
            "get {\n  url: http://a\n}\n\nvars:post-response {\n  ~v2: bar\n}\n"
        );
    }

    #[test]
    fn legacy_vars_block_is_preserved_when_modified() {
        let source = "get {\n  url: http://a\n}\n\nvars {\n  old_key: 123\n}\n";
        let out = apply(
            source,
            &[
                FieldEdit::VarsPreRequestValue {
                    index: 0,
                    value: "456".into(),
                },
                add(EntrySection::VarsPreRequest, "new_key", "789"),
            ],
        );
        // Le bloc d'origine s'appelait `vars`, le nom `vars` est conservé
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nvars {\n  old_key: 456\n  new_key: 789\n}\n"
        );
    }

    #[test]
    fn edit_existing_text_block_is_reindented() {
        let source = "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n\nscript:pre-request {\n  console.log(1);\n}\n";
        let out = apply(
            source,
            &[FieldEdit::ScriptPreRequest(
                "console.log(2);\nconst x = 3;".into(),
            )],
        );
        assert_eq!(
            out,
            "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n\nscript:pre-request {\n  console.log(2);\n  const x = 3;\n}\n"
        );
    }

    #[test]
    fn remove_text_block_when_empty() {
        let source = "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n\ntests {\n  test(\"ok\", () => {});\n}\n\ndocs {\n  Documentation\n}\n";
        let out = apply(source, &[FieldEdit::Tests("".into())]);
        assert_eq!(
            out,
            "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n\ndocs {\n  Documentation\n}\n"
        );
    }

    #[test]
    fn remove_text_block_when_whitespace_only() {
        let source = "get {\n  url: http://a\n}\n\ndocs {\n  doc\n}\n";
        let out = apply(source, &[FieldEdit::Docs("   \n  \t  \n".into())]);
        assert_eq!(out, "get {\n  url: http://a\n}\n");
    }

    #[test]
    fn create_absent_text_blocks_in_canonical_order() {
        let source = "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n";
        let out = apply(
            source,
            &[
                FieldEdit::Docs("Documentation finale".into()),
                FieldEdit::ScriptPreRequest("req.setHeader(\"x\", 1);".into()),
                FieldEdit::Tests("test(\"ok\", () => {});".into()),
            ],
        );
        assert_eq!(
            out,
            "meta {\n  name: a\n}\n\nget {\n  url: http://a\n}\n\nscript:pre-request {\n  req.setHeader(\"x\", 1);\n}\n\ntests {\n  test(\"ok\", () => {});\n}\n\ndocs {\n  Documentation finale\n}\n"
        );
    }

    #[test]
    fn empty_text_on_absent_block_does_not_create() {
        let source = "get {\n  url: http://a\n}\n";
        let out = apply(source, &[FieldEdit::ScriptPreRequest("".into())]);
        assert_eq!(out, source);
    }

    #[test]
    fn create_docs_on_folder_bru() {
        let source = "meta {\n  name: Mon Dossier\n}\n";
        let out = apply(source, &[FieldEdit::Docs("# Dossier\nExplication".into())]);
        assert_eq!(
            out,
            "meta {\n  name: Mon Dossier\n}\n\ndocs {\n  # Dossier\n  Explication\n}\n"
        );
    }

    #[test]
    fn assert_allows_spaces_in_key_and_refuses_colon_or_leading_tilde() {
        let source = "get {\n  url: http://a\n}\n";
        // Assert autorise les espaces dans l'expression
        let out = apply(
            source,
            &[add(EntrySection::Assert, "res.status == 200", "true")],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nassert {\n  res.status == 200: true\n}\n"
        );

        // Refuse les clés avec `:` ou `~` en tête
        let file = parse(source);
        let err_colon = resolve(&file, &[add(EntrySection::Assert, "res:status", "true")])
            .expect_err("colon refusé");
        assert!(matches!(
            err_colon,
            EditError::InvalidEntry {
                problem: EntryProblem::KeyColon,
                ..
            }
        ));

        let err_tilde = resolve(&file, &[add(EntrySection::Assert, "~res.status", "true")])
            .expect_err("tilde refusé");
        assert!(matches!(
            err_tilde,
            EditError::InvalidEntry {
                problem: EntryProblem::KeyLeadingMarker,
                ..
            }
        ));
    }

    #[test]
    fn settings_value_edit_is_allowed_but_structural_edits_are_refused() {
        let source = "get {\n  url: http://a\n}\n\nsettings {\n  encodeUrl: true\n}\n";
        // Modification de valeur permise
        let out = apply(
            source,
            &[FieldEdit::SettingsValue {
                index: 0,
                value: "false".into(),
            }],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nsettings {\n  encodeUrl: false\n}\n"
        );

        let file = parse(source);
        // Ajout refusé
        let err_add = resolve(&file, &[add(EntrySection::Settings, "timeout", "1000")])
            .expect_err("ajout refusé");
        assert_eq!(err_add, EditError::ImmutableBlock { block: "settings" });

        // Suppression refusée
        let err_remove = resolve(
            &file,
            &[FieldEdit::RemoveEntry {
                section: EntrySection::Settings,
                index: 0,
            }],
        )
        .expect_err("suppression refusée");
        assert_eq!(err_remove, EditError::ImmutableBlock { block: "settings" });

        // Renommage refusé
        let err_rename = resolve(
            &file,
            &[FieldEdit::RenameKey {
                section: EntrySection::Settings,
                index: 0,
                key: "newKey".into(),
            }],
        )
        .expect_err("renommage refusé");
        assert_eq!(err_rename, EditError::ImmutableBlock { block: "settings" });
    }

    #[test]
    fn creation_inserts_all_sections_in_canonical_bruno_order() {
        let source = "get {\n  url: http://a\n}\n";
        let out = apply(
            source,
            &[
                add(EntrySection::Assert, "res.status", "eq 200"),
                add(EntrySection::VarsPostResponse, "v2", "b"),
                add(EntrySection::VarsPreRequest, "v1", "a"),
                add(EntrySection::Headers, "H", "1"),
                add(EntrySection::PathParams, "p", "2"),
                add(EntrySection::QueryParams, "q", "3"),
            ],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a?q=3\n}\n\nparams:query {\n  q: 3\n}\n\nparams:path {\n  p: 2\n}\n\nheaders {\n  H: 1\n}\n\nvars:pre-request {\n  v1: a\n}\n\nvars:post-response {\n  v2: b\n}\n\nassert {\n  res.status: eq 200\n}\n"
        );
    }

    #[test]
    fn text_block_with_braces_and_closing_syntax() {
        let source = "get {\n  url: http://a\n}\n";
        let script = "function test() {\n  if (true) {\n    return {};\n  }\n}";
        let out = apply(source, &[FieldEdit::ScriptPostResponse(script.into())]);
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nscript:post-response {\n  function test() {\n    if (true) {\n      return {};\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn key_value_and_text_blocks_share_the_canonical_order() {
        let source = "get {\n  url: http://a\n}\n\nsettings {\n  encodeUrl: true\n}\n";
        let out = apply(
            source,
            &[
                FieldEdit::Docs("doc".into()),
                FieldEdit::Tests("test();".into()),
                add(EntrySection::Assert, "res.status", "eq 200"),
                FieldEdit::ScriptPreRequest("pre();".into()),
                add(EntrySection::VarsPostResponse, "v2", "b"),
                add(EntrySection::Headers, "H", "1"),
            ],
        );
        assert_eq!(
            out,
            "get {\n  url: http://a\n}\n\nheaders {\n  H: 1\n}\n\nvars:post-response {\n  v2: b\n}\n\nassert {\n  res.status: eq 200\n}\n\nscript:pre-request {\n  pre();\n}\n\ntests {\n  test();\n}\n\nsettings {\n  encodeUrl: true\n}\n\ndocs {\n  doc\n}\n"
        );
    }

    #[test]
    fn body_mode_replaces_existing_mode_and_preserves_other_blocks() {
        let source =
            "post {\n  url: http://a\n  body: json\n  auth: none\n}\n\nbody:json {\n  {}\n}\n";
        let out = apply(source, &[FieldEdit::BodyMode("formUrlEncoded".into())]);
        assert_eq!(
            out,
            "post {\n  url: http://a\n  body: formUrlEncoded\n  auth: none\n}\n\nbody:json {\n  {}\n}\n"
        );
    }

    #[test]
    fn body_mode_inserts_after_url_when_absent() {
        let source = "get {\n  url: http://a\n  auth: none\n}\n";
        let out = apply(source, &[FieldEdit::BodyMode("json".into())]);
        assert_eq!(
            out,
            "get {\n  url: http://a\n  body: json\n  auth: none\n}\n"
        );
    }

    #[test]
    fn body_mode_none_replaces_existing_mode() {
        let source = "post {\n  url: http://a\n  body: json\n}\n";
        let out = apply(source, &[FieldEdit::BodyMode("none".into())]);
        assert_eq!(out, "post {\n  url: http://a\n  body: none\n}\n");
    }

    #[test]
    fn body_mode_unknown_is_refused() {
        let file = parse(GET);
        let error = resolve(&file, &[FieldEdit::BodyMode("yaml".into())]).unwrap_err();
        assert_eq!(error, EditError::UnknownBodyMode);
    }

    #[test]
    fn body_text_creates_missing_block_at_canonical_anchor() {
        let source = "post {\n  url: http://a\n  body: json\n}\n\nheaders {\n  Content-Type: application/json\n}\n";
        let out = apply(source, &[FieldEdit::BodyText("{\n  \"a\": 1\n}".into())]);
        assert_eq!(
            out,
            "post {\n  url: http://a\n  body: json\n}\n\nheaders {\n  Content-Type: application/json\n}\n\nbody:json {\n  {\n    \"a\": 1\n  }\n}\n"
        );
    }

    #[test]
    fn body_text_removes_cleared_block() {
        let source =
            "post {\n  url: http://a\n  body: json\n}\n\nbody:json {\n  {\n    \"a\": 1\n  }\n}\n";
        let out = apply(source, &[FieldEdit::BodyText("".into())]);
        assert_eq!(out, "post {\n  url: http://a\n  body: json\n}\n");
    }

    #[test]
    fn body_text_does_not_create_block_if_empty() {
        let source = "post {\n  url: http://a\n  body: json\n}\n";
        let out = apply(source, &[FieldEdit::BodyText("   \n  ".into())]);
        assert_eq!(out, "post {\n  url: http://a\n  body: json\n}\n");
    }

    #[test]
    fn body_text_creates_block_alongside_body_mode_change() {
        let source = "post {\n  url: http://a\n}\n";
        let out = apply(
            source,
            &[
                FieldEdit::BodyMode("text".into()),
                FieldEdit::BodyText("hello world".into()),
            ],
        );
        assert_eq!(
            out,
            "post {\n  url: http://a\n  body: text\n}\n\nbody:text {\n  hello world\n}\n"
        );
    }

    #[test]
    fn graphql_vars_modifies_existing_block() {
        let source = "post {\n  url: http://a\n  body: graphql\n}\n\nbody:graphql {\n  query\n}\n\nbody:graphql:vars {\n  {\n    \"id\": 1\n  }\n}\n";
        let out = apply(
            source,
            &[FieldEdit::GraphqlVars("{\n  \"id\": 2\n}".into())],
        );
        assert_eq!(
            out,
            "post {\n  url: http://a\n  body: graphql\n}\n\nbody:graphql {\n  query\n}\n\nbody:graphql:vars {\n  {\n    \"id\": 2\n  }\n}\n"
        );
    }

    #[test]
    fn graphql_vars_creates_missing_block_at_canonical_anchor() {
        let source = "post {\n  url: http://a\n  body: graphql\n}\n\nbody:graphql {\n  query\n}\n";
        let out = apply(
            source,
            &[FieldEdit::GraphqlVars("{\n  \"id\": 1\n}".into())],
        );
        assert_eq!(
            out,
            "post {\n  url: http://a\n  body: graphql\n}\n\nbody:graphql {\n  query\n}\n\nbody:graphql:vars {\n  {\n    \"id\": 1\n  }\n}\n"
        );
    }

    #[test]
    fn graphql_vars_removes_cleared_block() {
        let source = "post {\n  url: http://a\n  body: graphql\n}\n\nbody:graphql {\n  query\n}\n\nbody:graphql:vars {\n  {\n    \"id\": 1\n  }\n}\n";
        let out = apply(source, &[FieldEdit::GraphqlVars("".into())]);
        assert_eq!(
            out,
            "post {\n  url: http://a\n  body: graphql\n}\n\nbody:graphql {\n  query\n}\n"
        );
    }
}
