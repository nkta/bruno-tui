//! Zones cliquables, calculées sans I/O (`mouse-support`, design D2 et D3).
//!
//! Tout est dérivé du modèle et de [`layout_for`], exactement comme le
//! rendu : aucune géométrie n'est mémorisée par `view`. Le panneau Statut,
//! le titre et la barre d'état n'ont pas de zone : un événement qui y
//! tombe est ignoré.

use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use super::detail::{detail_text, response_text};
use super::{detail_wraps, inner, layout_for};
use crate::app::model::{DragPanel, Model};

/// Cible d'un événement souris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Arbre : indice dans `tree.rows`, `None` sur la bordure ou sous la
    /// dernière ligne.
    Tree { row: Option<usize> },
    /// Détail : ligne logique, `None` sur la bordure ou après le contenu.
    Detail { line: Option<u16> },
    /// Réponse : même principe que le détail.
    Response { line: Option<u16> },
}

/// Position verticale du pointeur pendant un glisser, relativement à
/// l'intérieur du panneau où le glisser a commencé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragRow {
    /// Au-dessus de la première ligne intérieure.
    Above,
    /// Ligne logique sous le pointeur, ramenée à la dernière ligne du
    /// contenu quand le pointeur est plus bas que le contenu.
    Line(u16),
    /// Au-dessous de la dernière ligne intérieure.
    Below,
}

/// Panneau et ligne sous la position `(column, row)` du terminal ; `None`
/// hors de l'arbre, du détail et de la réponse, ou sous la taille minimale.
pub fn hit_test(model: &Model, column: u16, row: u16) -> Option<Hit> {
    let areas = layout_for(model.size)?;
    let position = Position::new(column, row);
    if areas.tree.contains(position) {
        let area = inner(areas.tree);
        let index = area
            .contains(position)
            .then(|| model.tree.offset + usize::from(row - area.y))
            .filter(|index| *index < model.tree.rows.len());
        return Some(Hit::Tree { row: index });
    }
    if areas.detail.contains(position) {
        return Some(Hit::Detail {
            line: content_line(model, DragPanel::Detail, areas.detail, position),
        });
    }
    if areas.response.contains(position) {
        return Some(Hit::Response {
            line: content_line(model, DragPanel::Response, areas.response, position),
        });
    }
    None
}

/// Position du pointeur pendant un glisser commencé dans `panel`, quelle
/// que soit la colonne. `None` si le panneau n'a aucun contenu ou sous la
/// taille minimale.
pub fn drag_row(model: &Model, panel: DragPanel, row: u16) -> Option<DragRow> {
    let areas = layout_for(model.size)?;
    let area = inner(panel_area(&areas, panel));
    let (lines, wraps, scroll) = panel_content(model, panel);
    let last = u16::try_from(lines.len().checked_sub(1)?).unwrap_or(u16::MAX);
    if row < area.y {
        return Some(DragRow::Above);
    }
    if row >= area.bottom() {
        return Some(DragRow::Below);
    }
    let line = line_at_row(&lines, wraps, area.width, scroll, row - area.y).unwrap_or(last);
    Some(DragRow::Line(line))
}

fn panel_area(areas: &super::Areas, panel: DragPanel) -> Rect {
    match panel {
        DragPanel::Detail => areas.detail,
        DragPanel::Response => areas.response,
    }
}

/// Lignes du panneau, présence du retour à la ligne et défilement, tels
/// que `view` les utilise.
fn panel_content(model: &Model, panel: DragPanel) -> (Vec<Line<'static>>, bool, u16) {
    match panel {
        DragPanel::Detail => (
            detail_text(model).lines,
            detail_wraps(model),
            model.detail_scroll,
        ),
        DragPanel::Response => (response_text(model).lines, true, model.response_scroll),
    }
}

fn content_line(model: &Model, panel: DragPanel, area: Rect, position: Position) -> Option<u16> {
    let area = inner(area);
    if !area.contains(position) {
        return None;
    }
    let (lines, wraps, scroll) = panel_content(model, panel);
    line_at_row(&lines, wraps, area.width, scroll, position.y - area.y)
}

/// Ligne logique affichée à la ligne `row` de l'intérieur d'un panneau de
/// largeur `width`, défilé de `scroll`. Avec retour à la ligne,
/// `Paragraph` saute `scroll` lignes **rendues** : le calcul suit le rendu,
/// chaque ligne logique occupant le nombre de lignes que `Paragraph` lui
/// donne à cette largeur.
fn line_at_row(
    lines: &[Line<'static>],
    wraps: bool,
    width: u16,
    scroll: u16,
    row: u16,
) -> Option<u16> {
    let target = usize::from(scroll) + usize::from(row);
    if !wraps {
        return (target < lines.len()).then(|| u16::try_from(target).unwrap_or(u16::MAX));
    }
    let mut start = 0;
    for (index, line) in lines.iter().enumerate() {
        let height = Paragraph::new(Text::from(line.clone()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .max(1);
        if target < start + height {
            return u16::try_from(index).ok();
        }
        start += height;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::{EditSession, EditState, Focus};
    use crate::app::test_support::{loaded_model, render, runner_probe_model, select};
    use crate::app::text_input::TextInput;
    use crate::app::view::detail::{plain_lines, response_plain_lines};
    use crate::app::view::{Areas, layout_for};
    use crate::collection::TreeNode;
    use crate::writer::FileStamp;

    fn compact(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    /// Pour chaque ligne intérieure du panneau, le texte affiché à l'écran
    /// appartient bien à la ligne logique renvoyée par le hit-testing.
    fn assert_rows_match(
        model: &Model,
        area: impl Fn(&Areas) -> Rect,
        lines: &[String],
        hit_line: impl Fn(Hit) -> Option<u16>,
    ) -> usize {
        let (width, height) = model.size;
        let screen = render(model, width, height);
        let areas = layout_for(model.size).expect("taille suffisante");
        let panel = inner(area(&areas));
        let mut checked = 0;
        for y in panel.y..panel.bottom() {
            let shown: String = screen[usize::from(y)]
                .chars()
                .skip(usize::from(panel.x))
                .take(usize::from(panel.width))
                .collect();
            let hit = hit_test(model, panel.x, y).expect("dans le panneau");
            match hit_line(hit) {
                Some(line) => {
                    let logical = compact(&lines[usize::from(line)]);
                    assert!(
                        logical.contains(&compact(&shown)),
                        "ligne écran {y} `{shown}` hors de la ligne {line} `{logical}`"
                    );
                    checked += 1;
                }
                None => assert!(shown.trim().is_empty(), "ligne {y} : `{shown}`"),
            }
        }
        checked
    }

    fn detail_line(hit: Hit) -> Option<u16> {
        match hit {
            Hit::Detail { line } => line,
            other => panic!("détail attendu : {other:?}"),
        }
    }

    fn response_line(hit: Hit) -> Option<u16> {
        match hit {
            Hit::Response { line } => line,
            other => panic!("réponse attendue : {other:?}"),
        }
    }

    #[test]
    fn detail_rows_follow_wrapping_and_scroll() {
        // 60 colonnes : le détail est au plancher, les lignes longues sont
        // retournées.
        for size in [(60, 40), (140, 40)] {
            let mut model = loaded_model(size);
            select(&mut model, "scripted.bru");
            let lines = plain_lines(&model);
            for scroll in [0, 3] {
                model.detail_scroll = scroll;
                let checked = assert_rows_match(&model, |a| a.detail, &lines, detail_line);
                assert!(checked > 0, "{size:?} {scroll}");
            }
        }
    }

    #[test]
    fn detail_rows_without_wrap_during_input() {
        let mut model = loaded_model((60, 30));
        select(&mut model, "post-json.bru");
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let path = request.path.clone();
        let fields = crate::app::model::EditableField::list_for(&request.view);
        let stamp = FileStamp::capture(&model.loaded().expect("chargée").root.join(&path))
            .expect("instantané");
        model.editing = Some(EditSession {
            path,
            stamp,
            fields,
            cursor: 0,
            state: EditState::Input(TextInput::new("https://{{host}}/items", false)),
            pending: Vec::new(),
            dirty: false,
            hscroll: 0,
        });
        model.focus = Focus::Detail;
        assert!(!detail_wraps(&model));
        let areas = layout_for(model.size).expect("taille");
        let panel = inner(areas.detail);
        for y in 0..panel.height {
            let expected = u16::try_from(usize::from(y))
                .ok()
                .filter(|l| usize::from(*l) < plain_lines(&model).len());
            assert_eq!(
                hit_test(&model, panel.x, panel.y + y),
                Some(Hit::Detail { line: expected })
            );
        }
    }

    #[test]
    fn response_rows_follow_wrapping() {
        let mut model = runner_probe_model();
        model.size = (60, 30);
        select(&mut model, "green.bru");
        let lines = response_plain_lines(&model);
        assert!(!lines.is_empty());
        let checked = assert_rows_match(&model, |a| a.response, &lines, response_line);
        assert!(checked > 0);
    }

    #[test]
    fn tree_rows_borders_and_other_areas() {
        let mut model = loaded_model((100, 12));
        let areas = layout_for(model.size).expect("taille");
        let tree = inner(areas.tree);
        assert_eq!(
            hit_test(&model, tree.x, tree.y),
            Some(Hit::Tree { row: Some(0) })
        );
        model.tree.offset = 4;
        assert_eq!(
            hit_test(&model, tree.x + 3, tree.y + 1),
            Some(Hit::Tree { row: Some(5) })
        );
        // Bordures : zone du panneau, sans ligne.
        assert_eq!(
            hit_test(&model, areas.tree.x, tree.y),
            Some(Hit::Tree { row: None })
        );
        assert_eq!(
            hit_test(&model, areas.detail.x, areas.detail.y),
            Some(Hit::Detail { line: None })
        );
        // Sous la dernière ligne de l'arbre.
        model.tree.offset = 0;
        let mut short = loaded_model((100, 40));
        short.tree.offset = 0;
        let tall = inner(layout_for(short.size).expect("taille").tree);
        assert_eq!(
            hit_test(&short, tall.x, tall.bottom() - 1),
            Some(Hit::Tree { row: None })
        );
        // Titre, barre d'état et panneau Statut : aucune zone.
        assert_eq!(hit_test(&model, 1, areas.title.y), None);
        assert_eq!(hit_test(&model, 1, areas.status.y), None);
        let status = areas.response_status;
        assert_eq!(hit_test(&model, status.x + 2, status.y + 1), None);
        // Réponse sans résultat : zone sans ligne.
        let response = inner(areas.response);
        assert_eq!(
            hit_test(&model, response.x, response.y),
            Some(Hit::Response { line: None })
        );
        // Terminal trop petit.
        model.size = (30, 8);
        assert_eq!(hit_test(&model, 1, 1), None);
    }

    #[test]
    fn drag_rows_outside_and_beyond_content() {
        let mut model = loaded_model((140, 60));
        select(&mut model, "simple-get.bru");
        let areas = layout_for(model.size).expect("taille");
        let panel = inner(areas.detail);
        let last = u16::try_from(plain_lines(&model).len() - 1).expect("court");
        assert_eq!(
            drag_row(&model, DragPanel::Detail, panel.y - 1),
            Some(DragRow::Above)
        );
        assert_eq!(
            drag_row(&model, DragPanel::Detail, panel.bottom()),
            Some(DragRow::Below)
        );
        assert_eq!(
            drag_row(&model, DragPanel::Detail, panel.y + 1),
            Some(DragRow::Line(1))
        );
        assert_eq!(
            drag_row(&model, DragPanel::Detail, panel.bottom() - 1),
            Some(DragRow::Line(last))
        );
        // Réponse sans contenu.
        assert_eq!(drag_row(&model, DragPanel::Response, panel.y), None);
    }
}
