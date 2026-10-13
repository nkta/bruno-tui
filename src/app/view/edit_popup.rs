//! Rendu des popups d'édition modaux centrés (`docs/edition-popups.md`).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

use super::inner;
use super::theme;
use crate::app::model::{
    EditScope, EditSession, EditState, EditableField, InputTarget, Model, PickerKind, RequestTab,
    field_value,
};
use crate::collection::KeyValue;

pub const TABLE_SEPARATOR: &str = " │ ";
pub const DISABLED_ENTRY_VALUE: Style =
    Style::new().add_modifier(Modifier::DIM.union(Modifier::ITALIC));

/// Zone du popup d'édition : centrée sur l'écran, largeur bornée entre 50 et 80 colonnes,
/// hauteur adaptée au contenu.
pub fn edit_popup_area(screen: Rect, session: &EditSession) -> Rect {
    let width = (screen.width * 3 / 4).clamp(50, 80).min(screen.width);
    let max_height = screen.height.saturating_sub(4).max(6);
    let is_adding = matches!(
        session.target,
        InputTarget::NewKey { .. } | InputTarget::NewValue { .. }
    );
    let add_extra = if is_adding { 1 } else { 0 };

    let content_rows = match session.scope {
        EditScope::Request => 2,
        EditScope::Tab(RequestTab::Headers) => {
            session.preview.headers.len().max(1) + add_extra + 1
        }
        EditScope::Tab(RequestTab::Params) => {
            session.preview.query_params.len().max(1)
                + session.preview.path_params.len().max(1)
                + add_extra
                + 2
        }
        EditScope::Tab(RequestTab::Auth) => 1 + session.preview.auth_params().len().max(1) + 1,
        EditScope::Tab(RequestTab::Vars) => {
            session.preview.extra.vars_pre_request.len().max(1)
                + session.preview.extra.vars_post_response.len().max(1)
                + add_extra
                + 2
        }
        EditScope::Tab(RequestTab::Assert) => {
            session.preview.extra.assertions.len().max(1) + add_extra + 1
        }
        EditScope::Tab(RequestTab::Settings) => session.preview.extra.settings.len().max(1) + 1,
        EditScope::Tab(RequestTab::Body) => 10,
        EditScope::Tab(RequestTab::Script) => 8,
        EditScope::Tab(RequestTab::Tests | RequestTab::Docs) => 6,
    };
    // +2 bordures + 1 ligne d'aide
    let height = (content_rows as u16 + 3).clamp(6, max_height);
    let x = screen.x + screen.width.saturating_sub(width) / 2;
    let y = screen.y + screen.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

/// Dessine le popup d'édition de la session active.
pub fn render_edit_popup(
    _model: &Model,
    session: &EditSession,
    frame: &mut Frame,
    area: Rect,
) {
    let file_name = session
        .path
        .file_name()
        .map_or("", |s| s.to_str().unwrap_or(""));
    let title = format!(" {} — {} ", session.scope.title(), file_name);
    let block = Block::bordered()
        .title(title)
        .border_type(BorderType::Rounded)
        .border_style(theme::FOCUS);
    frame.render_widget(block, area);

    let inner_area = inner(area);
    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }

    match session.scope {
        EditScope::Tab(RequestTab::Headers) => {
            render_headers_popup(session, frame, inner_area);
        }
        EditScope::Request => {
            render_request_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Params) => {
            render_params_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Auth) => {
            render_auth_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Vars) => {
            render_vars_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Assert) => {
            render_assert_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Settings) => {
            render_settings_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Body) => {
            render_body_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Script) => {
            render_script_popup(session, frame, inner_area);
        }
        EditScope::Tab(RequestTab::Tests | RequestTab::Docs) => {
            render_text_block_popup(session, frame, inner_area);
        }
    }
}

/// Dessine un tableau clé/valeur pour les en-têtes.
fn render_headers_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    let headers = &session.preview.headers;
    let is_adding = matches!(
        session.target,
        InputTarget::NewKey { .. } | InputTarget::NewValue { .. }
    );

    // Ligne d'aide au bas du popup
    let help_text = match &session.state {
        EditState::Input(_) => "Entrée/Tab valider  Échap annuler",
        EditState::Picker { .. } => "↑↓ choisir  Entrée valider  Échap annuler",
        EditState::FieldSelect => {
            if headers.is_empty() && !is_adding {
                "a ajouter  Ctrl+S fermer  Échap fermer"
            } else {
                "↑↓ naviguer  Entrée modifier  a ajouter  d supprimer  c renommer  Espace activer  Ctrl+S enregistrer  Échap fermer"
            }
        }
        EditState::PlaintextSecretPrompt { .. } => "",
    };
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(help_text, theme::LABEL)),
        help_rect,
    );

    let content_height = inner_area.height.saturating_sub(1);
    if content_height == 0 {
        return;
    }

    // Largeur de la colonne clé
    let provisional_key_len = match &session.target {
        InputTarget::NewKey { .. } => match &session.state {
            EditState::Input(input) => Some(input.text().len()),
            _ => None,
        },
        InputTarget::NewValue { key, .. } => Some(key.len()),
        InputTarget::RenameKey { .. } => match &session.state {
            EditState::Input(input) => Some(input.text().len()),
            _ => None,
        },
        _ => None,
    };
    let max_key_len = headers
        .iter()
        .map(|h| h.key.len())
        .chain(provisional_key_len)
        .max()
        .unwrap_or(3);
    let key_width = max_key_len.clamp(3, 25);

    // En-tête de colonnes
    let col_header = Line::styled(
        format!("{:<key_width$}{TABLE_SEPARATOR}Valeur", "Clé"),
        theme::LABEL,
    );
    frame.render_widget(
        Paragraph::new(col_header),
        Rect::new(inner_area.x, inner_area.y, inner_area.width, 1),
    );

    let table_rows = content_height.saturating_sub(1) as usize;
    if headers.is_empty() && !is_adding {
        let empty_rect = Rect::new(inner_area.x, inner_area.y + 1, inner_area.width, 1);
        frame.render_widget(
            Paragraph::new(Line::styled("aucun en-tête", theme::EMPTY_MESSAGE)),
            empty_rect,
        );
        return;
    }

    let cursor_row = if is_adding {
        headers.len()
    } else {
        session.cursor.min(headers.len().saturating_sub(1))
    };
    let scroll = if cursor_row < session.scroll {
        cursor_row
    } else if cursor_row >= session.scroll + table_rows {
        cursor_row + 1 - table_rows
    } else {
        session.scroll
    };

    for r in 0..table_rows {
        let idx = scroll + r;
        let y = inner_area.y + 1 + r as u16;
        let row_rect = Rect::new(inner_area.x, y, inner_area.width, 1);

        if idx < headers.len() {
            let entry = &headers[idx];
            let is_selected = idx == session.cursor && session.state == EditState::FieldSelect;
            let is_editing = idx == session.cursor && matches!(session.state, EditState::Input(_));

            let key_str = if is_editing
                && matches!(session.target, InputTarget::RenameKey { index, .. } if index == idx)
            {
                if let EditState::Input(input) = &session.state {
                    input.text().to_owned()
                } else {
                    entry.key.clone()
                }
            } else {
                entry.key.clone()
            };

            let key_span = Span::styled(
                format!("{:<key_width$}{TABLE_SEPARATOR}", key_str),
                theme::LABEL,
            );

            let val_span = if is_editing && session.target == InputTarget::Field {
                if let EditState::Input(input) = &session.state {
                    Span::raw(input.text().to_owned())
                } else {
                    Span::raw(entry.value.clone())
                }
            } else if entry.enabled {
                Span::raw(entry.value.clone())
            } else {
                Span::styled(
                    format!("{} (désactivé)", entry.value),
                    DISABLED_ENTRY_VALUE,
                )
            };

            let mut line = Line::from(vec![key_span, val_span]);
            if is_selected {
                line = line.style(theme::SELECTION);
            }
            frame.render_widget(Paragraph::new(line), row_rect);
        } else if idx == headers.len() && is_adding {
            let prov_line = match &session.target {
                InputTarget::NewKey { .. } => {
                    let text = if let EditState::Input(input) = &session.state {
                        input.text().to_owned()
                    } else {
                        String::new()
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("{:<key_width$}{TABLE_SEPARATOR}", text),
                            theme::LABEL,
                        ),
                        Span::raw(""),
                    ])
                }
                InputTarget::NewValue { key, .. } => {
                    let text = if let EditState::Input(input) = &session.state {
                        input.text().to_owned()
                    } else {
                        String::new()
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("{key:<key_width$}{TABLE_SEPARATOR}"),
                            theme::LABEL,
                        ),
                        Span::raw(text),
                    ])
                }
                _ => Line::default(),
            };
            let prov_line = prov_line.style(theme::SELECTION);
            frame.render_widget(Paragraph::new(prov_line), row_rect);
        }
    }

    // Positionnement du curseur de texte
    if let EditState::Input(input) = &session.state {
        let rel_row = cursor_row.saturating_sub(scroll);
        if rel_row < table_rows {
            let cursor_y = inner_area.y + 1 + rel_row as u16;
            let prefix_col = match &session.target {
                InputTarget::Field => key_width + TABLE_SEPARATOR.len(),
                InputTarget::RenameKey { .. } => 0,
                InputTarget::NewKey { .. } => 0,
                InputTarget::NewValue { key, .. } => {
                    key_width.max(key.len()) + TABLE_SEPARATOR.len()
                }
            };
            let text_offset = Line::raw(input.text_before_cursor_on_line()).width();
            let cursor_x = inner_area.x + prefix_col as u16 + text_offset as u16;
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

/// Rendu du popup Requête (Méthode + URL).
fn render_request_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    let help_text = match &session.state {
        EditState::Input(_) => "Entrée/Tab valider  Échap annuler",
        EditState::Picker { .. } => "↑↓ choisir  Entrée valider  Échap annuler",
        EditState::FieldSelect => "↑↓ champ  ←→ méthode  Entrée modifier  Ctrl+S enregistrer  Échap fermer",
        _ => "",
    };
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    let method_field = EditableField::Method;
    let url_field = EditableField::Url;

    let is_method_cursor = session.current_field() == Some(method_field);
    let is_url_cursor = session.current_field() == Some(url_field);

    let method_val = field_value(session, &method_field);
    let url_val = field_value(session, &url_field);

    // Ligne Méthode : Méthode ‹ GET ›
    let method_line_spans = vec![
        Span::styled("Méthode   ‹ ", theme::LABEL),
        Span::styled(
            method_val.to_ascii_uppercase(),
            theme::method_style(method_val).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ›", theme::LABEL),
    ];
    let mut method_line = Line::from(method_line_spans);
    if is_method_cursor && session.state == EditState::FieldSelect {
        method_line = method_line.style(theme::SELECTION.add_modifier(Modifier::REVERSED));
    }
    frame.render_widget(
        Paragraph::new(method_line),
        Rect::new(inner_area.x, inner_area.y, inner_area.width, 1),
    );

    // Ligne URL : URL       https://...
    let url_str = if is_url_cursor && matches!(session.state, EditState::Input(_)) {
        if let EditState::Input(input) = &session.state {
            let (visible, _) =
                crate::app::view::detail::skip_columns(input.text(), usize::from(session.hscroll));
            visible.to_owned()
        } else {
            url_val.to_owned()
        }
    } else {
        url_val.to_owned()
    };
    let url_line_spans = vec![
        Span::styled("URL       ", theme::LABEL),
        Span::raw(url_str),
    ];
    let mut url_line = Line::from(url_line_spans);
    if is_url_cursor && session.state == EditState::FieldSelect {
        url_line = url_line.style(theme::SELECTION.add_modifier(Modifier::REVERSED));
    }
    frame.render_widget(
        Paragraph::new(url_line),
        Rect::new(inner_area.x, inner_area.y + 1, inner_area.width, 1),
    );

    // Curseur de saisie sur URL
    if is_url_cursor && let EditState::Input(input) = &session.state {
        let cursor_y = inner_area.y + 1;
        let prefix_col = 10; // "URL       ".len()
        let text_offset = Line::raw(input.text_before_cursor_on_line())
            .width()
            .saturating_sub(usize::from(session.hscroll));
        let cursor_x = inner_area.x + prefix_col as u16 + text_offset as u16;
        frame.set_cursor_position((cursor_x, cursor_y));
    }

    // Sélecteur méthode ouvert
    if let EditState::Picker { kind: PickerKind::Method, selected } = session.state {
        let picker_y = inner_area.y + 2;
        let picker_height = inner_area.height.saturating_sub(3).min(PickerKind::Method.options().len() as u16);
        for (i, opt) in PickerKind::Method.options().iter().enumerate().take(picker_height as usize) {
            let mut line = Line::raw(format!("  {}", opt.to_ascii_uppercase()));
            if i == selected {
                line = line.style(theme::SELECTION);
            }
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(inner_area.x, picker_y + i as u16, inner_area.width, 1),
            );
        }
    }
}

/// Rendu du popup Params (Requête & Chemin).
fn render_params_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    render_two_tables_popup(
        session,
        frame,
        inner_area,
        "Paramètres de requête",
        &session.preview.query_params,
        "Paramètres de chemin",
        &session.preview.path_params,
    );
}

/// Rendu du popup Vars (Pré-requête & Post-réponse).
fn render_vars_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    render_two_tables_popup(
        session,
        frame,
        inner_area,
        "Variables pré-requête",
        &session.preview.extra.vars_pre_request,
        "Variables post-réponse",
        &session.preview.extra.vars_post_response,
    );
}

/// Rendu d'un popup à deux sections de tableau avec Tab / Maj+Tab.
fn render_two_tables_popup(
    session: &EditSession,
    frame: &mut Frame,
    inner_area: Rect,
    title1: &str,
    entries1: &[KeyValue],
    title2: &str,
    entries2: &[KeyValue],
) {
    let help_text = match &session.state {
        EditState::Input(_) => "Entrée/Tab valider  Échap annuler",
        EditState::FieldSelect => {
            "↑↓ naviguer  Tab/Maj+Tab section  Entrée modifier  a ajouter  d supprimer  c renommer  Espace activer  Ctrl+S enregistrer  Échap fermer"
        }
        _ => "",
    };
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    let active_part = session.active_part;
    let mut y = inner_area.y;

    // Titre 1
    let t1_style = if active_part == 0 { theme::FOCUS } else { theme::LABEL };
    frame.render_widget(
        Paragraph::new(Line::styled(format!("▸ {title1}"), t1_style)),
        Rect::new(inner_area.x, y, inner_area.width, 1),
    );
    y += 1;

    for (i, e) in entries1.iter().enumerate() {
        if y >= inner_area.bottom().saturating_sub(2) {
            break;
        }
        let is_sel = active_part == 0 && session.cursor == i && session.state == EditState::FieldSelect;
        let mut line = Line::from(vec![
            Span::styled(format!("  {}: ", e.key), theme::LABEL),
            Span::raw(e.value.clone()),
        ]);
        if is_sel {
            line = line.style(theme::SELECTION);
        }
        frame.render_widget(Paragraph::new(line), Rect::new(inner_area.x, y, inner_area.width, 1));
        y += 1;
    }
    if entries1.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("  aucun", theme::EMPTY_MESSAGE)),
            Rect::new(inner_area.x, y, inner_area.width, 1),
        );
        y += 1;
    }

    // Titre 2
    let t2_style = if active_part == 1 { theme::FOCUS } else { theme::LABEL };
    frame.render_widget(
        Paragraph::new(Line::styled(format!("▸ {title2}"), t2_style)),
        Rect::new(inner_area.x, y, inner_area.width, 1),
    );
    y += 1;

    for (i, e) in entries2.iter().enumerate() {
        if y >= inner_area.bottom().saturating_sub(1) {
            break;
        }
        let is_sel = active_part == 1 && session.cursor == (entries1.len() + i) && session.state == EditState::FieldSelect;
        let mut line = Line::from(vec![
            Span::styled(format!("  {}: ", e.key), theme::LABEL),
            Span::raw(e.value.clone()),
        ]);
        if is_sel {
            line = line.style(theme::SELECTION);
        }
        frame.render_widget(Paragraph::new(line), Rect::new(inner_area.x, y, inner_area.width, 1));
        y += 1;
    }
    if entries2.is_empty() && y < inner_area.bottom().saturating_sub(1) {
        frame.render_widget(
            Paragraph::new(Line::styled("  aucun", theme::EMPTY_MESSAGE)),
            Rect::new(inner_area.x, y, inner_area.width, 1),
        );
    }
}

/// Rendu du popup Auth.
fn render_auth_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    let help_text = match &session.state {
        EditState::Input(_) => "Entrée valider  Échap annuler",
        EditState::Picker { .. } => "↑↓ choisir  Entrée valider  Échap annuler",
        EditState::FieldSelect => "←→ mode  Entrée modifier  Ctrl+S enregistrer  Échap fermer",
        _ => "",
    };
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    let mode_str = session.preview.auth.as_ref().map_or("none", |m| m.as_str());
    let is_mode_cursor = session.current_field() == Some(EditableField::AuthMode);
    let mut mode_line = Line::from(vec![
        Span::styled("Mode         ‹ ", theme::LABEL),
        Span::styled(mode_str, theme::FOCUS),
        Span::styled(" ›", theme::LABEL),
    ]);
    if is_mode_cursor && session.state == EditState::FieldSelect {
        mode_line = mode_line.style(theme::SELECTION);
    }
    frame.render_widget(
        Paragraph::new(mode_line),
        Rect::new(inner_area.x, inner_area.y, inner_area.width, 1),
    );

    let params = session.preview.auth_params();
    for (i, p) in params.iter().enumerate() {
        let y = inner_area.y + 1 + i as u16;
        if y >= inner_area.bottom().saturating_sub(1) {
            break;
        }
        let is_sel = session.cursor == i + 1 && session.state == EditState::FieldSelect;
        let mut line = Line::from(vec![
            Span::styled(format!("{:<12} ", p.key), theme::LABEL),
            Span::raw(p.value.clone()),
        ]);
        if is_sel {
            line = line.style(theme::SELECTION);
        }
        frame.render_widget(Paragraph::new(line), Rect::new(inner_area.x, y, inner_area.width, 1));
    }
}

/// Rendu du popup Assert.
fn render_assert_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    render_single_table_popup(session, frame, inner_area, "Assertions", &session.preview.extra.assertions, "aucune assertion");
}

/// Rendu du popup Settings.
fn render_settings_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    render_single_table_popup(session, frame, inner_area, "Paramètres", &session.preview.extra.settings, "aucun paramètre");
}

fn render_single_table_popup(
    session: &EditSession,
    frame: &mut Frame,
    inner_area: Rect,
    _title: &str,
    entries: &[KeyValue],
    empty_label: &str,
) {
    let help_text = match &session.state {
        EditState::Input(_) => "Entrée/Tab valider  Échap annuler",
        EditState::FieldSelect => "↑↓ naviguer  Entrée modifier  a ajouter  d supprimer  Ctrl+S enregistrer  Échap fermer",
        _ => "",
    };
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    if entries.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(format!("  {empty_label}"), theme::EMPTY_MESSAGE)),
            Rect::new(inner_area.x, inner_area.y, inner_area.width, 1),
        );
        return;
    }

    for (i, e) in entries.iter().enumerate() {
        let y = inner_area.y + i as u16;
        if y >= inner_area.bottom().saturating_sub(1) {
            break;
        }
        let is_sel = session.cursor == i && session.state == EditState::FieldSelect;
        let mut line = Line::from(vec![
            Span::styled(format!("  {}: ", e.key), theme::LABEL),
            Span::raw(e.value.clone()),
        ]);
        if is_sel {
            line = line.style(theme::SELECTION);
        }
        frame.render_widget(Paragraph::new(line), Rect::new(inner_area.x, y, inner_area.width, 1));
    }
}

/// Rendu du popup Corps.
fn render_body_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    let help_text = match &session.state {
        EditState::Input(_) => "Entrée nouvelle ligne  Tab sortir  Échap annuler",
        EditState::Picker { .. } => "↑↓ choisir  Entrée valider  Échap annuler",
        EditState::FieldSelect => "←→ mode  Entrée modifier  Ctrl+S enregistrer  Échap fermer",
        _ => "",
    };
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    let mode_str = session.preview.body_mode();
    let is_mode_cursor = session.current_field() == Some(EditableField::BodyMode);
    let mut mode_line = Line::from(vec![
        Span::styled("Mode       ‹ ", theme::LABEL),
        Span::styled(mode_str, theme::FOCUS),
        Span::styled(" ›", theme::LABEL),
    ]);
    if is_mode_cursor && session.state == EditState::FieldSelect {
        mode_line = mode_line.style(theme::SELECTION);
    }
    frame.render_widget(
        Paragraph::new(mode_line),
        Rect::new(inner_area.x, inner_area.y, inner_area.width, 1),
    );
}

/// Rendu du popup Script (Pré-requête & Post-réponse).
fn render_script_popup(session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    let help_text = "Entrée éditer dans l'éditeur externe  Ctrl+S enregistrer  Échap fermer";
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    let pre_cursor = session.cursor == 0 && session.state == EditState::FieldSelect;
    let post_cursor = session.cursor == 1 && session.state == EditState::FieldSelect;

    let mut l1 = Line::raw("▸ Script pré-requête");
    if pre_cursor { l1 = l1.style(theme::SELECTION); }
    frame.render_widget(Paragraph::new(l1), Rect::new(inner_area.x, inner_area.y, inner_area.width, 1));

    let mut l2 = Line::raw("▸ Script post-réponse");
    if post_cursor { l2 = l2.style(theme::SELECTION); }
    frame.render_widget(Paragraph::new(l2), Rect::new(inner_area.x, inner_area.y + 2, inner_area.width, 1));
}

/// Rendu du popup Tests / Docs.
fn render_text_block_popup(_session: &EditSession, frame: &mut Frame, inner_area: Rect) {
    let help_text = "Entrée éditer dans l'éditeur externe  Ctrl+S enregistrer  Échap fermer";
    let help_rect = Rect::new(
        inner_area.x,
        inner_area.bottom().saturating_sub(1),
        inner_area.width,
        1,
    );
    frame.render_widget(Paragraph::new(Line::styled(help_text, theme::LABEL)), help_rect);

    let line = Line::raw("Entrée pour ouvrir l'éditeur externe");
    frame.render_widget(Paragraph::new(line.style(theme::SELECTION)), Rect::new(inner_area.x, inner_area.y, inner_area.width, 1));
}
