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
    Focus, HistoryEntry, HistoryOutcome, Model, SecretError, SecretInput, secret_rows,
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
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
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
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        area,
        &mut list_state,
    );
}

/// Dessine le panneau plein corps de sélection d'environnement : « Aucun »
/// puis les entrées de `Collection.environments`, une en erreur marquée
/// et non mise en valeur comme sélectionnable.
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
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        area,
        &mut list_state,
    );
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
        List::new(items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
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

/// Une ligne du panneau de sélection : `entry` est `None` pour « Aucun ».
fn environment_item_line(
    entry: Option<&Result<crate::collection::Environment, crate::collection::ErrorNode>>,
    is_current: bool,
) -> Line<'static> {
    let marker = if is_current { "● " } else { "  " };
    let label = match entry {
        None => Span::styled("Aucun", Style::new().add_modifier(Modifier::BOLD)),
        Some(Ok(env)) => Span::raw(env.name.clone()),
        Some(Err(err)) => Span::styled(
            format!("{} (invalide)", err.path.display()),
            theme::LOAD_ERROR,
        ),
    };
    Line::from(vec![Span::raw(marker), label])
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
                Span::raw(format!("{duration_secs:.2}s")),
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
