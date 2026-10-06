//! Rendu de l'interface.
//!
//! `view` est pur : il lit le modèle et dessine, sans I/O ni modification
//! d'état. `layout` est partagé avec `update`, qui en déduit la hauteur des
//! panneaux pour garder la sélection visible et borner le défilement.

pub mod detail;
pub mod help;
pub mod hit;
pub mod panels;
pub mod status;
pub mod theme;
pub mod tree;

pub use help::{help_max_scroll, help_popup_area};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use super::model::{
    CollectionState, EditState, Focus, Model, PendingConfirm, RunFailure, StatusMessage,
};
use super::update::{active_tree_filter_pattern, is_tree_filtered};
use crate::collection::TreeNode;

/// Largeur minimale du terminal : exactement la somme des planchers de
/// l'arbre, du détail et de la réponse (`MIN_TREE_WIDTH +
/// MIN_DETAIL_WIDTH + MIN_RESPONSE_WIDTH`), pour qu'à cette taille chaque
/// panneau soit exactement à son plancher, sans marge (`design.md`, D4).
pub const MIN_WIDTH: u16 = 60;
/// Hauteur minimale du terminal : assez pour que le panneau Réponse
/// garde au moins une ligne intérieure sous les formes compactes
/// d'Environnement et de Statut empilées au-dessus
/// (`add-environment-panel-and-edit-popup`).
pub const MIN_HEIGHT: u16 = 11;
/// Largeur minimale du panneau de l'arbre.
const MIN_TREE_WIDTH: u16 = 24;
/// Largeur minimale du panneau de détail.
const MIN_DETAIL_WIDTH: u16 = 18;
/// Largeur minimale du panneau de réponse.
const MIN_RESPONSE_WIDTH: u16 = 18;

/// Hauteur de terminal à partir de laquelle le pied de page porte le fil
/// d'Ariane et le panneau Environnement sa forme complète.
const FULL_MIN_TERMINAL_HEIGHT: u16 = 20;
/// Largeur maximale du panneau Environnement en surimpression.
const ENV_PANEL_MAX_WIDTH: u16 = 44;
/// Hauteur du panneau Environnement en forme complète (4 lignes
/// intérieures), fixe et indépendante du nombre d'environnements —
/// `layout()` reste une fonction de géométrie pure, indépendante du
/// modèle (`add-environment-panel-and-edit-popup`, design D1).
const ENV_PANEL_HEIGHT: u16 = 6;
/// Hauteur du panneau Environnement en forme compacte (1 ligne
/// intérieure).
const ENV_PANEL_COMPACT_HEIGHT: u16 = 3;

const TOO_SMALL: &str = "Terminal trop petit : agrandir à 60×11 au moins.";

const TITLE_TREE: &str = " Collection ";
const TITLE_TREE_ZOOMED: &str = " Collection [plein écran — z] ";
const TITLE_DETAIL: &str = " Détail ";
const TITLE_DETAIL_ZOOMED: &str = " Détail [plein écran — z] ";
const TITLE_RESPONSE: &str = " Réponse ";
const TITLE_RESPONSE_ZOOMED: &str = " Réponse [plein écran — z] ";

/// Zones de l'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Areas {
    pub title: Rect,
    /// Arbre, détail et réponse réunis.
    pub body: Rect,
    pub tree: Rect,
    pub detail: Rect,
    /// Panneau Environnement, en surimpression en haut à droite du corps :
    /// dessiné et cliquable seulement quand il a le focus (touche `E`).
    pub environment: Rect,
    /// Panneau Réponse, sur toute la hauteur de la colonne de droite ; sa
    /// première ligne intérieure porte les onglets et le statut.
    pub response: Rect,
    /// Barre d'état du bas de l'écran (rappel de touches, messages).
    pub status: Rect,
    /// Fil d'Ariane du nœud sélectionné, sous la barre d'état ; vide
    /// (hauteur 0) sous [`FULL_MIN_TERMINAL_HEIGHT`].
    pub breadcrumb: Rect,
}

impl Areas {
    /// Vrai si le panneau Environnement est en forme compacte.
    pub fn environment_panel_compact(&self) -> bool {
        self.environment.height < ENV_PANEL_HEIGHT
    }
}

/// Découpe l'écran ; `None` sous la taille minimale.
///
/// Si `zoom` vaut `Some(Focus::Tree)`, `Some(Focus::Detail)` ou
/// `Some(Focus::Response)`, le panneau désigné occupe l'intégralité de la
/// zone du corps.
pub fn layout(area: Rect, zoom: Option<Focus>) -> Option<Areas> {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return None;
    }
    let full_size = area.height >= FULL_MIN_TERMINAL_HEIGHT;
    // Pied de page : barre d'état, plus le fil d'Ariane en grande taille.
    let footer_height = if full_size { 2 } else { 1 };
    let body_height = area.height - 1 - footer_height;
    let body = Rect::new(area.x, area.y + 1, area.width, body_height);
    let title = Rect::new(area.x, area.y, area.width, 1);
    let status = Rect::new(area.x, area.y + 1 + body_height, area.width, 1);
    let breadcrumb = if full_size {
        Rect::new(area.x, area.y + area.height - 1, area.width, 1)
    } else {
        Rect::default()
    };

    match zoom {
        Some(Focus::Tree) => Some(Areas {
            title,
            body,
            tree: body,
            detail: Rect::default(),
            environment: Rect::default(),
            response: Rect::default(),
            status,
            breadcrumb,
        }),
        Some(Focus::Detail) => Some(Areas {
            title,
            body,
            tree: Rect::default(),
            detail: body,
            environment: Rect::default(),
            response: Rect::default(),
            status,
            breadcrumb,
        }),
        Some(Focus::Response) => Some(Areas {
            title,
            body,
            tree: Rect::default(),
            detail: Rect::default(),
            environment: Rect::default(),
            response: body,
            status,
            breadcrumb,
        }),
        _ => {
            // La réponse reçoit la plus large colonne : c'est elle qu'on lit
            // le plus (arbre 30 %, détail 45 % du reste, réponse le solde).
            let tree_width = (area.width * 30 / 100).max(MIN_TREE_WIDTH);
            let remaining = area.width - tree_width;
            let detail_width = (remaining * 45 / 100).max(MIN_DETAIL_WIDTH);
            let response_width = (remaining - detail_width).max(MIN_RESPONSE_WIDTH);
            let response_x = body.x + tree_width + detail_width;
            let env_panel_height = if full_size {
                ENV_PANEL_HEIGHT
            } else {
                ENV_PANEL_COMPACT_HEIGHT
            };
            // Environnement : surimpression calée en haut à droite, sous
            // la liste déroulante de l'en-tête, comme dans Bruno bureau.
            let env_width = response_width.min(ENV_PANEL_MAX_WIDTH);
            Some(Areas {
                title,
                body,
                tree: Rect::new(body.x, body.y, tree_width, body_height),
                detail: Rect::new(body.x + tree_width, body.y, detail_width, body_height),
                response: Rect::new(response_x, body.y, response_width, body_height),
                environment: Rect::new(
                    body.right() - env_width,
                    body.y,
                    env_width,
                    env_panel_height,
                ),
                status,
                breadcrumb,
            })
        }
    }
}

/// Zones de l'écran pour une taille de terminal.
pub fn layout_for(size: (u16, u16), zoom: Option<Focus>) -> Option<Areas> {
    layout(Rect::new(0, 0, size.0, size.1), zoom)
}

/// Intérieur d'un panneau bordé.
pub fn inner(area: Rect) -> Rect {
    Block::bordered().inner(area)
}

/// Dessine l'interface.
pub fn view(model: &Model, frame: &mut Frame) {
    frame.render_widget(Block::default().style(theme::BACKGROUND), frame.area());

    let Some(areas) = layout(frame.area(), model.zoomed_panel()) else {
        frame.render_widget(
            Paragraph::new(TOO_SMALL).wrap(Wrap { trim: true }),
            frame.area(),
        );
        return;
    };

    render_header(model, frame, areas.title);

    match &model.collection {
        CollectionState::Loading => {
            frame.render_widget(
                Paragraph::new("Chargement…").block(panel(" Collection ", true)),
                areas.tree,
            );
            frame.render_widget(panel(" Détail ", false), areas.detail);
            frame.render_widget(panel(" Réponse ", false), areas.response);
        }
        CollectionState::Failed(error) => {
            let lines = vec![
                Line::styled(
                    "Impossible de charger la collection",
                    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
                Line::default(),
                Line::raw(error.to_string()),
            ];
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(Wrap { trim: false })
                    .block(panel(" Erreur ", true)),
                areas.body,
            );
        }
        CollectionState::Loaded(_) => match model.focus {
            Focus::Diagnostics => {
                panels::render_diagnostics(model, frame, areas.body);
            }
            Focus::History => {
                panels::render_history(model, frame, areas.body);
            }
            Focus::Secrets => {
                panels::render_secrets(model, frame, areas.body);
            }
            Focus::Campaign => {
                panels::render_campaign(model, frame, areas.body);
            }
            Focus::Tree | Focus::Detail | Focus::Response | Focus::EnvironmentPicker => {
                if areas.tree.width > 0 && areas.tree.height > 0 {
                    render_tree(model, frame, areas.tree);
                }
                if areas.detail.width > 0 && areas.detail.height > 0 {
                    let title = if model.zoomed_panel() == Some(Focus::Detail) {
                        TITLE_DETAIL_ZOOMED
                    } else {
                        TITLE_DETAIL
                    };
                    let mut detail_block = panel(title, model.focus == Focus::Detail);
                    if let Some(TreeNode::Request(request)) = model.selected_node() {
                        // Onglet de la requête ouverte : méthode et nom
                        // en pastille orange, à la suite du titre.
                        detail_block = detail_block.title(Span::styled(
                            format!(
                                " {} {} ",
                                request.view.method.to_ascii_uppercase(),
                                detail::request_name(request)
                            ),
                            theme::SELECTION,
                        ));
                    }
                    match model.selected_node() {
                        Some(TreeNode::Request(_)) => {
                            render_boxed_detail(model, frame, areas.detail, detail_block);
                        }
                        _ => {
                            // Pendant une saisie, pas de retour à la ligne : une
                            // ligne logique = une ligne affichée, pour que le
                            // curseur de texte et le décalage horizontal tombent
                            // exactement (`improve-direct-editing`, D7).
                            let mut detail = Paragraph::new(detail::render_text(model))
                                .scroll((model.detail_scroll, 0))
                                .block(detail_block);
                            if detail_wraps(model) {
                                detail = detail.wrap(Wrap { trim: false });
                            }
                            frame.render_widget(detail, areas.detail);
                        }
                    }
                    if let Some(button) = hit::run_button_area(model, areas.detail) {
                        frame.render_widget(
                            Paragraph::new(Span::styled(
                                hit::RUN_BUTTON,
                                theme::SUCCESS.fg.map_or(theme::SUCCESS, theme::badge_on),
                            )),
                            button,
                        );
                    }
                    render_insert_cursor(model, frame, areas.detail);
                }
                if areas.response.width > 0 && areas.response.height > 0 {
                    render_response(model, frame, areas.response);
                }
                // Panneau Environnement : en surimpression, par-dessus la
                // réponse, seulement quand il a le focus (touche `E`).
                if model.focus == Focus::EnvironmentPicker
                    && areas.environment.width > 0
                    && areas.environment.height > 0
                {
                    frame.render_widget(Clear, areas.environment);
                    frame.render_widget(
                        Block::default().style(theme::BACKGROUND),
                        areas.environment,
                    );
                    panels::render_environment_picker(model, frame, areas.environment);
                }

                // Popup d'édition d'un environnement : superposé au centre
                // de l'écran, par-dessus tout le reste déjà dessiné
                // ci-dessus (`add-environment-panel-and-edit-popup`).
                if let Some(session) = &model.environment_editing {
                    let popup_area = environment_edit_popup_area(frame.area(), session);
                    frame.render_widget(Clear, popup_area);
                    frame.render_widget(Block::default().style(theme::BACKGROUND), popup_area);
                    panels::render_environment_edit_popup(
                        session,
                        model.focus == Focus::EnvironmentPicker,
                        frame,
                        popup_area,
                    );
                }
            }
        },
    }

    if let Some(help) = &model.help {
        let popup_area = help_popup_area(frame.area());
        frame.render_widget(Clear, popup_area);
        frame.render_widget(Block::default().style(theme::BACKGROUND), popup_area);
        help::render_help_popup(help, frame, popup_area);
    }

    frame.render_widget(
        Paragraph::new(status_line(model)).style(Style::new().add_modifier(Modifier::DIM)),
        areas.status,
    );
    if areas.breadcrumb.height > 0 {
        frame.render_widget(
            Paragraph::new(breadcrumb_line(model)).style(theme::BREADCRUMB),
            areas.breadcrumb,
        );
    }
}

/// En-tête sur une ligne : nom de l'application et de la collection à
/// gauche, rappel de recherche au centre, environnement actif à droite.
/// Les deux derniers ne sont posés que s'ils tiennent sans chevaucher.
fn render_header(model: &Model, frame: &mut Frame, area: Rect) {
    let title = title_line(model);
    let title_width = u16::try_from(title.width()).unwrap_or(u16::MAX);
    frame.render_widget(Paragraph::new(title), area);
    let CollectionState::Loaded(_) = model.collection else {
        return;
    };
    let mut right_edge = area.right();
    if let Some(chip) = environment_chip_area(model, area) {
        right_edge = chip.x;
        frame.render_widget(Paragraph::new(environment_chip(model)), chip);
    }
    let search = Line::from(Span::styled(SEARCH_HINT, theme::EDITABLE_BODY));
    let search_width = u16::try_from(search.width()).unwrap_or(u16::MAX);
    let centered = area.x + area.width.saturating_sub(search_width) / 2;
    if centered > area.x + title_width + 1 && centered + search_width + 1 < right_edge {
        frame.render_widget(
            Paragraph::new(search).style(theme::LABEL),
            Rect::new(centered, area.y, search_width, 1),
        );
    }
}

/// Zone de la liste déroulante d'environnement, calée à droite de
/// l'en-tête `title` ; `None` si elle chevaucherait le titre. Partagée
/// par le rendu et le clic.
pub(crate) fn environment_chip_area(model: &Model, title: Rect) -> Option<Rect> {
    let CollectionState::Loaded(_) = model.collection else {
        return None;
    };
    let title_width = u16::try_from(title_line(model).width()).unwrap_or(u16::MAX);
    let env_width = u16::try_from(environment_chip(model).width()).unwrap_or(u16::MAX);
    (title_width + 1 + env_width <= title.width)
        .then(|| Rect::new(title.right() - env_width, title.y, env_width, 1))
}

/// Rappel de la recherche dans l'en-tête : la touche `/` ouvre la saisie.
const SEARCH_HINT: &str = " ⌕ Rechercher : /                ";

/// Environnement actif en liste déroulante, ouverte par `E`.
fn environment_chip(model: &Model) -> Line<'static> {
    let env_label = model
        .current_environment
        .as_deref()
        .unwrap_or("Aucun environnement");
    Line::from(vec![
        Span::styled(" E ", theme::LABEL),
        Span::styled(format!(" {env_label} ▾ "), theme::ENV_CHIP),
    ])
}

/// Fil d'Ariane du nœud sélectionné : collection, dossiers, puis le nom du
/// nœud en évidence.
fn breadcrumb_line(model: &Model) -> Line<'static> {
    let CollectionState::Loaded(collection) = &model.collection else {
        return Line::default();
    };
    let separator = || Span::styled(" › ", theme::LABEL);
    let mut spans = vec![
        Span::styled(" ⌂", theme::METHOD),
        separator(),
        Span::raw(collection.name.clone()),
    ];
    let Some(node) = model.selected_node() else {
        return Line::from(spans);
    };
    if let Some(parent) = node.path().parent() {
        for component in parent.components() {
            spans.push(separator());
            spans.push(Span::raw(
                component.as_os_str().to_string_lossy().into_owned(),
            ));
        }
    }
    spans.push(separator());
    match node {
        TreeNode::Request(request) => {
            spans.push(Span::styled(
                format!("{} ", request.view.method.to_ascii_uppercase()),
                theme::method_style(&request.view.method),
            ));
            spans.push(Span::styled(detail::request_name(request), theme::FOCUS));
        }
        TreeNode::Folder(folder) => spans.push(Span::styled(folder.name.clone(), theme::FOCUS)),
        TreeNode::Error(error) => spans.push(Span::styled(
            error.path.display().to_string(),
            theme::LOAD_ERROR,
        )),
    }
    Line::from(spans)
}

/// Zone du popup d'édition d'un environnement : centrée sur `screen`,
/// dimensionnée au nombre de variables (largeur/hauteur bornées à
/// l'écran), jamais hors de l'écran
/// (`add-environment-panel-and-edit-popup`, design D3).
pub(crate) fn environment_edit_popup_area(
    screen: Rect,
    session: &crate::app::model::EnvironmentEditSession,
) -> Rect {
    let width = (screen.width / 2).clamp(36, 60).min(screen.width);
    let max_height = screen.height.saturating_sub(4).max(4);
    // En-tête de colonnes (1) + une ligne par variable (au moins 1 pour
    // « aucune variable ») + la ligne provisoire d'un ajout en cours,
    // s'il y en a un (`add-environment-entry-management`) + bordure (2).
    let is_adding = matches!(
        session.state,
        crate::app::model::EnvironmentEditState::AddingKey(_)
            | crate::app::model::EnvironmentEditState::AddingValue { .. }
    );
    let variable_rows = if is_adding {
        session.variables.len() + 1
    } else {
        session.variables.len().max(1)
    };
    let content_rows = u16::try_from(variable_rows).unwrap_or(u16::MAX);
    let height = content_rows.saturating_add(3).clamp(4, max_height);
    let x = screen.x + screen.width.saturating_sub(width) / 2;
    let y = screen.y + screen.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

/// Une saisie de champ est en cours sur la requête affichée.
/// Le détail est rendu avec retour à la ligne, sauf pendant une saisie.
/// Partagé avec le hit-testing (`hit`), pour ne jamais diverger du rendu.
pub(crate) fn detail_wraps(model: &Model) -> bool {
    !input_in_progress(model)
}

fn input_in_progress(model: &Model) -> bool {
    match (&model.editing, model.selected_node()) {
        (Some(session), Some(TreeNode::Request(request))) => {
            request.path == session.path && matches!(session.state, EditState::Input(_))
        }
        _ => false,
    }
}

/// Rend le détail d'une requête sélectionnée : chaque section (En-têtes,
/// Paramètres de requête, Paramètres de chemin, Corps) dans sa propre
/// boîte bordée, le reste en lignes simples (`add-boxed-detail-sections`).
/// Compose dans un tampon virtuel de la hauteur totale du contenu, puis
/// copie la tranche visible dans le tampon réel de la frame (design.md,
/// « Rendu par tampon virtuel composé ») : un `Paragraph::scroll` seul ne
/// peut pas dessiner un `Block` bordé autour d'un sous-ensemble de ses
/// lignes.
fn render_boxed_detail(model: &Model, frame: &mut Frame, area: Rect, block: Block<'static>) {
    frame.render_widget(block, area);
    let inner_area = inner(area);
    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }
    let text = detail::render_text(model);
    let boxes = detail::detail_section_boxes(model);
    let wraps = detail_wraps(model);
    let total_height = detail::content_height(&text.lines, &boxes, inner_area.width, wraps);
    let virtual_area = Rect::new(0, 0, inner_area.width, total_height.max(1));
    let mut virtual_buf = Buffer::empty(virtual_area);
    // Le tampon virtuel part de cellules vierges : il reprend le fond de
    // l'application pour ne pas trouer le panneau une fois recopié.
    virtual_buf.set_style(virtual_area, theme::BACKGROUND);
    detail::compose(&text.lines, &boxes, wraps, &mut virtual_buf);

    let scroll = model.detail_scroll;
    let frame_buf = frame.buffer_mut();
    for y in 0..inner_area.height {
        let Some(src_y) = scroll.checked_add(y) else {
            break;
        };
        if src_y >= total_height {
            break;
        }
        for x in 0..inner_area.width {
            let cell = virtual_buf[(x, src_y)].clone();
            frame_buf[(inner_area.x + x, inner_area.y + y)] = cell;
        }
    }
}

fn render_insert_cursor(model: &Model, frame: &mut Frame, detail_area: Rect) {
    let Some(session) = &model.editing else {
        return;
    };
    if !matches!(session.state, EditState::Input(_)) {
        return;
    }
    let Some(TreeNode::Request(req)) = model.selected_node() else {
        return;
    };
    if req.path != session.path {
        return;
    }
    let Some((line_index, col_index)) = detail::cursor_position_in_detail(req, session) else {
        return;
    };
    let inner_area = inner(detail_area);
    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }
    let scroll = model.detail_scroll as usize;
    if line_index < scroll {
        return;
    }
    let rel_y = line_index - scroll;
    if rel_y >= inner_area.height as usize {
        return;
    }
    // Une colonne de bordure gauche en plus quand la ligne appartient à
    // une boîte de section (`add-boxed-detail-sections`) : le modèle
    // logique de position ne change pas, seul son placement à l'écran
    // tient compte du cadre (design.md, « Position du curseur de texte »).
    let boxes = detail::detail_section_boxes(model);
    let border_offset: u16 = if boxes.iter().any(|b| b.contains(line_index)) {
        1
    } else {
        0
    };
    let cursor_y = inner_area.y + rel_y as u16;
    let col_index = u16::try_from(col_index).unwrap_or(u16::MAX);
    let max_col = inner_area.width.saturating_sub(1 + border_offset);
    let cursor_x = inner_area.x + border_offset + col_index.min(max_col);
    frame.set_cursor_position((cursor_x, cursor_y));
}

fn title_line(model: &Model) -> Line<'static> {
    match &model.collection {
        CollectionState::Loading => Line::from(vec![
            Span::styled("bruno-tui", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" · "),
            Span::raw(format!("Chargement de {}…", model.source.display())),
        ]),
        CollectionState::Failed(_) => Line::from(vec![
            Span::styled("bruno-tui", Style::new().add_modifier(Modifier::BOLD)),
            Span::raw(" · "),
            Span::raw("Erreur de chargement".to_owned()),
        ]),
        CollectionState::Loaded(collection) => {
            let mut spans = vec![
                Span::styled("bruno-tui", theme::FOCUS),
                Span::raw(" · "),
                Span::raw(collection.name.clone()),
            ];
            let error_count = crate::app::diagnostics::diagnostics(collection).len();
            if error_count > 0 {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    format!("⚠ {error_count}"),
                    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        }
    }
}

/// Statut d'exécution d'un nœud, pour l'indicateur de la ligne de l'arbre.
fn run_status(model: &Model, node: &TreeNode) -> tree::RunStatus {
    let TreeNode::Request(request) = node else {
        return tree::RunStatus::None;
    };
    if model
        .run
        .active
        .as_ref()
        .is_some_and(|active| active.target == request.path)
    {
        return tree::RunStatus::Running;
    }
    match model.run.outcomes.get(&request.path) {
        Some(outcome) if outcome.result.is_failure() => tree::RunStatus::Failure,
        Some(_) => tree::RunStatus::Success,
        None => tree::RunStatus::None,
    }
}

/// Message décrivant l'exécution en cours, s'il y en a une.
fn active_run_message(model: &Model) -> Option<String> {
    let active = model.run.active.as_ref()?;
    let target = active.target.display();
    if active.recursive {
        Some(format!("Exécution récursive de {target}… (Ctrl+X annuler)"))
    } else {
        Some(format!("Exécution de {target}… (Ctrl+X annuler)"))
    }
}

/// Message décrivant la dernière exécution n'ayant produit aucun résultat
/// exploitable, s'il y en a un.
fn last_failure_message(model: &Model) -> Option<String> {
    match model.run.last_failure.as_ref()? {
        RunFailure::Error { target, error } => Some(format!(
            "Échec de l'exécution de {} : {error}",
            target.display()
        )),
        RunFailure::Cancelled { target } => {
            Some(format!("Exécution de {} annulée", target.display()))
        }
    }
}

/// Ligne de saisie de filtre, tant qu'elle est ouverte : remplace la
/// barre d'état, à la place des rappels de touches habituels.
fn filter_input_line(model: &Model) -> Option<String> {
    let filter = model.filter.as_ref()?;
    filter.editing.then(|| format!("|{}", filter.draft))
}

/// Ligne de saisie du filtre de l'arbre Collection, tant qu'elle est ouverte :
/// remplace la barre d'état, à la place des rappels de touches habituels.
fn tree_filter_input_line(model: &Model) -> Option<String> {
    let filter = model.tree_filter.as_ref()?;
    filter
        .editing
        .then(|| format!("Filtre arbre : {}", filter.draft))
}

/// Ligne de saisie de recherche, tant qu'elle est ouverte : remplace la
/// barre d'état, à la place des rappels de touches habituels.
fn search_input_line(model: &Model) -> Option<String> {
    let search = model.search.as_ref()?;
    search.editing.then(|| format!("/{}", search.draft))
}

fn status_message_text(message: &StatusMessage) -> String {
    match message {
        StatusMessage::Copied => "Copié dans le presse-papiers".to_owned(),
        StatusMessage::ClipboardError(reason) => format!("Échec de la copie : {reason}"),
        StatusMessage::NoMatch => "Aucune correspondance".to_owned(),
        StatusMessage::SaveError(reason) => format!("Échec de sauvegarde : {reason}"),
        StatusMessage::EditRefused(reason) => format!("Modification refusée : {reason}"),
        StatusMessage::EditLocked => {
            "Modifications non enregistrées : Ctrl+S pour enregistrer, Échap pour abandonner"
                .to_owned()
        }
        StatusMessage::MouseCapture(true) => "Souris activée (M pour la désactiver)".to_owned(),
        StatusMessage::MouseCapture(false) => {
            "Souris désactivée : sélection native du terminal disponible (M pour réactiver)"
                .to_owned()
        }
        StatusMessage::MouseCaptureError(reason) => {
            format!("Échec de la capture souris : {reason}")
        }
        StatusMessage::NoResponseBody => "Aucun corps de réponse à ouvrir".to_owned(),
        StatusMessage::EditorError(reason) => format!("Échec de l'éditeur : {reason}"),
        StatusMessage::CampaignFinished(text) => text.clone(),
        StatusMessage::NoFailedRequests => "Aucune requête en échec".to_owned(),
    }
}

/// Barre d'aide d'une session d'édition : état, champ, indicateur de
/// modification non enregistrée et touches utiles à l'état.
fn session_help_line(session: &crate::app::model::EditSession) -> String {
    use crate::app::model::{EditableField, InputTarget, section_label};

    let field = session.current_field();
    let field_name = field.map_or_else(String::new, |f| f.display_name(&session.preview));
    let unsaved = if session.has_unsaved() {
        " ● non enregistré"
    } else {
        ""
    };
    match &session.state {
        EditState::FieldSelect => {
            let keys = match field {
                Some(EditableField::AddRow(_)) => {
                    "↑↓ champ  Entrée ajouter  Ctrl+S enregistrer  Échap fermer"
                }
                Some(f) if f.entry().is_some() => {
                    "↑↓ champ  Entrée modifier  Espace activer  a ajouter  d supprimer  c renommer  Ctrl+S enregistrer  Échap fermer"
                }
                _ => "↑↓ champ  Entrée modifier  Ctrl+S enregistrer  Échap fermer",
            };
            format!("Sélection · {field_name}{unsaved} — {keys}")
        }
        EditState::Input(input) => {
            let label = match &session.target {
                InputTarget::Field => field_name,
                InputTarget::NewKey { section, .. } => {
                    format!("Ajout · {} · clé", section_label(*section))
                }
                InputTarget::NewValue { section, key, .. } => {
                    format!("Ajout · {} · valeur de {key}", section_label(*section))
                }
                InputTarget::RenameKey { .. } => format!("Renommage · {field_name}"),
            };
            let keys = if input.is_multiline() {
                "Entrée nouvelle ligne  Tab valider  Échap annuler  Ctrl+S enregistrer"
            } else {
                "Entrée valider  Tab valider  Échap annuler  Ctrl+S enregistrer"
            };
            format!("Saisie · {label}{unsaved} — {keys}")
        }
        EditState::MethodPicker { .. } => {
            format!("Méthode · {field_name}{unsaved} — ↑↓ choisir  Entrée valider  Échap annuler")
        }
    }
}

/// Barre d'aide d'une session d'édition d'environnement : état, variable,
/// indicateur de modification non enregistrée et touches utiles à l'état.
fn environment_session_help_line(session: &crate::app::model::EnvironmentEditSession) -> String {
    use crate::app::model::EnvironmentEditState;

    let unsaved = if session.has_unsaved() {
        " ● non enregistré"
    } else {
        ""
    };
    match &session.state {
        EnvironmentEditState::Select => {
            let var_name = session.current_variable().map_or("", |v| v.key.as_str());
            format!(
                "Sélection · {var_name}{unsaved} — ↑↓ variable  Entrée éditer  a ajouter  d supprimer  Ctrl+S sauvegarder  Échap retour"
            )
        }
        EnvironmentEditState::Input(_) => {
            let var_name = session.current_variable().map_or("", |v| v.key.as_str());
            format!(
                "Saisie · {var_name}{unsaved} — Entrée valider  Échap annuler  Ctrl+S sauvegarder"
            )
        }
        EnvironmentEditState::AddingKey(_) => {
            format!("Ajout · clé{unsaved} — Entrée/Tab valider  Échap annuler")
        }
        EnvironmentEditState::AddingValue { key, .. } => {
            format!("Ajout · {key}{unsaved} — Entrée/Tab valider  Échap annuler")
        }
    }
}

fn status_line(model: &Model) -> String {
    if let Some(confirm) = model.confirm {
        return match confirm {
            PendingConfirm::DiscardEdit => "Abandonner les modifications ? (y/n)".to_owned(),
            PendingConfirm::QuitWithUnsavedEdit => {
                "Quitter sans sauvegarder les modifications ? (y/n)".to_owned()
            }
        };
    }
    if model.help.is_some() {
        return "↑↓ défiler  Début/Fin  ? / Échap / q fermer".to_owned();
    }
    if let Some(line) = tree_filter_input_line(model) {
        return line;
    }
    if let Some(line) = filter_input_line(model) {
        return line;
    }
    if let Some(line) = search_input_line(model) {
        return line;
    }
    if let Some(message) = active_run_message(model) {
        return message;
    }
    if let Some(message) = last_failure_message(model) {
        return message;
    }
    if let Some(message) = &model.last_status {
        return status_message_text(message);
    }
    if let Some(session) = &model.editing {
        return session_help_line(session);
    }
    if model.focus == Focus::EnvironmentPicker
        && let Some(session) = &model.environment_editing
    {
        return environment_session_help_line(session);
    }
    match (&model.collection, model.focus) {
        (CollectionState::Loaded(_), Focus::Tree) => {
            "↑↓ naviguer  →/← déplier  r lancer  f filtrer  / chercher  Tab détail  S secrets  ? aide  q quitter  M souris  z plein écran"
                .to_owned()
        }
        (CollectionState::Loaded(_), Focus::Detail) => {
            "↑↓ défiler  Début/Fin  Entrée éditer  / chercher  n/N suivant  v sélection  y copier  ? aide  Échap arbre  q quitter  z plein écran"
                .to_owned()
        }
        (CollectionState::Loaded(_), Focus::Response) => {
            "↑↓ défiler  Début/Fin  ←→ onglet  / chercher  n/N suivant  v sélection  y copier  ? aide  Échap arbre  q quitter  z plein écran"
                .to_owned()
        }
        (CollectionState::Loaded(_), Focus::Diagnostics) => {
            "↑↓ naviguer  → aller au nœud  Échap arbre".to_owned()
        }
        (CollectionState::Loaded(_), Focus::History) => {
            "↑↓ naviguer  r rejouer  Échap arbre".to_owned()
        }
        (CollectionState::Loaded(_), Focus::EnvironmentPicker) => {
            "↑↓ naviguer  Entrée activer  e éditer  Échap arbre".to_owned()
        }
        (CollectionState::Loaded(_), Focus::Campaign) => {
            "↑↓ naviguer  Entrée/→ aller au détail  Échap arbre".to_owned()
        }
        (CollectionState::Loaded(_), Focus::Secrets) => secrets_hint(model).to_owned(),
        _ => "q quitter".to_owned(),
    }
}

/// Rappel de touches du panneau des variables secrètes.
fn secrets_hint(model: &Model) -> &'static str {
    let secrets = &model.secrets;
    if secrets.input.is_some() {
        "Entrée valider  Échap annuler"
    } else if secrets.pending_run.is_some() {
        "↑↓ naviguer  Entrée saisir  a ajouter  d oublier  r lancer  Échap abandonner"
    } else {
        "↑↓ naviguer  Entrée saisir  a ajouter  d oublier  Échap fermer"
    }
}

pub(crate) fn panel(title: &'static str, focused: bool) -> Block<'static> {
    titled_panel(title.to_owned(), focused)
}

/// Panneau bordé : bordure et titre en couleur de focus quand il a le
/// focus ; sinon bordure discrète et titre gris lisible.
fn titled_panel(title: String, focused: bool) -> Block<'static> {
    let (border, title_style) = if focused {
        (theme::FOCUS, theme::FOCUS)
    } else {
        (theme::BORDER, theme::PANEL_TITLE)
    };
    theme::bordered()
        .title(Span::styled(title, title_style))
        .border_style(border)
}

fn render_tree(model: &Model, frame: &mut Frame, area: Rect) {
    let base = if model.zoomed_panel() == Some(Focus::Tree) {
        TITLE_TREE_ZOOMED
    } else {
        TITLE_TREE
    };
    let title = match active_tree_filter_pattern(model) {
        Some(pattern) => format!("{} — filtre : {pattern} ", base.trim_end()),
        None => base.to_owned(),
    };
    let block = titled_panel(title, model.focus == Focus::Tree);
    let state = &model.tree;
    if state.rows.is_empty() {
        let msg = if is_tree_filtered(model) {
            "Aucune requête ne correspond"
        } else {
            "collection vide"
        };
        frame.render_widget(Paragraph::new(msg).block(block), area);
        return;
    }
    let height = usize::from(inner(area).height);
    let end = (state.offset + height).min(state.rows.len());
    let start = state.offset.min(end);
    let items: Vec<ListItem> = state.rows[start..end]
        .iter()
        .filter_map(|row| {
            let node = model.node_at(&row.address)?;
            Some(ListItem::new(tree::row_line(
                node,
                row.depth,
                model.is_expanded(node),
                run_status(model, node),
            )))
        })
        .collect();
    let selected = state
        .selected
        .checked_sub(start)
        .filter(|index| *index < end - start);
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::SELECTION),
        area,
        &mut list_state,
    );
}

/// Zone du texte de la réponse : l'intérieur du panneau sous la ligne
/// des onglets et du statut. Partagée par le rendu, le défilement et le
/// clic.
pub fn response_text_area(response: Rect) -> Rect {
    let inner_area = inner(response);
    Rect {
        y: inner_area.y + 1.min(inner_area.height),
        height: inner_area.height.saturating_sub(1),
        ..inner_area
    }
}

/// Dessine le panneau Réponse : le résultat de la dernière exécution de la
/// requête sélectionnée, ou un message unique délibérément centré quand
/// il n'y en a aucun (`visual-theme`, `split-request-response-panels`).
fn render_response(model: &Model, frame: &mut Frame, area: Rect) {
    let title = if model.zoomed_panel() == Some(Focus::Response) {
        TITLE_RESPONSE_ZOOMED
    } else {
        TITLE_RESPONSE
    };
    // Statut résumé sur la bordure haute, calé à droite (comme le statut
    // à droite des onglets dans Bruno bureau) : toujours visible, même
    // sans résultat pendant une première exécution.
    let mut block = panel(title, model.focus == Focus::Response);
    let summary = status::status_summary(model);
    if summary.width() > 0 {
        let mut summary = summary;
        summary.spans.insert(0, Span::raw(" "));
        summary.spans.push(Span::raw(" "));
        block = block.title(summary.right_aligned());
    }
    let inner_area = block.inner(area);
    if !detail::response_has_result(model) {
        frame.render_widget(
            panels::empty_state_message(area, "aucun résultat").block(block),
            area,
        );
        return;
    }
    frame.render_widget(block, area);
    // Ligne des onglets, fixe : elle ne défile pas avec le contenu.
    let header = Rect {
        height: 1.min(inner_area.height),
        ..inner_area
    };
    frame.render_widget(Paragraph::new(detail::tab_bar(model.response_tab)), header);

    let text_area = response_text_area(area);
    let text = detail::render_response_text(model);
    let gutter = detail::response_gutter_width(model, text_area.width);
    let content = Rect {
        x: text_area.x + gutter,
        width: text_area.width - gutter,
        ..text_area
    };
    if gutter > 0
        && let Some(start) = detail::response_body_start(model)
    {
        frame.render_widget(
            Block::default().style(theme::GUTTER),
            Rect {
                width: gutter,
                ..text_area
            },
        );
        render_line_numbers(
            &text.lines,
            start,
            model.response_scroll,
            text_area,
            content.width,
            gutter,
            frame,
        );
    }
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((model.response_scroll, 0)),
        content,
    );
}

/// Numéros des lignes du corps dans la gouttière de gauche, sur la
/// première ligne visuelle de chaque ligne logique : même découpage que
/// `Paragraph` à la largeur `width` du contenu, comme `hit::line_at_row`.
fn render_line_numbers(
    lines: &[Line<'static>],
    body_start: usize,
    scroll: u16,
    area: Rect,
    width: u16,
    gutter: u16,
    frame: &mut Frame,
) {
    let first = usize::from(scroll);
    let last = first + usize::from(area.height);
    let mut row = 0usize;
    for (index, line) in lines.iter().enumerate() {
        if row >= last {
            break;
        }
        let height = Paragraph::new(Text::from(line.clone()))
            .wrap(Wrap { trim: false })
            .line_count(width)
            .max(1);
        if row >= first && index >= body_start {
            let number = format!(
                "{:>w$} ",
                index - body_start + 1,
                w = usize::from(gutter.saturating_sub(1))
            );
            let y = area.y + u16::try_from(row - first).unwrap_or(0);
            frame.render_widget(
                Paragraph::new(Span::styled(number, theme::GUTTER)),
                Rect::new(area.x, y, gutter, 1),
            );
        }
        row += height;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::message::{InputKey, Message};
    use crate::app::model::{HistoryEntry, HistoryOutcome};
    use crate::app::test_support::{fixture, loaded_model, render, select};
    use crate::app::update::update;
    use crate::collection::{Collection, LoadError};
    use std::path::PathBuf;

    #[test]
    fn initial_tree_lists_first_level_in_order_with_error_marks() {
        let model = loaded_model((100, 30));
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].contains("bruno-tui · parser-cases"),
            "{}",
            lines[0]
        );
        // Colonnes de l'arbre : 30 % de 100, bordures exclues.
        let tree: Vec<String> = lines[2..13]
            .iter()
            .map(|l| {
                l.chars()
                    .skip(1)
                    .take(28)
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect();
        assert_eq!(
            tree,
            [
                "▸ Groupe",
                "GET    ping",
                "POST   post-json",
                "GET    scripted",
                "▸ badmeta ✗",
                "✗ broken.bru",
                "▸ misc",
                "GET    multiline",
                "✗ no-method.bru",
                "GET    no-seq",
                "GET    unknown-block",
            ]
        );
        assert!(lines[13].starts_with('│') && lines[13].chars().nth(1) == Some(' '));
        // Ligne 28 = barre d'état, ligne 29 = fil d'Ariane, sur hauteur 30.
        assert!(lines[28].contains("q quitter"), "{}", lines[28]);
    }

    #[test]
    fn loading_and_failed_states() {
        let mut model = Model::new("/somewhere/else".into(), (100, 30));
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].contains("Chargement de /somewhere/else"),
            "{}",
            lines[0]
        );

        update(
            &mut model,
            Message::CollectionLoaded(Err(LoadError::NotACollection {
                path: "/somewhere/else".into(),
            })),
        );
        let screen = render(&model, 100, 30).join("\n");
        assert!(
            screen.contains("Impossible de charger la collection"),
            "{screen}"
        );
        assert!(screen.contains("/somewhere/else"), "{screen}");
        assert!(screen.contains("aucun bruno.json"), "{screen}");
    }

    #[test]
    fn empty_collection() {
        let mut model = Model::new(fixture(), (100, 30));
        update(
            &mut model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/vide".into(),
                name: "vide".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("collection vide"), "{screen}");
    }

    #[test]
    fn too_small_and_absurd_sizes() {
        let model = loaded_model((30, 8));
        let screen = render(&model, 30, 8).join("\n");
        assert!(screen.contains("trop petit"), "{screen}");
        assert!(!screen.contains("Groupe"), "{screen}");

        let model = loaded_model((1, 1));
        render(&model, 1, 1);
    }

    #[test]
    fn layout_respects_minimums() {
        assert_eq!(layout_for((59, 30), None), None);
        assert_eq!(layout_for((100, 10), None), None);
        let areas = layout_for((60, 11), None).expect("taille minimale");
        assert_eq!(areas.tree.width, MIN_TREE_WIDTH);
        assert_eq!(areas.detail.width, MIN_DETAIL_WIDTH);
        assert_eq!(areas.response.width, MIN_RESPONSE_WIDTH);
        assert_eq!(
            areas.tree.width + areas.detail.width + areas.response.width,
            60
        );
        assert_eq!(areas.tree.height, 9);
        // La réponse occupe toute la hauteur ; l'Environnement compact
        // est en surimpression.
        assert_eq!(areas.response.height, 9);
        assert!(response_text_area(areas.response).height >= 1);
        assert_eq!(areas.environment.height, 3);
        assert!(areas.environment_panel_compact());
    }

    /// La réponse occupe toute la colonne de droite ; l'Environnement est
    /// une surimpression calée en haut à droite du corps.
    #[test]
    fn response_fills_the_column_and_environment_overlays_it() {
        for (size, env_height) in [
            ((60, 11), 3),
            ((100, 19), 3),
            ((100, 20), 6),
            ((100, 30), 6),
        ] {
            let areas = layout_for(size, None).expect("taille suffisante");
            let (env, response) = (areas.environment, areas.response);
            assert_eq!(response.y, areas.body.y, "{size:?}");
            assert_eq!(response.height, areas.body.height, "{size:?}");
            assert_eq!(response.right(), areas.body.right(), "{size:?}");
            assert_eq!(env.height, env_height, "{size:?}");
            assert_eq!(
                env.height == 3,
                areas.environment_panel_compact(),
                "{size:?}"
            );
            assert_eq!(
                (env.y, env.right()),
                (areas.body.y, areas.body.right()),
                "{size:?}"
            );
            assert!(env.width <= response.width, "{size:?}");
            assert_eq!(areas.tree.height, areas.body.height, "{size:?}");
            assert_eq!(areas.detail.height, areas.body.height, "{size:?}");
        }
    }

    /// À la taille minimale, les trois panneaux sont affichés ; juste
    /// en dessous, le message « trop petit » remplace tout le reste
    /// (`tui-shell`, exigence « Disposition et barre d'état »).
    #[test]
    fn too_small_just_below_the_new_minimum_width() {
        let model = loaded_model((59, 30));
        let screen = crate::app::test_support::render(&model, 59, 30).join("\n");
        assert!(screen.contains("trop petit"), "{screen}");

        let model = loaded_model((60, 30));
        let screen = crate::app::test_support::render(&model, 60, 30).join("\n");
        assert!(!screen.contains("trop petit"), "{screen}");
        assert!(screen.contains("Réponse"), "{screen}");
    }

    #[test]
    fn status_line_shows_the_active_target_and_cancel_hint() {
        let mut model = loaded_model((100, 30));
        model.run.active = Some(crate::app::model::ActiveRun {
            id: crate::runner::RunId(1),
            target: "simple-get.bru".into(),
            recursive: false,
            handle: None,
        });
        let line = status_line(&model);
        assert!(line.contains("simple-get.bru"), "{line}");
        assert!(line.contains("Ctrl+X"), "{line}");
    }

    #[test]
    fn status_line_shows_the_last_failure_message() {
        let mut model = loaded_model((100, 30));
        model.run.last_failure = Some(RunFailure::Error {
            target: "simple-get.bru".into(),
            error: crate::runner::RunError::BruNotFound,
        });
        let line = status_line(&model);
        assert!(line.contains("simple-get.bru"), "{line}");
        assert!(line.contains("introuvable"), "{line}");
    }

    #[test]
    fn status_line_shows_the_usual_hint_otherwise() {
        let model = loaded_model((100, 30));
        assert!(model.run.active.is_none());
        assert!(model.run.last_failure.is_none());
        let line = status_line(&model);
        assert!(line.contains("q quitter"), "{line}");
        assert!(line.contains("r lancer"), "{line}");
    }

    #[test]
    fn search_input_line_replaces_the_status_bar_while_editing() {
        let mut model = loaded_model((100, 30));
        update(&mut model, Message::StartSearch);
        update(&mut model, Message::SearchInput('p'));
        update(&mut model, Message::SearchInput('o'));
        let line = status_line(&model);
        assert_eq!(line, "/po");

        update(&mut model, Message::ConfirmSearch);
        let line = status_line(&model);
        assert!(!line.starts_with('/'), "{line}");
    }

    #[test]
    fn filter_input_line_shows_draft_in_status_bar_before_confirmation() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");

        // Avant ouverture : status_line ordinaire
        let line_before = status_line(&model);
        assert!(!line_before.starts_with('|'), "{line_before}");

        // Ouvre le filtre et tape `.a`
        update(&mut model, Message::OpenFilter);
        update(&mut model, Message::FilterInput('.'));
        update(&mut model, Message::FilterInput('a'));
        assert!(model.filter.as_ref().unwrap().editing);

        // La barre d'état logique affiche le draft préfixé par |
        let line = status_line(&model);
        assert_eq!(line, "|.a");

        // Rendu à l'écran via TestBackend
        let screen = render(&model, 100, 30);
        let status_row = &screen[28]; // ligne 28 = barre d'état sur hauteur 30
        assert!(
            status_row.contains("|.a"),
            "la barre d'état à l'écran doit contenir |.a mais vaut :\n{status_row}"
        );

        // Après validation : le texte tapé disparaît de la barre d'état
        update(&mut model, Message::ConfirmFilter);
        assert!(!model.filter.as_ref().unwrap().editing);
        let line_after = status_line(&model);
        assert!(!line_after.starts_with('|'), "{line_after}");
        let screen_after = render(&model, 100, 30);
        assert!(!screen_after[28].contains("|.a"), "{}", screen_after[28]);
    }

    #[test]
    fn last_status_is_shown_when_no_run_is_active() {
        let mut model = loaded_model((100, 30));
        model.last_status = Some(crate::app::model::StatusMessage::Copied);
        let line = status_line(&model);
        assert!(line.contains("Copié"), "{line}");
    }

    #[test]
    fn editor_status_messages_render_in_status_bar() {
        let mut model = loaded_model((100, 30));

        model.last_status = Some(StatusMessage::NoResponseBody);
        let line = status_line(&model);
        assert!(line.contains("Aucun corps de réponse à ouvrir"), "{line}");
        let screen = render(&model, 100, 30);
        assert!(
            screen[28].contains("Aucun corps de réponse à ouvrir"),
            "{}",
            screen[28]
        );

        model.last_status = Some(StatusMessage::EditorError("binaire introuvable".into()));
        let line = status_line(&model);
        assert!(
            line.contains("Échec de l'éditeur : binaire introuvable"),
            "{line}"
        );
        let screen = render(&model, 100, 30);
        assert!(
            screen[28].contains("Échec de l'éditeur : binaire introuvable"),
            "{}",
            screen[28]
        );
    }

    #[test]
    fn selection_and_match_are_highlighted_distinctly_on_screen() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartSearch);
        for c in "res.body.ok".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 12)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let (line, range) = model.detail_match.clone().expect("correspondance");
        let detail_area = layout_for((100, 12), None)
            .expect("taille suffisante")
            .detail;
        let inner_area = inner(detail_area);
        let row = inner_area.y + (line - model.detail_scroll);
        let col = inner_area.x + range.start as u16;
        let highlighted = buffer[(col, row)].bg;
        assert_ne!(
            highlighted,
            ratatui::style::Color::Reset,
            "la correspondance doit avoir un fond distinct"
        );

        // Sélection visuelle : fond distinct de la surbrillance de motif.
        update(&mut model, Message::ToggleVisual);
        update(&mut model, Message::Down);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 12)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let selection_row = inner_area.y;
        let selection_bg = buffer[(inner_area.x, selection_row)].bg;
        assert_ne!(
            selection_bg,
            ratatui::style::Color::Reset,
            "sélection visible"
        );
    }

    #[test]
    fn field_under_cursor_is_distinct_and_shows_pending_edit() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");

        let detail_area = layout_for((100, 30), None).expect("layout").detail;
        let inner_area = inner(detail_area);
        let buffer = terminal.backend().buffer();

        // Ligne 4 du détail = ligne URL, la méthode ayant sa propre ligne
        // avant elle (`add-method-editing`) ; la valeur suit le libellé
        // « URL : » (6 colonnes).
        let url_row = inner_area.y + 4;
        let url_cell = &buffer[(inner_area.x + 6, url_row)];
        assert!(
            url_cell.modifier.contains(Modifier::REVERSED),
            "le champ sous le curseur doit être visuellement distinct (REVERSED)"
        );

        // Édition de l'URL
        update(&mut model, Message::Enter);
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char('!')),
        );
        update(&mut model, Message::ValidateInput);

        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let row_text: String = (inner_area.x..inner_area.x + inner_area.width)
            .map(|x| buffer[(x, url_row)].symbol())
            .collect();
        assert!(
            row_text.contains("https://{{host}}/ping!"),
            "la valeur affichée reflète l'édition en attente : {row_text}"
        );
    }

    #[test]
    fn status_line_and_confirm_states() {
        let mut model = loaded_model((100, 30));

        // 1. Aucune session : contenu habituel
        let line = status_line(&model);
        assert!(line.contains("q quitter"));
        assert!(!line.contains("Sélection ·"));

        // 2. Session ouverte en sélection de champ
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        assert!(status_line(&model).contains("Entrée éditer"));
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL
        let line = status_line(&model);
        assert!(line.contains("Sélection · Url"), "{line}");
        assert!(line.contains("Entrée modifier"), "{line}");
        assert!(line.contains("Ctrl+S enregistrer"), "{line}");
        assert!(line.contains("Échap fermer"), "{line}");
        assert!(!line.contains("non enregistré"), "{line}");

        // 3. Barre d'aide en saisie de l'URL
        update(&mut model, Message::Enter);
        let line = status_line(&model);
        assert!(line.contains("Saisie · Url"), "{line}");
        assert!(line.contains("Entrée valider"), "{line}");
        assert!(line.contains("Échap annuler"), "{line}");
        assert!(line.contains("Ctrl+S enregistrer"), "{line}");

        // 4. Indicateur de modification : saisie modifiée, puis validée
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char('x')),
        );
        assert!(status_line(&model).contains("● non enregistré"));
        update(&mut model, Message::ValidateInput);
        let line = status_line(&model);
        assert!(line.contains("Sélection · Url ● non enregistré"), "{line}");

        // 5. Confirmation active (prioritaire)
        model.confirm = Some(PendingConfirm::DiscardEdit);
        let line = status_line(&model);
        assert!(line.contains("Abandonner les modifications ?"), "{line}");
        assert!(!line.contains("Sélection ·"));

        model.confirm = Some(PendingConfirm::QuitWithUnsavedEdit);
        let line = status_line(&model);
        assert!(line.contains("Quitter sans sauvegarder"), "{line}");
    }

    #[test]
    fn help_line_while_editing_the_body() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        let body = model.editing.as_ref().expect("session").fields.len() - 1;
        for _ in 0..body {
            update(&mut model, Message::Down);
        }
        update(&mut model, Message::Enter);
        let line = status_line(&model);
        assert!(line.contains("Saisie · Corps"), "{line}");
        assert!(line.contains("Entrée nouvelle ligne"), "{line}");
        assert!(line.contains("Tab valider"), "{line}");
        // Indicateur effacé après une sauvegarde réussie.
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char('x')),
        );
        update(&mut model, Message::ValidateInput);
        assert!(status_line(&model).contains("● non enregistré"));
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let saved = crate::app::model::SavedEdit {
            stamp: model.editing.as_ref().expect("session").stamp,
            ast: request.ast.clone().expect("ast"),
            view: request.view.clone(),
        };
        update(
            &mut model,
            Message::EditSaved {
                path: "post-json.bru".into(),
                result: Ok(saved),
            },
        );
        assert!(!status_line(&model).contains("non enregistré"));
    }

    #[test]
    fn wide_url_input_keeps_the_text_cursor_inside_the_panel() {
        let mut model = loaded_model((60, 20));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter); // ouvre la session
        update(&mut model, Message::Down); // Méthode -> URL
        update(&mut model, Message::Enter); // commence la saisie de l'URL
        for _ in 0..80 {
            update(
                &mut model,
                Message::InputKey(crate::app::message::InputKey::Char('a')),
            );
        }
        update(
            &mut model,
            Message::InputKey(crate::app::message::InputKey::Char('Z')),
        );

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 20)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let inner_area = inner(layout_for((60, 20), None).expect("layout").detail);
        let cursor = terminal.get_cursor_position().expect("curseur");
        assert!(cursor.x >= inner_area.x && cursor.x < inner_area.x + inner_area.width);

        // La fin de l'URL est visible juste avant le curseur, sur la ligne
        // de l'URL qui n'est pas repliée.
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(cursor.x - 1, cursor.y)].symbol(), "Z");
        let next: String = (inner_area.x..inner_area.x + inner_area.width)
            .map(|x| buffer[(x, cursor.y + 1)].symbol())
            .collect();
        assert!(next.contains("Auth"), "pas de retour à la ligne : {next}");
    }

    #[test]
    fn insert_cursor_positioning_and_bounds() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");

        // En sélection de champ : pas de curseur positionné
        terminal.draw(|frame| view(&model, frame)).expect("rendu");

        // En saisie : curseur positionné
        update(&mut model, Message::Enter);
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let cursor = terminal.get_cursor_position().expect("cursor position");

        let detail_area = layout_for((100, 30), None).expect("layout").detail;
        let inner_area = inner(detail_area);
        assert_eq!(cursor.y, inner_area.y + 4);
        assert!(cursor.x >= inner_area.x);
        assert!(cursor.x < inner_area.x + inner_area.width);

        // Défilement lointain : curseur hors vue, ne panique pas
        model.detail_scroll = 500;
        terminal
            .draw(|frame| view(&model, frame))
            .expect("rendu sans panique");
    }

    #[test]
    fn title_line_badge_rendered_on_errors_and_hidden_otherwise() {
        // parser-cases a 3 erreurs
        let model = loaded_model((100, 30));
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].contains("⚠ 3"),
            "Titre attendu avec ⚠ 3: {}",
            lines[0]
        );

        // Collection sans erreur
        let mut clean_model = Model::new(fixture(), (100, 30));
        update(
            &mut clean_model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/clean".into(),
                name: "clean".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        let clean_lines = render(&clean_model, 100, 30);
        assert!(
            !clean_lines[0].contains('⚠'),
            "Titre ne doit pas contenir ⚠: {}",
            clean_lines[0]
        );
    }

    #[test]
    fn response_panel_shows_result_separately_from_detail() {
        use crate::app::test_support::runner_probe_model;

        // 1. Requête exécutée : le résultat est dans la réponse, pas
        // dans le détail.
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        let lines = render(&model, 100, 30);
        let screen = lines.join("\n");
        assert!(
            screen.contains("Réponse ─ 200 OK  6 ms  21 o  ✓ 2/2"),
            "{screen}"
        );
        let detail_col_end = layout_for((100, 30), None).expect("layout").detail.right();
        let detail_only: String = lines
            .iter()
            .map(|line| {
                line.chars()
                    .take(detail_col_end as usize)
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !detail_only.contains("200 OK"),
            "le détail ne doit plus afficher le résultat :\n{detail_only}"
        );

        // 2. Requête sans exécution : message unique dans la réponse.
        select(&mut model, "skip.bru");
        model.run.outcomes.remove(std::path::Path::new("skip.bru"));
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("aucun résultat"), "{screen}");

        // 3. Dossier sélectionné : même message.
        let mut folder_model = loaded_model((100, 30));
        select(&mut folder_model, "grp");
        let screen = render(&folder_model, 100, 30).join("\n");
        assert!(screen.contains("aucun résultat"), "{screen}");

        // 4. Nœud en erreur sélectionné : même message.
        let mut error_model = loaded_model((100, 30));
        select(&mut error_model, "broken.bru");
        let screen = render(&error_model, 100, 30).join("\n");
        assert!(screen.contains("aucun résultat"), "{screen}");
    }

    #[test]
    fn response_status_band_stays_visible_across_tabs() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse

        for tab in [
            crate::app::model::ResponseTab::Body,
            crate::app::model::ResponseTab::Headers,
            crate::app::model::ResponseTab::Tests,
        ] {
            model.response_tab = tab;
            let screen = render(&model, 100, 30).join("\n");
            assert!(
                screen.contains("200 OK  6 ms  21 o  ✓ 2/2"),
                "{tab:?} :\n{screen}"
            );
            // Aucune répétition dans le panneau Réponse.
            let response = detail::response_plain_lines(&model).join("\n");
            assert!(!response.contains("Verdict"), "{tab:?} :\n{response}");
        }
    }

    #[test]
    fn response_tab_content_is_shown_only_when_active() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse

        model.response_tab = crate::app::model::ResponseTab::Body;
        let body_screen = render(&model, 100, 30).join("\n");
        assert!(body_screen.contains("\"a\": ["), "{body_screen}");
        assert!(!body_screen.contains("content-type"), "{body_screen}");
        assert!(!body_screen.contains("body a"), "{body_screen}");

        model.response_tab = crate::app::model::ResponseTab::Headers;
        let headers_screen = render(&model, 100, 30).join("\n");
        assert!(headers_screen.contains("content-type"), "{headers_screen}");
        assert!(!headers_screen.contains("\"a\": ["), "{headers_screen}");
        assert!(!headers_screen.contains("body a"), "{headers_screen}");

        model.response_tab = crate::app::model::ResponseTab::Tests;
        let tests_screen = render(&model, 100, 30).join("\n");
        assert!(tests_screen.contains("body a"), "{tests_screen}");
        assert!(tests_screen.contains("res.status"), "{tests_screen}");
        assert!(!tests_screen.contains("\"a\": ["), "{tests_screen}");
        assert!(!tests_screen.contains("content-type"), "{tests_screen}");
    }

    #[test]
    fn render_diagnostics_and_history_panels_with_and_without_entries() {
        // 1. Diagnostics avec entrées (sur parser-cases)
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Diagnostics;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Diagnostics"), "{screen}");
        assert!(screen.contains("badmeta"), "{screen}");
        assert!(screen.contains("broken.bru"), "{screen}");
        assert!(screen.contains("no-method.bru"), "{screen}");

        // 2. Diagnostics sans erreur
        let mut clean_model = Model::new(fixture(), (100, 30));
        update(
            &mut clean_model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/clean".into(),
                name: "clean".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        clean_model.focus = Focus::Diagnostics;
        let screen = render(&clean_model, 100, 30).join("\n");
        assert!(screen.contains("aucune erreur"), "{screen}");

        // 3. Historique sans entrée
        let mut hist_model = loaded_model((100, 30));
        hist_model.focus = Focus::History;
        let screen = render(&hist_model, 100, 30).join("\n");
        assert!(screen.contains("Historique"), "{screen}");
        assert!(screen.contains("aucune exécution"), "{screen}");

        // 4. Historique avec entrée
        hist_model.history.push_back(HistoryEntry {
            started_at: std::time::SystemTime::now(),
            target: PathBuf::from("ping.bru"),
            recursive: false,
            outcome: HistoryOutcome::Completed {
                total: 1,
                failed: 0,
                duration_secs: 0.25,
            },
        });
        let screen = render(&hist_model, 100, 30).join("\n");
        assert!(screen.contains("ping.bru"), "{screen}");
        assert!(screen.contains("succès"), "{screen}");
        assert!(screen.contains("0,25 s"), "{screen}");
    }

    /// Le message unique d'un panneau vide n'est plus collé à la première
    /// ligne intérieure : il est présenté de façon délibérée
    /// (`visual-theme`), sans changer son texte.
    #[test]
    fn empty_diagnostics_and_history_messages_are_not_on_the_first_inner_row() {
        let mut clean_model = Model::new(fixture(), (100, 30));
        update(
            &mut clean_model,
            Message::CollectionLoaded(Ok(Collection {
                root: "/clean".into(),
                name: "clean".into(),
                settings: None,
                tree: Vec::new(),
                environments: Vec::new(),
            })),
        );
        clean_model.focus = Focus::Diagnostics;
        let lines = render(&clean_model, 100, 30);
        let row = lines
            .iter()
            .position(|l| l.contains("aucune erreur"))
            .expect("message « aucune erreur » présent");
        assert_eq!(row, 14, "message pas centré :\n{}", lines.join("\n"));

        let mut hist_model = loaded_model((100, 30));
        hist_model.focus = Focus::History;
        let lines = render(&hist_model, 100, 30);
        let row = lines
            .iter()
            .position(|l| l.contains("aucune exécution"))
            .expect("message « aucune exécution » présent");
        assert_eq!(row, 14, "message pas centré :\n{}", lines.join("\n"));

        let mut campaign_model = loaded_model((100, 30));
        campaign_model.focus = Focus::Campaign;
        let lines = render(&campaign_model, 100, 30);
        let row = lines
            .iter()
            .position(|l| l.contains("Aucune campagne lancée"))
            .expect("message « Aucune campagne lancée » présent");
        assert_eq!(row, 14, "message pas centré :\n{}", lines.join("\n"));
    }

    #[test]
    fn campaign_panel_renders_header_and_failures() {
        use crate::app::model::{CampaignFailure, CampaignSummary};
        use std::path::PathBuf;

        let mut model = loaded_model((100, 30));
        model.campaign = Some(CampaignSummary {
            target: PathBuf::from("my-suite"),
            total: 15,
            passed: 12,
            failed: 3,
            skipped: 0,
            duration_secs: 1.42,
            failures: vec![
                CampaignFailure {
                    path: PathBuf::from("req1.bru"),
                    name: "Premier échec".into(),
                    http_code: Some(500),
                    reason: "Internal server error".into(),
                },
                CampaignFailure {
                    path: PathBuf::from("folder/req2.bru"),
                    name: "Deuxième échec".into(),
                    http_code: None,
                    reason: "connect ECONNREFUSED".into(),
                },
            ],
        });
        model.focus = Focus::Campaign;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Campagne"), "{screen}");
        assert!(screen.contains("my-suite"), "{screen}");
        assert!(screen.contains("12/15 réussis"), "{screen}");
        assert!(screen.contains("3 échecs"), "{screen}");
        assert!(screen.contains("1,42 s"), "{screen}");
        assert!(screen.contains("req1.bru"), "{screen}");
        assert!(screen.contains("Premier échec"), "{screen}");
        assert!(screen.contains("HTTP 500"), "{screen}");
        assert!(screen.contains("Internal server error"), "{screen}");
        assert!(screen.contains("folder/req2.bru"), "{screen}");
        assert!(screen.contains("connect ECONNREFUSED"), "{screen}");
    }

    #[test]
    fn status_line_shows_expected_keys_for_diagnostics_and_history() {
        let mut model = loaded_model((100, 30));

        model.focus = Focus::Diagnostics;
        let lines = render(&model, 100, 30);
        let status = &lines[28];
        assert!(
            status.contains("↑↓ naviguer  → aller au nœud  Échap arbre"),
            "{status}"
        );

        model.focus = Focus::History;
        let lines = render(&model, 100, 30);
        let status = &lines[28];
        assert!(
            status.contains("↑↓ naviguer  r rejouer  Échap arbre"),
            "{status}"
        );

        model.focus = Focus::Campaign;
        let lines = render(&model, 100, 30);
        let status = &lines[28];
        assert!(
            status.contains("↑↓ naviguer  Entrée/→ aller au détail  Échap arbre"),
            "{status}"
        );

        model.focus = Focus::EnvironmentPicker;
        let lines = render(&model, 100, 30);
        let status = &lines[28];
        assert!(
            status.contains("↑↓ naviguer  Entrée activer  e éditer  Échap arbre"),
            "{status}"
        );

        // Le panneau Réponse a longtemps manqué son propre rappel de
        // touches (bug contre `tui-shell` : « une barre d'état rappelant
        // les touches utiles au focus courant »). Ligne plus longue que
        // 100 colonnes : rendue à une largeur qui l'accueille sans
        // troncature.
        model.focus = Focus::Response;
        let lines = render(&model, 130, 30);
        let status = &lines[28];
        assert!(
            status.contains(
                "↑↓ défiler  Début/Fin  ←→ onglet  / chercher  n/N suivant  v sélection  y copier  ? aide  Échap arbre  q quitter"
            ),
            "{status}"
        );
    }

    /// La fixture `parser-cases/environments/` porte, dans l'ordre
    /// alphabétique : `local` (valide), `malformed` (en erreur),
    /// `staging` (valide).
    #[test]
    fn environment_panel_opens_on_demand_and_header_shows_the_active_one() {
        let mut model = loaded_model((100, 30));

        // Fermé par défaut : seul l'en-tête indique l'environnement actif.
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Aucun environnement ▾"), "{screen}");
        assert!(!screen.contains("staging"), "{screen}");

        // Ouvert avec `E` : surimpression listant tous les environnements.
        update(&mut model, Message::ToggleEnvironmentPicker);
        let lines = render(&model, 100, 30);
        let screen = lines.join("\n");
        for name in [
            "Environnement",
            "Aucun",
            "local",
            "staging",
            "malformed.bru",
        ] {
            assert!(screen.contains(name), "{name} :\n{screen}");
        }
        assert!(
            lines[1].contains("Collection") && lines[1].contains("Détail"),
            "{}",
            lines[1]
        );

        // Indicateur mis à jour après sélection.
        model.current_environment = Some("staging".into());
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("staging ▾"), "{screen}");
        assert!(!screen.contains("Aucun environnement"), "{screen}");
    }

    /// La bordure d'un panneau porte `theme::BORDER` sans focus,
    /// `theme::FOCUS` avec focus ; le titre du panneau, posé sur la même
    /// ligne de bordure, doit porter la même couleur (`visual-theme`).
    #[test]
    fn panel_border_and_title_use_border_or_focus_color() {
        let model = loaded_model((100, 30)); // focus = Tree
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        let areas = layout_for((100, 30), None).expect("layout");
        let focus_fg = theme::FOCUS.fg.expect("fg défini");
        let border_fg = theme::BORDER.fg.expect("fg défini");

        // Coin de bordure : arbre focalisé en FOCUS, détail non focalisé
        // en BORDER.
        assert_eq!(
            buffer[(areas.tree.x, areas.tree.y)].fg,
            focus_fg,
            "bordure du panneau focalisé"
        );
        assert_eq!(
            buffer[(areas.detail.x, areas.detail.y)].fg,
            border_fg,
            "bordure du panneau non focalisé"
        );

        // Titre : couleur de focus si focalisé, gris lisible sinon,
        // distinct de la bordure discrète.
        assert_eq!(
            buffer[(areas.tree.x + 1, areas.tree.y)].fg,
            focus_fg,
            "titre du panneau focalisé"
        );
        assert_eq!(
            Some(buffer[(areas.detail.x + 1, areas.detail.y)].fg),
            theme::PANEL_TITLE.fg,
            "titre du panneau non focalisé"
        );
    }

    /// Le fond d'application (`visual-theme`) est posé une seule fois au
    /// début de `view()`, avant toute autre chose : il doit donc être
    /// visible dans tous les états, pas seulement `Loaded`.
    #[test]
    fn background_is_applied_in_every_state() {
        let bg = theme::BACKGROUND.bg.expect("fond défini");

        // État chargé : titre, panneau, barre d'état.
        let model = loaded_model((100, 30));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer[(0, 0)].bg, bg, "ligne de titre");
        assert_eq!(buffer[(2, 5)].bg, bg, "panneau de l'arbre");
        assert_eq!(buffer[(0, 28)].bg, bg, "barre d'état");
        assert_eq!(
            buffer[(0, 29)].bg,
            theme::BREADCRUMB.bg.expect("fond"),
            "fil d'Ariane"
        );

        // État Loading.
        let loading = Model::new("/somewhere/else".into(), (100, 30));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&loading, frame)).expect("rendu");
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, bg, "état Loading");

        // État Failed.
        let mut failed = Model::new("/somewhere/else".into(), (100, 30));
        update(
            &mut failed,
            Message::CollectionLoaded(Err(LoadError::NotACollection {
                path: "/somewhere/else".into(),
            })),
        );
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&failed, frame)).expect("rendu");
        assert_eq!(terminal.backend().buffer()[(0, 0)].bg, bg, "état Failed");

        // Terminal trop petit.
        let small = loaded_model((30, 8));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(30, 8)).expect("terminal");
        terminal.draw(|frame| view(&small, frame)).expect("rendu");
        assert_eq!(
            terminal.backend().buffer()[(0, 0)].bg,
            bg,
            "terminal trop petit"
        );
    }

    #[test]
    fn secrets_panel_shows_sources_never_values() {
        use crate::app::message::MaskedChar;
        use crate::runner::SecretString;
        use crate::secrets::{Resolved, SecretMapping, SecretSource};

        let mut model = loaded_model((120, 30));
        model.current_environment = Some("local".into());
        model.secrets.mappings = vec![SecretMapping {
            name: "oktaClientSecret".into(),
            key: None,
        }];
        model.secrets.resolved = vec![Resolved {
            name: "oktaClientSecret".into(),
            source: SecretSource::DotEnv {
                key: "OKTA_CLIENT_SECRET".into(),
            },
            value: Some(SecretString::new("dotenv-s3cr3t")),
        }];
        update(&mut model, Message::ToggleSecrets);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("Variables secrètes"), "{screen}");
        assert!(
            screen.contains("oktaClientSecret  .env (OKTA_CLIENT_SECRET)"),
            "{screen}"
        );
        assert!(
            screen.contains("token  non fournie (cherchée : token, TOKEN)"),
            "{screen}"
        );
        assert!(!screen.contains("dotenv-s3cr3t"), "{screen}");
        assert!(!screen.contains('•'), "longueur révélée : {screen}");
        assert!(
            screen.contains("↑↓ naviguer  Entrée saisir  a ajouter  d oublier  Échap fermer"),
            "{screen}"
        );

        // Saisie : un `•` par caractère, jamais la valeur.
        update(&mut model, Message::Down);
        update(&mut model, Message::Right);
        for c in "typed-s3cr3t".chars() {
            update(&mut model, Message::SecretInput(MaskedChar(c)));
        }
        let screen = render(&model, 120, 30).join("\n");
        assert!(
            screen.contains("Valeur de token : ••••••••••••"),
            "{screen}"
        );
        assert!(!screen.contains("typed-s3cr3t"), "{screen}");
        assert!(screen.contains("Entrée valider  Échap annuler"), "{screen}");

        update(&mut model, Message::ConfirmSecretInput);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("token  saisie"), "{screen}");
        assert!(!screen.contains("typed-s3cr3t"), "{screen}");
        assert!(!screen.contains('•'), "longueur révélée : {screen}");
    }

    #[test]
    fn secrets_panel_empty_pending_and_errors() {
        use crate::secrets::{InvalidReason, Resolved, SecretSource};

        let mut model = loaded_model((120, 30));
        update(&mut model, Message::ToggleSecrets);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("aucune variable secrète"), "{screen}");

        // Proposition au lancement : exécution en attente signalée.
        let mut model = loaded_model((120, 30));
        model.current_environment = Some("local".into());
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::RunSelected);
        let screen = render(&model, 120, 30).join("\n");
        assert!(
            screen.contains("Exécution de simple-get.bru en attente"),
            "{screen}"
        );
        assert!(screen.contains("r lancer  Échap abandonner"), "{screen}");

        model.secrets.resolved = vec![Resolved {
            name: "token".into(),
            source: SecretSource::Invalid {
                key: "TOKEN".into(),
                reason: InvalidReason::Multiline,
            },
            value: None,
        }];
        let screen = render(&model, 120, 30).join("\n");
        assert!(
            screen.contains("token  erreur (TOKEN) : valeur multiligne"),
            "{screen}"
        );

        update(&mut model, Message::Add);
        update(
            &mut model,
            Message::SecretInput(crate::app::message::MaskedChar('=')),
        );
        update(&mut model, Message::ConfirmSecretInput);
        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("Nom : ="), "{screen}");
        assert!(screen.contains("nom invalide"), "{screen}");
    }

    /// L'en-tête porte l'environnement actif à droite ; le fil d'Ariane,
    /// sur la dernière ligne, mène de la collection au nœud sélectionné.
    /// Sous la hauteur de forme complète, pas de fil d'Ariane.
    #[test]
    fn header_and_breadcrumb() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "folder/down.bru");
        let lines = render(&model, 100, 30);
        assert!(
            lines[0].starts_with("bruno-tui · runner-probe"),
            "{}",
            lines[0]
        );
        assert!(
            lines[0].trim_end().ends_with("Aucun environnement ▾"),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("Rechercher : /"), "{}", lines[0]);
        assert!(
            lines[29].starts_with(" ⌂ › runner-probe › folder › GET "),
            "{}",
            lines[29]
        );

        let areas = layout_for((100, 19), None).expect("layout");
        assert_eq!(areas.breadcrumb.height, 0);
        assert_eq!(areas.status.y, 18);
    }

    #[test]
    fn tree_status_line_mentions_secrets_panel() {
        let model = loaded_model((100, 30));
        let status = &render(&model, 100, 30)[28];
        assert!(status.contains("S secrets"), "{status}");
        // `M` vient en dernier : visible dès que le terminal est assez large,
        // sans jamais masquer les touches existantes.
        let wide = &render(&model, 120, 30)[28];
        assert!(wide.contains("q quitter  M souris"), "{wide}");
    }

    /// Lignes de l'écran restreintes à un rectangle.
    fn region(lines: &[String], area: Rect) -> Vec<String> {
        lines[usize::from(area.y)..usize::from(area.bottom())]
            .iter()
            .map(|line| {
                line.chars()
                    .skip(usize::from(area.x))
                    .take(usize::from(area.width))
                    .collect()
            })
            .collect()
    }

    /// Onglets sur la première ligne de la réponse, statut résumé sur sa
    /// bordure haute à droite. Le statut n'est pas répété dans le texte.
    #[test]
    fn response_header_shows_tabs_and_status_summary() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        let lines = render(&model, 100, 30);
        let areas = layout_for((100, 30), None).expect("layout");
        let header = &region(&lines, inner(areas.response))[0];
        assert!(header.starts_with("Corps   En-têtes   Tests"), "{header}");
        let border = &region(&lines, areas.response)[0];
        assert!(border.ends_with(" 200 OK  6 ms  21 o  ✓ 2/2 ╮"), "{border}");
        let text = region(&lines, response_text_area(areas.response)).join("\n");
        assert!(!text.contains("200 OK"), "{text}");
        assert!(!text.contains("Corps"), "{text}");

        // Le panneau Statut n'existe plus, quel que soit l'état.
        for focus in [
            Focus::History,
            Focus::Diagnostics,
            Focus::Secrets,
            Focus::Campaign,
        ] {
            model.focus = focus;
            let screen = render(&model, 100, 30).join("\n");
            assert!(!screen.contains(" Statut "), "{focus:?} :\n{screen}");
        }
    }

    #[test]
    fn response_header_survives_minimal_and_absurd_sizes() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");
        model.size = (60, 11);
        let lines = render(&model, 60, 11);
        let areas = layout_for((60, 11), None).expect("layout");
        let response = region(&lines, inner(areas.response));
        assert!(response[0].starts_with("Corps"), "{response:?}");
        assert!(
            response[1..].iter().any(|row| !row.trim().is_empty()),
            "{response:?}"
        );
        render(&model, 1, 1);
    }

    #[test]
    fn response_end_shows_the_last_line() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        model.size = (100, 12);
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        update(&mut model, Message::End);
        let last = detail::response_plain_lines(&model)
            .last()
            .cloned()
            .expect("réponse non vide");
        let lines = render(&model, 100, 12);
        let areas = layout_for((100, 12), None).expect("layout");
        let mut text_area = response_text_area(areas.response);
        // La gouttière des numéros de ligne précède le texte.
        let gutter = detail::response_gutter_width(&model, text_area.width);
        text_area.x += gutter;
        text_area.width -= gutter;
        let response = region(&lines, text_area);
        assert_eq!(response.last().map(|l| l.trim_end()), Some(last.as_str()));
    }

    /// Le corps de la réponse est numéroté dans une gouttière sur fond
    /// distinct, à partir de 1 sur sa première ligne.
    #[test]
    fn response_body_lines_are_numbered_in_a_gutter() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        let start = detail::response_body_start(&model).expect("onglet Corps");
        let areas = layout_for((100, 30), None).expect("layout");
        let panel = response_text_area(areas.response);
        let gutter = detail::response_gutter_width(&model, panel.width);
        assert!(gutter >= 2);
        let lines = render(&model, 100, 30);
        let gutter_text = |row: usize| -> String {
            lines[usize::from(panel.y) + row]
                .chars()
                .skip(usize::from(panel.x))
                .take(usize::from(gutter))
                .collect()
        };
        assert_eq!(gutter_text(start).trim(), "1");
        assert_eq!(gutter_text(start + 1).trim(), "2");
    }

    #[test]
    fn response_header_follows_the_run_lifecycle() {
        use crate::app::model::ActiveRun;
        use crate::app::test_support::runner_probe_model;
        use crate::runner::{RunEvent, RunId, RunOutcome};

        let areas = layout_for((100, 30), None).expect("layout");
        let header_of = |model: &Model| region(&render(model, 100, 30), areas.response)[0].clone();
        let start = |model: &mut Model| {
            model.run.active = Some(ActiveRun {
                id: RunId(7),
                target: "folder".into(),
                recursive: true,
                handle: None,
            });
        };
        let cancel = |model: &mut Model| {
            update(
                model,
                Message::RunFinished(RunEvent {
                    id: RunId(7),
                    outcome: RunOutcome::Cancelled,
                }),
            );
        };

        let mut model = runner_probe_model();
        select(&mut model, "folder/down.bru");
        start(&mut model);
        assert!(header_of(&model).contains(" en cours "));
        cancel(&mut model);
        let after = header_of(&model);
        assert!(!after.contains("en cours"), "{after}");
        assert!(after.contains("aucune réponse"), "{after}");

        // Première exécution : l'indicateur s'affiche aussi sans résultat.
        model.run.outcomes.clear();
        start(&mut model);
        assert!(header_of(&model).contains(" en cours "));
        cancel(&mut model);
        assert!(!header_of(&model).contains("en cours"));
    }

    // --- Ajout, suppression, renommage (`add-entry-management`) ---------

    fn key(model: &mut crate::app::model::Model, c: char) {
        update(
            model,
            Message::InputKey(crate::app::message::InputKey::Char(c)),
        );
    }

    fn go_to_field(model: &mut crate::app::model::Model, field: crate::app::model::EditableField) {
        let index = model
            .editing
            .as_ref()
            .and_then(|s| s.fields.iter().position(|f| *f == field))
            .expect("champ présent");
        let cursor =
            |model: &crate::app::model::Model| model.editing.as_ref().map_or(0, |s| s.cursor);
        while cursor(model) > index {
            update(model, Message::Up);
        }
        while cursor(model) < index {
            update(model, Message::Down);
        }
    }

    #[test]
    fn add_rows_are_shown_only_during_a_session() {
        let mut model = loaded_model((240, 40));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let screen = render(&model, 240, 40).join("\n");
        assert!(!screen.contains("+ Ajouter"), "{screen}");

        update(&mut model, Message::Enter);
        let screen = render(&model, 240, 40).join("\n");
        assert!(screen.contains("+ Ajouter un en-tête"), "{screen}");
        assert!(
            screen.contains("+ Ajouter un paramètre de requête"),
            "{screen}"
        );
        assert!(
            screen.contains("+ Ajouter un paramètre de chemin"),
            "{screen}"
        );
    }

    #[test]
    fn synchronised_url_is_shown_before_saving() {
        use crate::app::model::EditableField;
        use crate::writer::EntrySection;

        let mut model = loaded_model((240, 40));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        go_to_field(&mut model, EditableField::AddRow(EntrySection::QueryParams));
        update(&mut model, Message::Enter);
        for c in "page".chars() {
            key(&mut model, c);
        }
        update(&mut model, Message::ValidateInput);
        key(&mut model, '2');
        update(&mut model, Message::ValidateInput);

        let screen = render(&model, 240, 40).join("\n");
        assert!(screen.contains("GET"), "{screen}");
        assert!(screen.contains("https://{{host}}/ping?page=2"), "{screen}");
        assert!(screen.contains("page: 2"), "{screen}");
        assert!(model.editing.as_ref().is_some_and(|s| s.dirty));
    }

    #[test]
    fn text_cursor_follows_the_key_of_a_new_entry() {
        use crate::app::model::EditableField;
        use crate::writer::EntrySection;

        let mut model = loaded_model((100, 40));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        go_to_field(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Add);
        for c in "X-T".chars() {
            key(&mut model, c);
        }

        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 40)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let cursor = terminal.get_cursor_position().expect("curseur");
        let inner_area = inner(layout_for((100, 40), None).expect("layout").detail);
        // +1 pour la bordure gauche de la boîte de section « En-têtes »
        // (`add-boxed-detail-sections`), en plus de l'indentation « 2 » et
        // des 3 caractères tapés.
        assert_eq!(cursor.x, inner_area.x + 1 + 2 + 3);
        let buffer = terminal.backend().buffer();
        let row: String = (inner_area.x..inner_area.x + inner_area.width)
            .map(|x| buffer[(x, cursor.y)].symbol())
            .collect();
        let content: String = row.chars().skip(1).collect();
        assert!(content.starts_with("  X-T"), "{row}");

        // Étape de la valeur : le curseur suit la valeur après `clé: `.
        update(&mut model, Message::ValidateInput);
        key(&mut model, 'v');
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let cursor = terminal.get_cursor_position().expect("curseur");
        assert_eq!(cursor.x, inner_area.x + 1 + "  X-T: v".len() as u16);
    }

    #[test]
    fn help_line_on_an_entry_and_during_an_add() {
        use crate::app::model::EditableField;
        use crate::writer::EntrySection;

        let mut model = loaded_model((100, 40));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        go_to_field(&mut model, EditableField::HeaderValue(0));
        let line = status_line(&model);
        assert!(line.contains("a ajouter"), "{line}");
        assert!(line.contains("d supprimer"), "{line}");
        assert!(line.contains("c renommer"), "{line}");

        go_to_field(&mut model, EditableField::AddRow(EntrySection::Headers));
        let line = status_line(&model);
        assert!(line.contains("Entrée ajouter"), "{line}");
        assert!(!line.contains("d supprimer"), "{line}");

        update(&mut model, Message::Enter);
        let line = status_line(&model);
        assert!(line.contains("Saisie · Ajout · En-têtes · clé"), "{line}");
        assert!(line.contains("Échap annuler"), "{line}");

        for c in "X Y".chars() {
            key(&mut model, c);
        }
        update(&mut model, Message::ValidateInput);
        let line = status_line(&model);
        assert!(line.starts_with("Modification refusée"), "{line}");
        assert!(!line.contains("X Y"), "{line}");

        update(&mut model, Message::CancelInput);
        go_to_field(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Rename);
        let line = status_line(&model);
        assert!(
            line.contains("Saisie · Renommage · En-tête Accept"),
            "{line}"
        );
    }

    #[test]
    fn environment_edit_popup_rendering_and_cursor() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1;
        // `e` (StartEdit) ouvre désormais le popup — `Entrée` active
        // l'environnement (`add-environment-panel-and-edit-popup`).
        update(&mut model, Message::StartEdit);
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        assert_eq!(session.name, "local");

        // Rendu en tableau à deux colonnes : en-tête, puis une ligne par
        // variable, désactivée marquée.
        let lines = render(&model, 100, 30);
        let content = lines.join("\n");
        assert!(content.contains("Clé"), "{content}");
        assert!(content.contains("Valeur"), "{content}");
        assert!(
            content.contains("host") && content.contains("localhost:3000"),
            "{content}"
        );
        assert!(
            content.contains("debug") && content.contains("true (désactivé)"),
            "{content}"
        );

        // Passer en mode saisie (Entrée, sur la variable sous le curseur).
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('!')));

        let lines = render(&model, 100, 30);
        let content = lines.join("\n");
        assert!(content.contains("localhost:3000!"), "{content}");

        // Vérification de la position du curseur, relative au popup
        // centré (`add-environment-panel-and-edit-popup`, design D3).
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(100, 30)).expect("terminal");
        terminal.draw(|frame| view(&model, frame)).expect("rendu");
        let cursor = terminal.get_cursor_position().expect("curseur");
        let session = model.environment_editing.as_ref().expect("popup ouvert");
        let popup = environment_edit_popup_area(Rect::new(0, 0, 100, 30), session);
        // Bordure (+1) puis l'en-tête (+1) puis `host`, premier de la liste.
        assert_eq!(cursor.y, popup.y + 2);
        // Clé la plus longue « debug » (5) détermine la largeur de colonne,
        // suivie de « │ » (3) puis « localhost:3000! » (15).
        assert_eq!(cursor.x, popup.x + 1 + 5 + 3 + 15);
    }

    #[test]
    fn environment_variables_help_line() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 1;
        update(&mut model, Message::StartEdit);

        // En mode sélection
        let line = status_line(&model);
        assert!(line.contains("Sélection · host"), "{line}");
        assert!(line.contains("Entrée éditer"), "{line}");
        assert!(line.contains("Ctrl+S sauvegarder"), "{line}");
        assert!(line.contains("Échap retour"), "{line}");
        assert!(!line.contains("● non enregistré"), "{line}");

        // En mode saisie
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        let line = status_line(&model);
        assert!(line.contains("Saisie · host"), "{line}");
        assert!(line.contains("Entrée valider"), "{line}");
        assert!(line.contains("Échap annuler"), "{line}");
        assert!(line.contains("Ctrl+S sauvegarder"), "{line}");
        assert!(line.contains("● non enregistré"), "{line}");
    }

    #[test]
    fn help_popup_rendering_and_status_line() {
        use crate::app::model::HelpPopup;

        let mut model = loaded_model((120, 30));

        // Vérification des rappels de touches sur les focus principaux
        model.focus = Focus::Tree;
        assert!(status_line(&model).contains("? aide"));

        model.focus = Focus::Detail;
        assert!(status_line(&model).contains("? aide"));

        model.focus = Focus::Response;
        assert!(status_line(&model).contains("? aide"));

        // Popup ouvert
        model.help = Some(HelpPopup::default());
        let help_status = status_line(&model);
        assert!(
            help_status.contains("? / Échap / q fermer"),
            "{help_status}"
        );

        let screen = render(&model, 120, 30).join("\n");
        assert!(screen.contains("Aide"), "{screen}");
        assert!(screen.contains("Touche"), "{screen}");
        assert!(screen.contains("Effet"), "{screen}");
        assert!(screen.contains("Global"), "{screen}");
    }

    #[test]
    fn tree_filter_status_line_and_panel_rendering() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Tree;

        // Rappel "f filtrer" présent en focus Tree
        let normal_status = status_line(&model);
        assert!(normal_status.contains("f filtrer"), "{normal_status}");

        // Saisie en cours : rendu "f{draft}" dans la barre d'état (sur le modèle de search_input_line)
        update(&mut model, Message::OpenTreeFilter);
        for c in "ping".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        let input_status = status_line(&model);
        assert_eq!(input_status, "Filtre arbre : ping");

        // Validation du filtre : rappel dans le titre du panneau
        update(&mut model, Message::ConfirmTreeFilter);
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Collection — filtre : ping"), "{screen}");

        // Sans correspondance : message explicatif
        update(&mut model, Message::OpenTreeFilter);
        for c in "xyz-introuvable".chars() {
            update(&mut model, Message::TreeFilterInput(c));
        }
        let empty_screen = render(&model, 100, 30).join("\n");
        assert!(
            empty_screen.contains("Aucune requête ne correspond"),
            "{empty_screen}"
        );
    }

    #[test]
    fn zoomed_layout_allocates_full_body_and_masks_others() {
        let size = (100, 30);
        let normal = layout_for(size, None).expect("normal");

        // 1. Zoom Arbre
        let tree_zoom = layout_for(size, Some(Focus::Tree)).expect("tree zoom");
        assert_eq!(tree_zoom.title, normal.title);
        assert_eq!(tree_zoom.status, normal.status);
        assert_eq!(tree_zoom.body, normal.body);
        assert_eq!(tree_zoom.tree, normal.body);
        assert_eq!(tree_zoom.detail, Rect::default());
        assert_eq!(tree_zoom.environment, Rect::default());
        assert_eq!(tree_zoom.response, Rect::default());

        // 2. Zoom Détail
        let detail_zoom = layout_for(size, Some(Focus::Detail)).expect("detail zoom");
        assert_eq!(detail_zoom.body, normal.body);
        assert_eq!(detail_zoom.tree, Rect::default());
        assert_eq!(detail_zoom.detail, normal.body);
        assert_eq!(detail_zoom.environment, Rect::default());
        assert_eq!(detail_zoom.response, Rect::default());

        // 3. Zoom Réponse : la réponse occupe tout le corps.
        let resp_zoom = layout_for(size, Some(Focus::Response)).expect("resp zoom");
        assert_eq!(resp_zoom.tree, Rect::default());
        assert_eq!(resp_zoom.detail, Rect::default());
        assert_eq!(resp_zoom.environment, Rect::default());
        assert_eq!(resp_zoom.response, normal.body);
    }

    #[test]
    fn zoomed_panel_displays_fullscreen_indicator_in_title() {
        use crate::app::test_support::runner_probe_model;

        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        model.size = (100, 30);

        // Zoom Arbre
        model.focus = Focus::Tree;
        model.zoom = true;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Collection [plein écran — z]"), "{screen}");
        assert!(!screen.contains("Détail"), "{screen}");
        assert!(!screen.contains("Réponse"), "{screen}");

        // Zoom Détail
        model.focus = Focus::Detail;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Détail [plein écran — z]"), "{screen}");
        assert!(!screen.contains("Collection"), "{screen}");
        assert!(!screen.contains("Réponse"), "{screen}");

        // Zoom Réponse
        model.focus = Focus::Response;
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Réponse [plein écran — z]"), "{screen}");
        assert!(screen.contains("200 OK"), "{screen}");
        assert!(!screen.contains("Collection"), "{screen}");
        assert!(!screen.contains("Détail"), "{screen}");
    }

    #[test]
    fn status_line_mentions_zoom_key() {
        let mut model = loaded_model((100, 30));

        model.focus = Focus::Tree;
        assert!(status_line(&model).contains("z plein écran"));

        model.focus = Focus::Detail;
        assert!(status_line(&model).contains("z plein écran"));

        model.focus = Focus::Response;
        assert!(status_line(&model).contains("z plein écran"));
    }
}
