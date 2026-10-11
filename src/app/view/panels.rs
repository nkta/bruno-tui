//! Rendu des panneaux Diagnostics, Historique, sélection d'environnement
//! et variables secrètes.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::{inner, panel, theme};
use crate::app::diagnostics::diagnostics;
use crate::app::model::{
    EnvironmentEditSession, EnvironmentEditState, Focus, HistoryEntry, HistoryOutcome, Model,
    RunnerOptionsPopup, SecretError, SecretInput, secret_rows,
};
use crate::secrets::SecretSource;

/// Message unique d'un panneau plein corps sans entrée à lister, centré
/// verticalement dans la zone intérieure plutôt que collé en haut d'une
/// zone par ailleurs vide (`visual-theme`). Ne convient qu'à un message
/// tenant sur une seule ligne (voir design.md, Risks/Trade-offs).
pub(crate) fn empty_state_message(area: Rect, message: &'static str) -> Paragraph<'static> {
    let padding = inner(area).height.saturating_sub(1) / 2;
    let mut lines: Vec<Line<'static>> = (0..padding).map(|_| Line::default()).collect();
    lines.push(Line::styled(message, theme::EMPTY_MESSAGE));
    Paragraph::new(lines)
}

/// Dessine le panneau plein corps des diagnostics.
pub fn render_diagnostics(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Diagnostics ", model.focus == Focus::Diagnostics);
    let Some(collection) = model.loaded() else {
        frame.render_widget(
            empty_state_message(area, "aucune erreur").block(block),
            area,
        );
        return;
    };
    let entries = diagnostics(collection);
    if entries.is_empty() {
        frame.render_widget(
            empty_state_message(area, "aucune erreur").block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let line = Line::from(vec![
                Span::styled(
                    entry.path.display().to_string(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(entry.reason.clone(), theme::LOAD_ERROR),
            ]);
            ListItem::new(line)
        })
        .collect();
    let selected = if entries.is_empty() {
        None
    } else {
        Some(model.diagnostics_selected.min(entries.len() - 1))
    };
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::SELECTION),
        area,
        &mut list_state,
    );
}

/// Dessine le panneau plein corps de l'historique des exécutions.
pub fn render_history(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Historique ", model.focus == Focus::History);
    if model.history.is_empty() {
        frame.render_widget(
            empty_state_message(area, "aucune exécution").block(block),
            area,
        );
        return;
    }
    let items: Vec<ListItem> = model
        .history
        .iter()
        .map(|entry| ListItem::new(history_row_line(entry)))
        .collect();
    let selected = Some(model.history_selected.min(model.history.len() - 1));
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::SELECTION),
        area,
        &mut list_state,
    );
}

/// Dessine le panneau plein corps de la campagne de TNR.
pub fn render_campaign(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Campagne ", model.focus == Focus::Campaign);
    let Some(campaign) = &model.campaign else {
        frame.render_widget(
            empty_state_message(
                area,
                "Aucune campagne lancée — r sur un dossier pour lancer, O pour les options",
            )
            .block(block),
            area,
        );
        return;
    };

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let failures_str = if campaign.failed <= 1 {
        format!("{} échec", campaign.failed)
    } else {
        format!("{} échecs", campaign.failed)
    };
    let failures_style = if campaign.failed == 0 {
        theme::SUCCESS
    } else {
        theme::FAILURE
    };
    let header_line = Line::from(vec![
        Span::styled("Cible : ", theme::LABEL),
        Span::styled(
            campaign.target.display().to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  ·  "),
        Span::styled(
            format!("{}/{} réussis", campaign.passed, campaign.total),
            theme::SUCCESS,
        ),
        Span::raw("  ·  "),
        Span::styled(failures_str, failures_style),
        Span::raw("  ·  "),
        Span::styled(
            format_duration(campaign.duration_secs),
            Style::new().add_modifier(Modifier::DIM),
        ),
    ]);

    let options_str = model.runner_options.summary();
    let options_line = Line::from(vec![
        Span::styled("Options : ", theme::LABEL),
        Span::styled(
            options_str,
            if model.runner_options.has_active_options() {
                theme::SELECTION
            } else {
                Style::new().add_modifier(Modifier::DIM)
            },
        ),
        Span::raw("  "),
        Span::styled("(O pour modifier)", theme::LABEL),
    ]);

    let mut header = vec![header_line, options_line];
    if !model.last_reports.is_empty() {
        let reports_str = model
            .last_reports
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        header.push(Line::from(vec![
            Span::styled("Rapports : ", theme::LABEL),
            Span::styled(reports_str, theme::SUCCESS),
        ]));
    }
    header.push(Line::default());

    let header_height = u16::try_from(header.len()).unwrap_or(3);
    let [header_area, list_area] =
        Layout::vertical([Constraint::Length(header_height), Constraint::Min(0)]).areas(inner_area);

    frame.render_widget(Paragraph::new(header), header_area);

    if campaign.failures.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled("aucun échec", theme::EMPTY_MESSAGE)),
            list_area,
        );
        return;
    }

    let items: Vec<ListItem> = campaign
        .failures
        .iter()
        .map(|failure| {
            let mut spans = vec![
                Span::styled(
                    failure.path.display().to_string(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(format!("({})", failure.name), theme::LABEL),
                Span::raw("  ·  "),
            ];
            // Sans réponse HTTP (hôte injoignable…), seule la raison s'affiche.
            if let Some(code) = failure.http_code {
                spans.push(Span::styled(format!("HTTP {code}"), theme::LABEL));
                spans.push(Span::raw("  ·  "));
            }
            spans.push(Span::styled(failure.reason.clone(), theme::FAILURE));
            let line = Line::from(spans);
            ListItem::new(line)
        })
        .collect();

    let selected = Some(model.campaign_selected.min(campaign.failures.len() - 1));
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme::SELECTION),
        list_area,
        &mut list_state,
    );
}

const DISABLED_ENTRY_VALUE: Style =
    Style::new().add_modifier(Modifier::DIM.union(Modifier::ITALIC));

/// Dessine le panneau permanent Environnement : « Aucun » puis les
/// entrées de `Collection.environments`, une en erreur marquée et non
/// mise en valeur comme sélectionnable. Toujours la liste — l'édition
/// d'un environnement se fait dans un popup séparé
/// (`render_environment_edit_popup`,
/// `add-environment-panel-and-edit-popup`).
pub fn render_environment_picker(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Environnement ", model.focus == Focus::EnvironmentPicker);
    let Some(collection) = model.loaded() else {
        frame.render_widget(Paragraph::new("aucune collection").block(block), area);
        return;
    };

    let mut items = vec![environment_item_line(
        None,
        model.current_environment.is_none(),
    )];
    items.extend(collection.environments.iter().map(|entry| {
        let name = entry.as_ref().ok().map(|env| env.name.as_str());
        let is_current = match (name, model.current_environment.as_deref()) {
            (Some(n), Some(current)) => n == current,
            _ => false,
        };
        environment_item_line(Some(entry), is_current)
    }));
    let items: Vec<ListItem> = items.into_iter().map(ListItem::new).collect();
    let count = 1 + collection.environments.len();
    let selected = Some(model.environment_selected.min(count - 1));
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::SELECTION),
        area,
        &mut list_state,
    );
}

/// Largeur de la séparation entre les deux colonnes du tableau
/// (« │ », un espace de chaque côté).
const ENV_TABLE_SEPARATOR: &str = " │ ";

/// Dessine le popup d'édition des variables d'un environnement : tableau
/// à deux colonnes (Clé, Valeur), une ligne d'en-tête non sélectionnable
/// puis une ligne par variable (`add-environment-panel-and-edit-popup`).
pub fn render_environment_edit_popup(
    session: &EnvironmentEditSession,
    has_focus: bool,
    frame: &mut Frame,
    area: Rect,
) {
    let block = panel(" Environnement ", has_focus);
    let is_adding = matches!(
        session.state,
        EnvironmentEditState::AddingKey(_) | EnvironmentEditState::AddingValue { .. }
    );
    if session.variables.is_empty() && !is_adding {
        frame.render_widget(
            empty_state_message(area, "aucune variable").block(block),
            area,
        );
        return;
    }

    // La ligne provisoire (clé et/ou valeur en cours de saisie) compte
    // dans la largeur de la colonne Clé, pour ne pas désaligner le
    // tableau pendant qu'on tape une clé plus longue que les existantes.
    let provisional_key_width = match &session.state {
        EnvironmentEditState::AddingKey(input) => Some(Line::raw(input.text()).width()),
        EnvironmentEditState::AddingValue { key, .. } => Some(Line::raw(key.as_str()).width()),
        _ => None,
    };
    let key_width = session
        .variables
        .iter()
        .map(|v| Line::raw(v.key.as_str()).width())
        .chain(provisional_key_width)
        .max()
        .unwrap_or(0)
        .clamp(3, 20);

    let is_editing = matches!(session.state, EnvironmentEditState::Input(_));
    let mut items = vec![ListItem::new(Line::styled(
        format!("{:<key_width$}{ENV_TABLE_SEPARATOR}Valeur", "Clé"),
        theme::LABEL,
    ))];
    items.extend(session.variables.iter().enumerate().map(|(index, entry)| {
        let key_span = Span::styled(
            format!("{:<key_width$}{ENV_TABLE_SEPARATOR}", entry.key),
            theme::LABEL,
        );
        let val_span = if index == session.cursor && is_editing {
            if let EnvironmentEditState::Input(input) = &session.state {
                Span::raw(input.text().to_owned())
            } else {
                Span::raw(entry.value.clone())
            }
        } else if entry.enabled {
            Span::raw(entry.value.clone())
        } else {
            Span::styled(format!("{} (désactivé)", entry.value), DISABLED_ENTRY_VALUE)
        };
        ListItem::new(Line::from(vec![key_span, val_span]))
    }));

    // Ligne provisoire de l'ajout en cours, en fin de tableau
    // (`add-environment-entry-management`).
    let provisional_row = match &session.state {
        EnvironmentEditState::AddingKey(input) => Some(Line::from(vec![
            Span::styled(
                format!("{:<key_width$}{ENV_TABLE_SEPARATOR}", input.text()),
                theme::LABEL,
            ),
            Span::raw(""),
        ])),
        EnvironmentEditState::AddingValue { key, input } => Some(Line::from(vec![
            Span::styled(
                format!("{key:<key_width$}{ENV_TABLE_SEPARATOR}"),
                theme::LABEL,
            ),
            Span::raw(input.text().to_owned()),
        ])),
        _ => None,
    };
    if let Some(row) = provisional_row {
        items.push(ListItem::new(row));
    }

    // +1 : la ligne d'en-tête occupe l'indice 0, jamais sélectionnable.
    let selected = if is_adding {
        Some(items.len() - 1)
    } else {
        Some(
            session
                .cursor
                .min(session.variables.len().saturating_sub(1))
                + 1,
        )
    };
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme::SELECTION),
        area,
        &mut list_state,
    );

    let inner_area = inner(area);
    if inner_area.width == 0 || inner_area.height == 0 {
        return;
    }
    let (rel_y, prefix_width, text) = match &session.state {
        EnvironmentEditState::Input(input) => (
            session.cursor as u16 + 1,
            key_width,
            input.text_before_cursor_on_line(),
        ),
        EnvironmentEditState::AddingKey(input) => (
            selected.map_or(0, |i| i as u16),
            0,
            input.text_before_cursor_on_line(),
        ),
        EnvironmentEditState::AddingValue { key, input } => (
            selected.map_or(0, |i| i as u16),
            key_width.max(Line::raw(key.as_str()).width()),
            input.text_before_cursor_on_line(),
        ),
        EnvironmentEditState::Select => return,
    };
    if rel_y >= inner_area.height {
        return;
    }
    let cursor_y = inner_area.y + rel_y;
    let prefix_width =
        u16::try_from(prefix_width + ENV_TABLE_SEPARATOR.chars().count()).unwrap_or(u16::MAX);
    let text_width = u16::try_from(Line::raw(text.as_str()).width()).unwrap_or(u16::MAX);
    let max_col = inner_area.width.saturating_sub(1);
    let cursor_x = inner_area.x + (prefix_width + text_width).min(max_col);
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Dessine le panneau plein corps des variables secrètes : bandeau
/// (exécution en attente, saisie en cours, refus) puis une ligne par
/// variable avec sa source. Aucune valeur n'est jamais rendue ; seule la
/// saisie en cours affiche un `•` par caractère tapé.
pub fn render_secrets(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Variables secrètes ", model.focus == Focus::Secrets);
    let rows = secret_rows(model);
    let header = secrets_header(model);
    if rows.is_empty() && header.is_empty() {
        frame.render_widget(
            empty_state_message(
                area,
                "aucune variable secrète — a pour en ajouter, ou --secret NOM[=CLÉ]",
            )
            .block(block),
            area,
        );
        return;
    }
    let inner_area = block.inner(area);
    frame.render_widget(block, area);
    let [header_area, list_area] = Layout::vertical([
        Constraint::Length(u16::try_from(header.len()).unwrap_or(u16::MAX)),
        Constraint::Min(0),
    ])
    .areas(inner_area);
    frame.render_widget(Paragraph::new(header), header_area);
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                "aucune variable secrète",
                theme::EMPTY_MESSAGE,
            )),
            list_area,
        );
        return;
    }
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let (label, style) = secret_source_label(&row.source);
            ListItem::new(Line::from(vec![
                Span::styled(row.name.clone(), Style::new().add_modifier(Modifier::BOLD)),
                Span::raw("  "),
                Span::styled(label, style),
            ]))
        })
        .collect();
    let selected = Some(model.secrets.selected.min(rows.len() - 1));
    let mut list_state = ListState::default().with_selected(selected);
    frame.render_stateful_widget(
        List::new(items).highlight_style(theme::SELECTION),
        list_area,
        &mut list_state,
    );
}

fn secrets_header(model: &Model) -> Vec<Line<'static>> {
    let state = &model.secrets;
    let mut lines = Vec::new();
    if let Some((target, _)) = &state.pending_run {
        lines.push(Line::styled(
            format!(
                "Exécution de {} en attente : des variables secrètes ne sont pas fournies (r lancer quand même, Échap abandonner)",
                target.display()
            ),
            Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }
    match &state.input {
        Some(SecretInput::Name(name)) => lines.push(Line::from(vec![
            Span::styled("Nom : ", theme::LABEL),
            Span::raw(name.clone()),
        ])),
        Some(SecretInput::Value { name, buffer, .. }) => lines.push(Line::from(vec![
            Span::styled(format!("Valeur de {name} : "), theme::LABEL),
            Span::raw("•".repeat(buffer.char_count())),
        ])),
        None => {}
    }
    if let Some(error) = state.error {
        let message = match error {
            SecretError::InvalidName => "nom invalide : non vide, sans `=` ni espace",
            SecretError::DuplicateName => "ce nom est déjà dans la liste",
        };
        lines.push(Line::styled(message, theme::FAILURE));
    }
    lines
}

/// État affiché d'une variable : sa source et la clé utilisée, jamais sa
/// valeur ni sa longueur.
pub(crate) fn secret_source_label(source: &SecretSource) -> (String, Style) {
    match source {
        SecretSource::Typed => ("saisie".to_owned(), theme::SUCCESS),
        SecretSource::DotEnv { key } => (format!(".env ({key})"), theme::SUCCESS),
        SecretSource::Shell { key } => (format!("shell ({key})"), theme::SUCCESS),
        SecretSource::Missing { keys } if keys.is_empty() => {
            ("non fournie".to_owned(), theme::LABEL)
        }
        SecretSource::Missing { keys } => (
            format!("non fournie (cherchée : {})", keys.join(", ")),
            theme::LABEL,
        ),
        SecretSource::Invalid { key, reason } if key.is_empty() => {
            (format!("erreur : {reason}"), theme::LOAD_ERROR)
        }
        SecretSource::Invalid { key, reason } => {
            (format!("erreur ({key}) : {reason}"), theme::LOAD_ERROR)
        }
    }
}

/// Parse une couleur hexadécimale (`#rgb` ou `#rrggbb`).
/// Retourne `None` si la chaîne n'est pas une couleur hexadécimale valide.
pub(crate) fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    // Garde ASCII : évite une coupure au milieu d'un caractère multioctet
    // et refuse le signe `+` qu'accepte `from_str_radix`.
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some(Color::Rgb((r << 4) | r, (g << 4) | g, (b << 4) | b))
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}

/// Une ligne du panneau de sélection : `entry` est `None` pour « Aucun ».
pub(crate) fn environment_item_line(
    entry: Option<&Result<crate::collection::Environment, crate::collection::ErrorNode>>,
    is_current: bool,
) -> Line<'static> {
    let marker = if is_current { "● " } else { "  " };
    let mut spans = vec![Span::raw(marker)];
    match entry {
        None => {
            spans.push(Span::styled(
                "Aucun",
                Style::new().add_modifier(Modifier::BOLD),
            ));
        }
        Some(Ok(env)) => {
            // Pastille carrée : le rond est déjà le marqueur de l'environnement actif.
            if let Some(color) = env.color.as_deref().and_then(parse_hex_color) {
                spans.push(Span::styled("■ ", Style::new().fg(color)));
            }
            spans.push(Span::raw(env.name.clone()));
            if let Some(extends) = &env.extends {
                spans.push(Span::styled(
                    format!(" ← {}", extends.display_name()),
                    theme::LABEL,
                ));
            }
        }
        Some(Err(err)) => {
            spans.push(Span::styled(
                format!("{} (invalide)", err.path.display()),
                theme::LOAD_ERROR,
            ));
        }
    }
    Line::from(spans)
}

fn history_row_line(entry: &HistoryEntry) -> Line<'static> {
    let time_str = format_time(entry.started_at);
    let target_str = if entry.recursive {
        format!("{} (récursif)", entry.target.display())
    } else {
        entry.target.display().to_string()
    };

    let (verdict_span, duration_span) = match &entry.outcome {
        HistoryOutcome::Completed {
            total,
            failed,
            duration_secs,
        } => {
            let (text, style) = if *failed == 0 {
                ("succès".to_owned(), theme::SUCCESS)
            } else {
                (format!("échec ({failed}/{total})"), theme::FAILURE)
            };
            (
                Span::styled(text, style.add_modifier(Modifier::BOLD)),
                Span::raw(format_duration(*duration_secs)),
            )
        }
        HistoryOutcome::Cancelled => (
            Span::styled(
                "annulée".to_owned(),
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("-"),
        ),
        HistoryOutcome::Failed(error) => (
            Span::styled(
                format!("échec ({error})"),
                theme::FAILURE.add_modifier(Modifier::BOLD),
            ),
            Span::raw("-"),
        ),
    };

    Line::from(vec![
        Span::styled(time_str, Style::new().add_modifier(Modifier::DIM)),
        Span::raw("  "),
        Span::styled(target_str, Style::new().add_modifier(Modifier::BOLD)),
        Span::raw("  ·  "),
        verdict_span,
        Span::raw("  ·  "),
        duration_span,
    ])
}

fn format_time(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let seconds_in_day = secs % 86400;
    let hours = seconds_in_day / 3600;
    let minutes = (seconds_in_day % 3600) / 60;
    let seconds = seconds_in_day % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

/// Durée en secondes au format français, comme les tailles du panneau
/// Statut : `0,07 s`.
fn format_duration(secs: f64) -> String {
    format!("{secs:.2} s").replace('.', ",")
}

/// Calcule la zone centrée pour le popup des options de campagne.
pub fn runner_options_popup_area(area: Rect) -> Rect {
    let width = 72.min(area.width.saturating_sub(4)).max(42);
    let height = 16.min(area.height.saturating_sub(2)).max(12);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

/// Dessine le popup de configuration des options de campagne.
pub fn render_runner_options_popup(popup: &RunnerOptionsPopup, frame: &mut Frame, area: Rect) {
    let block = theme::bordered()
        .title(" Options de campagne (TNR) — touche O ")
        .border_style(theme::FOCUS);
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    // Champ 0: Tags inclus
    let c0 = popup.cursor == 0;
    let tags_val = if popup.tags_input.text().is_empty() {
        "(aucun)"
    } else {
        popup.tags_input.text()
    };
    let tags_style = if c0 {
        theme::SELECTION
    } else {
        Style::default()
    };
    lines.push(Line::from(vec![
        Span::styled(if c0 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Tags inclus        : ", theme::LABEL),
        Span::styled(format!("[ {tags_val} ]"), tags_style),
        if c0 && popup.editing_text {
            Span::styled("  (saisie…)", theme::LABEL)
        } else {
            Span::raw("")
        },
    ]));

    // Champ 1: Tags exclus
    let c1 = popup.cursor == 1;
    let excl_val = if popup.exclude_tags_input.text().is_empty() {
        "(aucun)"
    } else {
        popup.exclude_tags_input.text()
    };
    let excl_style = if c1 {
        theme::SELECTION
    } else {
        Style::default()
    };
    lines.push(Line::from(vec![
        Span::styled(if c1 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Tags exclus        : ", theme::LABEL),
        Span::styled(format!("[ {excl_val} ]"), excl_style),
        if c1 && popup.editing_text {
            Span::styled("  (saisie…)", theme::LABEL)
        } else {
            Span::raw("")
        },
    ]));

    // Champ 2: Délai en ms
    let c2 = popup.cursor == 2;
    let delay_val = if popup.delay_input.text().is_empty() {
        "aucun"
    } else {
        popup.delay_input.text()
    };
    let delay_style = if c2 {
        theme::SELECTION
    } else {
        Style::default()
    };
    lines.push(Line::from(vec![
        Span::styled(if c2 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Délai en ms        : ", theme::LABEL),
        Span::styled(format!("[ {delay_val} ]"), delay_style),
        if c2 && popup.editing_text {
            Span::styled("  (saisie…)", theme::LABEL)
        } else {
            Span::raw("")
        },
    ]));

    // Champ 3: Bail
    let c3 = popup.cursor == 3;
    let bail_check = if popup.bail { "[x]" } else { "[ ]" };
    lines.push(Line::from(vec![
        Span::styled(if c3 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Arrêt 1er échec    : ", theme::LABEL),
        Span::styled(
            format!("{bail_check} Interrompre au premier échec (--bail)"),
            if c3 {
                theme::SELECTION
            } else {
                Style::default()
            },
        ),
    ]));

    // Champ 4: Tests-only
    let c4 = popup.cursor == 4;
    let tests_check = if popup.tests_only { "[x]" } else { "[ ]" };
    lines.push(Line::from(vec![
        Span::styled(if c4 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Tests uniquement   : ", theme::LABEL),
        Span::styled(
            format!("{tests_check} Uniquement requêtes avec tests (--tests-only)"),
            if c4 {
                theme::SELECTION
            } else {
                Style::default()
            },
        ),
    ]));

    // Champ 5: Rapport JUnit
    let c5 = popup.cursor == 5;
    let junit_check = if popup.reporter_junit { "[x]" } else { "[ ]" };
    lines.push(Line::from(vec![
        Span::styled(if c5 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Rapport JUnit XML  : ", theme::LABEL),
        Span::styled(
            format!("{junit_check} Exporter rapport JUnit (--reporter-junit)"),
            if c5 {
                theme::SELECTION
            } else {
                Style::default()
            },
        ),
    ]));

    // Champ 6: Rapport HTML
    let c6 = popup.cursor == 6;
    let html_check = if popup.reporter_html { "[x]" } else { "[ ]" };
    lines.push(Line::from(vec![
        Span::styled(if c6 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Rapport HTML       : ", theme::LABEL),
        Span::styled(
            format!("{html_check} Exporter rapport HTML (--reporter-html)"),
            if c6 {
                theme::SELECTION
            } else {
                Style::default()
            },
        ),
    ]));

    // Champ 7: Masquer en-têtes sensibles
    let c7 = popup.cursor == 7;
    let skip_check = if popup.skip_headers { "[x]" } else { "[ ]" };
    lines.push(Line::from(vec![
        Span::styled(if c7 { "▶ " } else { "  " }, theme::FOCUS),
        Span::styled("Masquer en-têtes   : ", theme::LABEL),
        Span::styled(
            format!("{skip_check} Omettre en-têtes sensibles (--reporter-skip-all-headers)"),
            if c7 {
                theme::SELECTION
            } else {
                Style::default()
            },
        ),
    ]));

    lines.push(Line::default());

    // Boutons (Champs 8 et 9)
    let c8 = popup.cursor == 8;
    let c9 = popup.cursor == 9;
    let save_btn = Span::styled(
        " [ Enregistrer ] ",
        if c8 { theme::SELECTION } else { theme::SECTION },
    );
    let run_btn = Span::styled(
        " [ Lancer la campagne (r) ] ",
        if c9 { theme::SELECTION } else { theme::SECTION },
    );
    lines.push(Line::from(vec![
        Span::raw("   "),
        save_btn,
        Span::raw("    "),
        run_btn,
    ]));

    lines.push(Line::default());

    // Erreur ou raccourcis
    if let Some(err) = &popup.error {
        lines.push(Line::styled(format!(" ⚠ {err}"), theme::FAILURE));
    } else {
        lines.push(Line::styled(
            " ↑↓ naviguer · Espace basculer · Entrée modifier / valider · Échap fermer",
            theme::LABEL,
        ));
    }

    frame.render_widget(Paragraph::new(lines), inner_area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Environment, EnvironmentExtends};
    use std::path::PathBuf;

    #[test]
    fn parse_hex_color_valid_formats() {
        assert_eq!(parse_hex_color("#f00"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_hex_color("#é1"), None);
        assert_eq!(parse_hex_color("#+f+f+f"), None);
        assert_eq!(parse_hex_color("#0f0"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_hex_color("#00f"), Some(Color::Rgb(0, 0, 255)));
        assert_eq!(parse_hex_color("#123"), Some(Color::Rgb(0x11, 0x22, 0x33)));
        assert_eq!(parse_hex_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_hex_color("#00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_hex_color("#ffaa00"), Some(Color::Rgb(255, 170, 0)));
        assert_eq!(parse_hex_color("#FFAA00"), Some(Color::Rgb(255, 170, 0)));
        assert_eq!(parse_hex_color("  #00ff00  "), Some(Color::Rgb(0, 255, 0)));
    }

    #[test]
    fn parse_hex_color_invalid_formats_return_none() {
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("#"), None);
        assert_eq!(parse_hex_color("#1"), None);
        assert_eq!(parse_hex_color("#12"), None);
        assert_eq!(parse_hex_color("#1234"), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("#1234567"), None);
        assert_eq!(parse_hex_color("red"), None);
        assert_eq!(parse_hex_color("#xyz"), None);
        assert_eq!(parse_hex_color("#12345z"), None);
    }

    #[test]
    fn environment_item_line_none_entry() {
        let line = environment_item_line(None, true);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "● Aucun");

        let line = environment_item_line(None, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "  Aucun");
    }

    #[test]
    fn environment_item_line_with_extends_and_valid_color() {
        let env = Environment {
            name: "staging".into(),
            path: PathBuf::from("environments/staging.bru"),
            variables: Vec::new(),
            secret_names: Vec::new(),
            extends: Some(EnvironmentExtends::Single("base".into())),
            color: Some("#ff0000".into()),
            ast: None,
        };
        let ok_entry = Ok(env);

        let line_current = environment_item_line(Some(&ok_entry), true);
        assert_eq!(line_current.spans.len(), 4);
        assert_eq!(line_current.spans[0].content, "● ");
        assert_eq!(line_current.spans[1].content, "■ ");
        assert_eq!(
            line_current.spans[1].style,
            Style::new().fg(Color::Rgb(255, 0, 0))
        );
        assert_eq!(line_current.spans[2].content, "staging");
        assert_eq!(line_current.spans[3].content, " ← base");

        let line_inactive = environment_item_line(Some(&ok_entry), false);
        assert_eq!(line_inactive.spans[0].content, "  ");
        assert_eq!(line_inactive.spans[1].content, "■ ");
        assert_eq!(
            line_inactive.spans[1].style,
            Style::new().fg(Color::Rgb(255, 0, 0))
        );
        assert_eq!(line_inactive.spans[2].content, "staging");
        assert_eq!(line_inactive.spans[3].content, " ← base");
    }

    #[test]
    fn environment_item_line_with_list_extends_and_invalid_color() {
        let env = Environment {
            name: "multi".into(),
            path: PathBuf::from("environments/multi.bru"),
            variables: Vec::new(),
            secret_names: Vec::new(),
            extends: Some(EnvironmentExtends::List(vec![
                "base".into(),
                "parent2".into(),
            ])),
            color: Some("not-a-color".into()),
            ast: None,
        };
        let ok_entry = Ok(env);

        let line = environment_item_line(Some(&ok_entry), false);
        // La couleur invalide est ignorée sans erreur : aucune pastille
        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[0].content, "  ");
        assert_eq!(line.spans[1].content, "multi");
        assert_eq!(line.spans[2].content, " ← base, parent2");
    }
}
