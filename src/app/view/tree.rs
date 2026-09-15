//! Lignes du panneau de l'arbre.

use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::collection::TreeNode;

/// Marqueur des nœuds en erreur.
pub const ERROR_MARK: &str = "✗";

/// Nom affiché : nom déclaré, sinon nom du dossier ; pour un nœud en
/// erreur, nom de fichier avec son extension.
pub fn display_name(node: &TreeNode) -> String {
    match node {
        TreeNode::Error(error) => file_name(&error.path),
        other => other.name(),
    }
}

/// Dernier composant d'un chemin, extension comprise.
pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Ligne de l'arbre pour un nœud.
pub fn row_line(node: &TreeNode, depth: usize, expanded: bool) -> Line<'static> {
    let indent = Span::raw("  ".repeat(depth));
    let error = Style::new().fg(Color::Red);
    match node {
        TreeNode::Folder(folder) => {
            let mut spans = vec![
                indent,
                Span::raw(if expanded { "▾ " } else { "▸ " }),
                Span::styled(
                    folder.name.clone(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
            ];
            if matches!(folder.meta, Some(Err(_))) {
                spans.push(Span::styled(format!(" {ERROR_MARK}"), error));
            }
            Line::from(spans)
        }
        TreeNode::Request(request) => Line::from(vec![
            indent,
            Span::styled(
                format!("{:<6} ", request.view.method),
                Style::new().fg(Color::Cyan),
            ),
            Span::raw(display_name(node)),
        ]),
        TreeNode::Error(_) => Line::from(vec![
            indent,
            Span::styled(format!("{ERROR_MARK} {}", display_name(node)), error),
        ]),
    }
}
