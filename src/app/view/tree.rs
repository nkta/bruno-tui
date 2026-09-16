//! Lignes du panneau de l'arbre.

use std::path::Path;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::collection::TreeNode;

/// Marqueur des nœuds en erreur de chargement (`.bru` invalide).
pub const ERROR_MARK: &str = "✗";
/// Marqueur d'une requête dont le dernier résultat est un succès.
pub const SUCCESS_MARK: &str = "✓";
/// Marqueur d'une requête dont le dernier résultat est un échec, distinct
/// de [`ERROR_MARK`].
pub const FAILURE_MARK: &str = "●";
/// Indicateur d'une requête en cours d'exécution.
pub const RUNNING_MARK: &str = "…";

/// Statut d'exécution d'une requête, tel qu'affiché sur sa ligne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStatus {
    /// Aucun résultat connu, aucune exécution en cours.
    None,
    /// Cible de l'exécution en cours.
    Running,
    /// Dernier résultat connu : succès.
    Success,
    /// Dernier résultat connu : échec.
    Failure,
}

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

/// Ligne de l'arbre pour un nœud, avec son statut d'exécution s'il s'agit
/// d'une requête.
pub fn row_line(node: &TreeNode, depth: usize, expanded: bool, status: RunStatus) -> Line<'static> {
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
        TreeNode::Request(request) => {
            let mut spans = vec![
                indent,
                Span::styled(
                    format!("{:<6} ", request.view.method),
                    Style::new().fg(Color::Cyan),
                ),
                Span::raw(display_name(node)),
            ];
            match status {
                RunStatus::None => {}
                RunStatus::Running => {
                    spans.push(Span::styled(
                        format!(" {RUNNING_MARK}"),
                        Style::new().fg(Color::Yellow),
                    ));
                }
                RunStatus::Success => {
                    spans.push(Span::styled(
                        format!(" {SUCCESS_MARK}"),
                        Style::new().fg(Color::Green),
                    ));
                }
                RunStatus::Failure => {
                    spans.push(Span::styled(format!(" {FAILURE_MARK}"), error));
                }
            }
            Line::from(spans)
        }
        TreeNode::Error(_) => Line::from(vec![
            indent,
            Span::styled(format!("{ERROR_MARK} {}", display_name(node)), error),
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{RequestNode, RequestView};

    fn plain(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    }

    fn request_node() -> TreeNode {
        TreeNode::Request(RequestNode {
            path: "ok.bru".into(),
            ast: None,
            view: RequestView {
                name: Some("ok".to_owned()),
                kind: Some("http".to_owned()),
                seq: Some(1),
                method: "GET".to_owned(),
                url: "http://x".to_owned(),
                headers: Vec::new(),
                query_params: Vec::new(),
                path_params: Vec::new(),
                body: None,
                auth: None,
                has_pre_request_script: false,
                has_post_response_script: false,
                has_tests: false,
                has_assert: false,
                assertions: Vec::new(),
            },
        })
    }

    #[test]
    fn success_failure_and_running_marks_are_distinct_from_the_error_mark() {
        let node = request_node();
        let none = plain(&row_line(&node, 0, false, RunStatus::None));
        let success = plain(&row_line(&node, 0, false, RunStatus::Success));
        let failure = plain(&row_line(&node, 0, false, RunStatus::Failure));
        let running = plain(&row_line(&node, 0, false, RunStatus::Running));

        assert!(!none.contains(SUCCESS_MARK));
        assert!(!none.contains(FAILURE_MARK));
        assert!(!none.contains(RUNNING_MARK));

        assert!(success.contains(SUCCESS_MARK), "{success}");
        assert!(failure.contains(FAILURE_MARK), "{failure}");
        assert!(running.contains(RUNNING_MARK), "{running}");

        // Distincts textuellement du marqueur de parsing en erreur.
        assert_ne!(SUCCESS_MARK, ERROR_MARK);
        assert_ne!(FAILURE_MARK, ERROR_MARK);
        assert_ne!(RUNNING_MARK, ERROR_MARK);
        assert!(!success.contains(ERROR_MARK));
        assert!(!failure.contains(ERROR_MARK));
        assert!(!running.contains(ERROR_MARK));
    }
}
