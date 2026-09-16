//! Recherche de sous-chaîne dans l'arbre et dans le détail.
//!
//! Une seule recherche à la fois, insensible à la casse, circulaire. L'état
//! ne connaît que le dernier motif validé et le panneau où il a été
//! validé ; `n`/`N` le rejouent quel que soit le focus courant.

use std::ops::Range;

use super::model::{Row, tree_node_at};
use super::view::tree::display_name;
use crate::collection::TreeNode;

/// Panneau associé à un motif validé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    Tree,
    Detail,
}

#[derive(Debug, Default)]
pub struct SearchState {
    /// Motif validé ; vide avant toute recherche.
    pub pattern: String,
    /// Panneau actif au moment de la validation, pour `n`/`N`.
    pub scope: Option<SearchScope>,
    /// Ligne de saisie ouverte.
    pub editing: bool,
    /// Texte en cours de frappe, distinct du motif déjà validé.
    pub draft: String,
    /// Sens de la dernière recherche, pour `N` (qui l'inverse).
    pub direction_forward: bool,
    /// `Some(false)` : la dernière recherche n'a rien trouvé.
    pub last_result: Option<bool>,
}

impl SearchState {
    /// Vrai si une ligne de saisie de recherche est ouverte.
    pub fn is_editing(&self) -> bool {
        self.editing
    }
}

/// Compare deux textes par sous-chaîne, insensible à la casse.
fn contains_ci(haystack: &str, pattern: &str) -> bool {
    haystack.to_lowercase().contains(&pattern.to_lowercase())
}

/// Cherche la prochaine ligne (ou la précédente) dont le nœud correspond
/// au motif, à partir de `from` exclu, avec retour circulaire.
///
/// `rows` doit énumérer tous les nœuds (voir `model::all_rows`), dossiers
/// repliés compris : la recherche porte sur toute la collection.
pub fn find_tree_match(
    tree: &[TreeNode],
    rows: &[Row],
    from: usize,
    pattern: &str,
    forward: bool,
) -> Option<usize> {
    if rows.is_empty() || pattern.is_empty() {
        return None;
    }
    let len = rows.len();
    let step = |index: usize| -> usize {
        if forward {
            (index + 1) % len
        } else {
            (index + len - 1) % len
        }
    };
    let mut index = from.min(len - 1);
    for _ in 0..len {
        index = step(index);
        if let Some(node) = tree_node_at(tree, &rows[index].address)
            && contains_ci(&display_name(node), pattern)
        {
            return Some(index);
        }
    }
    None
}

/// Cherche la prochaine ligne de texte (ou la précédente) contenant le
/// motif, à partir de `from` exclue, avec retour circulaire. Rend le
/// numéro de ligne et la position en octets du motif dans cette ligne.
pub fn find_detail_match(
    lines: &[String],
    from: u16,
    pattern: &str,
    forward: bool,
) -> Option<(u16, Range<usize>)> {
    if lines.is_empty() || pattern.is_empty() {
        return None;
    }
    let len = lines.len();
    let from = usize::from(from).min(len - 1);
    let step = |index: usize| -> usize {
        if forward {
            (index + 1) % len
        } else {
            (index + len - 1) % len
        }
    };
    let pattern_lower = pattern.to_lowercase();
    let mut index = from;
    for _ in 0..len {
        index = step(index);
        let line_lower = lines[index].to_lowercase();
        if let Some(start) = line_lower.find(&pattern_lower) {
            return Some((index as u16, start..start + pattern_lower.len()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::model::Row;
    use crate::collection::{FolderNode, RequestNode, RequestView};

    fn folder(name: &str, children: Vec<TreeNode>) -> TreeNode {
        TreeNode::Folder(FolderNode {
            path: name.into(),
            name: name.to_owned(),
            seq: None,
            meta: None,
            children,
        })
    }

    fn request(name: &str) -> TreeNode {
        TreeNode::Request(RequestNode {
            path: format!("{name}.bru").into(),
            ast: None,
            view: RequestView {
                name: Some(name.to_owned()),
                kind: None,
                seq: None,
                method: "GET".into(),
                url: "http://x".into(),
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

    fn all_rows_of(tree: &[TreeNode]) -> Vec<Row> {
        super::super::model::all_rows(tree)
    }

    #[test]
    fn empty_tree_has_no_match() {
        let tree: Vec<TreeNode> = Vec::new();
        let rows = all_rows_of(&tree);
        assert_eq!(find_tree_match(&tree, &rows, 0, "x", true), None);
    }

    #[test]
    fn single_node_matches_itself_circularly() {
        let tree = vec![request("ping")];
        let rows = all_rows_of(&tree);
        assert_eq!(find_tree_match(&tree, &rows, 0, "PING", true), Some(0));
    }

    #[test]
    fn wraps_around_forward_and_backward() {
        let tree = vec![request("a"), request("b"), request("c")];
        let rows = all_rows_of(&tree);
        // Sélection sur "c" (indice 2), recherche de "a" vers l'avant :
        // reprend depuis le début après un tour complet.
        assert_eq!(find_tree_match(&tree, &rows, 2, "a", true), Some(0));
        // Sélection sur "a" (indice 0), recherche vers l'arrière : reprend
        // depuis la fin.
        assert_eq!(find_tree_match(&tree, &rows, 0, "c", false), Some(2));
    }

    #[test]
    fn matches_inside_collapsed_folders() {
        let tree = vec![folder("Groupe", vec![request("inherit")])];
        let rows = all_rows_of(&tree); // tous les nœuds, dossier compris
        assert_eq!(rows.len(), 2);
        let found = find_tree_match(&tree, &rows, 0, "inherit", true).expect("trouvé");
        assert_eq!(rows[found].address, [0, 0]);
    }

    #[test]
    fn no_match_returns_none() {
        let tree = vec![request("ping")];
        let rows = all_rows_of(&tree);
        assert_eq!(find_tree_match(&tree, &rows, 0, "zzz", true), None);
    }

    #[test]
    fn detail_match_at_end_wraps_and_is_case_insensitive() {
        let lines: Vec<String> = ["alpha", "beta", "GAMMA"].map(str::to_owned).into();
        assert_eq!(find_detail_match(&lines, 0, "gamma", true), Some((2, 0..5)));
        // Depuis la ligne trouvée, la recherche suivante reprend au début.
        assert_eq!(find_detail_match(&lines, 2, "alpha", true), Some((0, 0..5)));
    }

    #[test]
    fn detail_match_position_within_line() {
        let lines: Vec<String> = ["  res.body.ok: isTrue".to_owned()].into();
        let (line, range) = find_detail_match(&lines, 0, "res.body.ok", true).expect("trouvé");
        assert_eq!(line, 0);
        assert_eq!(&lines[0][range], "res.body.ok");
    }

    #[test]
    fn detail_no_match() {
        let lines: Vec<String> = ["alpha".to_owned()].into();
        assert_eq!(find_detail_match(&lines, 0, "zzz", true), None);
    }
}
