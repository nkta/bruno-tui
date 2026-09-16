//! État de l'interface.
//!
//! Le modèle est modifié uniquement par `update` et lu par `view`. Les
//! lignes visibles de l'arbre y sont précalculées pour que le rendu n'ait
//! qu'à les parcourir.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::PathBuf;

use crate::collection::{Collection, LoadError, TreeNode};
use crate::runner;

/// État du chargement de la collection.
#[derive(Debug)]
pub enum CollectionState {
    Loading,
    Loaded(Collection),
    Failed(LoadError),
}

/// Panneau recevant les touches de navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Detail,
}

/// Motif d'arrêt de la boucle.
#[derive(Debug)]
pub enum Exit {
    /// Fermeture demandée par l'utilisateur.
    Normal,
    /// Le terminal ne peut plus être lu.
    TerminalError(io::Error),
}

/// Ligne visible de l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Indices successifs depuis `Collection::tree` jusqu'au nœud.
    pub address: Vec<usize>,
    /// Profondeur, 0 au premier niveau.
    pub depth: usize,
}

#[derive(Debug, Default)]
pub struct TreeState {
    /// Chemins relatifs des dossiers dépliés.
    pub expanded: HashSet<PathBuf>,
    /// Nœuds visibles, dans l'ordre d'affichage.
    pub rows: Vec<Row>,
    /// Indice de la ligne sélectionnée dans `rows`.
    pub selected: usize,
    /// Première ligne affichée.
    pub offset: usize,
}

/// État des exécutions : au plus une active, un résultat par requête.
#[derive(Debug, Default)]
pub struct RunState {
    /// Exécution en cours, s'il y en a une.
    pub active: Option<ActiveRun>,
    /// Dernier résultat connu par requête. Clé : chemin relatif à la racine
    /// de la collection, identique à `RequestNode::path` et à
    /// `RequestResult::test.filename` (avec l'extension `.bru`) — aucune
    /// conversion n'est nécessaire pour indexer ou pour retrouver un nœud de
    /// l'arbre à partir d'une clé.
    pub outcomes: HashMap<PathBuf, RequestOutcome>,
    /// Dernière exécution n'ayant produit aucun résultat exploitable. Effacé
    /// au lancement réussi d'une nouvelle exécution.
    pub last_failure: Option<RunFailure>,
}

#[derive(Debug)]
pub struct ActiveRun {
    pub id: runner::RunId,
    /// Chemin lancé : la requête, ou le dossier en mode récursif.
    pub target: PathBuf,
    pub recursive: bool,
    /// `None` juste après une annulation déjà demandée : un second appui sur
    /// la touche d'annulation reste sans effet (idempotent).
    pub handle: Option<runner::RunHandle>,
}

/// Résultat conservé pour une requête, issu d'un rapport valide — que le
/// verdict de la requête soit un succès ou un échec.
#[derive(Debug)]
pub struct RequestOutcome {
    pub result: runner::report::RequestResult,
    pub exit_code: Option<i32>,
}

/// Exécution n'ayant pas produit de résultat exploitable.
#[derive(Debug)]
pub enum RunFailure {
    /// `bru` n'a pas pu s'exécuter ou son rapport est inexploitable.
    Error {
        target: PathBuf,
        error: runner::RunError,
    },
    Cancelled {
        target: PathBuf,
    },
}

#[derive(Debug)]
pub struct Model {
    /// Chemin demandé au lancement.
    pub source: PathBuf,
    pub collection: CollectionState,
    pub tree: TreeState,
    pub focus: Focus,
    /// Première ligne affichée du détail.
    pub detail_scroll: u16,
    /// Taille du terminal (colonnes, lignes).
    pub size: (u16, u16),
    /// `Some` : la boucle doit s'arrêter.
    pub exit: Option<Exit>,
    /// État des exécutions de requêtes.
    pub run: RunState,
}

impl Model {
    pub fn new(source: PathBuf, size: (u16, u16)) -> Self {
        Self {
            source,
            collection: CollectionState::Loading,
            tree: TreeState::default(),
            focus: Focus::Tree,
            detail_scroll: 0,
            size,
            exit: None,
            run: RunState::default(),
        }
    }

    /// Collection chargée, s'il y en a une.
    pub fn loaded(&self) -> Option<&Collection> {
        match &self.collection {
            CollectionState::Loaded(collection) => Some(collection),
            _ => None,
        }
    }

    /// Nœud désigné par une adresse.
    pub fn node_at(&self, address: &[usize]) -> Option<&TreeNode> {
        let (first, rest) = address.split_first()?;
        let mut node = self.loaded()?.tree.get(*first)?;
        for index in rest {
            node = match node {
                TreeNode::Folder(folder) => folder.children.get(*index)?,
                _ => return None,
            };
        }
        Some(node)
    }

    /// Nœud de la ligne sélectionnée.
    pub fn selected_node(&self) -> Option<&TreeNode> {
        let row = self.tree.rows.get(self.tree.selected)?;
        self.node_at(&row.address)
    }

    /// Vrai si le nœud est un dossier déplié.
    pub fn is_expanded(&self, node: &TreeNode) -> bool {
        matches!(node, TreeNode::Folder(folder) if self.tree.expanded.contains(&folder.path))
    }
}

/// Calcule les lignes visibles : les enfants d'un dossier ne sont listés
/// que s'il est déplié.
pub fn visible_rows(tree: &[TreeNode], expanded: &HashSet<PathBuf>) -> Vec<Row> {
    fn walk(
        nodes: &[TreeNode],
        expanded: &HashSet<PathBuf>,
        prefix: &mut Vec<usize>,
        rows: &mut Vec<Row>,
    ) {
        for (index, node) in nodes.iter().enumerate() {
            prefix.push(index);
            rows.push(Row {
                address: prefix.clone(),
                depth: prefix.len() - 1,
            });
            if let TreeNode::Folder(folder) = node
                && expanded.contains(&folder.path)
            {
                walk(&folder.children, expanded, prefix, rows);
            }
            prefix.pop();
        }
    }
    let mut rows = Vec::new();
    walk(tree, expanded, &mut Vec::new(), &mut rows);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::loaded_model;

    #[test]
    fn initial_rows_are_first_level_nodes_in_order() {
        let model = loaded_model((100, 30));
        let names: Vec<String> = model
            .tree
            .rows
            .iter()
            .map(|row| {
                assert_eq!(row.depth, 0);
                crate::app::view::tree::display_name(model.node_at(&row.address).expect("nœud"))
            })
            .collect();
        assert_eq!(
            names,
            [
                "Groupe",
                "ping",
                "post-json",
                "scripted",
                "badmeta",
                "broken.bru",
                "misc",
                "multiline",
                "no-method.bru",
                "no-seq",
                "unknown-block"
            ]
        );
        assert_eq!(model.tree.selected, 0);
        let selected = model.selected_node().expect("sélection");
        assert!(matches!(selected, TreeNode::Folder(f) if f.name == "Groupe"));
        assert!(!model.is_expanded(selected));
    }

    #[test]
    fn expanded_folders_list_their_children() {
        let mut model = loaded_model((100, 30));
        model.tree.expanded.insert("grp".into());
        model.tree.expanded.insert("grp/sub".into());
        let collection = model.loaded().expect("chargée");
        let rows = visible_rows(&collection.tree, &model.tree.expanded);
        let depths: Vec<(Vec<usize>, usize)> = rows
            .iter()
            .take(6)
            .map(|r| (r.address.clone(), r.depth))
            .collect();
        assert_eq!(
            depths,
            [
                (vec![0], 0),
                (vec![0, 0], 1),
                (vec![0, 1], 1),
                (vec![0, 1, 0], 2),
                (vec![0, 2], 1),
                (vec![0, 3], 1),
            ]
        );
        assert_eq!(rows.len(), 11 + 4 + 1);
    }
}
