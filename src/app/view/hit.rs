//! Zones cliquables, calculées sans I/O (`mouse-support`, design D2 et D3).
//!
//! Tout est dérivé du modèle et de [`layout_for`], exactement comme le
//! rendu : aucune géométrie n'est mémorisée par `view`. Le titre et la
//! barre d'état n'ont pas de zone : un événement qui y tombe est ignoré.

use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Paragraph, Wrap};

use super::detail::{
    SectionBox, detail_section_boxes, detail_text, is_box_border, line_width,
    response_gutter_width, response_text,
};
use super::detail::{URL_BAR_METHOD_SUFFIX, url_bar_values};
use super::{detail_wraps, inner, layout_for, response_text_area};
use crate::app::model::{DragPanel, EditableField, Model};

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
    /// Bouton « Lancer » de la barre d'URL.
    RunButton,
    /// Barre d'URL : champ Méthode ou URL sous le pointeur.
    UrlBar { field: EditableField },
}

/// Libellé du bouton « Lancer » du panneau Détail.
pub const RUN_BUTTON: &str = " ▶ Lancer (r) ";

/// Zone du bouton « Lancer », calée à droite sur la ligne intérieure de
/// la barre d'URL `url_bar` : présent seulement sur une requête, hors
/// session d'édition, et si la barre est assez large. Partagée par le
/// rendu et le clic.
pub fn run_button_area(model: &Model, url_bar: Rect) -> Option<Rect> {
    let is_request = matches!(
        model.selected_node(),
        Some(crate::collection::TreeNode::Request(_))
    );
    let width = u16::try_from(Line::raw(RUN_BUTTON).width()).ok()?;
    if !is_request || model.editing.is_some() || url_bar.width < width + 4 || url_bar.height < 3 {
        return None;
    }
    let inner_area = inner(url_bar);
    Some(Rect::new(
        inner_area.right() - width,
        inner_area.y,
        width,
        1,
    ))
}

/// Champ de la barre d'URL sous `position` : la méthode et sa flèche, ou
/// l'URL sur le reste de la ligne intérieure.
fn url_bar_field(model: &Model, url_bar: Rect, position: Position) -> Option<EditableField> {
    let inner_area = inner(url_bar);
    if !inner_area.contains(position) {
        return None;
    }
    let (method, _) = url_bar_values(model)?;
    let method_width = Line::raw(format!("{method}{URL_BAR_METHOD_SUFFIX}")).width();
    let column = usize::from(position.x - inner_area.x);
    Some(if column < method_width {
        EditableField::Method
    } else {
        EditableField::Url
    })
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
    let areas = layout_for(model.size, model.zoomed_panel())?;
    let position = Position::new(column, row);
    if areas.tree.contains(position) {
        let area = inner(areas.tree);
        let index = area
            .contains(position)
            .then(|| model.tree.offset + usize::from(row - area.y))
            .filter(|index| *index < model.tree.rows.len());
        return Some(Hit::Tree { row: index });
    }
    if run_button_area(model, areas.url_bar).is_some_and(|button| button.contains(position)) {
        return Some(Hit::RunButton);
    }
    if let Some(field) = url_bar_field(model, areas.url_bar, position) {
        return Some(Hit::UrlBar { field });
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
    let areas = layout_for(model.size, model.zoomed_panel())?;
    let area = text_area(panel, panel_area(&areas, panel));
    let (lines, wraps, scroll, boxes) = panel_content(model, panel);
    let last = u16::try_from(lines.len().checked_sub(1)?).unwrap_or(u16::MAX);
    if row < area.y {
        return Some(DragRow::Above);
    }
    if row >= area.bottom() {
        return Some(DragRow::Below);
    }
    let width = content_width(model, panel, area.width);
    let line = line_at_row(&lines, &boxes, wraps, width, scroll, row - area.y).unwrap_or(last);
    Some(DragRow::Line(line))
}

/// Zone du texte d'un panneau : son intérieur pour le détail, l'intérieur
/// sous la ligne des onglets pour la réponse.
fn text_area(panel: DragPanel, area: Rect) -> Rect {
    match panel {
        DragPanel::Detail => inner(area),
        DragPanel::Response => response_text_area(area),
    }
}

fn panel_area(areas: &super::Areas, panel: DragPanel) -> Rect {
    match panel {
        DragPanel::Detail => areas.detail,
        DragPanel::Response => areas.response,
    }
}

/// Lignes du panneau, présence du retour à la ligne, défilement et
/// boîtes de section (vide pour la réponse, qui n'en a pas), tels que
/// `view` les utilise (`add-boxed-detail-sections`).
fn panel_content(
    model: &Model,
    panel: DragPanel,
) -> (Vec<Line<'static>>, bool, u16, Vec<SectionBox>) {
    match panel {
        DragPanel::Detail => (
            detail_text(model).lines,
            detail_wraps(model),
            model.detail_scroll,
            detail_section_boxes(model),
        ),
        DragPanel::Response => (
            response_text(model).lines,
            true,
            model.response_scroll,
            Vec::new(),
        ),
    }
}

/// Largeur du texte dans l'intérieur d'un panneau : la réponse laisse la
/// place de sa gouttière de numéros de ligne.
fn content_width(model: &Model, panel: DragPanel, inner_width: u16) -> u16 {
    match panel {
        DragPanel::Detail => inner_width,
        DragPanel::Response => inner_width - response_gutter_width(model, inner_width),
    }
}

/// Ligne logique sous le pointeur, `None` sur la bordure ou après le
/// contenu.
fn content_line(model: &Model, panel: DragPanel, area: Rect, position: Position) -> Option<u16> {
    let area = text_area(panel, area);
    if !area.contains(position) {
        return None;
    }
    let (lines, wraps, scroll, boxes) = panel_content(model, panel);
    line_at_row(
        &lines,
        &boxes,
        wraps,
        content_width(model, panel, area.width),
        scroll,
        position.y - area.y,
    )
}

/// Ligne logique affichée à la ligne `row` de l'intérieur d'un panneau de
/// largeur `width`, défilé de `scroll`. Avec retour à la ligne,
/// `Paragraph` saute `scroll` lignes **rendues** : le calcul suit le rendu,
/// chaque ligne logique occupant le nombre de lignes que `Paragraph` lui
/// donne à cette largeur — réduite de 2 colonnes pour une ligne d'une
/// boîte de section (`add-boxed-detail-sections`, [`line_width`]), pour
/// ne jamais laisser diverger l'endroit affiché de l'endroit cliqué.
fn line_at_row(
    lines: &[Line<'static>],
    boxes: &[SectionBox],
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
        let height = if is_box_border(boxes, index) {
            1
        } else {
            Paragraph::new(Text::from(line.clone()))
                .wrap(Wrap { trim: false })
                .line_count(line_width(boxes, index, width))
                .max(1)
        };
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
        boxes: &[SectionBox],
        gutter: u16,
        hit_line: impl Fn(Hit) -> Option<u16>,
    ) -> usize {
        let (width, height) = model.size;
        let screen = render(model, width, height);
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille suffisante");
        // `area` donne directement la zone du texte du panneau.
        let panel = area(&areas);
        let mut checked = 0;
        for y in panel.y..panel.bottom() {
            let shown: String = screen[usize::from(y)]
                .chars()
                .skip(usize::from(panel.x + gutter))
                .take(usize::from(panel.width - gutter))
                .collect();
            let hit = hit_test(model, panel.x, y).expect("dans le panneau");
            match hit_line(hit) {
                Some(line) => {
                    let in_box = boxes.iter().any(|b| b.contains(usize::from(line)));
                    // Une bordure de boîte de section est dessinée par un
                    // vrai `Block` au rendu : son texte à l'écran n'a plus
                    // de rapport avec le texte logique de décor qu'elle
                    // remplace (`add-boxed-detail-sections`, design.md) ;
                    // seule la correspondance de ligne compte pour elle.
                    let is_border = is_box_border(boxes, usize::from(line));
                    if !is_border {
                        // Une ligne de contenu à l'intérieur d'une boîte
                        // porte les colonnes de bordure gauche et droite de
                        // la boîte de part et d'autre du contenu affiché.
                        let content: String = if in_box {
                            let chars: Vec<char> = shown.chars().collect();
                            if chars.len() >= 2 {
                                chars[1..chars.len() - 1].iter().collect()
                            } else {
                                String::new()
                            }
                        } else {
                            shown.clone()
                        };
                        let logical = compact(&lines[usize::from(line)]);
                        assert!(
                            logical.contains(&compact(&content)),
                            "ligne écran {y} `{shown}` hors de la ligne {line} `{logical}`"
                        );
                    }
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
            let boxes = detail_section_boxes(&model);
            for scroll in [0, 3] {
                model.detail_scroll = scroll;
                let checked =
                    assert_rows_match(&model, |a| inner(a.detail), &lines, &boxes, 0, detail_line);
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
        let view = request.view.clone();
        let stamp = FileStamp::capture(&model.loaded().expect("chargée").root.join(&path))
            .expect("instantané");
        let mut session = EditSession::new(path, stamp, view);
        session.state = EditState::Input(TextInput::new("https://{{host}}/items", false));
        model.editing = Some(session);
        model.focus = Focus::Detail;
        assert!(!detail_wraps(&model));
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
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
        let areas = layout_for(model.size, None).expect("taille suffisante");
        let gutter = response_gutter_width(&model, response_text_area(areas.response).width);
        assert!(gutter > 0, "le corps JSON doit être numéroté");
        let checked = assert_rows_match(
            &model,
            |a| response_text_area(a.response),
            &lines,
            &[],
            gutter,
            response_line,
        );
        assert!(checked > 0);
    }

    #[test]
    fn tree_rows_borders_and_other_areas() {
        let mut model = loaded_model((100, 12));
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
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
        let tall = inner(
            layout_for(short.size, short.zoomed_panel())
                .expect("taille")
                .tree,
        );
        assert_eq!(
            hit_test(&short, tall.x, tall.bottom() - 1),
            Some(Hit::Tree { row: None })
        );
        // Titre et barre d'état : aucune zone.
        assert_eq!(hit_test(&model, 1, areas.title.y), None);
        assert_eq!(hit_test(&model, 1, areas.status.y), None);
        // Réponse sans résultat : zone sans ligne.
        let response = response_text_area(areas.response);
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
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
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

    #[test]
    fn hit_test_in_zoomed_panels() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        model.size = (100, 30);
        model.zoom = true;

        // 1. Arbre zoomé : occupe tout le corps (x: 0..100, y: 1..29)
        model.focus = Focus::Tree;
        let hit = hit_test(&model, 80, 5);
        assert!(
            matches!(hit, Some(Hit::Tree { .. })),
            "un clic à droite en zoom arbre doit toucher l'arbre: {hit:?}"
        );

        // 2. Détail zoomé : occupe tout le corps
        model.focus = Focus::Detail;
        let hit = hit_test(&model, 5, 5);
        assert!(
            matches!(hit, Some(Hit::Detail { .. })),
            "un clic à gauche en zoom détail doit toucher le détail: {hit:?}"
        );

        // 3. Réponse zoomée : occupe tout le corps.
        model.focus = Focus::Response;
        let hit_response = hit_test(&model, 5, 6);
        assert!(
            matches!(hit_response, Some(Hit::Response { .. })),
            "un clic dans la réponse zoomée doit toucher la réponse: {hit_response:?}"
        );
    }
}
