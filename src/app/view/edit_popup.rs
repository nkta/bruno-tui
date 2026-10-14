//! Popup d'édition d'un onglet ou de la requête (`docs/edition-popups.md`).
//!
//! Le contenu du popup est décrit par [`edit_popup_layout`], fonction pure
//! partagée par le rendu, le défilement (`update::edit`) et la souris :
//! chaque position du curseur de champ y a ses lignes, la saisie en cours
//! sa colonne. Les valeurs sont celles de l'aperçu de la session, telles
//! qu'elles seraient écrites : aucune variable n'est résolue.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::detail::skip_columns;
use super::theme;
use crate::app::model::{
    EditScope, EditSession, EditState, EditableField, InputTarget, PickerKind, RequestTab,
    field_enabled, field_value, section_entries,
};
use crate::app::text_input::TextInput;
use crate::collection::auth::{is_sensitive_auth_key, mask_auth_value};
use crate::collection::{BodyContent, BodyKind};
use crate::writer::EntrySection;

/// Mise en valeur de la ligne sous le curseur de champ, et du choix
/// présélectionné d'une liste.
pub const CURSOR_STYLE: Style = Style::new().add_modifier(Modifier::REVERSED);

/// Ligne désactivée d'un tableau : grisée, suivie de « (désactivé) ».
pub const DISABLED_STYLE: Style = Style::new().add_modifier(Modifier::DIM.union(Modifier::ITALIC));

/// Valeur en lecture seule (entrée `@file(...)`, corps `file`).
const READ_ONLY_STYLE: Style = Style::new()
    .fg(Color::Rgb(150, 150, 155))
    .add_modifier(Modifier::ITALIC);

/// Question du secret en clair, dans le bas du popup.
const WARNING_STYLE: Style = Style::new().fg(Color::Yellow);

/// Séparateur des colonnes Clé et Valeur, comme le popup d'environnement.
const SEPARATOR: &str = " │ ";

/// Largeur minimale de la colonne des libellés (Méthode, URL, Mode, clés
/// d'auth).
const LABEL_WIDTH: usize = 10;

/// Lignes montrées d'un script, des tests ou de la documentation, édités
/// dans l'éditeur externe.
const PREVIEW_LINES: usize = 6;

/// Lignes occupées par une position du curseur de champ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldRows {
    pub field: EditableField,
    pub line: usize,
    pub count: usize,
}

/// Contenu du popup, avant découpage par le défilement.
#[derive(Debug, Default)]
pub struct PopupContent {
    pub lines: Vec<Line<'static>>,
    pub fields: Vec<FieldRows>,
    /// Ligne et colonne d'affichage du curseur de texte pendant une saisie.
    pub text_cursor: Option<(usize, usize)>,
    /// Colonne où commence le texte en saisie : la place restante sert au
    /// décalage horizontal.
    pub input_prefix: usize,
    /// Ligne à garder visible : champ sous le curseur, ligne du curseur
    /// de texte ou choix présélectionné.
    pub focus_line: usize,
}

impl PopupContent {
    /// Position du curseur de champ affichée à la ligne `line`.
    pub fn field_at_line(&self, line: usize) -> Option<EditableField> {
        self.fields
            .iter()
            .find(|rows| (rows.line..rows.line + rows.count).contains(&line))
            .map(|rows| rows.field)
    }
}

/// Zones et contenu du popup pour un écran donné.
#[derive(Debug)]
pub struct EditPopupLayout {
    /// Cadre complet, bordure comprise.
    pub area: Rect,
    /// Zone du contenu défilant.
    pub body: Rect,
    /// Zone des lignes d'aide, en bas du cadre.
    pub help: Rect,
    pub content: PopupContent,
    pub help_lines: Vec<Line<'static>>,
    /// Défilement effectif : celui de la session, borné au contenu.
    pub scroll: usize,
}

/// Calcule le popup de `session` centré sur `screen` : au plus 100
/// colonnes, hauteur ajustée au contenu dans la limite de l'écran.
pub fn edit_popup_layout(screen: Rect, session: &EditSession) -> EditPopupLayout {
    let content = popup_content(session);
    let width = screen
        .width
        .saturating_sub(8)
        .min(100)
        .max(screen.width.min(40));
    let inner_width = usize::from(width.saturating_sub(2));
    let help_lines = help_lines(session, inner_width);
    let wanted = content.lines.len().max(1) + help_lines.len() + 2;
    let height = u16::try_from(wanted)
        .unwrap_or(u16::MAX)
        .min(screen.height.saturating_sub(2))
        .max(screen.height.min(5));
    let area = Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let inner = super::inner(area);
    let help_height = u16::try_from(help_lines.len())
        .unwrap_or(u16::MAX)
        .min(inner.height.saturating_sub(1));
    let body = Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(help_height),
    );
    let help = Rect::new(inner.x, body.bottom(), inner.width, help_height);
    let max_scroll = content.lines.len().saturating_sub(usize::from(body.height));
    let scroll = session.scroll.min(max_scroll);
    EditPopupLayout {
        area,
        body,
        help,
        content,
        help_lines,
        scroll,
    }
}

/// Cadre du popup de `session` sur `screen`.
pub fn edit_popup_area(screen: Rect, session: &EditSession) -> Rect {
    edit_popup_layout(screen, session).area
}

/// Titre du popup : portée, fichier édité et indicateur de modification
/// non enregistrée.
pub fn edit_popup_title(session: &EditSession) -> String {
    let file = session
        .path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unsaved = if session.has_unsaved() {
        " ● non enregistré"
    } else {
        ""
    };
    format!(" {} — {file}{unsaved} ", session.scope.title())
}

/// Dessine le popup d'édition par-dessus l'écran.
pub fn render_edit_popup(session: &EditSession, frame: &mut Frame) {
    let layout = edit_popup_layout(frame.area(), session);
    frame.render_widget(Clear, layout.area);
    let block = Block::bordered()
        .title(edit_popup_title(session))
        .border_type(BorderType::Rounded)
        .border_style(theme::FOCUS)
        .style(theme::BACKGROUND);
    frame.render_widget(block, layout.area);
    if layout.body.width == 0 || layout.body.height == 0 {
        return;
    }
    let visible: Vec<Line<'static>> = layout
        .content
        .lines
        .iter()
        .skip(layout.scroll)
        .take(usize::from(layout.body.height))
        .cloned()
        .collect();
    frame.render_widget(Paragraph::new(visible), layout.body);
    frame.render_widget(Paragraph::new(layout.help_lines.clone()), layout.help);

    if let Some((line, column)) = layout.content.text_cursor
        && line >= layout.scroll
        && line < layout.scroll + usize::from(layout.body.height)
    {
        let row = u16::try_from(line - layout.scroll).unwrap_or(0);
        let column = u16::try_from(column)
            .unwrap_or(u16::MAX)
            .min(layout.body.width.saturating_sub(1));
        frame.set_cursor_position((layout.body.x + column, layout.body.y + row));
    }
}

/// Lignes du popup de `session`, sans défilement.
pub fn popup_content(session: &EditSession) -> PopupContent {
    let mut builder = Builder {
        session,
        content: PopupContent::default(),
    };
    match session.scope {
        EditScope::Request => builder.request(),
        EditScope::Tab(RequestTab::Headers) => builder.table(EntrySection::Headers, None),
        EditScope::Tab(RequestTab::Params) => {
            builder.table(EntrySection::QueryParams, Some("Paramètres de requête"));
            builder.blank();
            builder.table(EntrySection::PathParams, Some("Paramètres de chemin"));
        }
        EditScope::Tab(RequestTab::Vars) => {
            builder.table(EntrySection::VarsPreRequest, Some("Pré-requête"));
            builder.blank();
            builder.table(EntrySection::VarsPostResponse, Some("Post-réponse"));
        }
        EditScope::Tab(RequestTab::Assert) => builder.table(EntrySection::Assert, None),
        EditScope::Tab(RequestTab::Settings) => builder.table(EntrySection::Settings, None),
        EditScope::Tab(RequestTab::Auth) => builder.auth(),
        EditScope::Tab(RequestTab::Body) => builder.body(),
        EditScope::Tab(RequestTab::Script) => {
            builder.title("Pré-requête");
            builder.text_preview(EditableField::ScriptPreRequest);
            builder.blank();
            builder.title("Post-réponse");
            builder.text_preview(EditableField::ScriptPostResponse);
        }
        EditScope::Tab(RequestTab::Tests) => builder.text_preview(EditableField::Tests),
        EditScope::Tab(RequestTab::Docs) => builder.text_preview(EditableField::Docs),
    }
    builder.secret_prompt();
    builder.content
}

/// Construit le contenu ligne à ligne.
struct Builder<'a> {
    session: &'a EditSession,
    content: PopupContent,
}

impl Builder<'_> {
    fn push(&mut self, line: Line<'static>) {
        self.content.lines.push(line);
    }

    fn blank(&mut self) {
        self.push(Line::default());
    }

    fn title(&mut self, text: &str) {
        self.push(Line::styled(text.to_owned(), theme::TITLE));
    }

    fn next_line(&self) -> usize {
        self.content.lines.len()
    }

    /// Un ajout est en cours : la ligne provisoire porte la mise en valeur.
    fn adding(&self) -> Option<EntrySection> {
        match (&self.session.state, &self.session.target) {
            (
                EditState::Input(_),
                InputTarget::NewKey { section, .. } | InputTarget::NewValue { section, .. },
            ) => Some(*section),
            _ => None,
        }
    }

    fn is_cursor(&self, field: EditableField) -> bool {
        self.adding().is_none() && self.session.current_field() == Some(field)
    }

    /// Saisie en cours sur la valeur de `field`.
    fn value_input(&self, field: EditableField) -> Option<&TextInput> {
        match &self.session.state {
            EditState::Input(input)
                if self.session.target == InputTarget::Field
                    && self.session.current_field() == Some(field) =>
            {
                Some(input)
            }
            _ => None,
        }
    }

    /// Ajoute les lignes de `field`, mises en valeur sous le curseur.
    fn field(&mut self, field: EditableField, lines: Vec<Line<'static>>) {
        let first = self.next_line();
        let cursor = self.is_cursor(field);
        if cursor && self.content.text_cursor.is_none() {
            self.content.focus_line = first;
        }
        let count = lines.len();
        for line in lines {
            self.push(if cursor {
                line.patch_style(CURSOR_STYLE)
            } else {
                line
            });
        }
        self.content.fields.push(FieldRows {
            field,
            line: first,
            count,
        });
    }

    /// Texte visible d'une saisie sur une ligne commençant à la colonne
    /// `prefix`, et place le curseur de texte sur la ligne suivante.
    fn input_text(&mut self, input: &TextInput, prefix: usize, masked: bool) -> String {
        let hscroll = usize::from(self.session.hscroll);
        let (text, before) = if masked {
            (
                "•".repeat(input.text().chars().count()),
                "•".repeat(input.text_before_cursor_on_line().chars().count()),
            )
        } else {
            (
                input.text().to_owned(),
                input.text_before_cursor_on_line().to_owned(),
            )
        };
        let (visible, skipped) = skip_columns(&text, hscroll);
        let line = self.next_line();
        let column = prefix + Line::raw(before).width().saturating_sub(skipped);
        self.content.text_cursor = Some((line, column));
        self.content.input_prefix = prefix;
        self.content.focus_line = line;
        visible.to_owned()
    }

    /// Choix présélectionnable d'une liste ouverte sous `field`.
    fn picker_rows(&mut self, field: EditableField) {
        let EditState::Picker { kind, selected } = self.session.state else {
            return;
        };
        if self.session.current_field() != Some(field) {
            return;
        }
        for (index, option) in kind.options().iter().enumerate() {
            let label = if kind == PickerKind::Method {
                option.to_ascii_uppercase()
            } else {
                (*option).to_owned()
            };
            let line = Line::raw(format!("{}  {label}", " ".repeat(LABEL_WIDTH)));
            if index == selected {
                self.content.focus_line = self.next_line();
                self.push(line.patch_style(CURSOR_STYLE));
            } else {
                self.push(line);
            }
        }
    }

    /// Ligne « libellé ‹ valeur › » d'un choix, suivie de sa liste ouverte.
    fn choice(&mut self, field: EditableField, label: &str, width: usize, value: Span<'static>) {
        self.choice_noted(field, label, width, value, None);
    }

    /// Comme [`Self::choice`], avec une note après la valeur.
    fn choice_noted(
        &mut self,
        field: EditableField,
        label: &str,
        width: usize,
        value: Span<'static>,
        note: Option<&'static str>,
    ) {
        let mut spans = vec![
            Span::styled(pad(label, width), theme::LABEL),
            Span::raw("‹ "),
            value,
            Span::raw(" ›"),
        ];
        if let Some(note) = note {
            spans.push(Span::styled(note, theme::EMPTY_MESSAGE));
        }
        self.field(field, vec![Line::from(spans)]);
        self.picker_rows(field);
    }

    /// Ligne « libellé valeur » d'un champ texte d'une ligne.
    fn labelled(&mut self, field: EditableField, label: &str, width: usize, masked: bool) {
        let prefix = width;
        let value = match self.value_input(field) {
            Some(input) => {
                let input = input.clone();
                Span::raw(self.input_text(&input, prefix, masked))
            }
            None => Span::raw(display_value(self.session, field)),
        };
        let line = Line::from(vec![Span::styled(pad(label, width), theme::LABEL), value]);
        self.field(field, vec![line]);
    }

    fn request(&mut self) {
        let method = field_value(self.session, &EditableField::Method).to_ascii_uppercase();
        let style = theme::method_style(&method).add_modifier(Modifier::BOLD);
        self.choice(
            EditableField::Method,
            "Méthode",
            LABEL_WIDTH,
            Span::styled(method, style),
        );
        self.labelled(EditableField::Url, "URL", LABEL_WIDTH, false);
    }

    fn auth(&mut self) {
        let params = self.session.preview.auth_params().to_vec();
        let width = params
            .iter()
            .map(|kv| Line::raw(kv.key.as_str()).width() + 1)
            .max()
            .unwrap_or(0)
            .max(LABEL_WIDTH);
        let mode = field_value(self.session, &EditableField::AuthMode).to_owned();
        // Sans mode dans le fichier, `none` est la valeur par défaut :
        // le détail dit « non déclarée », le popup le rappelle.
        let note = self
            .session
            .preview
            .auth
            .is_none()
            .then_some(" (non déclarée)");
        self.choice_noted(
            EditableField::AuthMode,
            "Mode",
            width,
            Span::styled(mode, theme::FOCUS),
            note,
        );
        if params.is_empty() {
            self.push(Line::styled("aucun paramètre", theme::EMPTY_MESSAGE));
        }
        for (index, param) in params.iter().enumerate() {
            let field = EditableField::AuthParamValue(index);
            if param.key == "placement" {
                let value = field_value(self.session, &field).to_owned();
                self.choice(field, &param.key, width, Span::raw(value));
            } else {
                let masked = is_sensitive_auth_key(&param.key) && !self.session.unmasked;
                self.labelled(field, &param.key, width, masked);
            }
        }
    }

    fn body(&mut self) {
        let preview = &self.session.preview;
        let is_graphql = preview.kind.as_deref() == Some("graphql");
        if !is_graphql {
            let mode = field_value(self.session, &EditableField::BodyMode).to_owned();
            self.choice(
                EditableField::BodyMode,
                "Mode",
                LABEL_WIDTH,
                Span::styled(mode, theme::FOCUS),
            );
        }
        let kind = preview.body.as_ref().map(|body| body.kind.clone());
        match kind {
            Some(BodyKind::Json | BodyKind::Text | BodyKind::Xml | BodyKind::Sparql) => {
                self.blank();
                self.title("Contenu");
                self.text_area(EditableField::BodyText);
            }
            Some(BodyKind::Graphql) => self.graphql(),
            None if is_graphql => self.graphql(),
            Some(BodyKind::FormUrlEncoded) => {
                self.blank();
                self.table(EntrySection::FormUrlEncoded, None);
            }
            Some(BodyKind::MultipartForm) => {
                self.blank();
                self.table(EntrySection::MultipartForm, None);
            }
            Some(BodyKind::File | BodyKind::Other(_)) => {
                self.blank();
                self.push(Line::styled(
                    "Lecture seule : ce mode ne se modifie pas ici.",
                    theme::EMPTY_MESSAGE,
                ));
                let lines: Vec<String> = match preview.body.as_ref().map(|b| &b.content) {
                    Some(BodyContent::Text(text)) => text.lines().map(str::to_owned).collect(),
                    Some(BodyContent::Entries(entries)) => entries
                        .iter()
                        .map(|kv| format!("{}{SEPARATOR}{}", kv.key, kv.value))
                        .collect(),
                    _ => Vec::new(),
                };
                for line in lines {
                    self.push(Line::styled(line, READ_ONLY_STYLE));
                }
            }
            None => {}
        }
    }

    fn graphql(&mut self) {
        if self.next_line() > 0 {
            self.blank();
        }
        self.title("Requête");
        self.text_area(EditableField::BodyText);
        self.blank();
        self.title("Variables");
        self.text_area(EditableField::GraphqlVars);
    }

    /// Zone de texte multiligne, éditée dans le popup.
    fn text_area(&mut self, field: EditableField) {
        let hscroll = usize::from(self.session.hscroll);
        let first = self.next_line();
        let lines: Vec<Line<'static>> = match self.value_input(field) {
            Some(input) => {
                let (line, _) = input.cursor_line_col();
                let column = Line::raw(input.text_before_cursor_on_line()).width();
                let rows: Vec<Line<'static>> = input
                    .text()
                    .split('\n')
                    .map(|text| {
                        let (visible, _) = skip_columns(text, hscroll);
                        Line::raw(format!("  {visible}"))
                    })
                    .collect();
                self.content.text_cursor = Some((first + line, 2 + column.saturating_sub(hscroll)));
                self.content.input_prefix = 2;
                self.content.focus_line = first + line;
                rows
            }
            None => {
                let text = field_value(self.session, &field);
                if text.is_empty() {
                    vec![Line::styled("  (vide)", theme::EMPTY_MESSAGE)]
                } else {
                    text.split('\n')
                        .map(|line| Line::raw(format!("  {line}")))
                        .collect()
                }
            }
        };
        let lines = lines
            .into_iter()
            .map(|line| line.patch_style(theme::EDITABLE_BODY))
            .collect();
        self.field(field, lines);
    }

    /// Aperçu des premières lignes d'un bloc édité dans l'éditeur externe.
    fn text_preview(&mut self, field: EditableField) {
        let text = field_value(self.session, &field);
        let mut lines: Vec<Line<'static>> = Vec::new();
        if text.trim().is_empty() {
            lines.push(Line::styled("  aucun", theme::EMPTY_MESSAGE));
        } else {
            let all: Vec<&str> = text.lines().collect();
            for line in all.iter().take(PREVIEW_LINES) {
                lines.push(Line::raw(format!("  {line}")));
            }
            if all.len() > PREVIEW_LINES {
                lines.push(Line::styled(
                    format!("  … (+{} lignes)", all.len() - PREVIEW_LINES),
                    theme::EMPTY_MESSAGE,
                ));
            }
        }
        self.field(field, lines);
    }

    /// Tableau clé / valeur d'une section, précédé de son titre s'il y en
    /// a un ; ligne d'ajout si la section est vide, ligne provisoire
    /// pendant un ajout.
    fn table(&mut self, section: EntrySection, title: Option<&str>) {
        if let Some(title) = title {
            self.title(title);
        }
        let entries = section_entries(&self.session.preview, section).to_vec();
        let adding = self.adding() == Some(section);
        let provisional_key = match (&self.session.state, &self.session.target) {
            (EditState::Input(input), InputTarget::NewKey { section: s, .. }) if *s == section => {
                Some(input.text().to_owned())
            }
            (
                EditState::Input(_),
                InputTarget::NewValue {
                    section: s, key, ..
                },
            ) if *s == section => Some(key.clone()),
            (EditState::Input(input), InputTarget::RenameKey { section: s, .. })
                if *s == section =>
            {
                Some(input.text().to_owned())
            }
            _ => None,
        };
        let key_width = entries
            .iter()
            .map(|kv| Line::raw(kv.key.as_str()).width())
            .chain(provisional_key.map(|key| Line::raw(key).width()))
            .max()
            .unwrap_or(0)
            .clamp(3, 24);

        if entries.is_empty() && !adding {
            if section == EntrySection::Settings {
                self.push(Line::styled("aucun paramètre", theme::EMPTY_MESSAGE));
            } else {
                let line = Line::styled("aucune ligne · a ajouter", theme::EMPTY_MESSAGE);
                self.field(EditableField::AddRow(section), vec![line]);
            }
            return;
        }
        self.push(Line::styled(
            format!("{}{SEPARATOR}Valeur", pad("Clé", key_width)),
            theme::LABEL,
        ));
        for (index, entry) in entries.iter().enumerate() {
            let field = EditableField::of_entry(section, index);
            let renaming = matches!(
                (&self.session.state, &self.session.target),
                (EditState::Input(_), InputTarget::RenameKey { section: s, index: i })
                    if *s == section && *i == index
            );
            let key = if renaming {
                let EditState::Input(input) = &self.session.state else {
                    continue;
                };
                let input = input.clone();
                self.input_text(&input, 0, false)
            } else {
                entry.key.clone()
            };
            let key_cell = pad(&key, key_width);
            let prefix = Line::raw(key_cell.as_str()).width() + separator_width();
            let enabled = field_enabled(self.session, &field);
            let is_file = section == EntrySection::MultipartForm
                && entry.value.trim_start().starts_with("@file(");
            let value = match self.value_input(field) {
                Some(input) => {
                    let input = input.clone();
                    Span::raw(self.input_text(&input, prefix, false))
                }
                None if !enabled => {
                    Span::styled(format!("{} (désactivé)", entry.value), DISABLED_STYLE)
                }
                None if is_file => {
                    Span::styled(format!("{} (lecture seule)", entry.value), READ_ONLY_STYLE)
                }
                None => Span::raw(entry.value.clone()),
            };
            let key_style = if enabled {
                theme::LABEL
            } else {
                DISABLED_STYLE
            };
            let line = Line::from(vec![
                Span::styled(format!("{key_cell}{SEPARATOR}"), key_style),
                value,
            ]);
            self.field(field, vec![line]);
        }
        if adding {
            self.provisional_row(section, key_width, entries.is_empty());
        }
    }

    /// Ligne de l'entrée en cours d'ajout, en fin de tableau.
    fn provisional_row(&mut self, section: EntrySection, key_width: usize, empty: bool) {
        let EditState::Input(input) = &self.session.state else {
            return;
        };
        let input = input.clone();
        let line = match &self.session.target {
            InputTarget::NewKey { .. } => {
                let key = self.input_text(&input, 0, false);
                Line::from(vec![Span::styled(
                    format!("{}{SEPARATOR}", pad(&key, key_width)),
                    theme::LABEL,
                )])
            }
            InputTarget::NewValue { key, .. } => {
                let key_cell = pad(key, key_width);
                let prefix = Line::raw(key_cell.as_str()).width() + separator_width();
                let value = self.input_text(&input, prefix, false);
                Line::from(vec![
                    Span::styled(format!("{key_cell}{SEPARATOR}"), theme::LABEL),
                    Span::raw(value),
                ])
            }
            InputTarget::Field | InputTarget::RenameKey { .. } => return,
        };
        let line = line.patch_style(CURSOR_STYLE);
        if empty {
            // La ligne d'ajout de la section vide devient la ligne provisoire.
            let first = self.next_line();
            self.push(line);
            self.content.fields.push(FieldRows {
                field: EditableField::AddRow(section),
                line: first,
                count: 1,
            });
        } else {
            self.push(line);
        }
    }

    /// Question du secret en clair, dans le bas du popup (§ 3.1).
    fn secret_prompt(&mut self) {
        let EditState::PlaintextSecretPrompt { key, var_name, .. } = &self.session.state else {
            return;
        };
        let (key, var_name) = (key.clone(), var_name.clone());
        self.blank();
        self.content.focus_line = self.next_line();
        self.push(Line::styled(
            format!(
                "Secret en clair pour « {key} » dans le fichier .bru (souvent partagé via git)."
            ),
            WARNING_STYLE,
        ));
        self.push(Line::from(vec![
            Span::styled("  Entrée  ", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(format!("remplacer par {{{{{var_name}}}}}")),
        ]));
        self.push(Line::from(vec![
            Span::styled("  g       ", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw("garder en clair"),
        ]));
        self.content.focus_line = self.next_line();
        self.push(Line::from(vec![
            Span::styled("  Échap   ", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw("revenir à la saisie"),
        ]));
    }
}

/// Valeur affichée d'un champ hors saisie : secrets d'auth masqués.
fn display_value(session: &EditSession, field: EditableField) -> String {
    let value = field_value(session, &field);
    match field {
        EditableField::AuthParamValue(index) => session
            .preview
            .auth_params()
            .get(index)
            .map_or_else(|| value.to_owned(), |kv| mask_auth_value(&kv.key, value)),
        _ => value.to_owned(),
    }
}

/// Largeur d'affichage de [`SEPARATOR`].
fn separator_width() -> usize {
    Line::raw(SEPARATOR).width()
}

/// `text` complété d'espaces jusqu'à `width` colonnes d'affichage.
fn pad(text: &str, width: usize) -> String {
    let used = Line::raw(text).width();
    format!("{text}{}", " ".repeat(width.saturating_sub(used)))
}

/// Le popup a plusieurs parties : `Tab` y a un effet.
fn has_several_parts(session: &EditSession) -> bool {
    session
        .fields
        .windows(2)
        .any(|pair| !pair[0].same_part(pair[1]))
}

/// Champ sur lequel `←` `→` changent la valeur.
pub fn is_choice(session: &EditSession, field: EditableField) -> bool {
    match field {
        EditableField::Method | EditableField::BodyMode | EditableField::AuthMode => true,
        EditableField::AuthParamValue(index) => session
            .preview
            .auth_params()
            .get(index)
            .is_some_and(|kv| kv.key == "placement"),
        _ => false,
    }
}

/// Touches utiles à l'endroit du curseur, découpées en lignes de
/// `width` colonnes au plus.
pub fn help_lines(session: &EditSession, width: usize) -> Vec<Line<'static>> {
    let chunks = help_chunks(session);
    let mut lines = Vec::new();
    let mut current = String::new();
    for chunk in chunks {
        let candidate = if current.is_empty() {
            chunk.clone()
        } else {
            format!("{current}  {chunk}")
        };
        if !current.is_empty() && Line::raw(candidate.as_str()).width() > width {
            lines.push(Line::styled(std::mem::take(&mut current), theme::LABEL));
            current = chunk;
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        lines.push(Line::styled(current, theme::LABEL));
    }
    lines
}

/// Touches utiles à l'endroit du curseur, une par élément.
pub fn help_chunks(session: &EditSession) -> Vec<String> {
    let owned = |items: &[&str]| items.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    match &session.state {
        EditState::PlaintextSecretPrompt { var_name, .. } => vec![
            format!("Entrée remplacer par {{{{{var_name}}}}}"),
            "g garder".to_owned(),
            "Échap revenir".to_owned(),
        ],
        EditState::Picker { .. } => owned(&["↑↓ choisir", "Entrée valider", "Échap annuler"]),
        EditState::Input(input) if input.is_multiline() => owned(&[
            "Entrée nouvelle ligne",
            "Tab sortir",
            "Échap annuler",
            "Ctrl+S enregistrer",
        ]),
        EditState::Input(_) => {
            let mut chunks = owned(&["Entrée valider", "Échap annuler", "Ctrl+S enregistrer"]);
            let sensitive = matches!(session.target, InputTarget::Field)
                && matches!(
                    session.current_field(),
                    Some(EditableField::AuthParamValue(index))
                        if session.preview.auth_params().get(index)
                            .is_some_and(|kv| is_sensitive_auth_key(&kv.key))
                );
            if sensitive {
                chunks.push("Ctrl+R afficher".to_owned());
            }
            chunks
        }
        EditState::FieldSelect => {
            let mut chunks = vec!["↑↓ ligne".to_owned()];
            match session.current_field() {
                Some(field) if is_choice(session, field) => {
                    chunks.extend(owned(&["←→ changer", "Entrée liste"]));
                }
                Some(EditableField::AddRow(_)) => chunks.push("a ajouter".to_owned()),
                Some(EditableField::SettingsValue(_)) => chunks.push("Entrée modifier".to_owned()),
                Some(EditableField::MultipartFormValue(index))
                    if section_entries(&session.preview, EntrySection::MultipartForm)
                        .get(index)
                        .is_some_and(|kv| kv.value.trim_start().starts_with("@file(")) =>
                {
                    chunks.extend(owned(&["Espace activer", "d supprimer"]));
                }
                Some(field) if field.entry().is_some() => chunks.extend(owned(&[
                    "Entrée modifier",
                    "c renommer",
                    "a ajouter",
                    "d supprimer",
                    "Espace activer",
                ])),
                Some(
                    EditableField::ScriptPreRequest
                    | EditableField::ScriptPostResponse
                    | EditableField::Tests
                    | EditableField::Docs,
                ) => chunks.push("Entrée éditeur externe".to_owned()),
                Some(_) => chunks.push("Entrée modifier".to_owned()),
                None => {}
            }
            if has_several_parts(session) {
                chunks.push("Tab partie".to_owned());
            }
            chunks.extend(owned(&["Ctrl+S enregistrer", "Échap fermer"]));
            chunks
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::message::{InputKey, Message};
    use crate::app::model::Model;
    use crate::app::test_support::{loaded_model, render, select};
    use crate::app::update::update;

    /// Ouvre le popup de l'onglet `tab` de `path` (`Entrée`), ou le popup
    /// Requête (`m`) si `tab` vaut `None`.
    fn popup_on(path: &str, tab: Option<RequestTab>, size: (u16, u16)) -> Model {
        let mut model = loaded_model(size);
        select(&mut model, path);
        update(&mut model, Message::NextFocus);
        match tab {
            Some(tab) => {
                model.request_tab = tab;
                update(&mut model, Message::Enter);
            }
            None => {
                update(&mut model, Message::OpenRequestEdit);
            }
        }
        assert!(model.editing.is_some(), "popup ouvert sur {path}");
        model
    }

    fn session(model: &Model) -> &EditSession {
        model.editing.as_ref().expect("popup")
    }

    fn content(model: &Model) -> PopupContent {
        popup_content(session(model))
    }

    fn plain(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn texts(model: &Model) -> Vec<String> {
        content(model).lines.iter().map(plain).collect()
    }

    fn line_of(model: &Model, needle: &str) -> usize {
        texts(model)
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("ligne {needle:?} absente : {:#?}", texts(model)))
    }

    fn type_text(model: &mut Model, text: &str) {
        for c in text.chars() {
            update(model, Message::InputKey(InputKey::Char(c)));
        }
    }

    fn go_to(model: &mut Model, field: EditableField) {
        let index = session(model)
            .fields
            .iter()
            .position(|f| *f == field)
            .expect("champ du popup");
        model.editing.as_mut().expect("popup").cursor = index;
    }

    /// Chaque popup tient dans l'écran, centré, avec son titre, son
    /// contenu et sa ligne d'aide, sur 80 et 120 colonnes (§ 6.5).
    #[test]
    fn popups_render_inside_the_screen_at_80_and_120_columns() {
        let cases: [(&str, Option<RequestTab>, &str, &[&str]); 9] = [
            (
                "post-json.bru",
                None,
                "Requête — post-json.bru",
                &["‹ POST ›", "https://{{host}}/items"],
            ),
            (
                "scripted.bru",
                Some(RequestTab::Headers),
                "En-têtes — scripted.bru",
                &[
                    "Clé",
                    "Accept",
                    "application/json",
                    "X-Debug",
                    "(désactivé)",
                ],
            ),
            (
                "multiline.bru",
                Some(RequestTab::Params),
                "Params — multiline.bru",
                &["Paramètres de requête", "page", "Paramètres de chemin"],
            ),
            (
                "scripted.bru",
                Some(RequestTab::Auth),
                "Authentification — scripted.bru",
                &["Mode", "‹ bearer ›", "token", "{{tok}}"],
            ),
            (
                "post-json.bru",
                Some(RequestTab::Body),
                "Corps — post-json.bru",
                &["‹ json ›", "Contenu", "\"a\": {"],
            ),
            (
                "scripted.bru",
                Some(RequestTab::Vars),
                "Vars — scripted.bru",
                &["Pré-requête", "Post-réponse", "a ajouter"],
            ),
            (
                "scripted.bru",
                Some(RequestTab::Assert),
                "Assert — scripted.bru",
                &["res.status", "eq 200", "isTrue"],
            ),
            (
                "scripted.bru",
                Some(RequestTab::Script),
                "Script — scripted.bru",
                &["Pré-requête", "bru.setVar", "Post-réponse", "console.log"],
            ),
            (
                "scripted.bru",
                Some(RequestTab::Tests),
                "Tests — scripted.bru",
                &["test(\"ok\""],
            ),
        ];
        for width in [80u16, 120] {
            for (path, tab, title, needles) in cases {
                let model = popup_on(path, tab, (width, 30));
                let screen = render(&model, width, 30).join("\n");
                assert!(screen.contains(title), "{width} {title}\n{screen}");
                for needle in needles {
                    assert!(
                        screen.contains(needle),
                        "{width} {title} {needle}\n{screen}"
                    );
                }
                assert!(screen.contains("Échap fermer"), "{width} {title}\n{screen}");
                let area = edit_popup_area(Rect::new(0, 0, width, 30), session(&model));
                assert!(
                    area.right() <= width && area.bottom() <= 30,
                    "{width} {title}"
                );
                assert_eq!(area.x, (width - area.width) / 2, "centré : {title}");
                assert!(area.width <= 100);
            }
        }
    }

    /// Une ligne désactivée est grisée, suivie de « (désactivé) ».
    #[test]
    fn disabled_row_is_greyed_with_a_marker() {
        let model = popup_on("scripted.bru", Some(RequestTab::Headers), (100, 30));
        let lines = content(&model).lines;
        let line = &lines[line_of(&model, "X-Debug")];
        let value = line.spans.last().expect("valeur");
        assert_eq!(value.content.as_ref(), "1 (désactivé)");
        assert_eq!(value.style, DISABLED_STYLE);
        assert_eq!(line.spans[0].style, DISABLED_STYLE);
        // Ligne active : clé en libellé, valeur sans style.
        let accept = &lines[line_of(&model, "Accept")];
        assert_ne!(accept.spans[0].style, accept.spans[1].style);
    }

    /// La liste des méthodes n'existe que pendant qu'elle est ouverte,
    /// présélection sur la valeur courante.
    #[test]
    fn method_list_exists_only_while_open() {
        let mut model = popup_on("simple-get.bru", None, (100, 30));
        let before = content(&model).lines.len();
        assert_eq!(session(&model).current_field(), Some(EditableField::Method));
        update(&mut model, Message::Enter);
        let lines = content(&model).lines;
        assert_eq!(lines.len(), before + 9);
        let get = &lines[line_of(&model, "    GET")];
        assert!(get.style.add_modifier.contains(Modifier::REVERSED));
        assert!(texts(&model).iter().any(|l| l.trim() == "PATCH"));
        assert_eq!(content(&model).focus_line, line_of(&model, "    GET"));
        update(&mut model, Message::CancelInput);
        assert_eq!(content(&model).lines.len(), before);
    }

    /// Popup Assert : bascule, puis ajout d'une clé avec espaces ; popup
    /// Vars : `Tab` / `Maj+Tab` passent d'une partie à l'autre.
    #[test]
    fn assert_and_vars_popups_toggle_add_and_switch_parts() {
        let mut model = popup_on("scripted.bru", Some(RequestTab::Assert), (100, 30));
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::AssertValue(0))
        );
        update(&mut model, Message::ToggleField);
        assert!(texts(&model)[line_of(&model, "res.status")].contains("eq 200 (désactivé)"));
        update(&mut model, Message::ToggleField);
        assert!(!texts(&model)[line_of(&model, "res.status")].contains("(désactivé)"));

        update(&mut model, Message::Add);
        type_text(&mut model, "res.headers['content-type']");
        update(&mut model, Message::ValidateInput);
        type_text(&mut model, "isDefined");
        update(&mut model, Message::InputKey(InputKey::Enter));
        let row = &texts(&model)[line_of(&model, "res.headers['content-type']")];
        assert!(row.contains("isDefined"), "{row}");
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::AssertValue(2))
        );

        let mut model = popup_on("scripted.bru", Some(RequestTab::Vars), (100, 30));
        use crate::writer::EntrySection::{VarsPostResponse, VarsPreRequest};
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::AddRow(VarsPreRequest))
        );
        update(&mut model, Message::NextFocus);
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::AddRow(VarsPostResponse))
        );
        update(&mut model, Message::NextFocus);
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::AddRow(VarsPreRequest))
        );
        update(&mut model, Message::PreviousPart);
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::AddRow(VarsPostResponse))
        );
    }

    #[test]
    fn body_mode_list_offers_every_mode() {
        let mut model = popup_on("simple-get.bru", Some(RequestTab::Body), (100, 40));
        assert_eq!(
            session(&model).current_field(),
            Some(EditableField::BodyMode)
        );
        assert!(texts(&model)[0].contains("‹ none ›"));
        update(&mut model, Message::Enter);
        for mode in crate::writer::BODY_MODES {
            assert!(texts(&model).iter().any(|l| l.trim() == mode), "{mode}");
        }
    }

    /// Le curseur de texte suit la valeur, la clé renommée, la clé puis la
    /// valeur d'un ajout, et la ligne d'un corps multiligne.
    #[test]
    fn text_cursor_follows_value_key_new_entry_and_body_line() {
        let mut model = popup_on("post-json.bru", Some(RequestTab::Headers), (100, 30));
        let row = line_of(&model, "Content-Type");
        let key_width = Line::raw("Content-Type").width();
        let value_column = key_width + separator_width();

        // Valeur : curseur en fin de saisie.
        update(&mut model, Message::Enter);
        assert_eq!(
            content(&model).text_cursor,
            Some((row, value_column + "application/json".len()))
        );
        assert_eq!(content(&model).input_prefix, value_column);
        update(&mut model, Message::CancelInput);

        // Renommage : curseur dans la colonne Clé.
        update(&mut model, Message::Rename);
        assert_eq!(content(&model).text_cursor, Some((row, key_width)));
        update(&mut model, Message::CancelInput);

        // Ajout : la ligne provisoire suit le tableau.
        update(&mut model, Message::Add);
        type_text(&mut model, "X-T");
        let last = content(&model).lines.len() - 1;
        assert_eq!(content(&model).text_cursor, Some((last, 3)));
        assert!(texts(&model)[last].starts_with("X-T"));
        update(&mut model, Message::ValidateInput);
        type_text(&mut model, "v");
        assert_eq!(content(&model).text_cursor, Some((last, value_column + 1)));

        // Corps : deuxième ligne, une colonne après l'indentation.
        let mut model = popup_on("post-json.bru", Some(RequestTab::Body), (100, 40));
        go_to(&mut model, EditableField::BodyText);
        update(&mut model, Message::Enter);
        for _ in 0..10 {
            update(&mut model, Message::InputKey(InputKey::Up));
        }
        update(&mut model, Message::InputKey(InputKey::Down));
        update(&mut model, Message::InputKey(InputKey::Home));
        update(&mut model, Message::InputKey(InputKey::Right));
        let first = content(&model)
            .fields
            .iter()
            .find(|rows| rows.field == EditableField::BodyText)
            .expect("corps")
            .line;
        assert_eq!(content(&model).text_cursor, Some((first + 1, 3)));
    }

    /// Un secret d'auth est masqué, en saisie comme hors saisie, sauf
    /// `Ctrl+R` ; un secret en clair ouvre la question dans le bas du popup.
    #[test]
    fn secrets_are_masked_and_the_plaintext_question_is_in_the_popup() {
        let mut model = popup_on("scripted.bru", Some(RequestTab::Auth), (100, 30));
        go_to(&mut model, EditableField::AuthParamValue(0));
        update(&mut model, Message::Enter);
        for _ in 0.."{{tok}}".len() {
            update(&mut model, Message::InputKey(InputKey::Backspace));
        }
        type_text(&mut model, "abc");
        let row = line_of(&model, "token");
        assert!(
            texts(&model)[row].ends_with("•••"),
            "{}",
            texts(&model)[row]
        );
        assert!(help_chunks(session(&model)).contains(&"Ctrl+R afficher".to_owned()));
        update(&mut model, Message::ToggleRevealSecret);
        assert!(texts(&model)[row].ends_with("abc"));
        update(&mut model, Message::ToggleRevealSecret);

        update(&mut model, Message::ValidateInput);
        assert!(matches!(
            session(&model).state,
            EditState::PlaintextSecretPrompt { .. }
        ));
        let lines = texts(&model);
        let question = line_of(&model, "Secret en clair");
        assert!(question > row);
        assert!(lines[question + 1].contains("remplacer par {{bearerToken}}"));
        assert!(lines[question + 2].contains("garder en clair"));
        assert!(lines[question + 3].contains("revenir à la saisie"));
        assert!(help_chunks(session(&model)).contains(&"g garder".to_owned()));
        // Hors saisie, la valeur en clair ne s'affiche jamais.
        update(&mut model, Message::KeepPlaintextSecret);
        assert!(
            !texts(&model)[row].contains("abc"),
            "{}",
            texts(&model)[row]
        );
    }

    /// Remplace le corps de la requête sélectionnée.
    fn with_body(model: &mut Model, content: &str) {
        use crate::collection::{BruFile, TreeNode};
        let ast = BruFile::parse(content.to_owned()).expect("AST");
        let view = crate::collection::RequestView::from_ast(&ast).expect("vue");
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let path = request.path.clone();
        model.replace_request_node(&path, ast, view);
    }

    /// Corps `file` : le mode seul se modifie, le contenu est montré en
    /// lecture seule. GraphQL : mode fixe, parties Requête et Variables.
    #[test]
    fn file_body_is_read_only_and_graphql_has_two_parts() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        with_body(
            &mut model,
            "post {\n  url: https://a\n  body: file\n}\n\nbody:file {\n  file: @file(/tmp/a.bin) @contentType(application/octet-stream)\n}\n",
        );
        model.request_tab = RequestTab::Body;
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert_eq!(session(&model).fields, vec![EditableField::BodyMode]);
        let lines = texts(&model);
        assert!(
            lines.iter().any(|l| l.contains("Lecture seule")),
            "{lines:#?}"
        );
        assert!(lines.iter().any(|l| l.contains("/tmp/a.bin")), "{lines:#?}");

        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        with_body(
            &mut model,
            "meta {\n  name: gql\n  type: graphql\n}\n\npost {\n  url: https://a\n  body: graphql\n}\n\nbody:graphql {\n  query { me }\n}\n\nbody:graphql:vars {\n  {}\n}\n",
        );
        model.request_tab = RequestTab::Body;
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        let lines = texts(&model);
        assert!(!lines.iter().any(|l| l.contains("Mode")), "{lines:#?}");
        let query = line_of(&model, "Requête");
        let vars = line_of(&model, "Variables");
        assert!(query < line_of(&model, "query { me }"));
        assert!(vars > query);
        update(&mut model, Message::Left);
        assert!(!session(&model).dirty, "mode fixe");
    }

    /// Les touches d'aide se replient sur la largeur du popup.
    #[test]
    fn help_lines_wrap_to_the_popup_width() {
        let model = popup_on("scripted.bru", Some(RequestTab::Headers), (100, 30));
        let chunks = help_chunks(session(&model));
        let lines = help_lines(session(&model), 30);
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(line.width() <= 30, "{}", plain(line));
        }
        let joined: Vec<String> = lines.iter().map(plain).collect();
        for chunk in chunks {
            assert!(joined.iter().any(|l| l.contains(&chunk)), "{chunk}");
        }
    }

    /// Un script trop long montre ses premières lignes et le nombre de
    /// lignes restantes.
    #[test]
    fn long_text_blocks_show_a_preview() {
        let mut model = popup_on("scripted.bru", Some(RequestTab::Script), (100, 30));
        let text = (1..=9)
            .map(|n| format!("line{n}"))
            .collect::<Vec<_>>()
            .join("\n");
        update(
            &mut model,
            Message::TextEditorClosed {
                field: EditableField::ScriptPreRequest,
                result: Ok(text),
            },
        );
        let lines = texts(&model);
        assert!(lines.iter().any(|l| l.trim() == "line6"));
        assert!(!lines.iter().any(|l| l.trim() == "line7"));
        assert!(lines.iter().any(|l| l.contains("… (+3 lignes)")));
        assert!(session(&model).dirty);
        assert!(edit_popup_title(session(&model)).contains("● non enregistré"));
    }

    /// Dossier sans bloc `auth` : `none` est rappelé comme « non
    /// déclarée », comme dans le détail, sans rien écrire tant que
    /// l'utilisateur ne change rien ; un mode choisi s'affiche normalement.
    #[test]
    fn undeclared_auth_mode_is_labelled_until_a_mode_is_chosen() {
        let mut model = popup_on("grp/sub", Some(RequestTab::Auth), (100, 30));
        let line = texts(&model)[line_of(&model, "Mode")].clone();
        assert!(line.contains("‹ none › (non déclarée)"), "{line}");

        // Liste ouverte puis validée sur `none` : aucune modification.
        update(&mut model, Message::Enter);
        update(&mut model, Message::Enter);
        assert!(!session(&model).dirty);
        assert!(session(&model).pending.is_empty());
        assert!(texts(&model)[line_of(&model, "Mode")].contains("(non déclarée)"));

        update(&mut model, Message::Right);
        let line = texts(&model)[line_of(&model, "Mode")].clone();
        assert!(line.contains("‹ inherit ›"), "{line}");
        assert!(!line.contains("non déclarée"), "{line}");
        update(&mut model, Message::Left);
        let line = texts(&model)[line_of(&model, "Mode")].clone();
        assert!(line.contains("‹ none ›"), "{line}");
        assert!(!line.contains("non déclarée"), "{line}");
    }
}
