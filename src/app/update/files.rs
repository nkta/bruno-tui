//! Opérations de fichiers depuis l'arbre (`docs/edition-phase3.md`) :
//! `a` nouvelle requête, `A` nouveau dossier, `F2` renommer, `p` cloner,
//! `Maj+↑`/`Maj+↓` réordonner, `d` mettre à la corbeille.
//!
//! `update` ne fait aucune I/O : il ouvre les popups, calcule les `seq`
//! comme Bruno à partir de l'arbre chargé, puis demande l'opération à la
//! boucle (`Command::FileOp`). Au retour, la collection est rechargée et la
//! sélection suit l'élément créé, renommé ou déplacé.

use std::path::{Path, PathBuf};

use crate::app::file_popup::FilePopup;
use crate::app::message::Message;
use crate::app::model::{Model, PendingConfirm, StatusMessage};
use crate::app::workspace::{active_member, member_of_root, run_in_progress};
use crate::collection::{FolderNode, TreeNode};
use crate::writer::files::{FileOp, FileOpError, FileOpOutcome, SeqChange};
use crate::writer::sequence::{self, SeqItem};

use super::tree::clear_detail_view_state;
use super::{Command, refresh_rows, scroll_tree_into_view};

/// Ce que vise l'arbre : l'en-tête de la collection, ou un nœud.
enum Selected<'a> {
    Root,
    Node(&'a TreeNode),
}

fn selected(model: &Model) -> Option<Selected<'_>> {
    if model.selected_collection || model.selected_row().is_some_and(|row| row.is_header()) {
        return Some(Selected::Root);
    }
    model.selected_node().map(Selected::Node)
}

fn refuse(model: &mut Model, reason: impl Into<String>) {
    model.last_status = Some(StatusMessage::FileOpRefused(reason.into()));
}

/// Refus communs (§ 7) : collection absente ou YAML, exécution en cours.
/// Retourne la racine de la collection si l'opération est permise.
fn writable_root(model: &mut Model) -> Option<PathBuf> {
    let collection = model.loaded()?;
    if !collection.format.is_editable() {
        model.last_status = Some(StatusMessage::ReadOnlyCollection);
        return None;
    }
    let root = collection.root.clone();
    if run_in_progress(model) {
        refuse(
            model,
            "Exécution en cours : opération sur les fichiers refusée",
        );
        return None;
    }
    Some(root)
}

/// Enfants du répertoire `dir` (relatif à la racine) dans l'arbre chargé.
fn children_of<'a>(model: &'a Model, dir: &Path) -> Option<&'a [TreeNode]> {
    let mut nodes = model.loaded()?.tree.as_slice();
    let mut current = PathBuf::new();
    for component in dir.components() {
        current.push(component);
        nodes = nodes.iter().find_map(|node| match node {
            TreeNode::Folder(folder) if folder.path == current => Some(folder.children.as_slice()),
            _ => None,
        })?;
    }
    Some(nodes)
}

/// Nom affiché du répertoire `dir` : la collection pour la racine.
fn dir_label(model: &Model, dir: &Path) -> String {
    let Some(collection) = model.loaded() else {
        return String::new();
    };
    if dir.as_os_str().is_empty() {
        return collection.name.clone();
    }
    let parent = dir.parent().unwrap_or(Path::new(""));
    children_of(model, parent)
        .and_then(|nodes| nodes.iter().find(|node| node.path() == dir))
        .map(TreeNode::name)
        .unwrap_or_else(|| dir.display().to_string())
}

/// Répertoire de création : le dossier sélectionné, celui de la requête,
/// ou la racine.
fn creation_dir(target: &Selected<'_>) -> PathBuf {
    match target {
        Selected::Root => PathBuf::new(),
        Selected::Node(TreeNode::Folder(folder)) => folder.path.clone(),
        Selected::Node(node) => node
            .path()
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    }
}

/// Source chargé de `folder.bru` : `Ok(None)` s'il n'existe pas, `Err` s'il
/// est illisible.
fn folder_source(folder: &FolderNode) -> Result<Option<String>, ()> {
    match &folder.meta {
        None => Ok(None),
        Some(Ok(meta)) => meta
            .ast
            .as_ref()
            .map(|ast| Some(ast.raw().to_owned()))
            .ok_or(()),
        Some(Err(_)) => Err(()),
    }
}

/// `a` (nouvelle requête) ou `A` (nouveau dossier) dans l'arbre.
pub(crate) fn open_create(model: &mut Model, folder: bool) -> Command {
    if writable_root(model).is_none() {
        return Command::None;
    }
    let Some(target) = selected(model) else {
        return Command::None;
    };
    let dir = creation_dir(&target);
    // `seq` de Bruno : nombre de dossiers et requêtes du répertoire, plus 1.
    let seq = children_of(model, &dir).map_or(0, <[TreeNode]>::len) + 1;
    let context = dir_label(model, &dir);
    let seq = u32::try_from(seq).unwrap_or(u32::MAX);
    model.file_popup = Some(if folder {
        FilePopup::new_folder(dir, context, seq)
    } else {
        FilePopup::new_request(dir, context, seq)
    });
    Command::None
}

/// Nœud visé par `F2`, `p`, `Maj+↑↓` et `d` ; refus sur l'en-tête.
fn selected_node_or_refuse(model: &Model) -> Result<&TreeNode, &'static str> {
    match selected(model) {
        Some(Selected::Node(node)) => Ok(node),
        Some(Selected::Root) => Err("En-tête de la collection : seules a et A sont permises"),
        None => Err(""),
    }
}

/// `F2` : renommer la ligne sélectionnée.
pub(crate) fn open_rename(model: &mut Model) -> Command {
    if writable_root(model).is_none() {
        return Command::None;
    }
    let popup = match selected_node_or_refuse(model) {
        Err(reason) => Err(reason),
        Ok(node @ TreeNode::Request(request)) => match &request.ast {
            Some(ast) => Ok(FilePopup::rename(
                request.path.clone(),
                false,
                &node.name(),
                &file_stem(&request.path),
                Some(ast.raw().to_owned()),
            )),
            None => Err("Requête en lecture seule"),
        },
        Ok(TreeNode::Folder(folder)) => match folder_source(folder) {
            Ok(expected) => Ok(FilePopup::rename(
                folder.path.clone(),
                true,
                &folder.name,
                &file_name(&folder.path),
                expected,
            )),
            Err(()) => Err("folder.bru illisible : renommage refusé"),
        },
        Ok(TreeNode::Error(_)) => Err("Fichier illisible : renommage refusé"),
    };
    open_or_refuse(model, popup)
}

/// `p` : cloner la ligne sélectionnée.
pub(crate) fn open_clone(model: &mut Model) -> Command {
    if writable_root(model).is_none() {
        return Command::None;
    }
    let popup = match selected_node_or_refuse(model) {
        Err(reason) => Err(reason),
        Ok(node) => {
            let parent = node
                .path()
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            let siblings = children_of(model, &parent).unwrap_or_default();
            match node {
                TreeNode::Request(request) => match &request.ast {
                    // `seq` de Bruno : nombre d'éléments non-dossiers du
                    // répertoire, plus 1.
                    Some(ast) => Ok(FilePopup::clone_of(
                        request.path.clone(),
                        false,
                        &node.name(),
                        Some(ast.raw().to_owned()),
                        count_seq(
                            siblings
                                .iter()
                                .filter(|node| !matches!(node, TreeNode::Folder(_)))
                                .count(),
                        ),
                    )),
                    None => Err("Requête en lecture seule"),
                },
                // `seq` de Bruno : nombre d'éléments du répertoire, plus 1.
                TreeNode::Folder(folder) => Ok(FilePopup::clone_of(
                    folder.path.clone(),
                    true,
                    &folder.name,
                    None,
                    count_seq(siblings.len()),
                )),
                TreeNode::Error(_) => Err("Fichier illisible : clonage refusé"),
            }
        }
    };
    open_or_refuse(model, popup)
}

fn count_seq(count: usize) -> u32 {
    u32::try_from(count + 1).unwrap_or(u32::MAX)
}

fn open_or_refuse(model: &mut Model, popup: Result<FilePopup, &'static str>) -> Command {
    match popup {
        Ok(popup) => model.file_popup = Some(popup),
        Err("") => {}
        Err(reason) => refuse(model, reason),
    }
    Command::None
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Messages du popup de fichier ouvert ; `None` laisse passer le message
/// au traitement ordinaire (redimensionnement, sortie forcée…).
pub(crate) fn file_popup_message(model: &mut Model, message: &Message) -> Option<Command> {
    let popup = model.file_popup.as_mut()?;
    match message {
        Message::FilePopupKey(key) => popup.key(*key),
        Message::FilePopupNext => popup.next_field(),
        Message::FilePopupPrevious => popup.previous_field(),
        Message::FilePopupEnter if !popup.on_last_field() => popup.next_field(),
        Message::FilePopupEnter | Message::SaveEdit => return Some(submit(model)),
        Message::FilePopupCancel => model.file_popup = None,
        Message::Resize { .. } | Message::ForceQuit | Message::TerminalClosed(_) => return None,
        _ => {}
    }
    Some(Command::None)
}

fn submit(model: &mut Model) -> Command {
    let Some(root) = writable_root(model) else {
        model.file_popup = None;
        return Command::None;
    };
    let Some(popup) = model.file_popup.as_mut() else {
        return Command::None;
    };
    match popup.submit() {
        Err(reason) => {
            popup.error = Some(reason);
            Command::None
        }
        Ok(None) => {
            model.file_popup = None;
            Command::None
        }
        Ok(Some(op)) => {
            model.file_popup = None;
            if let FileOp::RenameEnvironment { old_name, new_name } = &op
                && model.current_environment.as_deref() == Some(old_name)
            {
                model.current_environment = Some(new_name.clone());
            }
            Command::FileOp { root, op }
        }
    }
}

/// `Maj+↑` (`up`) ou `Maj+↓` : échange la ligne avec sa voisine parmi ses
/// frères, et réécrit les `seq` comme Bruno (`resequence-items`).
pub(crate) fn move_item(model: &mut Model, up: bool) -> Command {
    let Some(root) = writable_root(model) else {
        return Command::None;
    };
    let node = match selected_node_or_refuse(model) {
        Ok(node) => node,
        Err(reason) => {
            if !reason.is_empty() {
                refuse(model, reason);
            }
            return Command::None;
        }
    };
    if matches!(node, TreeNode::Error(_)) {
        refuse(model, "Fichier illisible : déplacement refusé");
        return Command::None;
    }
    let moved = node.path().to_path_buf();
    let parent = moved.parent().map(Path::to_path_buf).unwrap_or_default();
    let nodes = siblings(model, &parent, None);
    let Some(position) = nodes.iter().position(|node| node.path() == moved) else {
        return Command::None;
    };
    // Ordre de Bruno (`sortItems`), puis échange avec le voisin.
    let changes = match sequence::move_one(&seq_items(&nodes), position, up) {
        Ok(Some(changes)) => changes,
        Ok(None) => return Command::None,
        Err(error) => {
            refuse(model, format!("{error} : déplacement refusé"));
            return Command::None;
        }
    };
    match seq_changes(&nodes, changes) {
        Ok(changes) => Command::FileOp {
            root,
            op: FileOp::Resequence { changes, moved },
        },
        Err(reason) => {
            refuse(model, format!("{reason} : déplacement refusé"));
            Command::None
        }
    }
}

/// Frères lisibles du répertoire `parent`, sans `exclude` : les fichiers
/// illisibles n'ont pas de `seq` à réécrire.
fn siblings<'a>(model: &'a Model, parent: &Path, exclude: Option<&Path>) -> Vec<&'a TreeNode> {
    children_of(model, parent)
        .unwrap_or_default()
        .iter()
        .filter(|node| !matches!(node, TreeNode::Error(_)))
        .filter(|node| Some(node.path()) != exclude)
        .collect()
}

fn seq_items(nodes: &[&TreeNode]) -> Vec<SeqItem> {
    nodes
        .iter()
        .map(|node| SeqItem {
            name: node.name(),
            seq: node.seq(),
        })
        .collect()
}

/// Nouveaux `seq` `(indice dans nodes, seq)` en opérations d'écriture.
fn seq_changes(
    nodes: &[&TreeNode],
    changes: Vec<(usize, u32)>,
) -> Result<Vec<SeqChange>, &'static str> {
    changes
        .into_iter()
        .map(|(index, seq)| {
            let node = nodes[index];
            let (is_folder, expected) = match node {
                TreeNode::Request(request) => match &request.ast {
                    Some(ast) => (false, Some(ast.raw().to_owned())),
                    None => return Err("requête en lecture seule"),
                },
                TreeNode::Folder(folder) => match folder_source(folder) {
                    Ok(expected) => (true, expected),
                    Err(()) => return Err("folder.bru illisible"),
                },
                TreeNode::Error(_) => return Err("fichier illisible"),
            };
            Ok(SeqChange {
                path: node.path().to_path_buf(),
                is_folder,
                seq,
                expected,
            })
        })
        .collect()
}

/// `d` dans l'arbre : demande confirmation avant la corbeille.
pub(crate) fn ask_trash(model: &mut Model) -> Command {
    if writable_root(model).is_none() {
        return Command::None;
    }
    let confirm = match selected_node_or_refuse(model) {
        Err(reason) => {
            if !reason.is_empty() {
                refuse(model, reason);
            }
            return Command::None;
        }
        Ok(node) => PendingConfirm::TrashItem {
            path: node.path().to_path_buf(),
            label: node.name(),
            requests: match node {
                TreeNode::Folder(folder) => Some(count_requests(&folder.children)),
                _ => None,
            },
        },
    };
    model.confirm = Some(confirm);
    Command::None
}

fn count_requests(nodes: &[TreeNode]) -> usize {
    nodes
        .iter()
        .map(|node| match node {
            TreeNode::Request(_) => 1,
            TreeNode::Folder(folder) => count_requests(&folder.children),
            TreeNode::Error(_) => 0,
        })
        .sum()
}

/// Réponse « oui » à la confirmation de corbeille.
pub(crate) fn confirm_trash(model: &mut Model, path: PathBuf) -> Command {
    let Some(root) = writable_root(model) else {
        return Command::None;
    };
    // Frères restants renumérotés comme Bruno après la suppression
    // (`getItemsToResequence`), calculés sur l'arbre chargé.
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let nodes = siblings(model, &parent, Some(&path));
    let renumber = sequence::items_to_resequence(&seq_items(&nodes))
        .map_err(|error| error.to_string())
        .and_then(|changes| seq_changes(&nodes, changes).map_err(str::to_owned));
    Command::FileOp {
        root,
        op: FileOp::Trash { path, renumber },
    }
}

/// Retour d'une opération : statut, rechargement, sélection de l'élément.
pub(crate) fn file_op_done(
    model: &mut Model,
    root: &Path,
    result: Result<FileOpOutcome, FileOpError>,
) -> Command {
    let select = match result {
        Ok(outcome) => {
            model.last_status = Some(StatusMessage::FileOpDone(outcome.message));
            outcome.select
        }
        Err(error) => {
            model.last_status = Some(StatusMessage::FileOpRefused(error.to_string()));
            None
        }
    };
    if let Some(member) = member_of_root(model, root)
        && Some(member) != active_member(model)
    {
        return Command::ReloadMember(member);
    }
    if model
        .loaded()
        .is_none_or(|collection| collection.root != root)
    {
        return Command::None;
    }
    if let Some(path) = select {
        if path.starts_with("environments") {
            let env_name = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned);
            model.select_environment_after_reload = env_name;
        } else {
            // Les dossiers qui mènent à l'élément sont dépliés.
            let mut ancestor = path.parent();
            while let Some(dir) = ancestor.filter(|dir| !dir.as_os_str().is_empty()) {
                model.tree.expanded.insert(dir.to_path_buf());
                ancestor = dir.parent();
            }
            model.select_after_reload = Some(path);
        }
    }
    Command::ReloadCollection
}

/// Sélectionne l'élément attendu après une opération de fichier, s'il est
/// dans l'arbre rechargé ; sinon l'attente continue (un rechargement plus
/// ancien a pu arriver d'abord).
pub(crate) fn apply_pending_selection(model: &mut Model) {
    if let Some(target_env) = model.select_environment_after_reload.take()
        && let Some(collection) = model.loaded()
    {
        model.environment_selected =
            super::environment::environment_index_for(collection, Some(&target_env));
    }
    let Some(path) = model.select_after_reload.clone() else {
        return;
    };
    refresh_rows(model);
    let Some(index) = model.active_row_position(|node| node.path() == path) else {
        return;
    };
    model.select_after_reload = None;
    if model.tree.selected != index || model.selected_collection {
        model.tree.selected = index;
        model.selected_collection = false;
        clear_detail_view_state(model);
    }
    scroll_tree_into_view(model);
}
