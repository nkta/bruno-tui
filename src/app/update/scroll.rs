//! Défilement, calcul des limites d'affichage et sélection d'onglets.

use std::ops::RangeInclusive;

use crate::app::message::Message;
use crate::app::model::{Model, RequestTab};
use crate::app::view::detail::{detail_text, response_text};
use crate::app::view::{inner, layout_for, response_text_area};
use crate::collection::TreeNode;

/// Nombre de lignes logiques du détail (`Text.lines`, une entrée par
/// `Line`). Unité commune au défilement, à la recherche et à la sélection
/// visuelle : `ratatui::Paragraph` ne permet pas de récupérer le texte
/// réellement retourné à la ligne à une largeur donnée (seul un compte est
/// public, `Paragraph::line_count`), donc le défilement avance d'une ligne
/// logique à la fois plutôt que d'une ligne visuelle après retour à la
/// ligne. Le rendu continue d'envelopper visuellement les lignes trop
/// longues (`Wrap`) ; seule l'unité de défilement change.
pub(crate) fn detail_line_count(model: &Model) -> usize {
    detail_text(model).lines.len()
}

/// Même principe que [`detail_line_count`], pour la réponse
/// (`split-request-response-panels`).
pub(crate) fn response_line_count(model: &Model) -> usize {
    response_text(model).lines.len()
}

/// Défilement maximal étant donné un nombre de lignes logiques et la
/// hauteur intérieure du panneau. Pure : partagée par le détail et la
/// réponse, qui ne diffèrent que par la source du nombre de lignes et de
/// la hauteur (`design.md`, D3).
pub(crate) fn max_scroll(line_count: usize, height: u16) -> u16 {
    let max = line_count.saturating_sub(usize::from(height));
    u16::try_from(max).unwrap_or(u16::MAX)
}

/// Dernière ligne visible d'un panneau étant donné son défilement
/// courant, sa hauteur intérieure et son nombre de lignes logiques.
/// Toujours la vraie dernière ligne du contenu quand `scroll` est à son
/// maximum (voir le calcul dans `design.md`, D5 de
/// `improve-visual-design`) : c'est ce qui permet à une sélection
/// visuelle d'atteindre des lignes qu'aucun sommet de panneau ne peut
/// jamais atteindre seul.
pub(crate) fn viewport_bottom(scroll: u16, height: u16, line_count: usize) -> u16 {
    if line_count == 0 {
        return 0;
    }
    let bottom = scroll.saturating_add(height.saturating_sub(1));
    bottom.min(u16::try_from(line_count - 1).unwrap_or(u16::MAX))
}

/// Défilement maximal du détail : lignes logiques moins la hauteur du
/// panneau.
pub(crate) fn detail_max_scroll(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size, model.zoomed_panel()) else {
        return 0;
    };
    let inner_area = inner(areas.detail);
    // Le détail d'une requête est rendu par tampon virtuel : son
    // défilement compte des lignes affichées, retours à la ligne compris.
    if let Some(TreeNode::Request(_)) = model.selected_node() {
        let lines = detail_text(model).lines;
        let boxes = crate::app::view::detail::detail_section_boxes(model);
        let height = crate::app::view::detail::content_height(
            &lines,
            &boxes,
            inner_area.width,
            crate::app::view::detail_wraps(model),
        );
        return max_scroll(usize::from(height), inner_area.height);
    }
    max_scroll(detail_line_count(model), inner_area.height)
}

/// Même principe que [`detail_max_scroll`], pour la réponse.
pub(crate) fn response_max_scroll(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size, model.zoomed_panel()) else {
        return 0;
    };
    max_scroll(
        response_line_count(model),
        response_text_area(areas.response).height,
    )
}

/// Dernière ligne visible du panneau de détail, à la position de
/// défilement courante.
pub(crate) fn bottom_of_viewport(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size, model.zoomed_panel()) else {
        return model.detail_scroll;
    };
    viewport_bottom(
        model.detail_scroll,
        inner(areas.detail).height,
        detail_line_count(model),
    )
}

/// Même principe que [`bottom_of_viewport`], pour la réponse.
pub(crate) fn response_bottom_of_viewport(model: &Model) -> u16 {
    let Some(areas) = layout_for(model.size, model.zoomed_panel()) else {
        return model.response_scroll;
    };
    viewport_bottom(
        model.response_scroll,
        response_text_area(areas.response).height,
        response_line_count(model),
    )
}

/// Plage de lignes couvertes par la sélection visuelle active, s'il y en a
/// une. Jamais stockée à part : dérivée de l'ancre et du défilement
/// courant à chaque appel.
pub(crate) fn selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let selection = model.detail_selection?;
    let bottom = selection.head.unwrap_or_else(|| bottom_of_viewport(model));
    Some(selection.anchor.min(bottom)..=selection.anchor.max(bottom))
}

/// Même principe que [`selection_range`], pour la réponse.
pub(crate) fn response_selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let selection = model.response_selection?;
    let bottom = selection
        .head
        .unwrap_or_else(|| response_bottom_of_viewport(model));
    Some(selection.anchor.min(bottom)..=selection.anchor.max(bottom))
}

pub(crate) fn scroll_detail(model: &mut Model, message: Message) {
    let page = layout_for(model.size, model.zoomed_panel())
        .map_or(1, |areas| inner(areas.detail).height.max(1));
    let max = detail_max_scroll(model);
    let scroll = model.detail_scroll;
    model.detail_scroll = match message {
        Message::Up => scroll.saturating_sub(1),
        Message::Down => scroll.saturating_add(1).min(max),
        Message::PageUp => scroll.saturating_sub(page),
        Message::PageDown => scroll.saturating_add(page).min(max),
        Message::Home => 0,
        Message::End => max,
        _ => scroll,
    };
}

/// Même principe que [`scroll_detail`], pour la réponse.
pub(crate) fn scroll_response(model: &mut Model, message: Message) {
    let page = layout_for(model.size, model.zoomed_panel())
        .map_or(1, |areas| response_text_area(areas.response).height.max(1));
    let max = response_max_scroll(model);
    let scroll = model.response_scroll;
    model.response_scroll = match message {
        Message::Up => scroll.saturating_sub(1),
        Message::Down => scroll.saturating_add(1).min(max),
        Message::PageUp => scroll.saturating_sub(page),
        Message::PageDown => scroll.saturating_add(page).min(max),
        Message::Home => 0,
        Message::End => max,
        _ => scroll,
    };
}

/// Réinitialise l'état d'affichage propre à un onglet de la réponse
/// (défilement, sélection, mise en évidence de recherche) : appelé à
/// chaque changement d'onglet, comme `clear_detail_view_state` l'est à
/// chaque changement de sélection (`response-tabs`).
pub(crate) fn reset_response_tab_view_state(model: &mut Model) {
    model.response_scroll = 0;
    model.response_match = None;
    model.response_selection = None;
}

/// Affiche l'onglet `tab` du détail d'une requête, défilement, sélection
/// et correspondance de recherche remis à zéro comme pour la réponse.
pub(crate) fn select_request_tab(model: &mut Model, tab: RequestTab) {
    if model.request_tab == tab {
        return;
    }
    model.request_tab = tab;
    model.detail_scroll = 0;
    model.detail_match = None;
    model.detail_selection = None;
}

pub(crate) fn previous_response_tab(model: &mut Model) {
    model.response_tab = model.response_tab.previous();
    reset_response_tab_view_state(model);
}

pub(crate) fn next_response_tab(model: &mut Model) {
    model.response_tab = model.response_tab.next();
    reset_response_tab_view_state(model);
}

/// Hauteur utile du corps de l'écran (pour les panneaux plein corps).
pub(crate) fn body_height(model: &Model) -> usize {
    layout_for(model.size, model.zoomed_panel())
        .map_or(1, |areas| usize::from(inner(areas.body).height).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::message::Message;
    use crate::app::model::{DetailSelection, Focus, RequestTab, ResponseTab};
    use crate::app::test_support::{loaded_model, runner_probe_model, select};
    use crate::app::update::update;

    #[test]
    fn detail_scroll_is_bounded_and_reset() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::End);
        let max = model.detail_scroll;
        let page = inner(layout_for(model.size, None).expect("taille").detail).height;
        assert!(max > 0, "le détail de scripted doit dépasser {page} lignes");
        assert_eq!(max, detail_max_scroll(&model));
        update(&mut model, Message::Down);
        assert_eq!(model.detail_scroll, max);
        update(&mut model, Message::PageUp);
        assert_eq!(model.detail_scroll, max.saturating_sub(page));
        update(&mut model, Message::Home);
        assert_eq!(model.detail_scroll, 0);
        update(&mut model, Message::PageDown);
        assert_eq!(model.detail_scroll, page.min(max));

        let before = model.tree.selected;
        update(&mut model, Message::FocusTree);
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, before + 1);
        assert_eq!(model.detail_scroll, 0);
    }

    /// ←/→ changent l'onglet du détail hors session ; en session, l'onglet
    /// affiché suit le champ sous le curseur et le reste après la session.
    #[test]
    fn request_tabs_follow_arrows_and_the_edit_cursor() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(model.shown_request_tab(), RequestTab::Params);
        update(&mut model, Message::Right);
        assert_eq!(model.shown_request_tab(), RequestTab::Body);
        update(&mut model, Message::Left);
        update(&mut model, Message::Left);
        assert_eq!(model.shown_request_tab(), RequestTab::Settings);

        // Session : StartEdit ouvre le popup de l'onglet actif (Settings)
        update(&mut model, Message::StartEdit);
        assert_eq!(
            model.editing.as_ref().map(|s| s.scope),
            Some(crate::app::model::EditScope::Tab(RequestTab::Settings))
        );
        // ←/→ ne changent pas d'onglet pendant la session
        update(&mut model, Message::Right);
        assert_eq!(model.shown_request_tab(), RequestTab::Settings);
        update(&mut model, Message::FocusTree);
        assert!(model.editing.is_none());
        assert_eq!(model.request_tab, RequestTab::Settings);
    }

    #[test]
    fn last_detail_line_is_reachable() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        model.request_tab = RequestTab::Assert;
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::End);
        let lines = crate::app::test_support::render(&model, 100, 12);
        let screen = lines.join("\n");
        assert!(screen.contains("res.body.ok: isTrue"), "{screen}");
    }

    #[test]
    fn next_focus_cycles_through_tree_detail_and_response() {
        let mut model = loaded_model((100, 30));
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Response);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn response_tab_cycles_forward_with_right() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        assert_eq!(model.focus, Focus::Response);
        assert_eq!(model.response_tab, ResponseTab::Body);

        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Headers);
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Tests);
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Timeline);
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Console);
        // Cycle circulaire : un cinquième Right revient à Corps.
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Body);
    }

    #[test]
    fn response_tab_cycles_backward_with_left() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        assert_eq!(model.response_tab, ResponseTab::Body);

        // Cycle circulaire vers l'arrière : depuis Corps, Left va à Console.
        update(&mut model, Message::Left);
        assert_eq!(model.response_tab, ResponseTab::Console);
        update(&mut model, Message::Left);
        assert_eq!(model.response_tab, ResponseTab::Timeline);
        update(&mut model, Message::Left);
        assert_eq!(model.response_tab, ResponseTab::Tests);
    }

    #[test]
    fn changing_selection_resets_the_active_response_tab() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        update(&mut model, Message::Right);
        update(&mut model, Message::Right);
        assert_eq!(model.response_tab, ResponseTab::Tests);

        update(&mut model, Message::FocusTree);
        update(&mut model, Message::Down);
        assert_eq!(model.response_tab, ResponseTab::Body);
    }

    #[test]
    fn opening_the_filter_switches_to_the_body_tab() {
        let mut model = runner_probe_model();
        select(&mut model, "json.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        update(&mut model, Message::Right); // En-têtes
        assert_eq!(model.response_tab, ResponseTab::Headers);

        update(&mut model, Message::OpenFilter);
        assert_eq!(model.response_tab, ResponseTab::Body);
    }

    #[test]
    fn changing_response_tab_resets_scroll_match_and_selection() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        model.response_scroll = 2;
        model.response_match = Some((2, 0..1));
        model.response_selection = Some(DetailSelection {
            anchor: 2,
            head: None,
        });

        update(&mut model, Message::Right);

        assert_eq!(model.response_scroll, 0);
        assert!(model.response_match.is_none());
        assert!(model.response_selection.is_none());
    }
}
