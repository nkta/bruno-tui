//! Rendu du popup « Déplacer vers » (`docs/edition-phase4.md` § 4.2). Pur :
//! lit le popup et dessine.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Wrap};

use super::edit_popup::CURSOR_STYLE;
use super::theme;
use crate::app::move_popup::MovePopup;

/// Libellé de la ligne du filtre.
const FILTER_LABEL: &str = "Vers ";

/// Lignes de liste visibles au plus ; la liste défile au-delà.
const MAX_LIST_ROWS: usize = 10;

/// Touches utiles, sur la dernière ligne du popup.
pub fn help_text() -> String {
    "↑↓ choisir  Entrée déplacer  Échap annuler".to_owned()
}

/// Lignes du popup ; la seconde valeur donne la ligne et la colonne du
/// curseur de saisie.
pub fn move_popup_lines(popup: &MovePopup) -> (Vec<Line<'static>>, (usize, usize)) {
    let mut lines = vec![Line::from(vec![
        Span::styled(FILTER_LABEL, theme::LABEL),
        Span::raw(popup.filter.text().to_owned()),
    ])];
    let cursor = (0, FILTER_LABEL.chars().count() + popup.filter.cursor());
    let visible = popup.visible();
    if visible.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (aucun dossier ne correspond)",
            theme::LABEL,
        )));
    }
    // La ligne choisie reste dans la fenêtre.
    let first = popup.selected.saturating_sub(MAX_LIST_ROWS - 1);
    for (index, destination) in visible.iter().enumerate().skip(first).take(MAX_LIST_ROWS) {
        let current = index == popup.selected;
        let marker = if current { "▸ " } else { "  " };
        let text = format!(
            "{marker}{}{}",
            "  ".repeat(destination.depth),
            destination.label
        );
        let style = if current {
            CURSOR_STYLE
        } else {
            theme::BACKGROUND
        };
        lines.push(Line::from(Span::styled(text, style)));
    }
    lines.push(Line::raw(""));
    if let Some(name) = popup
        .selected_destination()
        .and_then(|destination| destination.renamed.as_deref())
    {
        lines.push(Line::from(Span::styled(
            format!("Sera nommé : {name}"),
            theme::LABEL,
        )));
    }
    if let Some(error) = &popup.error {
        lines.push(Line::from(Span::styled(error.clone(), theme::FAILURE)));
    }
    lines.push(Line::from(Span::styled(help_text(), theme::LABEL)));
    (lines, cursor)
}

/// Cadre du popup, centré sur `screen`.
pub fn move_popup_area(screen: Rect, popup: &MovePopup) -> Rect {
    let (lines, _) = move_popup_lines(popup);
    let width = screen
        .width
        .saturating_sub(4)
        .clamp(screen.width.min(30), 70);
    // Un refus long passe à la ligne : il prend plusieurs lignes.
    let inner_width = usize::from(width.saturating_sub(2)).max(1);
    let error_rows = popup.error.as_ref().map_or(0, |error| {
        error
            .chars()
            .count()
            .div_ceil(inner_width)
            .saturating_sub(1)
    });
    let wanted = u16::try_from(lines.len() + error_rows + 2).unwrap_or(u16::MAX);
    let height = wanted.min(screen.height);
    Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

/// Dessine le popup par-dessus l'écran.
pub fn render_move_popup(popup: &MovePopup, frame: &mut Frame) {
    let area = move_popup_area(frame.area(), popup);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(popup.title())
        .border_type(BorderType::Rounded)
        .border_style(theme::FOCUS)
        .style(theme::BACKGROUND);
    let inner = super::inner(area);
    frame.render_widget(block, area);
    let (lines, (line, column)) = move_popup_lines(popup);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    if line < usize::from(inner.height) {
        let row = u16::try_from(line).unwrap_or(0);
        let column = u16::try_from(column)
            .unwrap_or(u16::MAX)
            .min(inner.width.saturating_sub(1));
        frame.set_cursor_position((inner.x + column, inner.y + row));
    }
}
