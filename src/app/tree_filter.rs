//! Filtrage de l'arbre de la collection par nom ou chemin.
//!
//! Ce module implémente le filtrage de l'arbre selon un motif en sous-chaîne,
//! insensible à la casse et aux diacritiques courants. Seules les requêtes et
//! les nœuds en erreur correspondants sont conservés, ainsi que leurs dossiers
//! parents dépliés automatiquement.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::model::Row;
use super::view::tree::display_name;
use crate::collection::TreeNode;

/// État du filtre appliqué à l'arbre de la collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeFilterState {
    /// Vrai si la ligne de saisie du filtre est ouverte dans la barre d'état.
    pub editing: bool,
    /// Texte en cours de frappe dans la barre d'état.
    pub draft: String,
    /// Motif validé (`None` avant la première validation ou si effacé).
    pub pattern: Option<String>,
    /// Chemins des dossiers qui étaient dépliés avant l'activation du filtre,
    /// pour les restaurer fidèlement à la désactivation.
    pub saved_expanded: HashSet<PathBuf>,
    /// Motif précédent sauvegardé lors de la réouverture par 'f' d'un filtre déjà actif,
    /// pour permettre l'annulation par Échap.
    pub previous_pattern: Option<String>,
}

impl TreeFilterState {
    /// Crée un nouvel état de filtre en démarrant la saisie.
    pub fn new(saved_expanded: HashSet<PathBuf>) -> Self {
        Self {
            editing: true,
            draft: String::new(),
            pattern: None,
            saved_expanded,
            previous_pattern: None,
        }
    }

    /// Réouvre la saisie pour un filtre déjà actif.
    pub fn reopen(&mut self) {
        self.editing = true;
        self.previous_pattern = self.pattern.clone();
        self.draft.clear();
    }
}

/// Supprime les accents, développe les ligatures et convertit en minuscules
/// pour une comparaison insensible à la casse et aux diacritiques.
fn normalize_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => {
                out.push('a')
            }
            'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => out.push('e'),
            'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => out.push('i'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => out.push('o'),
            'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => out.push('u'),
            'ý' | 'ÿ' | 'Ý' => out.push('y'),
            'ç' | 'Ç' => out.push('c'),
            'ñ' | 'Ñ' => out.push('n'),
            'æ' | 'Æ' => out.push_str("ae"),
            'œ' | 'Œ' => out.push_str("oe"),
            other => {
                for lc in other.to_lowercase() {
                    out.push(lc);
                }
            }
        }
    }
    out
}

/// Vérifie si `haystack` contient `needle`, sans tenir compte de la casse ni des accents.
pub fn contains_ci_diacritics(haystack: &str, needle: &str) -> bool {
    let h = normalize_str(haystack);
    let n = normalize_str(needle);
    h.contains(&n)
}

/// Détermine si un nœud individuel correspond directement au motif de filtre.
/// Seules les requêtes et les nœuds en erreur de chargement peuvent correspondre directement
/// par leur nom affiché ou leur chemin.
pub fn matches_tree_filter(node: &TreeNode, pattern: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    match node {
        TreeNode::Request(_) | TreeNode::Error(_) => {
            let name = display_name(node);
            let path_str = node.path().to_string_lossy();
            contains_ci_diacritics(&name, pattern) || contains_ci_diacritics(&path_str, pattern)
        }
        TreeNode::Folder(_) => false,
    }
}

/// Détermine si un nœud (ou un de ses descendants) correspond au motif.
pub fn has_matching_descendants(node: &TreeNode, pattern: &str) -> bool {
    match node {
        TreeNode::Request(_) | TreeNode::Error(_) => matches_tree_filter(node, pattern),
        TreeNode::Folder(folder) => folder
            .children
            .iter()
            .any(|child| has_matching_descendants(child, pattern)),
    }
}

/// Calcule les lignes visibles de l'arbre filtré : seules les requêtes et nœuds en erreur
/// correspondants ainsi que leurs dossiers parents sont conservés.
/// Les dossiers parents correspondants sont ajoutés à `expanded`.
pub fn filtered_rows(
    tree: &[TreeNode],
    pattern: &str,
    expanded: &mut HashSet<PathBuf>,
) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut prefix = Vec::new();
    filter_rows_walk(tree, &mut prefix, pattern, &mut rows, expanded);
    rows
}

fn filter_rows_walk(
    nodes: &[TreeNode],
    prefix: &mut Vec<usize>,
    pattern: &str,
    rows: &mut Vec<Row>,
    expanded: &mut HashSet<PathBuf>,
) {
    for (index, node) in nodes.iter().enumerate() {
        prefix.push(index);
        match node {
            TreeNode::Request(_) | TreeNode::Error(_) => {
                if matches_tree_filter(node, pattern) {
                    rows.push(Row {
                        address: prefix.clone(),
                        depth: prefix.len() - 1,
                    });
                }
            }
            TreeNode::Folder(folder) => {
                if has_matching_descendants(node, pattern) {
                    rows.push(Row {
                        address: prefix.clone(),
                        depth: prefix.len() - 1,
                    });
                    expanded.insert(folder.path.clone());
                    filter_rows_walk(&folder.children, prefix, pattern, rows, expanded);
                }
            }
        }
        prefix.pop();
    }
}

/// Recherche récursive d'un nœud dans l'arborescence par son chemin relatif.
pub fn find_node_by_path<'a>(nodes: &'a [TreeNode], target: &Path) -> Option<&'a TreeNode> {
    for node in nodes {
        if node.path() == target {
            return Some(node);
        }
        if let TreeNode::Folder(folder) = node
            && let Some(found) = find_node_by_path(&folder.children, target)
        {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{ErrorNode, FolderNode, ParseError, RequestNode, RequestView};

    fn request(name: &str, path: &str) -> TreeNode {
        TreeNode::Request(RequestNode {
            path: path.into(),
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

    fn folder(name: &str, path: &str, children: Vec<TreeNode>) -> TreeNode {
        TreeNode::Folder(FolderNode {
            path: path.into(),
            name: name.to_owned(),
            seq: None,
            meta: None,
            children,
        })
    }

    fn error_node(path: &str) -> TreeNode {
        TreeNode::Error(ErrorNode {
            path: path.into(),
            error: ParseError::UnclosedBlock {
                name: "headers".into(),
                line: 1,
            },
        })
    }

    #[test]
    fn case_and_diacritics_matching() {
        assert!(contains_ci_diacritics("Créer Utilisateur", "creer"));
        assert!(contains_ci_diacritics("creer utilisateur", "Créer"));
        assert!(contains_ci_diacritics("Requête Élève", "eleve"));
        assert!(contains_ci_diacritics("requete", "REQUÊTE"));
        assert!(contains_ci_diacritics("cœur", "coeur"));
        assert!(contains_ci_diacritics("garçon", "garcon"));
        assert!(!contains_ci_diacritics("autre", "creer"));
    }

    #[test]
    fn request_matching_by_name_and_path() {
        let req = request("Afficher profil", "users/profile.bru");
        // Correspondance par le nom affiché (avec ou sans accent/casse)
        assert!(matches_tree_filter(&req, "afficher"));
        assert!(matches_tree_filter(&req, "profil"));
        // Correspondance par le chemin
        assert!(matches_tree_filter(&req, "users"));
        assert!(matches_tree_filter(&req, "profile.bru"));
        assert!(!matches_tree_filter(&req, "auth"));
    }

    #[test]
    fn error_node_matching_by_path_and_filename() {
        let err = error_node("auth/broken_request.bru");
        assert!(matches_tree_filter(&err, "broken"));
        assert!(matches_tree_filter(&err, "auth"));
        assert!(!matches_tree_filter(&err, "users"));
    }

    #[test]
    fn filtered_rows_keeps_parents_expanded() {
        let tree = vec![
            folder(
                "Users",
                "users",
                vec![
                    request("List Users", "users/list.bru"),
                    folder(
                        "Admin",
                        "users/admin",
                        vec![request("Delete User", "users/admin/delete.bru")],
                    ),
                ],
            ),
            folder("Auth", "auth", vec![request("Login", "auth/login.bru")]),
            error_node("corrupt.bru"),
        ];

        let mut expanded = HashSet::new();
        let rows = filtered_rows(&tree, "delete", &mut expanded);

        // Doit inclure : users (depth 0), users/admin (depth 1), Delete User (depth 2)
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].address, vec![0]);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].address, vec![0, 1]);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].address, vec![0, 1, 0]);
        assert_eq!(rows[2].depth, 2);

        // Les dossiers parents doivent être ajoutés dans `expanded`
        assert!(expanded.contains(Path::new("users")));
        assert!(expanded.contains(Path::new("users/admin")));
        assert!(!expanded.contains(Path::new("auth")));
    }

    #[test]
    fn filtered_rows_no_match_returns_empty() {
        let tree = vec![folder(
            "Users",
            "users",
            vec![request("List Users", "users/list.bru")],
        )];
        let mut expanded = HashSet::new();
        let rows = filtered_rows(&tree, "inexistant", &mut expanded);
        assert!(rows.is_empty());
        assert!(expanded.is_empty());
    }

    #[test]
    fn state_transitions() {
        let mut initial_expanded = HashSet::new();
        initial_expanded.insert(PathBuf::from("users"));

        let mut state = TreeFilterState::new(initial_expanded.clone());
        assert!(state.editing);
        assert!(state.draft.is_empty());
        assert!(state.pattern.is_none());
        assert_eq!(state.saved_expanded, initial_expanded);

        // Simulation de validation
        state.draft = "test".to_string();
        state.pattern = Some("test".to_string());
        state.editing = false;

        // Réouverture
        state.reopen();
        assert!(state.editing);
        assert!(state.draft.is_empty());
        assert_eq!(state.previous_pattern, Some("test".to_string()));
    }
}
