use std::time::{Duration, Instant};

use ratatui::layout::{Position, Rect};

use super::scroll::{
    bottom_of_viewport, response_bottom_of_viewport, scroll_detail, scroll_response,
    select_request_tab,
};
use super::{
    Command, Validation, begin_input, cancel_input, open_environment_editing, refresh_rows,
    scroll_tree_into_view, select_environment_picker, select_row, selected_folder, start_edit,
    toggle_environment_picker, tree_height, validate_input,
};
use crate::app::message::{Message, MouseInput, MouseKind, TextCapture};
use crate::app::model::{
    DetailSelection, Drag, DragPanel, EditState, EditableField, EnvironmentEditState, Focus, Model,
};
use crate::app::view::detail::{detail_text_fields_boxes, field_at_line};
use crate::app::view::hit::{DragRow, Hit, drag_row, hit_test};
use crate::app::view::{environment_chip_area, environment_edit_popup_area, inner, layout_for};

// --- Souris (`mouse-support`) ---------------------------------------------

/// Lignes défilées par cran de molette.
const WHEEL_LINES: u16 = 3;

/// Traite un événement souris déjà traduit par `to_message`.
pub(crate) fn mouse(model: &mut Model, input: MouseInput) -> Command {
    if !mouse_accepted(model) {
        model.mouse.drag = None;
        return Command::None;
    }
    match input.kind {
        MouseKind::Press => {
            mouse_press(model, input);
            Command::None
        }
        MouseKind::Drag => {
            mouse_drag(model, input);
            Command::None
        }
        MouseKind::Release => mouse_release(model),
        MouseKind::WheelUp => {
            mouse_wheel(model, input, true);
            Command::None
        }
        MouseKind::WheelDown => {
            mouse_wheel(model, input, false);
            Command::None
        }
    }
}

/// États où la souris est sans effet : capture inactive, confirmation en
/// attente, saisie autre que celle d'un champ, panneau superposé,
/// collection non chargée, terminal trop petit (design D9).
pub(crate) fn mouse_accepted(model: &Model) -> bool {
    model.mouse.capture
        && model.confirm.is_none()
        && matches!(
            model.text_capture(),
            None | Some(TextCapture::Input) | Some(TextCapture::MethodPicker)
        )
        && matches!(
            model.focus,
            Focus::Tree | Focus::Detail | Focus::Response | Focus::EnvironmentPicker
        )
        && model.loaded().is_some()
        && layout_for(model.size, model.zoomed_panel()).is_some()
}

/// Lignes logiques du champ en cours de saisie, s'il y en a un.
fn input_field_lines(model: &Model) -> Option<std::ops::Range<usize>> {
    let session = model.editing.as_ref()?;
    if !matches!(session.state, EditState::Input(_)) {
        return None;
    }
    let field = session.current_field()?;
    let (_, fields, _) = detail_text_fields_boxes(model);
    let location = fields.into_iter().find(|l| l.field == field)?;
    Some(location.line..location.line + location.count)
}

fn mouse_press(model: &mut Model, input: MouseInput) {
    // Le sélecteur de méthode se pilote au clavier seulement (aucune
    // entrée n'y est cliquable) : un clic ailleurs le referme sans rien
    // changer, comme `Échap`, plutôt que de rendre la souris inerte
    // jusqu'à une touche (`add-method-editing`). Le clic s'arrête là,
    // sans agir sur une nouvelle cible dans le même geste : la boîte du
    // sélecteur occupe des lignes qui disparaissent de la séquence une
    // fois refermée, ce qui rendrait la position cliquée incohérente
    // avec ce que l'utilisateur voyait.
    if model
        .editing
        .as_ref()
        .is_some_and(|s| matches!(s.state, EditState::MethodPicker { .. }))
    {
        cancel_input(model);
        return;
    }
    // Panneau Environnement (permanent) et popup d'édition : résolution
    // de clic auto-contenue, comme le sélecteur de méthode, plutôt
    // qu'intégrée à `hit_test` (`add-environment-panel-and-edit-popup`,
    // design D6). Vérifiée avant `hit_test` : un popup ouvert capture le
    // clic même si sa zone recouvre l'arbre/le détail dessinés dessous.
    if model.environment_editing.is_some() {
        environment_popup_mouse_press(model, input);
        return;
    }
    if let Some(areas) = layout_for(model.size, model.zoomed_panel()) {
        // Liste déroulante de l'en-tête : ouvre ou ferme le panneau
        // Environnement, comme `E`.
        if environment_chip_area(model, areas.title)
            .is_some_and(|chip| chip.contains(Position::new(input.column, input.row)))
        {
            toggle_environment_picker(model);
            return;
        }
        // Le panneau Environnement n'est cliquable que dessiné, donc
        // ouvert (focus).
        if model.focus == Focus::EnvironmentPicker
            && let Some(index) = environment_panel_row_at(areas.environment, input)
        {
            environment_panel_mouse_press(model, index);
            return;
        }
    }
    // Cible calculée avant toute validation : elle correspond à l'écran
    // que l'utilisateur voyait (sans retour à la ligne pendant la saisie).
    let Some(hit) = hit_test(model, input.column, input.row) else {
        return;
    };
    if let Some(range) = input_field_lines(model) {
        if let Hit::Detail { line: Some(line) } = hit
            && range.contains(&usize::from(line))
        {
            return;
        }
        // Comme Tab : si la saisie reste ouverte (refus, ou clé d'ajout
        // validée et saisie de la valeur), le clic s'arrête là.
        if !matches!(validate_input(model, false), Validation::Done) {
            return;
        }
    }
    model.mouse.drag = None;
    match hit {
        Hit::TreeHeader => {
            model.focus = Focus::Tree;
            model.selected_collection = true;
            model.detail_scroll = 0;
        }
        Hit::Tree { row } => {
            model.focus = Focus::Tree;
            if let Some(index) = row {
                model.selected_collection = false;
                click_tree_row(model, index);
            }
        }

        Hit::Detail { line } => {
            model.focus = Focus::Detail;
            model.mouse.drag = line.map(|anchor| Drag {
                panel: DragPanel::Detail,
                anchor,
                moved: false,
            });
        }
        Hit::Response { line } => {
            model.focus = Focus::Response;
            model.mouse.drag = line.map(|anchor| Drag {
                panel: DragPanel::Response,
                anchor,
                moved: false,
            });
        }
        // Traité dans `update`, qui doit renvoyer la commande d'exécution.
        Hit::RunButton => {}
        // Clic sur la méthode ou l'URL : édition directe de ce champ.
        Hit::UrlBar { field } => {
            begin_input_at(model, field);
        }
        // Clic sur un onglet du détail, hors session d'édition.
        Hit::RequestTab(tab) => {
            model.focus = Focus::Detail;
            if model.editing.is_none() {
                select_request_tab(model, tab);
            }
        }
    }
}

/// Délai maximal entre deux clics sur la même entrée du panneau
/// Environnement pour qu'ils comptent comme un double-clic
/// (`add-environment-panel-and-edit-popup`, design D4).
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);

/// Ligne de la liste du panneau Environnement sous `row`, ou `None` hors
/// de la zone intérieure (résolution par position, sans tenir compte
/// d'un éventuel défilement interne — design D6).
fn environment_panel_row_at(area: Rect, input: MouseInput) -> Option<usize> {
    let inner_area = inner(area);
    if input.column < inner_area.x
        || input.column >= inner_area.x + inner_area.width
        || input.row < inner_area.y
        || input.row >= inner_area.y + inner_area.height
    {
        return None;
    }
    Some(usize::from(input.row - inner_area.y))
}

/// Clic gauche sur la ligne `index` du panneau Environnement (aucun
/// popup ouvert) : donne le focus au panneau, active l'entrée comme le
/// ferait `Entrée`, et ouvre en plus le popup d'édition si c'est un
/// double-clic sur la même entrée (`add-environment-panel-and-edit-popup`,
/// design D4).
fn environment_panel_mouse_press(model: &mut Model, index: usize) {
    model.focus = Focus::EnvironmentPicker;
    let Some(collection) = model.loaded() else {
        return;
    };
    let count = 1 + collection.environments.len();
    if index >= count {
        return;
    }
    model.environment_selected = index;
    select_environment_picker(model, index);

    let now = Instant::now();
    let is_double_click = model
        .mouse
        .last_environment_click
        .is_some_and(|(time, clicked)| {
            clicked == index && now.saturating_duration_since(time) < DOUBLE_CLICK_WINDOW
        });
    model.mouse.last_environment_click = Some((now, index));
    if is_double_click {
        open_environment_editing(model, index);
    }
}

/// Clic gauche dans le popup d'édition d'un environnement : démarre la
/// saisie de la valeur cliquée si elle est activée, comme le clic sur un
/// en-tête de requête (`mouse-support`). Une saisie déjà en cours sur une
/// autre variable est d'abord validée, y compris si le clic ne démarre
/// pas de nouvelle saisie (hors du tableau, sur l'en-tête, ou sur une
/// variable désactivée) — même règle que « Clic pendant une session
/// d'édition » pour une requête.
fn environment_popup_mouse_press(model: &mut Model, input: MouseInput) {
    let Some(session) = &mut model.environment_editing else {
        return;
    };
    if matches!(session.state, EnvironmentEditState::Input(_)) {
        session.commit_input();
    }
    let popup = environment_edit_popup_area(Rect::new(0, 0, model.size.0, model.size.1), session);
    let Some(row) = environment_panel_row_at(popup, input) else {
        return;
    };
    // La première ligne intérieure est l'en-tête de colonnes, jamais
    // sélectionnable.
    let Some(index) = row.checked_sub(1) else {
        return;
    };
    if !session.variables.get(index).is_some_and(|v| v.enabled) {
        return;
    }
    session.cursor = index;
    session.begin_input();
}

/// Clic sur une ligne de l'arbre : sélection, ou dépliage/repli du dossier
/// déjà sélectionné.
fn click_tree_row(model: &mut Model, index: usize) {
    if index != model.tree.selected {
        select_row(model, index);
        return;
    }
    if let Some((path, expanded, _)) = selected_folder(model) {
        if expanded {
            model.tree.expanded.remove(&path);
        } else {
            model.tree.expanded.insert(path);
        }
        refresh_rows(model);
        scroll_tree_into_view(model);
    }
}

fn mouse_drag(model: &mut Model, input: MouseInput) {
    let Some(drag) = model.mouse.drag else {
        return;
    };
    let Some(position) = drag_row(model, drag.panel, input.row) else {
        return;
    };
    let line = match (position, drag.panel) {
        (DragRow::Line(line), _) => line,
        (DragRow::Above, DragPanel::Detail) => {
            scroll_detail(model, Message::Up);
            model.detail_scroll
        }
        (DragRow::Above, DragPanel::Response) => {
            scroll_response(model, Message::Up);
            model.response_scroll
        }
        (DragRow::Below, DragPanel::Detail) => {
            scroll_detail(model, Message::Down);
            bottom_of_viewport(model)
        }
        (DragRow::Below, DragPanel::Response) => {
            scroll_response(model, Message::Down);
            response_bottom_of_viewport(model)
        }
    };
    if line == drag.anchor && !drag.moved {
        return;
    }
    let selection = Some(DetailSelection {
        anchor: drag.anchor,
        head: Some(line),
    });
    match drag.panel {
        DragPanel::Detail => model.detail_selection = selection,
        DragPanel::Response => model.response_selection = selection,
    }
    model.mouse.drag = Some(Drag {
        moved: true,
        ..drag
    });
}

fn mouse_release(model: &mut Model) -> Command {
    let Some(drag) = model.mouse.drag.take() else {
        return Command::None;
    };
    if drag.moved {
        return Command::None;
    }
    match drag.panel {
        DragPanel::Response => {
            model.response_selection = None;
            Command::None
        }
        DragPanel::Detail => {
            model.detail_selection = None;
            if let Some(field) = field_at_line(model, drag.anchor) {
                begin_input_at(model, field)
            } else {
                Command::None
            }
        }
    }
}

/// Clic sur un champ éditable : ouvre la session si besoin, place le
/// curseur de champ et commence la saisie, comme `Entrée`.
fn begin_input_at(model: &mut Model, field: EditableField) -> Command {
    model.focus = Focus::Detail;
    if model.editing.is_none() {
        start_edit(model);
    }
    let Some(session) = &mut model.editing else {
        return Command::None;
    };
    if session.state != EditState::FieldSelect {
        return Command::None;
    }
    let Some(index) = session.fields.iter().position(|f| *f == field) else {
        return Command::None;
    };
    session.cursor = index;
    begin_input(model)
}

fn mouse_wheel(model: &mut Model, input: MouseInput, up: bool) {
    let Some(hit) = hit_test(model, input.column, input.row) else {
        return;
    };
    let step = || if up { Message::Up } else { Message::Down };
    match hit {
        Hit::Tree { .. } => {
            let max = model.tree.rows.len().saturating_sub(tree_height(model));
            let lines = usize::from(WHEEL_LINES);
            model.tree.offset = if up {
                model.tree.offset.saturating_sub(lines)
            } else {
                (model.tree.offset + lines).min(max)
            };
        }
        Hit::Detail { .. } => {
            for _ in 0..WHEEL_LINES {
                scroll_detail(model, step());
            }
        }
        Hit::Response { .. } => {
            for _ in 0..WHEEL_LINES {
                scroll_response(model, step());
            }
        }
        Hit::RunButton | Hit::UrlBar { .. } | Hit::RequestTab(_) | Hit::TreeHeader => {}
    }
}

#[cfg(test)]
mod tests {
    //! Souris (`mouse-support`) : scénarios de la spécification, rejoués
    //! sur `update` avec des positions calculées par le hit-testing.

    use std::path::Path;

    use super::super::*;
    use super::*;
    use crate::app::model::{MouseState, PendingConfirm, RequestTab};
    use crate::app::test_support::{loaded_model, runner_probe_model, select, selected_name};
    use crate::app::view::detail::request_text_and_fields;
    use crate::app::view::detail::{plain_lines, response_plain_lines};
    use crate::app::view::hit::{Hit, hit_test};
    use crate::app::view::response_text_area;

    fn model_on(path: &str, size: (u16, u16)) -> Model {
        let mut model = loaded_model(size);
        model.mouse.capture = true;
        select(&mut model, path);
        model
    }

    fn event(model: &mut Model, kind: MouseKind, (column, row): (u16, u16)) -> Command {
        update(model, Message::Mouse(MouseInput { kind, column, row }))
    }

    fn click(model: &mut Model, point: (u16, u16)) {
        event(model, MouseKind::Press, point);
        event(model, MouseKind::Release, point);
    }

    /// Un clic sur le bouton « Lancer » de la barre d'URL lance la requête
    /// sélectionnée ; le bouton disparaît pendant une session d'édition.
    #[test]
    fn run_button_click_starts_the_selected_request() {
        use crate::app::view::hit::run_button_area;
        let mut model = model_on("simple-get.bru", (100, 30));
        let areas = layout_for(model.size, None).expect("layout");
        let button = run_button_area(&model, areas.url_bar).expect("bouton sur une requête");
        assert_eq!(button.y, areas.url_bar.y + 1);
        assert_eq!(button.right(), areas.url_bar.right() - 1);
        assert_eq!(hit_test(&model, button.x, button.y), Some(Hit::RunButton));
        let command = event(&mut model, MouseKind::Press, (button.x, button.y));
        assert!(matches!(command, Command::StartRun { .. }), "{command:?}");

        let mut editing = model_on("simple-get.bru", (100, 30));
        update(&mut editing, Message::NextFocus);
        update(&mut editing, Message::StartEdit);
        assert!(run_button_area(&editing, areas.url_bar).is_none());
    }

    fn row_of(model: &Model, path: &str) -> usize {
        model
            .tree
            .rows
            .iter()
            .position(|row| {
                model
                    .node_at(&row.address)
                    .is_some_and(|node| node.path() == Path::new(path))
            })
            .unwrap_or_else(|| panic!("`{path}` non visible"))
    }

    fn tree_point(model: &Model, path: &str) -> (u16, u16) {
        let area = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .tree,
        );
        let index = row_of(model, path) - model.tree.offset;
        (area.x + 2, area.y + u16::try_from(index).expect("petit"))
    }

    /// Position écran de la ligne logique `line` du détail ou de la réponse.
    fn line_point(model: &Model, detail: bool, line: u16) -> (u16, u16) {
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
        let area = if detail {
            inner(areas.detail)
        } else {
            response_text_area(areas.response)
        };
        (area.y..area.bottom())
            .map(|y| (area.x + 1, y))
            .find(|(x, y)| {
                matches!(
                    hit_test(model, *x, *y),
                    Some(Hit::Detail { line: Some(l) } | Hit::Response { line: Some(l) }) if l == line
                )
            })
            .unwrap_or_else(|| panic!("ligne {line} non visible"))
    }

    /// Ligne logique d'un champ dans le détail, après avoir affiché son
    /// onglet (hors session ; en session, l'onglet suit le curseur).
    fn field_line(model: &mut Model, field: EditableField) -> u16 {
        // L'onglet choisi n'est affiché que hors session, ou en session
        // quand le curseur est sur la méthode ou l'URL.
        let chosen_is_shown = model
            .editing
            .as_ref()
            .and_then(EditSession::current_field)
            .and_then(RequestTab::of_field)
            .is_none();
        if chosen_is_shown && let Some(tab) = RequestTab::of_field(field) {
            model.request_tab = tab;
        }
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let session = model.editing.as_ref().filter(|s| s.path == request.path);
        let (_, fields, _) = request_text_and_fields(request, session, model.shown_request_tab());
        let location = fields
            .into_iter()
            .find(|l| l.field == field)
            .expect("champ présent");
        u16::try_from(location.line).expect("petit")
    }

    /// Position écran d'un champ : hors session, Méthode et URL sont dans
    /// la barre d'URL ; les autres champs, et tous pendant une session,
    /// dans le détail.
    fn field_point(model: &mut Model, field: EditableField) -> (u16, u16) {
        if model.editing.is_none() && matches!(field, EditableField::Method | EditableField::Url) {
            let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
            let bar = inner(areas.url_bar);
            let (method, _) = crate::app::view::detail::url_bar_values(model).expect("requête");
            let url_x = bar.x
                + u16::try_from(
                    Line::raw(format!(
                        "{method}{}",
                        crate::app::view::detail::URL_BAR_METHOD_SUFFIX
                    ))
                    .width(),
                )
                .expect("petit");
            return match field {
                EditableField::Method => (bar.x, bar.y),
                _ => (url_x + 1, bar.y),
            };
        }
        let line = field_line(model, field);
        line_point(model, true, line)
    }

    fn input_text(model: &Model) -> Option<String> {
        match &model.editing.as_ref()?.state {
            EditState::Input(input) => Some(input.text().to_owned()),
            EditState::FieldSelect | EditState::MethodPicker { .. } => None,
        }
    }

    fn current_field(model: &Model) -> Option<EditableField> {
        model.editing.as_ref()?.current_field()
    }

    // --- Capture ----------------------------------------------------------

    #[test]
    fn toggle_emits_the_opposite_state_and_applies_only_on_success() {
        let mut model = model_on("post-json.bru", (100, 30));
        assert!(matches!(
            update(&mut model, Message::ToggleMouseCapture),
            Command::SetMouseCapture(false)
        ));
        model.mouse.drag = Some(Drag {
            panel: DragPanel::Detail,
            anchor: 1,
            moved: false,
        });
        update(
            &mut model,
            Message::MouseCaptureChanged {
                enabled: false,
                result: Ok(()),
            },
        );
        assert!(!model.mouse.capture);
        assert!(model.mouse.drag.is_none());
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::MouseCapture(false))
        ));
        assert!(matches!(
            update(&mut model, Message::ToggleMouseCapture),
            Command::SetMouseCapture(true)
        ));
        update(
            &mut model,
            Message::MouseCaptureChanged {
                enabled: true,
                result: Err("refusé".into()),
            },
        );
        assert!(!model.mouse.capture);
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::MouseCaptureError(_))
        ));
    }

    #[test]
    fn new_model_starts_without_capture() {
        let model = Model::new("x".into(), (100, 30));
        assert_eq!(model.mouse, MouseState::default());
        assert!(!model.mouse.capture);
    }

    // --- Garde --------------------------------------------------------------

    #[test]
    fn mouse_is_inert_in_guarded_states() {
        let base = || {
            let mut model = model_on("simple-get.bru", (100, 30));
            model.tree.selected = 0;
            model
        };
        let target = |model: &Model| tree_point(model, "post-json.bru");
        type Setup = Box<dyn Fn(&mut Model)>;
        let setups: Vec<(&str, Setup)> = vec![
            ("capture inactive", Box::new(|m| m.mouse.capture = false)),
            (
                "confirmation",
                Box::new(|m| m.confirm = Some(PendingConfirm::QuitWithUnsavedEdit)),
            ),
            (
                "recherche",
                Box::new(|m| {
                    update(m, Message::StartSearch);
                    update(m, Message::SearchInput('x'));
                }),
            ),
            ("diagnostics", Box::new(|m| m.focus = Focus::Diagnostics)),
        ];
        for (name, setup) in setups {
            let mut model = base();
            let point = target(&model);
            setup(&mut model);
            let focus = model.focus;
            for kind in [
                MouseKind::Press,
                MouseKind::Drag,
                MouseKind::Release,
                MouseKind::WheelDown,
            ] {
                event(&mut model, kind, point);
            }
            assert_eq!(model.tree.selected, 0, "{name}");
            assert_eq!(model.focus, focus, "{name}");
            assert_eq!(model.tree.offset, 0, "{name}");
        }

        // Chargement en cours et terminal trop petit.
        let mut loading = Model::new("x".into(), (100, 30));
        loading.mouse.capture = true;
        event(&mut loading, MouseKind::Press, (80, 5));
        assert_eq!(loading.focus, Focus::Tree);
        let mut small = base();
        small.size = (30, 8);
        event(&mut small, MouseKind::Press, (2, 4));
        assert_eq!(small.tree.selected, 0);
    }

    #[test]
    fn clicks_on_title_and_status_bar_do_nothing() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        select(&mut model, "green.bru");
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
        for point in [(1, areas.title.y), (1, areas.status.y)] {
            click(&mut model, point);
            event(&mut model, MouseKind::WheelDown, point);
            assert_eq!(model.focus, Focus::Tree);
            assert_eq!(selected_name(&model), "green");
            assert_eq!(model.response_scroll, 0);
        }
    }

    // --- Arbre --------------------------------------------------------------

    #[test]
    fn click_selects_a_node_and_focuses_panels() {
        let mut model = model_on("grp", (100, 30));
        assert_eq!(selected_name(&model), "Groupe");
        let point = tree_point(&model, "post-json.bru");
        click(&mut model, point);
        assert_eq!(selected_name(&model), "post-json");
        let (method, _) = crate::app::view::detail::url_bar_values(&model).expect("requête");
        assert_eq!(method, "POST");

        let response = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .response,
        );
        click(&mut model, (response.x, response.y));
        assert_eq!(model.focus, Focus::Response);
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn click_on_the_selected_folder_toggles_it() {
        let mut model = model_on("grp", (100, 30));
        let point = tree_point(&model, "grp");
        click(&mut model, point);
        assert!(model.tree.expanded.contains(Path::new("grp")));
        assert_eq!(selected_name(&model), "Groupe");
        assert!(row_of(&model, "grp/x.bru") > 0);
        click(&mut model, point);
        assert!(!model.tree.expanded.contains(Path::new("grp")));
        assert_eq!(selected_name(&model), "Groupe");
    }

    #[test]
    fn click_in_a_scrolled_tree_and_on_borders() {
        let mut model = model_on("grp", (100, 11));
        let area = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .tree,
        );
        model.tree.offset = 4;
        click(&mut model, (area.x + 1, area.y));
        assert_eq!(model.tree.selected, 4);

        let tree = layout_for(model.size, model.zoomed_panel())
            .expect("taille")
            .tree;
        model.focus = Focus::Detail;
        click(&mut model, (tree.x, area.y + 1));
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.tree.selected, 4);
    }

    // --- Session d'édition -------------------------------------------------

    fn session_on_post_json() -> Model {
        let mut model = model_on("post-json.bru", (140, 40));
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_some());
        model
    }

    #[test]
    fn clean_session_closes_when_another_node_is_clicked() {
        let mut model = session_on_post_json();
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        assert!(model.editing.is_none());
        assert_eq!(selected_name(&model), "ping");
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn modified_session_refuses_the_clicked_node() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        update(&mut model, Message::ValidateInput);
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        assert_eq!(selected_name(&model), "post-json");
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.editing.as_ref().is_some_and(|s| s.dirty));
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
    }

    #[test]
    fn input_is_validated_before_a_click_in_the_tree() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        let point = tree_point(&model, "simple-get.bru");
        click(&mut model, point);
        let session = model.editing.as_ref().expect("session");
        assert!(session.dirty);
        assert_eq!(session.state, EditState::FieldSelect);
        assert_eq!(selected_name(&model), "post-json");
        assert_eq!(model.focus, Focus::Tree);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
    }

    #[test]
    fn unchanged_input_ends_on_a_click_in_the_response() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Enter);
        let response = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .response,
        );
        click(&mut model, (response.x, response.y));
        let session = model.editing.as_ref().expect("session");
        assert!(!session.dirty);
        assert_eq!(session.state, EditState::FieldSelect);
        assert_eq!(model.focus, Focus::Response);
    }

    #[test]
    fn click_on_the_field_being_edited_changes_nothing() {
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Left));
        let before = model.editing.clone().map(|s| s.state);
        let point = field_point(&mut model, EditableField::Url);
        click(&mut model, point);
        event(&mut model, MouseKind::Press, point);
        event(&mut model, MouseKind::Drag, (point.0, point.1 + 2));
        event(&mut model, MouseKind::Release, (point.0, point.1 + 2));
        assert_eq!(model.editing.map(|s| s.state), before);
        assert!(model.detail_selection.is_none());
    }

    fn move_to(model: &mut Model, field: EditableField) {
        let index = model
            .editing
            .as_ref()
            .and_then(|s| s.fields.iter().position(|f| *f == field))
            .expect("champ");
        model.editing.as_mut().expect("session").cursor = index;
    }

    // --- Molette ------------------------------------------------------------

    #[test]
    fn wheel_scrolls_the_hovered_panel_without_focus_change() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 11);
        select(&mut model, "green.bru");
        let max = response_max_scroll(&model);
        assert!(max >= 1, "réponse assez longue");
        let response = response_text_area(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .response,
        );
        event(&mut model, MouseKind::WheelDown, (response.x, response.y));
        assert_eq!(model.response_scroll, 3.min(max));
        assert_eq!(model.focus, Focus::Tree);
        event(&mut model, MouseKind::WheelUp, (response.x, response.y));
        assert_eq!(model.response_scroll, 0);
    }

    #[test]
    fn wheel_stops_at_the_end_of_the_detail() {
        let mut model = model_on("scripted.bru", (100, 12));
        let max = detail_max_scroll(&model);
        model.detail_scroll = max;
        let detail = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .detail,
        );
        event(&mut model, MouseKind::WheelDown, (detail.x, detail.y));
        assert_eq!(model.detail_scroll, max);
    }

    #[test]
    fn wheel_in_the_tree_keeps_the_selection() {
        let mut model = model_on("grp", (100, 11));
        model.tree.selected = 0;
        model.tree.offset = 0;
        let area = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .tree,
        );
        assert!(model.tree.rows.len() > usize::from(area.height) + 3);
        event(&mut model, MouseKind::WheelDown, (area.x, area.y));
        assert_eq!(model.tree.offset, 3);
        assert_eq!(model.tree.selected, 0);
        // Borne basse.
        for _ in 0..20 {
            event(&mut model, MouseKind::WheelDown, (area.x, area.y));
        }
        assert_eq!(
            model.tree.offset,
            model.tree.rows.len() - usize::from(area.height)
        );
        // `↓` ramène la sélection à l'écran.
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, 1);
        assert!(model.tree.offset <= 1);
    }

    #[test]
    fn wheel_in_a_session_keeps_the_field_cursor() {
        let mut model = model_on("scripted.bru", (100, 12));
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        let cursor = model.editing.as_ref().map(|s| s.cursor);
        let detail = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .detail,
        );
        event(&mut model, MouseKind::WheelDown, (detail.x, detail.y));
        assert_eq!(model.detail_scroll, 3);
        assert_eq!(model.editing.as_ref().map(|s| s.cursor), cursor);
    }

    // --- Glisser --------------------------------------------------------------

    #[test]
    fn drag_selects_lines_and_keeps_fixed_bounds() {
        // Onglet Corps de post-json : assez de lignes pour défiler.
        let mut model = model_on("post-json.bru", (140, 11));
        model.request_tab = RequestTab::Body;
        assert!(detail_max_scroll(&model) >= 3, "détail assez long");
        let start = line_point(&model, true, 1);
        let end = line_point(&model, true, 3);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        event(&mut model, MouseKind::Release, end);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(selection_range(&model), Some(1..=3));
        assert!(model.editing.is_none());

        let detail = inner(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .detail,
        );
        event(&mut model, MouseKind::WheelDown, (detail.x, detail.y));
        assert_eq!(model.detail_scroll, 3);
        assert_eq!(selection_range(&model), Some(1..=3));
        assert!(model.response_selection.is_none());

        // Un clic sans glisser lève la sélection.
        click(&mut model, (detail.x, detail.y));
        assert!(model.detail_selection.is_none());
    }

    #[test]
    fn drag_beyond_the_panel_scrolls_and_extends() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 11);
        select(&mut model, "green.bru");
        // Onglet Tests : assez de lignes pour défiler sur toute la hauteur.
        model.response_tab = ResponseTab::Tests;
        assert!(response_max_scroll(&model) >= 2, "réponse assez longue");
        let response = response_text_area(
            layout_for(model.size, model.zoomed_panel())
                .expect("taille")
                .response,
        );
        event(&mut model, MouseKind::Press, (response.x, response.y));
        event(&mut model, MouseKind::Drag, (response.x, response.bottom()));
        assert_eq!(model.response_scroll, 1);
        let bottom = response_bottom_of_viewport(&model);
        assert_eq!(response_selection_range(&model), Some(0..=bottom));
        event(
            &mut model,
            MouseKind::Drag,
            (response.x, response.bottom() + 3),
        );
        assert_eq!(model.response_scroll, 2);
        assert_eq!(model.focus, Focus::Response);

        // Au-dessus : remonte.
        event(&mut model, MouseKind::Drag, (response.x, response.y - 1));
        assert_eq!(model.response_scroll, 1);
        assert_eq!(response_selection_range(&model), Some(0..=1));
    }

    #[test]
    fn drag_started_on_a_field_opens_no_session() {
        let mut model = model_on("post-json.bru", (140, 40));
        let url = field_line(&mut model, EditableField::HeaderValue(0));
        let start = line_point(&model, true, url);
        let end = line_point(&model, true, url + 1);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        event(&mut model, MouseKind::Release, end);
        assert!(model.editing.is_none());
        assert_eq!(selection_range(&model), Some(url..=url + 1));
    }

    #[test]
    fn orphan_release_does_nothing() {
        let mut model = model_on("post-json.bru", (140, 40));
        let point = field_point(&mut model, EditableField::Url);
        event(&mut model, MouseKind::Release, point);
        assert!(model.editing.is_none());
        assert_eq!(model.focus, Focus::Tree);
    }

    // --- Saisie d'un champ au clic ------------------------------------------

    #[test]
    fn click_on_the_url_opens_the_session_in_input() {
        let mut model = model_on("post-json.bru", (140, 40));
        let point = field_point(&mut model, EditableField::Url);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(current_field(&model), Some(EditableField::Url));
        assert_eq!(
            input_text(&model).as_deref(),
            Some("https://{{host}}/items")
        );
        let EditState::Input(input) = &model.editing.as_ref().expect("session").state else {
            panic!("saisie attendue");
        };
        assert_eq!(input.cursor(), "https://{{host}}/items".chars().count());
    }

    /// Un clic ailleurs referme le sélecteur de méthode sans rien changer,
    /// comme `Échap`, sans laisser la souris inerte tant qu'il est ouvert :
    /// un clic suivant agit normalement sur sa cible (`add-method-editing`).
    #[test]
    fn click_elsewhere_closes_the_method_picker_without_leaving_the_mouse_inert() {
        let mut model = model_on("post-json.bru", (140, 40));
        let method_point = field_point(&mut model, EditableField::Method);
        click(&mut model, method_point);
        assert!(matches!(
            model.editing.as_ref().expect("session").state,
            EditState::MethodPicker { .. }
        ));

        // Premier clic (même sur la ligne Méthode) : referme le sélecteur,
        // rien d'autre.
        click(&mut model, method_point);
        assert_eq!(
            model.editing.as_ref().expect("session").state,
            EditState::FieldSelect
        );
        assert_eq!(
            field_value(
                model.editing.as_ref().expect("session"),
                &EditableField::Method
            ),
            "POST",
            "méthode inchangée par le clic de fermeture"
        );

        // Second clic, sur l'URL : la souris n'est pas restée bloquée.
        let url_point = field_point(&mut model, EditableField::Url);
        click(&mut model, url_point);
        assert_eq!(current_field(&model), Some(EditableField::Url));
        assert_eq!(
            input_text(&model).as_deref(),
            Some("https://{{host}}/items")
        );
    }

    /// Clic sur un onglet de la bordure haute du détail : l'affiche et
    /// donne le focus au détail.
    #[test]
    fn click_on_a_request_tab_shows_it() {
        let mut model = model_on("post-json.bru", (100, 30));
        let detail = layout_for(model.size, None).expect("taille").detail;
        // « Corps » commence à la colonne 9 de la barre d'onglets.
        click(&mut model, (detail.x + 10, detail.y));
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(model.request_tab, RequestTab::Body);
    }

    #[test]
    fn click_on_a_field_from_field_select_and_from_another_input() {
        let mut model = session_on_post_json();
        let point = field_point(&mut model, EditableField::HeaderValue(0));
        click(&mut model, point);
        assert_eq!(current_field(&model), Some(EditableField::HeaderValue(0)));
        assert_eq!(input_text(&model).as_deref(), Some("application/json"));

        // Saisie de l'URL modifiée, puis clic sur l'en-tête.
        let mut model = session_on_post_json();
        move_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));
        let point = field_point(&mut model, EditableField::HeaderValue(0));
        click(&mut model, point);
        let session = model.editing.as_ref().expect("session");
        assert!(session.dirty);
        assert!(
            session
                .pending
                .contains(&FieldEdit::Url("https://{{host}}/items!".into()))
        );
        assert_eq!(current_field(&model), Some(EditableField::HeaderValue(0)));
        assert_eq!(input_text(&model).as_deref(), Some("application/json"));
    }

    #[test]
    fn click_on_the_body_and_on_non_field_lines() {
        let mut model = model_on("post-json.bru", (140, 40));
        let body = field_line(&mut model, EditableField::BodyText);
        let point = line_point(&model, true, body + 2);
        click(&mut model, point);
        assert_eq!(current_field(&model), Some(EditableField::BodyText));
        assert!(input_text(&model).is_some());

        // Titre de section des en-têtes : focus seul.
        let mut model = model_on("post-json.bru", (140, 40));
        let header = field_line(&mut model, EditableField::HeaderValue(0));
        let point = line_point(&model, true, header - 1);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());

        // Dossier : focus seul.
        let mut model = model_on("grp", (140, 40));
        let point = line_point(&model, true, 0);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());
    }

    #[test]
    fn click_on_a_non_editable_body_starts_no_input() {
        use crate::collection::{BruLoader, CollectionLoader};
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases");
        let mut model = Model::new(root.clone(), (140, 60));
        update(&mut model, Message::CollectionLoaded(BruLoader.load(&root)));
        model.mouse.capture = true;
        select(&mut model, "form-body.bru");
        model.request_tab = RequestTab::Body;
        let lines = plain_lines(&model);
        let body = lines
            .iter()
            .position(|l| l.contains("Corps"))
            .expect("corps");
        assert!(lines[body + 1].trim_start().contains(':'), "{lines:?}");
        let point = line_point(&model, true, u16::try_from(body + 1).expect("petit"));
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());
        assert!(input_text(&model).is_none());
    }

    /// Clic sur la ligne « aucun » d'une section vide hors session :
    /// ouvre la session et démarre l'ajout sur cette section.
    #[test]
    fn click_on_empty_section_row_without_session_starts_add() {
        let mut model = model_on("simple-get.bru", (140, 40));
        let point = field_point(&mut model, EditableField::AddRow(EntrySection::Headers));
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        let session = model.editing.as_ref().expect("session");
        assert!(matches!(session.state, EditState::Input(_)));
        assert!(matches!(
            session.target,
            InputTarget::NewKey {
                section: EntrySection::Headers,
                ..
            }
        ));
        assert_eq!(input_text(&model).as_deref(), Some(""));
    }

    /// Clic sur la ligne « aucun » pour chacune des trois sections :
    /// chacune ouvre l'ajout sur la bonne section et non sur une autre.
    #[test]
    fn click_on_empty_section_row_starts_add_for_correct_section() {
        for section in [
            EntrySection::Headers,
            EntrySection::QueryParams,
            EntrySection::PathParams,
        ] {
            let mut model = model_on("simple-get.bru", (140, 40));
            let point = field_point(&mut model, EditableField::AddRow(section));
            click(&mut model, point);
            assert_eq!(model.focus, Focus::Detail);
            let session = model.editing.as_ref().expect("session");
            assert!(matches!(session.state, EditState::Input(_)));
            assert!(
                matches!(session.target, InputTarget::NewKey { section: s, .. } if s == section),
                "l'ajout doit démarrer sur {section:?}"
            );
            assert_eq!(input_text(&model).as_deref(), Some(""));
        }
    }

    /// Section ayant déjà une entrée, sans session ouverte :
    /// un clic sous la dernière entrée (cadre inférieur de section)
    /// ne fait que donner le focus au détail sans ouvrir de session.
    #[test]
    fn click_under_existing_entry_without_session_only_focuses_detail() {
        let mut model = model_on("post-json.bru", (140, 40));
        let header = field_line(&mut model, EditableField::HeaderValue(0));
        let point = line_point(&model, true, header + 1);
        click(&mut model, point);
        assert_eq!(model.focus, Focus::Detail);
        assert!(model.editing.is_none());
    }

    // --- Copie ----------------------------------------------------------------

    #[test]
    fn y_copies_the_dragged_lines() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 30);
        select(&mut model, "green.bru");
        let start = line_point(&model, false, 1);
        let end = line_point(&model, false, 2);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        // Relâchement : aucune copie.
        assert!(matches!(
            event(&mut model, MouseKind::Release, end),
            Command::None
        ));
        let lines = response_plain_lines(&model);
        let Command::CopyToClipboard { token, text } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        assert_eq!(text, format!("{}\n{}", lines[1], lines[2]));

        // Échec : sélection et défilement inchangés, échec signalé.
        let scroll = model.response_scroll;
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Err(crate::app::clipboard::test_support::error_for_test()),
            },
        );
        assert_eq!(model.response_scroll, scroll);
        assert_eq!(response_selection_range(&model), Some(1..=2));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::ClipboardError(_))
        ));

        // Nouvelle tentative réussie : sélection levée, confirmation.
        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(model.response_selection.is_none());
        assert_eq!(model.response_scroll, scroll);
        assert!(matches!(model.last_status, Some(StatusMessage::Copied)));
    }

    #[test]
    fn y_in_field_select_copies_without_starting_input() {
        let mut model = session_on_post_json();
        let start = line_point(&model, true, 0);
        let end = line_point(&model, true, 1);
        event(&mut model, MouseKind::Press, start);
        event(&mut model, MouseKind::Drag, end);
        event(&mut model, MouseKind::Release, end);
        assert!(matches!(
            update(&mut model, Message::Yank),
            Command::CopyToClipboard { .. }
        ));
        assert_eq!(
            model.editing.as_ref().map(|s| s.state.clone()),
            Some(EditState::FieldSelect)
        );
    }

    // --- Non-régression clavier ---------------------------------------------

    #[test]
    fn keyboard_navigation_is_unchanged_with_capture() {
        let mut with = loaded_model((100, 30));
        with.mouse.capture = true;
        let mut without = loaded_model((100, 30));
        for message in [
            || Message::Down,
            || Message::Right,
            || Message::Left,
            || Message::Down,
        ] {
            update(&mut with, message());
            update(&mut without, message());
            assert_eq!(with.tree.selected, without.tree.selected);
            assert_eq!(with.tree.expanded, without.tree.expanded);
        }
    }

    // --- Panneau et popup Environnement (`add-environment-panel-and-edit-popup`) --

    /// Position écran de l'entrée `index` du panneau permanent Environnement.
    fn environment_panel_point(model: &Model, index: usize) -> (u16, u16) {
        let areas = layout_for(model.size, model.zoomed_panel()).expect("taille");
        let area = inner(areas.environment);
        (area.x + 1, area.y + u16::try_from(index).expect("petit"))
    }

    /// Position écran de la ligne `row` (0 = en-tête) du popup d'édition
    /// ouvert sur `session`.
    fn environment_popup_point(
        model: &Model,
        session: &EnvironmentEditSession,
        row: usize,
    ) -> (u16, u16) {
        let popup =
            environment_edit_popup_area(Rect::new(0, 0, model.size.0, model.size.1), session);
        let area = inner(popup);
        (area.x + 1, area.y + u16::try_from(row).expect("petit"))
    }

    /// Modèle avec le panneau Environnement ouvert (surimpression).
    fn environment_model() -> Model {
        let mut model = loaded_model((100, 30));
        model.mouse.capture = true;
        update(&mut model, Message::ToggleEnvironmentPicker);
        model
    }

    /// La liste déroulante de l'en-tête ouvre puis referme le panneau ;
    /// fermé, le panneau n'intercepte aucun clic.
    #[test]
    fn header_chip_toggles_the_environment_panel() {
        let mut model = loaded_model((100, 30));
        model.mouse.capture = true;
        let areas = layout_for(model.size, None).expect("taille");
        let chip = crate::app::view::environment_chip_area(&model, areas.title).expect("liste");
        let point = environment_panel_point(&model, 1);
        click(&mut model, point);
        assert_ne!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment, None);

        click(&mut model, (chip.x + 1, chip.y));
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        click(&mut model, (chip.x + 1, chip.y));
        assert_eq!(model.focus, Focus::Tree);
    }

    #[test]
    fn click_on_environment_panel_activates_without_opening_popup() {
        let mut model = environment_model();
        let point = environment_panel_point(&model, 1); // "local"
        click(&mut model, point);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        assert!(model.environment_editing.is_none());
    }

    #[test]
    fn double_click_on_environment_panel_activates_and_opens_popup() {
        let mut model = environment_model();
        let point = environment_panel_point(&model, 1); // "local"
        click(&mut model, point);
        click(&mut model, point);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.name, "local");
    }

    #[test]
    fn clicks_far_apart_in_time_do_not_open_the_popup() {
        let mut model = environment_model();
        let point = environment_panel_point(&model, 1);
        click(&mut model, point);
        // Simule un premier clic ancien : hors de la fenêtre de double-clic.
        model.mouse.last_environment_click = Some((Instant::now() - Duration::from_secs(1), 1));
        click(&mut model, point);
        assert!(model.environment_editing.is_none());
    }

    #[test]
    fn rapid_clicks_on_different_entries_do_not_open_the_popup() {
        let mut model = environment_model();
        let local = environment_panel_point(&model, 1); // "local"
        click(&mut model, local);
        let staging = environment_panel_point(&model, 3); // "staging"
        click(&mut model, staging);
        assert!(model.environment_editing.is_none());
        assert_eq!(model.current_environment.as_deref(), Some("staging"));
    }

    #[test]
    fn click_on_none_or_error_entry_in_the_panel() {
        let mut model = environment_model();
        model.current_environment = Some("local".into());
        // « Aucun » (indice 0) : désactive l'environnement courant.
        let none_point = environment_panel_point(&model, 0);
        click(&mut model, none_point);
        assert_eq!(model.current_environment, None);
        // Entrée en erreur (indice 2 : malformed) : sans effet.
        model.current_environment = Some("local".into());
        let error_point = environment_panel_point(&model, 2);
        click(&mut model, error_point);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
    }

    #[test]
    fn click_in_the_popup_starts_editing_the_clicked_variable() {
        let mut model = environment_model();
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);
        let session = model.environment_editing.clone().expect("popup ouvert");
        assert_eq!(session.cursor, 0);

        // "debug" est la seconde variable (indice 1), sur la ligne 2 du
        // popup (0 : en-tête, 1 : host, 2 : debug) — désactivée.
        let point = environment_popup_point(&model, &session, 2);
        click(&mut model, point);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.state, EnvironmentEditState::Select, "désactivée");

        // "host" (indice 0), sur la ligne 1 du popup : activée.
        let point = environment_popup_point(&model, &session.clone(), 1);
        click(&mut model, point);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.cursor, 0);
        assert!(matches!(session.state, EnvironmentEditState::Input(_)));
    }

    #[test]
    fn click_in_the_popup_while_editing_another_variable_commits_first() {
        let mut model = environment_model();
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1; // "local"
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::StartEdit); // démarre la saisie sur "host"
        for c in "!!!".chars() {
            update(&mut model, Message::InputKey(InputKey::Char(c)));
        }

        let session = model.environment_editing.clone().expect("popup ouvert");
        // "debug" est désactivée : cliquer dessus ne démarre pas de
        // nouvelle saisie, mais valide déjà la saisie en cours sur "host".
        let point = environment_popup_point(&model, &session, 2);
        click(&mut model, point);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.variables[0].value, "localhost:3000!!!");
        assert_eq!(session.state, EnvironmentEditState::Select);
    }

    #[test]
    fn click_outside_the_popup_has_no_effect() {
        let mut model = environment_model();
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1;
        update(&mut model, Message::StartEdit);

        // Un clic sur l'arbre, dessiné dessous mais capturé par le popup
        // modal, ne change rien (`add-environment-panel-and-edit-popup`,
        // design D6).
        let point = tree_point(&model, "grp");
        click(&mut model, point);
        assert!(model.environment_editing.is_some());
        assert_eq!(model.focus, Focus::EnvironmentPicker);
    }

    #[test]
    fn status_message_lasts_until_next_user_action() {
        let mut model = loaded_model((100, 30));
        model.last_status = Some(crate::app::model::StatusMessage::Copied);
        update(
            &mut model,
            Message::Resize {
                width: 100,
                height: 30,
            },
        );
        assert!(model.last_status.is_some());
        update(&mut model, Message::Down);
        assert!(model.last_status.is_none());
    }

    #[test]
    fn help_popup_does_not_swallow_run_finished() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(crate::app::model::ActiveRun {
            id: crate::runner::RunId(3),
            target: "simple-get.bru".into(),
            recursive: false,
            is_campaign: false,
            handle: None,
        });
        update(&mut model, Message::ToggleHelp);
        update(
            &mut model,
            Message::RunFinished(crate::runner::RunEvent {
                id: crate::runner::RunId(3),
                outcome: crate::runner::RunOutcome::Failed(crate::runner::RunError::BruNotFound),
            }),
        );
        assert!(model.help.is_some());
        assert!(model.run.active.is_none());
        assert!(model.run.last_failure.is_some());
    }

    #[test]
    fn help_popup_lifecycle_open_and_close() {
        let mut model = loaded_model((100, 30));
        assert!(model.help.is_none());

        // '?' ouvre le popup
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());
        assert_eq!(model.help.unwrap().scroll, 0);

        // '?' le ferme
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_none());

        // Échap le ferme
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());
        update(&mut model, Message::FocusTree);
        assert!(model.help.is_none());

        // 'q' le ferme sans quitter l'application
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());
        update(&mut model, Message::Quit);
        assert!(model.help.is_none());
        assert!(model.exit.is_none());
    }

    #[test]
    fn help_popup_navigation_and_scrolling() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());

        // Défilement vers le bas
        update(&mut model, Message::Down);
        assert_eq!(model.help.unwrap().scroll, 1);
        update(&mut model, Message::Down);
        assert_eq!(model.help.unwrap().scroll, 2);

        // Défilement vers le haut
        update(&mut model, Message::Up);
        assert_eq!(model.help.unwrap().scroll, 1);

        // Fin de liste
        update(&mut model, Message::End);
        let max = help_max_scroll(&model);
        assert_eq!(model.help.unwrap().scroll, max);

        // Début de liste
        update(&mut model, Message::Home);
        assert_eq!(model.help.unwrap().scroll, 0);
    }

    #[test]
    fn help_popup_ignores_other_keys() {
        let mut model = loaded_model((100, 30));
        let selected_before = model.tree.selected;
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());

        // Les touches ordinaires (r, d, s, a, Tab, Entrée) sont ignorées
        update(&mut model, Message::RunSelected);
        assert!(model.run.active.is_none());

        update(&mut model, Message::Delete);
        update(&mut model, Message::Add);
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert_eq!(model.tree.selected, selected_before);
        assert!(model.editing.is_none());
        assert!(model.help.is_some());
    }

    #[test]
    fn help_popup_ctrl_c_force_quits() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());

        update(&mut model, Message::ForceQuit);
        assert!(model.help.is_none());
        assert!(matches!(model.exit, Some(Exit::Normal)));
    }

    #[test]
    fn help_popup_mouse_click_outside_closes_and_inside_keeps_open() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleHelp);
        assert!(model.help.is_some());

        let area = help_popup_area(Rect::new(0, 0, model.size.0, model.size.1));

        // Clic à l'intérieur du popup d'aide : reste ouvert
        let inside = MouseInput {
            kind: MouseKind::Press,
            column: area.x + 5,
            row: area.y + 5,
        };
        update(&mut model, Message::Mouse(inside));
        assert!(model.help.is_some());

        // Clic en dehors du popup d'aide : ferme le popup
        let outside = MouseInput {
            kind: MouseKind::Press,
            column: 0,
            row: 0,
        };
        update(&mut model, Message::Mouse(outside));
        assert!(model.help.is_none());
    }

    // --- Mode plein écran (zoom — touche 'z') ------------------------------

    #[test]
    fn zoom_toggle_on_three_main_panels() {
        let mut model = loaded_model((100, 30));

        // 1. Arbre
        model.focus = Focus::Tree;
        assert_eq!(model.zoomed_panel(), None);
        update(&mut model, Message::ToggleZoom);
        assert!(model.zoom);
        assert_eq!(model.zoomed_panel(), Some(Focus::Tree));
        update(&mut model, Message::ToggleZoom);
        assert!(!model.zoom);
        assert_eq!(model.zoomed_panel(), None);

        // 2. Détail
        model.focus = Focus::Detail;
        update(&mut model, Message::ToggleZoom);
        assert!(model.zoom);
        assert_eq!(model.zoomed_panel(), Some(Focus::Detail));
        update(&mut model, Message::ToggleZoom);
        assert!(!model.zoom);
        assert_eq!(model.zoomed_panel(), None);

        // 3. Réponse
        model.focus = Focus::Response;
        update(&mut model, Message::ToggleZoom);
        assert!(model.zoom);
        assert_eq!(model.zoomed_panel(), Some(Focus::Response));
        update(&mut model, Message::ToggleZoom);
        assert!(!model.zoom);
        assert_eq!(model.zoomed_panel(), None);
    }

    #[test]
    fn zoom_ignored_on_auxiliary_panels() {
        let mut model = loaded_model((100, 30));

        for aux_focus in [
            Focus::Diagnostics,
            Focus::History,
            Focus::Secrets,
            Focus::Campaign,
            Focus::EnvironmentPicker,
        ] {
            model.focus = aux_focus;
            model.zoom = false;
            update(&mut model, Message::ToggleZoom);
            assert!(!model.zoom, "{aux_focus:?} ne doit pas basculer le zoom");
            assert_eq!(model.zoomed_panel(), None);
        }
    }

    #[test]
    fn tab_rotates_focus_and_displayed_panel_in_zoom() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Tree;
        model.zoom = true;
        assert_eq!(model.zoomed_panel(), Some(Focus::Tree));

        // Tab -> Détail
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(model.zoomed_panel(), Some(Focus::Detail));

        // Tab -> Réponse
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Response);
        assert_eq!(model.zoomed_panel(), Some(Focus::Response));

        // Tab -> Arbre
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.zoomed_panel(), Some(Focus::Tree));
    }

    #[test]
    fn escape_exits_zoom_and_returns_to_tree() {
        let mut model = loaded_model((100, 30));

        // Depuis le Détail zoomé
        model.focus = Focus::Detail;
        model.zoom = true;
        assert_eq!(model.zoomed_panel(), Some(Focus::Detail));
        update(&mut model, Message::FocusTree);
        assert!(!model.zoom);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.zoomed_panel(), None);

        // Depuis la Réponse zoomée
        model.focus = Focus::Response;
        model.zoom = true;
        assert_eq!(model.zoomed_panel(), Some(Focus::Response));
        update(&mut model, Message::FocusTree);
        assert!(!model.zoom);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.zoomed_panel(), None);
    }

    #[test]
    fn escape_during_edit_session_closes_session_without_exiting_zoom() {
        let mut model = session_on_post_json();
        model.zoom = true;
        assert!(model.editing.is_some());
        assert_eq!(model.zoomed_panel(), Some(Focus::Detail));

        // Premier Échap : ferme la session propre, reste en zoom
        update(&mut model, Message::FocusTree);
        assert!(model.editing.is_none());
        assert!(model.zoom);
        assert_eq!(model.zoomed_panel(), Some(Focus::Detail));

        // Deuxième Échap : quitte le zoom et revient à l'arbre
        update(&mut model, Message::FocusTree);
        assert!(!model.zoom);
        assert_eq!(model.focus, Focus::Tree);
        assert_eq!(model.zoomed_panel(), None);
    }

    #[test]
    fn scroll_and_mouse_consistency_in_zoom() {
        let mut model = runner_probe_model();
        model.mouse.capture = true;
        model.size = (100, 20);
        select(&mut model, "green.bru");

        let normal_resp_max = response_max_scroll(&model);
        let normal_areas = layout_for(model.size, None).expect("layout normal");

        // Activer le zoom sur la réponse
        model.focus = Focus::Response;
        model.zoom = true;
        let zoomed_resp_max = response_max_scroll(&model);
        let zoomed_areas = layout_for(model.size, model.zoomed_panel()).expect("layout zoom");

        // La réponse est plus large en zoom qu'en disposition normale
        // (elle a déjà toute la hauteur de la colonne).
        assert!(zoomed_areas.response.width > normal_areas.response.width);
        // Donc le défilement maximal nécessaire est inférieur ou égal
        assert!(zoomed_resp_max <= normal_resp_max);

        // Molette sur la zone zoomée de réponse
        let center = (zoomed_areas.response.x + 5, zoomed_areas.response.y + 5);
        event(&mut model, MouseKind::WheelDown, center);
        if zoomed_resp_max > 0 {
            assert!(model.response_scroll > 0);
        }

        // Clic sur l'arbre masqué ne modifie pas le focus
        assert_eq!(zoomed_areas.tree, Rect::default());
        let tree_old_pos = (normal_areas.tree.x + 2, normal_areas.tree.y + 2);
        click(&mut model, tree_old_pos);
        // Le clic tombe soit hors hit soit sur le panneau zoomé selon coordonnées,
        // mais ne peut pas sélectionner l'arbre masqué
        assert_ne!(model.focus, Focus::Tree);
    }

    #[test]
    fn test_runner_options_popup_toggle_and_close() {
        let mut model = loaded_model((100, 30));
        assert!(model.runner_options_popup.is_none());

        // Ouverture via ToggleRunnerOptions
        update(&mut model, Message::ToggleRunnerOptions);
        assert!(model.runner_options_popup.is_some());
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Tags
        );

        // Fermeture via ToggleRunnerOptions
        update(&mut model, Message::ToggleRunnerOptions);
        assert!(model.runner_options_popup.is_none());

        // Réouverture puis fermeture via FocusTree (Échap)
        update(&mut model, Message::ToggleRunnerOptions);
        assert!(model.runner_options_popup.is_some());
        update(&mut model, Message::FocusTree);
        assert!(model.runner_options_popup.is_none());
    }

    #[test]
    fn test_runner_options_popup_navigation_and_boolean_toggles() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleRunnerOptions);
        let popup = model.runner_options_popup.as_mut().unwrap();
        assert!(!popup.bail);
        assert!(!popup.tests_only);
        assert!(!popup.insecure);
        assert!(!popup.noproxy);
        assert!(!popup.disable_cookies);
        assert!(!popup.reporter_junit);
        assert!(!popup.reporter_html);
        assert!(popup.skip_headers);

        // Navigation vers Bail (3x Down)
        update(&mut model, Message::Down);
        update(&mut model, Message::Down);
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Bail
        );
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().bail);

        // TestsOnly (champ 4)
        update(&mut model, Message::Down);
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().tests_only);

        // Insecure (champ 5)
        update(&mut model, Message::Down);
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().insecure);

        // Avancer jusqu'à NoProxy (champ 9) : Cacert (6), IgnoreTruststore (7), ClientCertConfig (8), NoProxy (9)
        for _ in 0..4 {
            update(&mut model, Message::Down);
        }
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::NoProxy
        );
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().noproxy);

        // DisableCookies (champ 10)
        update(&mut model, Message::Down);
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().disable_cookies);

        // Sandbox (champ 15) : 5x Down depuis DisableCookies (champ 10)
        for _ in 0..5 {
            update(&mut model, Message::Down);
        }
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Sandbox
        );
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().sandbox,
            crate::runner::SandboxMode::Safe
        );
        update(&mut model, Message::ToggleField);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().sandbox,
            crate::runner::SandboxMode::Developer
        );

        // Verbose (champ 16)
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Verbose
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().verbose);
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().verbose);

        // ReporterJunit (champ 17)
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::ReporterJunit
        );
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().reporter_junit);

        // ReporterHtml (champ 18)
        update(&mut model, Message::Down);
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().reporter_html);

        // SkipHeaders (champ 19, actif par défaut, on le bascule à false)
        update(&mut model, Message::Down);
        update(&mut model, Message::ToggleField);
        assert!(!model.runner_options_popup.as_ref().unwrap().skip_headers);

        // Fermeture avec FocusTree (sauvegarde auto car valide)
        update(&mut model, Message::FocusTree);
        assert!(model.runner_options_popup.is_none());
        assert!(model.runner_options.bail);
        assert!(model.runner_options.tests_only);
        assert!(model.runner_options.insecure);
        assert!(model.runner_options.noproxy);
        assert!(model.runner_options.disable_cookies);
        assert_eq!(
            model.runner_options.sandbox,
            crate::runner::SandboxMode::Developer
        );
        assert!(model.runner_options.verbose);
        assert!(model.runner_options.export_junit);
        assert!(model.runner_options.export_html);
        assert!(!model.runner_options.skip_headers);
    }

    #[test]
    fn test_runner_options_popup_text_inputs_and_validation() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleRunnerOptions);

        // Champ Tags : saisie valide "smoke,unit"
        update(&mut model, Message::Enter);
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);

        for c in "smoke,unit".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_none());

        // Erreur de validation avec un tag contenant un espace
        update(&mut model, Message::Enter);
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char(' ')),
        );
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char('x')),
        );
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_some());

        // Correction de l'erreur
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Backspace),
        );
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Backspace),
        );
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_none());

        // Champ Delay (champ d'indice 2)
        update(&mut model, Message::Down);
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Delay
        );
        update(&mut model, Message::Enter);
        // Saisie invalide "abc"
        for c in "abc".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_some());

        // Effacement et saisie valide "150"
        for _ in 0..3 {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Backspace),
            );
        }
        for c in "150".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_none());

        // Naviguer jusqu'au bouton Enregistrer (champ fixe 23, 21 fois Down
        // depuis le champ 2, plus une case par environnement) et presser Entrée
        let envs = model
            .runner_options_popup
            .as_ref()
            .unwrap()
            .available_environments
            .len();
        for _ in 0..21 + envs {
            update(&mut model, Message::Down);
        }
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::SaveButton
        );
        update(&mut model, Message::Enter);

        assert!(model.runner_options_popup.is_none());
        assert_eq!(model.last_status, Some(StatusMessage::RunnerOptionsSaved));
        assert_eq!(model.runner_options.tags, vec!["smoke", "unit"]);
        assert_eq!(model.runner_options.delay_ms, Some(150));
    }

    #[test]
    fn test_runner_options_popup_network_and_tls_validation() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleRunnerOptions);

        // Aller à Cacert (champ 6) : 6x Down
        for _ in 0..6 {
            update(&mut model, Message::Down);
        }
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Cacert
        );

        // Saisir un fichier qui n'existe pas
        update(&mut model, Message::Enter);
        for c in "non_existent_cert.pem".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_some());
        assert!(
            model
                .runner_options_popup
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("n'existe pas")
        );

        // Remplacer par un fichier existant ("Cargo.toml")
        for _ in 0.."non_existent_cert.pem".len() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Backspace),
            );
        }
        for c in "Cargo.toml".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_none());

        // Champ 7: IgnoreTruststore
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::IgnoreTruststore
        );
        update(&mut model, Message::ToggleField);
        assert!(
            model
                .runner_options_popup
                .as_ref()
                .unwrap()
                .ignore_truststore
        );

        // Maintenant, vider cacert pour vérifier l'erreur IgnoreTruststoreWithoutCacert
        update(&mut model, Message::Up); // retour à Cacert
        update(&mut model, Message::Enter);
        for _ in 0.."Cargo.toml".len() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Backspace),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        // La validation doit échouer car ignore_truststore est actif sans cacert
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(
            model
                .runner_options_popup
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("--ignore-truststore")
        );
    }

    #[test]
    fn test_runner_options_popup_data_and_iterations_validation() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleRunnerOptions);

        // Aller à CsvFilePath (champ 11) : 11x Down
        for _ in 0..11 {
            update(&mut model, Message::Down);
        }
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::CsvFilePath
        );

        // Saisir un fichier CSV qui n'existe pas
        update(&mut model, Message::Enter);
        for c in "non_existent.csv".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_some());

        // Remplacer par un fichier existant ("Cargo.toml")
        for _ in 0.."non_existent.csv".len() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Backspace),
            );
        }
        for c in "Cargo.toml".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(model.runner_options_popup.as_ref().unwrap().error.is_none());

        // Champ 12 : JsonFilePath
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::JsonFilePath
        );
        // Tenter de définir aussi un JSON alors que CSV est renseigné
        update(&mut model, Message::Enter);
        for c in "Cargo.lock".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(
            model
                .runner_options_popup
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("exclusifs")
        );

        // Vider JsonFilePath
        for _ in 0.."Cargo.lock".len() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Backspace),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);

        // Champ 13 : IterationCount
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::IterationCount
        );
        update(&mut model, Message::Enter);
        for c in "0".chars() {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char(c)),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(model.runner_options_popup.as_ref().unwrap().editing_text);
        assert!(
            model
                .runner_options_popup
                .as_ref()
                .unwrap()
                .error
                .as_ref()
                .unwrap()
                .contains("positif non nul")
        );

        // Corriger en "5"
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Backspace),
        );
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char('5')),
        );
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Enter),
        );
        assert!(!model.runner_options_popup.as_ref().unwrap().editing_text);

        // Champ 14 : Parallel
        update(&mut model, Message::Down);
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::Parallel
        );
        update(&mut model, Message::ToggleField);
        assert!(model.runner_options_popup.as_ref().unwrap().parallel);

        // Aller à SaveButton (champ fixe 23) : 9x Down, plus une case par environnement
        let envs = model
            .runner_options_popup
            .as_ref()
            .unwrap()
            .available_environments
            .len();
        for _ in 0..9 + envs {
            update(&mut model, Message::Down);
        }
        assert_eq!(
            model.runner_options_popup.as_ref().unwrap().current_field(),
            crate::app::model::RunnerOptionsField::SaveButton
        );
        update(&mut model, Message::Enter);
        assert!(model.runner_options_popup.is_none());
        assert_eq!(model.last_status, Some(StatusMessage::RunnerOptionsSaved));
        assert!(model.runner_options.csv_file_path.is_some());
        assert_eq!(model.runner_options.iteration_count, Some(5));
        assert!(model.runner_options.parallel);
    }

    #[test]
    fn test_run_finished_multi_iterations_aggregates_and_stores_last_outcome() {
        let mut model = runner_probe_model();
        let report: crate::runner::report::Report = serde_json::from_str(include_str!(
            "../../../tests/fixtures/reports/csv-iterations.json"
        ))
        .expect("rapport csv-iterations valide");

        let run_id = crate::runner::RunId(42);
        model.run.active = Some(crate::app::model::ActiveRun {
            id: run_id,
            target: std::path::PathBuf::from("csv-probe"),
            recursive: true,
            is_campaign: true,
            handle: None,
        });

        update(
            &mut model,
            Message::RunFinished(crate::runner::RunEvent {
                id: run_id,
                outcome: crate::runner::RunOutcome::Completed {
                    report,
                    exit_code: Some(0),
                    output: "2 iterations completed".to_string(),
                },
            }),
        );

        let campaign = model.campaign.as_ref().expect("campagne présente");
        assert_eq!(campaign.total_iterations, 2);
        assert_eq!(campaign.total, 2);
        assert_eq!(campaign.passed, 2);
        assert_eq!(campaign.failed, 0);
        assert_eq!(campaign.items.len(), 2);
        assert_eq!(campaign.items[0].iteration_index, 0);
        assert_eq!(campaign.items[1].iteration_index, 1);

        // Outcomes : la requête req.bru doit contenir la dernière itération avec mention (2, 2)
        let outcome = model
            .run
            .outcomes
            .get(std::path::Path::new("req.bru"))
            .expect("outcome présent pour req.bru");
        assert_eq!(outcome.iteration, Some((2, 2)));
    }

    #[test]
    fn test_run_selected_from_campaign_panel_uses_runner_options() {
        let mut model = runner_probe_model();
        model.runner_options = crate::runner::RunnerOptionsConfig {
            tags: vec!["smoke".into()],
            exclude_tags: vec!["skip".into()],
            delay_ms: Some(50),
            bail: true,
            tests_only: true,
            export_junit: true,
            export_html: true,
            skip_headers: true,
            ..crate::runner::RunnerOptionsConfig::default()
        };

        model.focus = Focus::Campaign;
        let cmd = update(&mut model, Message::RunSelected);

        match cmd {
            Command::StartRun { request, .. } => {
                assert!(request.recursive);
                assert_eq!(request.options.tags, vec!["smoke"]);
                assert_eq!(request.options.exclude_tags, vec!["skip"]);
                assert_eq!(request.options.delay_ms, Some(50));
                assert!(request.options.bail);
                assert!(request.options.tests_only);
                assert!(request.options.reporter_junit.is_some());
                assert!(request.options.reporter_html.is_some());
                assert!(request.options.reporter_skip_all_headers);
                assert_eq!(model.last_reports.len(), 2);
            }
            other => panic!("attendu StartRun, reçu {other:?}"),
        }
    }

    // --- Tests de la recherche globale (Ctrl+K) ---

    #[test]
    fn test_global_search_open_close_toggle() {
        let mut model = loaded_model((100, 30));
        assert!(model.global_search.is_none());

        // Ouverture via ToggleGlobalSearch
        update(&mut model, Message::ToggleGlobalSearch);
        assert!(model.global_search.is_some());
        assert_eq!(model.global_search.as_ref().unwrap().input.text(), "");
        assert_eq!(model.global_search.as_ref().unwrap().selected, 0);

        // Fermeture via ToggleGlobalSearch (bascule)
        update(&mut model, Message::ToggleGlobalSearch);
        assert!(model.global_search.is_none());

        // Réouverture puis fermeture par CancelGlobalSearch (Échap)
        update(&mut model, Message::ToggleGlobalSearch);
        assert!(model.global_search.is_some());
        update(&mut model, Message::CancelGlobalSearch);
        assert!(model.global_search.is_none());
    }

    #[test]
    fn test_global_search_live_input_and_navigation() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleGlobalSearch);

        // Saisie en direct de 'g', 'e', 't'
        update(&mut model, Message::GlobalSearchInput('g'));
        update(&mut model, Message::GlobalSearchInput('e'));
        update(&mut model, Message::GlobalSearchInput('t'));
        assert_eq!(model.global_search.as_ref().unwrap().input.text(), "get");
        assert_eq!(model.global_search.as_ref().unwrap().selected, 0);

        // Navigation vers le bas puis vers le haut
        update(&mut model, Message::GlobalSearchDown);
        assert_eq!(model.global_search.as_ref().unwrap().selected, 1);
        update(&mut model, Message::GlobalSearchUp);
        assert_eq!(model.global_search.as_ref().unwrap().selected, 0);

        // Effacement par Backspace
        update(&mut model, Message::GlobalSearchBackspace);
        assert_eq!(model.global_search.as_ref().unwrap().input.text(), "ge");
    }

    #[test]
    fn test_global_search_confirm_selects_and_focuses_detail() {
        use std::path::Path;
        let mut model = loaded_model((100, 30));
        assert_eq!(model.focus, Focus::Tree);

        // Ouvrir et chercher "scripted"
        update(&mut model, Message::ToggleGlobalSearch);
        for c in "scripted".chars() {
            update(&mut model, Message::GlobalSearchInput(c));
        }

        // Valider avec Entrée
        update(&mut model, Message::ConfirmGlobalSearch);
        assert!(model.global_search.is_none());
        assert_eq!(model.focus, Focus::Detail);
        assert_eq!(
            model.selected_node().unwrap().path(),
            Path::new("scripted.bru")
        );
    }

    #[test]
    fn test_global_search_mouse_click_outside_closes() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::ToggleGlobalSearch);
        assert!(model.global_search.is_some());

        // Clic à l'extérieur du popup (en coin 0, 0)
        update(
            &mut model,
            Message::Mouse(MouseInput {
                kind: MouseKind::Press,
                column: 0,
                row: 0,
            }),
        );
        assert!(model.global_search.is_none());
    }

    #[test]
    fn test_collection_loaded_requests_history_when_enabled() {
        use crate::collection::CollectionLoader;
        let fixture = crate::app::test_support::fixture();
        let mut model = Model::new(fixture.clone(), (100, 30));
        model.history_enabled = true;
        let loaded = crate::collection::BruLoader.load(&fixture);
        let cmd = update(&mut model, Message::CollectionLoaded(loaded));
        match cmd {
            Command::Batch(cmds) => {
                assert!(
                    cmds.iter()
                        .any(|c| matches!(c, Command::LoadHistory { .. }))
                );
            }
            Command::LoadHistory { root } => {
                assert_eq!(root, fixture);
            }
            other => panic!("LoadHistory ou Batch attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn test_history_loaded_populates_model_and_records_notice() {
        use std::path::PathBuf;
        let mut model = loaded_model((100, 30));
        let root = model.loaded().unwrap().root.clone();
        let entries = vec![crate::app::model::HistoryEntry {
            started_at: std::time::SystemTime::UNIX_EPOCH,
            target: PathBuf::from("req.bru"),
            recursive: false,
            outcome: crate::app::model::HistoryOutcome::Cancelled,
        }];
        update(
            &mut model,
            Message::HistoryLoaded {
                root,
                entries,
                notice: Some("corrompu".to_owned()),
            },
        );
        assert_eq!(model.history.len(), 1);
        assert_eq!(model.history[0].target, PathBuf::from("req.bru"));
        assert!(
            model
                .loaded()
                .unwrap()
                .notices
                .iter()
                .any(|n| n.message.contains("corrompu"))
        );
    }
}
