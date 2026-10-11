//! Rendu du popup de recherche globale (`Ctrl+K`).
//!
//! Affiche un popup centré avec la saisie en direct en haut, un séparateur
//! discret avec le nombre de résultats, et la liste des requêtes correspondantes
//! (méthode, nom, chemin relatif et URL brute avec mise en évidence).

use std::ops::Range;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::theme;
use crate::app::global_search::{
    GlobalSearchPopup, SearchResult, collect_candidates, search, truncate_url_with_matches,
};
use crate::app::model::Model;

/// Style de mise en évidence d'une correspondance hors sélection.
pub const MATCH_STYLE: Style = Style::new()
    .bg(Color::Rgb(96, 78, 0))
    .fg(Color::Rgb(255, 235, 130))
    .add_modifier(Modifier::BOLD);

/// Style de mise en évidence d'une correspondance sur la ligne sélectionnée.
pub const SELECTED_MATCH_STYLE: Style = Style::new()
    .bg(Color::Rgb(120, 95, 0))
    .fg(Color::Rgb(255, 245, 160))
    .add_modifier(Modifier::BOLD);

/// Calcule la zone centrée pour le popup de recherche globale.
pub fn global_search_popup_area(screen: Rect) -> Rect {
    let width = 84.min(screen.width.saturating_sub(4)).max(44);
    let height = 20.min(screen.height.saturating_sub(2)).max(8);
    let x = screen.x + (screen.width.saturating_sub(width)) / 2;
    let y = screen.y + (screen.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// Hauteur utile de la liste des résultats dans le popup de recherche globale.
pub fn global_search_visible_height(model: &Model) -> usize {
    let screen = Rect::new(0, 0, model.size.0, model.size.1);
    let area = global_search_popup_area(screen);
    // Bordure haut (1) + saisie (1) + séparateur (1) + bordure bas (1) = 4
    area.height.saturating_sub(4) as usize
}

/// Découpe `text` en `Span`s selon `ranges` en appliquant `match_style` sur les correspondances
/// et `base_style` ailleurs.
pub fn build_highlighted_spans(
    text: &str,
    ranges: &[Range<usize>],
    base_style: Style,
    match_style: Style,
) -> Vec<Span<'static>> {
    if ranges.is_empty() {
        return vec![Span::styled(text.to_owned(), base_style)];
    }
    let mut spans = Vec::new();
    let mut last_idx = 0;
    for r in ranges {
        let start = r.start.max(last_idx).min(text.len());
        let end = r.end.min(text.len());
        if start > last_idx {
            spans.push(Span::styled(text[last_idx..start].to_owned(), base_style));
        }
        if end > start {
            spans.push(Span::styled(text[start..end].to_owned(), match_style));
            last_idx = end;
        }
    }
    if last_idx < text.len() {
        spans.push(Span::styled(text[last_idx..].to_owned(), base_style));
    }
    spans
}

/// Dessine le popup de recherche globale.
pub fn render_global_search_popup(
    model: &Model,
    popup: &GlobalSearchPopup,
    frame: &mut Frame,
    area: Rect,
) {
    let block = theme::bordered()
        .title(" Recherche globale [Ctrl+K] ")
        .border_style(theme::FOCUS);
    let inner_area = block.inner(area);

    frame.render_widget(Clear, area);
    frame.render_widget(theme::bordered().style(theme::BACKGROUND), area);
    frame.render_widget(block, area);

    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }

    // 1. Ligne de saisie en direct
    let prompt_span = Span::styled(" > ", theme::FOCUS);
    let query_text = popup.input.text();
    let query_span = Span::styled(
        query_text,
        Style::new()
            .fg(Color::Rgb(245, 245, 245))
            .add_modifier(Modifier::BOLD),
    );
    let prompt_line = Line::from(vec![prompt_span, query_span]);
    frame.render_widget(
        Paragraph::new(prompt_line),
        Rect::new(inner_area.x, inner_area.y, inner_area.width, 1),
    );

    // Positionnement du curseur dans la zone de saisie
    let cursor_x =
        inner_area.x + 3 + (popup.input.cursor() as u16).min(inner_area.width.saturating_sub(4));
    frame.set_cursor_position((cursor_x, inner_area.y));

    // 2. Séparateur avec décompte
    let candidates = model
        .loaded()
        .map(|c| collect_candidates(&c.tree))
        .unwrap_or_default();
    let results = search(&candidates, query_text);

    let count = results.len();
    let count_label = if count == 0 {
        "── Aucun résultat ".to_owned()
    } else if count == 1 {
        "── 1 résultat ".to_owned()
    } else {
        format!("── {count} résultats ")
    };
    let remaining = (inner_area.width as usize).saturating_sub(count_label.chars().count());
    let sep_text = format!("{}{}", count_label, "─".repeat(remaining));
    frame.render_widget(
        Paragraph::new(Line::styled(sep_text, theme::BORDER)),
        Rect::new(inner_area.x, inner_area.y + 1, inner_area.width, 1),
    );

    // 3. Liste des résultats
    let list_area = Rect::new(
        inner_area.x,
        inner_area.y + 2,
        inner_area.width,
        inner_area.height.saturating_sub(2),
    );
    let visible_height = list_area.height as usize;

    if results.is_empty() {
        let msg = if query_text.trim().is_empty() {
            "  Aucune requête dans la collection"
        } else {
            "  Aucun résultat"
        };
        frame.render_widget(
            Paragraph::new(Line::styled(msg, theme::EMPTY_MESSAGE)),
            list_area,
        );
        return;
    }

    let mut lines = Vec::new();
    for i in 0..visible_height {
        let idx = popup.scroll + i;
        let Some(res) = results.get(idx) else {
            break;
        };
        let is_selected = idx == popup.selected;

        let line = format_result_line(res, inner_area.width as usize, is_selected);
        lines.push(line);
    }

    frame.render_widget(Paragraph::new(lines), list_area);
}

/// Formate une ligne de résultat pour la liste.
fn format_result_line(
    res: &SearchResult,
    available_width: usize,
    is_selected: bool,
) -> Line<'static> {
    let (match_style, line_bg) = if is_selected {
        (SELECTED_MATCH_STYLE, Some(Color::Rgb(58, 58, 62)))
    } else {
        (MATCH_STYLE, None)
    };

    let base_method_style = theme::method_style(&res.candidate.method);
    let base_name_style = if is_selected {
        theme::SELECTION
    } else {
        Style::new()
            .fg(Color::Rgb(240, 240, 240))
            .add_modifier(Modifier::BOLD)
    };
    let base_path_style = if is_selected {
        Style::new()
            .bg(Color::Rgb(58, 58, 62))
            .fg(Color::Rgb(180, 180, 185))
    } else {
        theme::LABEL
    };
    let base_url_style = if is_selected {
        Style::new()
            .bg(Color::Rgb(58, 58, 62))
            .fg(Color::Rgb(160, 190, 210))
    } else {
        Style::new().fg(Color::Rgb(130, 170, 195))
    };

    let mut spans = Vec::new();

    // Curseur de sélection : "▶ " si sélectionné, "  " sinon
    let pointer = if is_selected { "▶ " } else { "  " };
    let pointer_style = if is_selected {
        theme::FOCUS
    } else {
        Style::new()
    };
    spans.push(Span::styled(pointer, pointer_style));

    // Méthode (ex: "GET   ")
    let method_text = format!("{:<6} ", res.candidate.method);
    let method_spans = build_highlighted_spans(
        &method_text,
        &res.method_matches,
        base_method_style,
        match_style,
    );
    spans.extend(method_spans);

    // Détermination des largeurs pour Nom, Chemin et URL
    // Méthode = 9 colonnes (2 pointeur + 7)
    let remaining_width = available_width.saturating_sub(9);

    // On réserve de la place pour que l'URL ait au moins un espace décent
    let max_name_width = (remaining_width * 30 / 100).clamp(12, 28);
    let max_path_width = (remaining_width * 25 / 100).clamp(10, 24);

    // Nom (tronqué si trop long)
    let name_str = &res.candidate.name;
    let (display_name, name_matches) = if name_str.chars().count() > max_name_width {
        let (t, m) = truncate_url_with_matches(name_str, max_name_width, &res.name_matches);
        (format!("{t:<max_name_width$} "), m)
    } else {
        (
            format!("{name_str:<max_name_width$} "),
            res.name_matches.clone(),
        )
    };
    spans.extend(build_highlighted_spans(
        &display_name,
        &name_matches,
        base_name_style,
        match_style,
    ));

    // Chemin relatif (tronqué si trop long)
    let path_raw = res.candidate.path.to_string_lossy();
    let (display_path, path_matches) = if path_raw.chars().count() > max_path_width {
        let (t, m) = truncate_url_with_matches(&path_raw, max_path_width, &res.path_matches);
        (format!("{t:<max_path_width$} "), m)
    } else {
        (
            format!("{path_raw:<max_path_width$} "),
            res.path_matches.clone(),
        )
    };
    spans.extend(build_highlighted_spans(
        &display_path,
        &path_matches,
        base_path_style,
        match_style,
    ));

    // URL brute restante
    let used_so_far: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let max_url_width = available_width.saturating_sub(used_so_far);
    let (display_url, url_matches) =
        truncate_url_with_matches(&res.candidate.url, max_url_width, &res.url_matches);
    spans.extend(build_highlighted_spans(
        &display_url,
        &url_matches,
        base_url_style,
        match_style,
    ));

    // Si sélectionné, remplir le reste de la ligne avec le fond de sélection
    if let Some(bg) = line_bg {
        let total_chars: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        if total_chars < available_width {
            let pad = " ".repeat(available_width - total_chars);
            spans.push(Span::styled(pad, Style::new().bg(bg)));
        }
        // Appliquer le fond de sélection sur les spans qui n'ont pas de fond propre
        for span in &mut spans {
            if span.style.bg.is_none() {
                span.style = span.style.bg(bg);
            }
        }
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::global_search::SearchCandidate;
    use std::path::PathBuf;

    #[test]
    fn popup_area_centered_within_screen() {
        let screen = Rect::new(0, 0, 100, 30);
        let area = global_search_popup_area(screen);
        assert!(area.width <= screen.width);
        assert!(area.height <= screen.height);
        assert_eq!(area.x, (screen.width - area.width) / 2);
        assert_eq!(area.y, (screen.height - area.height) / 2);
    }

    #[test]
    fn format_result_line_produces_highlighted_spans() {
        let candidate = SearchCandidate {
            name: "List Users".into(),
            method: "GET".into(),
            path: PathBuf::from("users/list.bru"),
            url: "https://{{host}}/api/users".into(),
        };
        let res = SearchResult {
            candidate,
            score: 100,
            name_matches: vec![Range { start: 0, end: 4 }], // "List"
            method_matches: vec![Range { start: 0, end: 3 }], // "GET"
            path_matches: Vec::new(),
            url_matches: vec![Range { start: 8, end: 16 }], // "{{host}}"
        };

        let line = format_result_line(&res, 80, false);
        let texts: Vec<&str> = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(texts.contains(&"List"), "{texts:?}");
        assert!(texts.contains(&"GET"), "{texts:?}");
    }
}
