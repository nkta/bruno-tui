//! Rendu des popups de fichiers (`docs/edition-phase3.md` § 4) : nouvelle
//! requête, nouveau dossier, renommer, cloner. Pur : lit le popup et
//! dessine.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};

use super::edit_popup::CURSOR_STYLE;
use super::theme;
use crate::app::file_popup::{FilePopup, PopupField};

/// Largeur de la colonne des libellés.
const LABEL_WIDTH: usize = 11;

/// Lignes du popup ; la seconde valeur donne la ligne et la colonne du
/// curseur de texte, s'il y en a un.
pub fn file_popup_lines(popup: &FilePopup) -> (Vec<Line<'static>>, Option<(usize, usize)>) {
    let mut lines = Vec::new();
    let mut cursor = None;
    for (index, field) in popup.fields.iter().enumerate() {
        let current = index == popup.cursor;
        let label_style = if current { CURSOR_STYLE } else { theme::LABEL };
        let (label, value, input) = match field {
            PopupField::Name => ("Nom", popup.name.text().to_owned(), Some(&popup.name)),
            PopupField::Filename => (
                popup.filename_label(),
                popup.filename.text().to_owned(),
                Some(&popup.filename),
            ),
            PopupField::Kind => ("Type", format!("‹ {} ›", popup.kind_label()), None),
            PopupField::Method => ("Méthode", format!("‹ {} ›", popup.method_label()), None),
            PopupField::Url => ("URL", popup.url.text().to_owned(), Some(&popup.url)),
            PopupField::RenameFile => {
                let mark = if popup.rename_file { "x" } else { " " };
                let text = format!("[{mark}] {}", popup.rename_file_label());
                let style = if current {
                    CURSOR_STYLE
                } else {
                    theme::BACKGROUND
                };
                lines.push(Line::from(Span::styled(text, style)));
                continue;
            }
        };
        let mut spans = vec![
            Span::styled(
                format!("{label:<width$}", width = LABEL_WIDTH - 1),
                label_style,
            ),
            Span::raw(" "),
            Span::raw(value),
        ];
        if *field == PopupField::Filename && !popup.is_folder {
            spans.push(Span::styled(".bru", theme::LABEL));
        }
        if current && let Some(input) = input {
            cursor = Some((lines.len(), LABEL_WIDTH + input.cursor()));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::raw(""));
    if let Some(error) = &popup.error {
        lines.push(Line::from(Span::styled(error.clone(), theme::FAILURE)));
    }
    lines.push(Line::from(Span::styled(help_text(popup), theme::LABEL)));
    (lines, cursor)
}

/// Touches utiles, sur la dernière ligne du popup.
pub fn help_text(popup: &FilePopup) -> String {
    if popup.is_environment() {
        return format!(
            "Entrée {}  Ctrl+S {}  Échap annuler",
            popup.verb(),
            popup.verb()
        );
    }
    let choice = match popup.current_field() {
        PopupField::Kind | PopupField::Method => "←→ choisir  ",
        PopupField::RenameFile => "Espace cocher  ",
        _ => "",
    };
    let enter = if popup.on_last_field() {
        format!("Entrée {}  ", popup.verb())
    } else {
        "Entrée suivant  ".to_owned()
    };
    format!(
        "{choice}↑↓ ligne  {enter}Ctrl+S {}  Échap annuler",
        popup.verb()
    )
}

/// Cadre du popup, centré sur `screen`.
pub fn file_popup_area(screen: Rect, popup: &FilePopup) -> Rect {
    let (lines, _) = file_popup_lines(popup);
    let width = screen
        .width
        .saturating_sub(4)
        .clamp(screen.width.min(30), 70);
    let wanted = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let height = wanted.min(screen.height);
    Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Dessine le popup par-dessus l'écran.
pub fn render_file_popup(popup: &FilePopup, frame: &mut Frame) {
    let area = file_popup_area(frame.area(), popup);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(popup.title())
        .border_type(BorderType::Rounded)
        .border_style(theme::FOCUS)
        .style(theme::BACKGROUND);
    let inner = super::inner(area);
    frame.render_widget(block, area);
    let (lines, cursor) = file_popup_lines(popup);
    frame.render_widget(Paragraph::new(lines), inner);
    if let Some((line, column)) = cursor
        && line < usize::from(inner.height)
    {
        let row = u16::try_from(line).unwrap_or(0);
        let column = u16::try_from(column)
            .unwrap_or(u16::MAX)
            .min(inner.width.saturating_sub(1));
        frame.set_cursor_position((inner.x + column, inner.y + row));
    }
}
