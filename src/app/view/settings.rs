//! Rendu du popup « Réglages » de la collection
//! (`docs/reglages-collection.md` § 4). Pur : lit la session et dessine.
//!
//! Aucune valeur de secret n'est dessinée en clair : un mot de passe ou une
//! passphrase est rendu en puces, un littéral déjà présent dans le fichier
//! en `********`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::edit_popup::CURSOR_STYLE;
use super::theme;
use crate::app::settings::{
    CertField, NewCert, Row, SettingsConfirm, SettingsPart, SettingsSession,
};

/// Largeur de la colonne des libellés.
const LABEL_WIDTH: usize = 18;

/// Lignes du popup ; la seconde valeur donne la ligne et la colonne du
/// curseur de texte, s'il y en a un.
pub fn settings_lines(session: &SettingsSession) -> (Vec<Line<'static>>, Option<(usize, usize)>) {
    let mut lines = Vec::new();
    let mut cursor = None;
    match session.part {
        SettingsPart::Presets => {
            for row in [Row::PresetType, Row::PresetUrl, Row::PresetEnv] {
                push_row(session, row, &mut lines, &mut cursor);
            }
        }
        SettingsPart::Proxy => {
            let rows = [
                Row::ProxyMode,
                Row::ProxyProtocol,
                Row::ProxyHost,
                Row::ProxyPort,
                Row::ProxyAuth,
                Row::ProxyUser,
                Row::ProxyPassword,
                Row::ProxyBypass,
            ];
            for row in rows {
                push_row(session, row, &mut lines, &mut cursor);
            }
            if session.legacy_proxy {
                lines.push(Line::from(Span::styled(
                    "Ancien format : converti au nouveau format de Bruno à l'écriture.",
                    theme::LABEL,
                )));
            }
        }
        SettingsPart::Certs => push_certs(session, &mut lines),
    }
    lines.push(Line::raw(""));
    if let Some(error) = &session.error {
        lines.push(Line::from(Span::styled(error.clone(), theme::FAILURE)));
    }
    lines.push(Line::from(Span::styled(help_text(session), theme::LABEL)));
    (lines, cursor)
}

fn push_row(
    session: &SettingsSession,
    row: Row,
    lines: &mut Vec<Line<'static>>,
    cursor: &mut Option<(usize, usize)>,
) {
    let selectable = session.selectable_rows().contains(&row);
    let current = selectable && session.current_row() == Some(row);
    let (label_style, value_style) = if !selectable {
        (theme::LABEL, theme::LABEL)
    } else if current {
        (CURSOR_STYLE, theme::BACKGROUND)
    } else {
        (theme::LABEL, theme::BACKGROUND)
    };
    if row == Row::ProxyAuth {
        let mark = if session.proxy.auth { "x" } else { " " };
        lines.push(Line::from(Span::styled(
            format!("[{mark}] {}", row.label()),
            if current { CURSOR_STYLE } else { value_style },
        )));
        return;
    }
    let editing = session.input.as_ref().filter(|active| active.row == row);
    let value = match editing {
        Some(active) if row.is_secret() => session.masked_input(active.input.text()),
        Some(active) => active.input.text().to_owned(),
        None if row.is_choice() => format!("‹ {} ›", session.row_value(row)),
        None => session.row_value(row),
    };
    if let Some(active) = editing {
        *cursor = Some((lines.len(), LABEL_WIDTH + active.input.cursor()));
    }
    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<width$}", row.label(), width = LABEL_WIDTH - 1),
            label_style,
        ),
        Span::raw(" "),
        Span::styled(value, value_style),
    ]));
}

fn push_certs(session: &SettingsSession, lines: &mut Vec<Line<'static>>) {
    let certs = session.cert_summaries();
    if certs.is_empty() {
        lines.push(Line::from(Span::styled(
            "Aucun certificat client.",
            theme::EMPTY_MESSAGE,
        )));
        return;
    }
    let width = certs
        .iter()
        .map(|c| c.domain.chars().count())
        .max()
        .unwrap_or(0)
        .min(28);
    for (index, cert) in certs.iter().enumerate() {
        let current = index == session.cursor();
        let marker = if current { "▸" } else { " " };
        let mut text = format!(
            "{marker} {:<width$}  {:<4} {}",
            cert.domain, cert.kind, cert.path
        );
        if cert.disabled {
            text.push_str(" (désactivé)");
        }
        let style = if current {
            CURSOR_STYLE
        } else if cert.disabled {
            theme::LABEL
        } else {
            theme::BACKGROUND
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
}

/// Touches utiles, sur la dernière ligne du popup et dans la barre de statut.
pub fn help_text(session: &SettingsSession) -> String {
    match &session.confirm {
        Some(SettingsConfirm::Discard) => return "Abandonner les modifications ? (y/n)".to_owned(),
        Some(SettingsConfirm::DeleteCert(index)) => {
            let domain = session
                .cert_summaries()
                .get(*index)
                .map(|c| c.domain.clone())
                .unwrap_or_default();
            return format!("Supprimer le certificat « {domain} » ? (y/n)");
        }
        None => {}
    }
    if let Some(add) = &session.add {
        let choice = if add.current() == CertField::Kind {
            "←→ type  "
        } else {
            ""
        };
        return format!("{choice}↑↓ ligne  Entrée suivant  Ctrl+S ajouter  Échap annuler");
    }
    if let Some(active) = &session.input {
        let reveal = if active.row.is_secret() {
            "  Ctrl+R afficher"
        } else {
            ""
        };
        return format!("Saisie — Entrée valider  Échap annuler{reveal}");
    }
    let common = "Tab partie  Ctrl+S enregistrer  Échap fermer";
    match session.part {
        SettingsPart::Certs => {
            format!("a ajouter  d supprimer  Espace activer/désactiver  {common}")
        }
        _ => match session.current_row() {
            Some(row) if row.is_choice() => format!("←→ choisir  ↑↓ ligne  {common}"),
            Some(Row::ProxyAuth) => format!("Espace cocher  ↑↓ ligne  {common}"),
            Some(row) if row.is_secret() => {
                format!("Entrée modifier  Ctrl+R afficher  ↑↓ ligne  {common}")
            }
            _ => format!("Entrée modifier  ↑↓ ligne  {common}"),
        },
    }
}

/// Cadre du popup, centré sur `screen`.
pub fn settings_area(screen: Rect, session: &SettingsSession) -> Rect {
    let (lines, _) = settings_lines(session);
    let width = screen
        .width
        .saturating_sub(4)
        .clamp(screen.width.min(30), 76);
    let wanted = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let height = wanted.min(screen.height);
    Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn title(session: &SettingsSession) -> Line<'static> {
    let mut spans = vec![Span::raw(session.title()), Span::raw("─ ")];
    for (index, part) in SettingsPart::ALL.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" │ "));
        }
        let style = if *part == session.part {
            theme::ACTIVE_TAB
        } else {
            theme::LABEL
        };
        spans.push(Span::styled(part.label(), style));
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Dessine le popup par-dessus l'écran.
pub fn render_settings(session: &SettingsSession, frame: &mut Frame) {
    let area = settings_area(frame.area(), session);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(title(session))
        .border_type(BorderType::Rounded)
        .border_style(theme::FOCUS)
        .style(theme::BACKGROUND);
    let inner = super::inner(area);
    frame.render_widget(block, area);
    let (lines, cursor) = settings_lines(session);
    frame.render_widget(Paragraph::new(lines), inner);
    if let Some((line, column)) = cursor {
        place_cursor(frame, inner, line, column);
    }
    if let Some(add) = &session.add {
        render_add(session, add, frame);
    }
}

fn place_cursor(frame: &mut Frame, inner: Rect, line: usize, column: usize) {
    if line < usize::from(inner.height) {
        let row = u16::try_from(line).unwrap_or(0);
        let column = u16::try_from(column)
            .unwrap_or(u16::MAX)
            .min(inner.width.saturating_sub(1));
        frame.set_cursor_position((inner.x + column, inner.y + row));
    }
}

/// Lignes du sous-popup « Nouveau certificat ».
pub fn add_lines(add: &NewCert) -> (Vec<Line<'static>>, Option<(usize, usize)>) {
    const WIDTH: usize = 14;
    let mut lines = Vec::new();
    let mut cursor = None;
    for (index, field) in add.fields().into_iter().enumerate() {
        let current = index == add.cursor;
        let label_style = if current { CURSOR_STYLE } else { theme::LABEL };
        let value = match field {
            CertField::Kind => format!("‹ {} ›", add.kind.label()),
            CertField::Passphrase => {
                let text = add.passphrase.text();
                "•".repeat(text.chars().count())
            }
            other => add
                .input(other)
                .map(|input| input.text().to_owned())
                .unwrap_or_default(),
        };
        if current && let Some(input) = add.input(field) {
            cursor = Some((lines.len(), WIDTH + input.cursor()));
        }
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<width$}", field.label(), width = WIDTH - 1),
                label_style,
            ),
            Span::raw(" "),
            Span::raw(value),
        ]));
    }
    lines.push(Line::raw(""));
    if let Some(error) = &add.error {
        lines.push(Line::from(Span::styled(error.clone(), theme::FAILURE)));
    }
    (lines, cursor)
}

fn render_add(session: &SettingsSession, add: &NewCert, frame: &mut Frame) {
    let screen = frame.area();
    let (mut lines, cursor) = add_lines(add);
    lines.push(Line::from(Span::styled(help_text(session), theme::LABEL)));
    let width = screen
        .width
        .saturating_sub(8)
        .clamp(screen.width.min(30), 66);
    let wanted = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let height = wanted.min(screen.height);
    let area = Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Nouveau certificat ")
        .border_type(BorderType::Rounded)
        .border_style(theme::FOCUS)
        .style(theme::BACKGROUND);
    let inner = super::inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
    if let Some((line, column)) = cursor {
        place_cursor(frame, inner, line, column);
    }
}
