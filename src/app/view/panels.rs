//! Rendu des panneaux Diagnostics et Historique.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use super::panel;
use crate::app::diagnostics::diagnostics;
use crate::app::model::{Focus, HistoryEntry, HistoryOutcome, Model};

/// Dessine le panneau plein corps des diagnostics.
pub fn render_diagnostics(model: &Model, frame: &mut Frame, area: Rect) {
    let block = panel(" Diagnostics ", model.focus == Focus::Diagnostics);
    let Some(collection) = model.loaded() else {
        frame.render_widget(Paragraph::new("aucune erreur").block(block), area);
        return;
    };
    let entries = diagnostics(collection);
    if entries.is_empty() {
        frame.render_widget(Paragraph::new("aucune erreur").block(block), area);
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
                Span::styled(entry.reason.clone(), Style::new().fg(Color::Red)),
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
        frame.render_widget(Paragraph::new("aucune exécution").block(block), area);
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
            let (text, color) = if *failed == 0 {
                ("succès".to_owned(), Color::Green)
            } else {
                (format!("échec ({failed}/{total})"), Color::Red)
            };
            (
                Span::styled(text, Style::new().fg(color).add_modifier(Modifier::BOLD)),
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
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
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
