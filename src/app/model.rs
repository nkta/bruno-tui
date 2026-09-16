//! État de l'interface.
//!
//! Le modèle est modifié uniquement par `update` et lu par `view`. Les
//! lignes visibles de l'arbre y sont précalculées pour que le rendu n'ait
//! qu'à les parcourir.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::ops::Range;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::collection::{Collection, LoadError, TreeNode};
use crate::runner;

use super::message::TextCapture;
use super::search::SearchState;

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
    Diagnostics,
    History,
}

/// Nombre maximal d'entrées conservées dans le journal d'historique.
pub const HISTORY_LIMIT: usize = 200;

/// Une entrée du journal d'exécutions de la session.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub started_at: SystemTime,
    pub target: PathBuf,
    pub recursive: bool,
    pub outcome: HistoryOutcome,
}

/// Issue d'une exécution dans le journal d'historique.
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryOutcome {
    Completed {
        total: u64,
        failed: u64,
        duration_secs: f64,
    },
    Failed(String),
    Cancelled,
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

/// Sélection visuelle de lignes dans le détail, ancrée au sommet du
/// panneau au moment de son activation ; la borne mobile se déduit du
/// défilement courant (`update::selection_range`), jamais stockée à part.
#[derive(Debug, Clone, Copy)]
pub struct DetailSelection {
    pub anchor: u16,
}

#[derive(Debug)]
pub struct Model {
    /// Chemin demandé au lancement.
    pub source: PathBuf,
    pub collection: CollectionState,
    pub tree: TreeState,
    pub focus: Focus,
    /// Indice de l'entrée sélectionnée dans le panneau de diagnostics.
    pub diagnostics_selected: usize,
    /// Indice de l'entrée sélectionnée dans le panneau d'historique.
    pub history_selected: usize,
    /// Journal des exécutions de la session, les plus récentes en tête.
    pub history: VecDeque<HistoryEntry>,
    /// Première ligne affichée du détail.
    pub detail_scroll: u16,
    /// Taille du terminal (colonnes, lignes).
    pub size: (u16, u16),
    /// `Some` : la boucle doit s'arrêter.
    pub exit: Option<Exit>,
    /// État des exécutions de requêtes.
    pub run: RunState,
    /// État de la recherche, `None` tant que `/` n'a jamais été pressé.
    pub search: Option<SearchState>,
    /// Sélection visuelle active dans le détail.
    pub detail_selection: Option<DetailSelection>,
    /// Ligne et position en octets de la dernière correspondance de
    /// recherche trouvée dans le détail, pour la surbrillance.
    pub detail_match: Option<(u16, Range<usize>)>,
    /// Dernier statut à afficher dans la barre d'état (copie, recherche
    /// sans résultat), en plus des rappels habituels. Persiste jusqu'au
    /// prochain statut, aucune minuterie.
    pub last_status: Option<StatusMessage>,
    /// Jeton de la copie en cours, s'il y en a une : un `ClipboardResult`
    /// dont le jeton ne correspond plus est ignoré (résultat en retard sur
    /// une copie plus récente).
    pub(crate) pending_clipboard_token: Option<u64>,
    /// Prochain jeton à distribuer à une copie.
    pub(crate) next_clipboard_token: u64,
}

/// Message affiché temporairement dans la barre d'état.
#[derive(Debug, Clone)]
pub enum StatusMessage {
    Copied,
    ClipboardError(String),
    /// Recherche validée sans aucune correspondance.
    NoMatch,
}

impl Model {
    pub fn new(source: PathBuf, size: (u16, u16)) -> Self {
        Self {
            source,
            collection: CollectionState::Loading,
            tree: TreeState::default(),
            focus: Focus::Tree,
            diagnostics_selected: 0,
            history_selected: 0,
            history: VecDeque::new(),
            detail_scroll: 0,
            size,
            exit: None,
            run: RunState::default(),
            search: None,
            detail_selection: None,
            detail_match: None,
            last_status: None,
            pending_clipboard_token: None,
            next_clipboard_token: 0,
        }
    }

    /// Ce que la boucle doit capturer au clavier hors navigation normale.
    /// Conçu pour que `add-field-editing`/`add-response-filter` y ajoutent
    /// leur propre branche sans toucher à celle-ci.
    pub fn text_capture(&self) -> Option<TextCapture> {
        self.search
            .as_ref()
            .is_some_and(SearchState::is_editing)
            .then_some(TextCapture::Search)
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
        tree_node_at(&self.loaded()?.tree, address)
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

/// Nœud désigné par une adresse, dans un arbre donné. Partagé par
/// `Model::node_at` et par la recherche, qui doit résoudre des adresses de
/// `all_rows` sans dépendre du `Model`.
pub(crate) fn tree_node_at<'a>(tree: &'a [TreeNode], address: &[usize]) -> Option<&'a TreeNode> {
    let (first, rest) = address.split_first()?;
    let mut node = tree.get(*first)?;
    for index in rest {
        node = match node {
            TreeNode::Folder(folder) => folder.children.get(*index)?,
            _ => return None,
        };
    }
    Some(node)
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

/// Toutes les lignes de la collection, dossiers repliés compris : pour la
/// recherche, qui doit pouvoir trouver un nœud non visible.
pub fn all_rows(tree: &[TreeNode]) -> Vec<Row> {
    fn walk(nodes: &[TreeNode], prefix: &mut Vec<usize>, rows: &mut Vec<Row>) {
        for (index, node) in nodes.iter().enumerate() {
            prefix.push(index);
            rows.push(Row {
                address: prefix.clone(),
                depth: prefix.len() - 1,
            });
            if let TreeNode::Folder(folder) = node {
                walk(&folder.children, prefix, rows);
            }
            prefix.pop();
        }
    }
    let mut rows = Vec::new();
    walk(tree, &mut Vec::new(), &mut rows);
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

    #[test]
    fn new_model_starts_with_empty_history() {
        let model = Model::new(PathBuf::from("test"), (100, 30));
        assert!(model.history.is_empty());
        assert_eq!(model.diagnostics_selected, 0);
        assert_eq!(model.history_selected, 0);
    }
}
