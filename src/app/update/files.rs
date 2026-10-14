//! Opérations de fichiers depuis l'arbre (`docs/edition-phase3.md`) :
//! `a` nouvelle requête, `A` nouveau dossier, `F2` renommer, `p` cloner,
//! `Maj+↑`/`Maj+↓` réordonner, `d` mettre à la corbeille, `m` déplacer vers
//! un autre dossier (`docs/edition-phase4.md`).
//!
//! `update` ne fait aucune I/O : il ouvre les popups, calcule les `seq`
//! comme Bruno à partir de l'arbre chargé, puis demande l'opération à la
//! boucle (`Command::FileOp`). Au retour, la collection est rechargée et la
//! sélection suit l'élément créé, renommé ou déplacé.

use std::path::{Path, PathBuf};

use crate::app::file_popup::FilePopup;
use crate::app::message::Message;
use crate::app::model::{Model, PendingConfirm, StatusMessage};
use crate::app::move_popup::{MoveDestination, MovePopup, ROOT_LABEL};
use crate::app::text_input::TextInput;
use crate::app::workspace::{active_member, member_of_root, run_in_progress};
use crate::collection::{FolderNode, TreeNode};
use crate::writer::files::{FileOp, FileOpError, FileOpOutcome, RequestKind, SeqChange};
use crate::writer::names::first_free_name;
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
    let mut popup = if folder {
        FilePopup::new_folder(dir, context, seq)
    } else {
        FilePopup::new_request(dir, context, seq)
    };
    if !folder {
        apply_presets(model, &mut popup);
    }
    model.file_popup = Some(popup);
    Command::None
}

/// `presets.requestType` et `presets.requestUrl` de `bruno.json`
/// préremplissent le popup de nouvelle requête (`docs/reglages-collection.md`
/// § 4.1, D6-A). `grpc` et `ws` ne sont pas créables : repli sur `http`,
/// avec une mention dans le statut. L'URL reste une chaîne, jamais résolue.
fn apply_presets(model: &mut Model, popup: &mut FilePopup) {
    let Some(presets) = model.loaded().and_then(|c| c.presets.as_deref()).cloned() else {
        return;
    };
    match presets.request_type.as_deref() {
        Some("graphql") => popup.request_kind = RequestKind::Graphql,
        Some(kind @ ("grpc" | "ws")) => {
            model.last_status = Some(StatusMessage::FileOpDone(format!(
                "Preset « {kind} » non créable dans le TUI : http utilisé"
            )));
        }
        _ => {}
    }
    if let Some(url) = presets.request_url.filter(|url| !url.is_empty()) {
        popup.url = TextInput::new(&url, false);
    }
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
            if let FileOp::RenameEnvironment {
                old_name, new_name, ..
            } = &op
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
    let renumber = renumber_siblings(model, &path);
    Command::FileOp {
        root,
        op: FileOp::Trash { path, renumber },
    }
}

/// Frères restants de `path` renumérotés comme Bruno après une suppression
/// (`getItemsToResequence`), calculés sur l'arbre chargé. Sert à la
/// corbeille et au déplacement (source).
fn renumber_siblings(model: &Model, path: &Path) -> Result<Vec<SeqChange>, String> {
    let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let nodes = siblings(model, &parent, Some(path));
    sequence::items_to_resequence(&seq_items(&nodes))
        .map_err(|error| error.to_string())
        .and_then(|changes| seq_changes(&nodes, changes).map_err(str::to_owned))
}

/// `m` dans l'arbre : ouvre « Déplacer vers » pour la ligne sélectionnée.
pub(crate) fn open_move(model: &mut Model) -> Command {
    if writable_root(model).is_none() {
        return Command::None;
    }
    let node = match selected_node_or_refuse(model) {
        Ok(node) => node,
        Err(reason) => {
            if !reason.is_empty() {
                refuse(model, reason);
            }
            return Command::None;
        }
    };
    let item = node.path().to_path_buf();
    let is_folder = matches!(node, TreeNode::Folder(_));
    match node {
        TreeNode::Request(request) if request.ast.is_none() => {
            refuse(model, "Requête en lecture seule : déplacement refusé");
            return Command::None;
        }
        TreeNode::Error(_) => {
            refuse(model, "Fichier illisible : déplacement refusé");
            return Command::None;
        }
        _ => {}
    }
    let destinations = move_destinations(model, &item, is_folder);
    if destinations.is_empty() {
        refuse(model, "Aucun autre dossier où déplacer cet élément");
        return Command::None;
    }
    model.move_popup = Some(MovePopup::new(
        item.clone(),
        is_folder,
        file_name(&item),
        destinations,
    ));
    Command::None
}

/// Racine de la collection, puis tous les dossiers en arbre. Masqués : le
/// dossier actuel de l'élément (Bruno ne fait rien), l'élément lui-même et
/// ses descendants si c'est un dossier.
fn move_destinations(model: &Model, item: &Path, is_folder: bool) -> Vec<MoveDestination> {
    let current = item.parent().unwrap_or(Path::new(""));
    // Nom que l'élément prendrait dans `dir` s'il y est déjà pris (suffixes
    // de Bruno), d'après les noms de l'arbre chargé.
    let renamed_in = |dir: &Path| {
        let (base, extension) = if is_folder {
            (file_name(item), "")
        } else {
            (file_stem(item), "bru")
        };
        let own = if is_folder {
            file_name(item)
        } else {
            format!("{base}.{extension}")
        };
        let taken: Vec<String> = children_of(model, dir)
            .unwrap_or_default()
            .iter()
            .map(|node| file_name(node.path()))
            .collect();
        first_free_name(dir, &base, extension, |candidate| {
            taken.contains(&file_name(candidate))
        })
        .ok()
        .filter(|name| *name != own)
    };
    fn walk(
        nodes: &[TreeNode],
        depth: usize,
        item: &Path,
        is_folder: bool,
        current: &Path,
        renamed_in: &dyn Fn(&Path) -> Option<String>,
        out: &mut Vec<MoveDestination>,
    ) {
        for node in nodes {
            let TreeNode::Folder(folder) = node else {
                continue;
            };
            if is_folder && folder.path == item {
                continue;
            }
            if folder.path != current {
                out.push(MoveDestination {
                    dir: folder.path.clone(),
                    label: folder.name.clone(),
                    depth,
                    renamed: renamed_in(&folder.path),
                });
            }
            walk(
                &folder.children,
                depth + 1,
                item,
                is_folder,
                current,
                renamed_in,
                out,
            );
        }
    }
    let mut out = Vec::new();
    if !current.as_os_str().is_empty() {
        out.push(MoveDestination {
            dir: PathBuf::new(),
            label: ROOT_LABEL.to_owned(),
            depth: 0,
            renamed: renamed_in(Path::new("")),
        });
    }
    if let Some(collection) = model.loaded() {
        walk(
            &collection.tree,
            0,
            item,
            is_folder,
            current,
            &renamed_in,
            &mut out,
        );
    }
    out
}

/// Messages du popup « Déplacer vers » ouvert ; `None` laisse passer le
/// message au traitement ordinaire (redimensionnement, sortie forcée…).
pub(crate) fn move_popup_message(model: &mut Model, message: &Message) -> Option<Command> {
    let popup = model.move_popup.as_mut()?;
    match message {
        Message::MovePopupKey(key) => popup.key(*key),
        Message::MovePopupNext => popup.next(),
        Message::MovePopupPrevious => popup.previous(),
        // Un déplacement est déjà parti : pas de second envoi.
        Message::MovePopupEnter if popup.pending => {}
        Message::MovePopupEnter => return Some(submit_move(model)),
        Message::MovePopupCancel => model.move_popup = None,
        Message::Resize { .. } | Message::ForceQuit | Message::TerminalClosed(_) => return None,
        _ => {}
    }
    Some(Command::None)
}

/// `Entrée` : monte l'opération sur l'arbre chargé à cet instant. Un refus
/// s'affiche dans le popup, qui reste ouvert.
fn submit_move(model: &mut Model) -> Command {
    let Some(root) = writable_root(model) else {
        model.move_popup = None;
        return Command::None;
    };
    let Some(popup) = model.move_popup.as_ref() else {
        return Command::None;
    };
    let item = popup.item.clone();
    let Some(target_dir) = popup.selected_destination().map(|d| d.dir.clone()) else {
        if let Some(popup) = model.move_popup.as_mut() {
            popup.error = Some("Aucun dossier ne correspond au filtre.".to_owned());
        }
        return Command::None;
    };
    match build_move(model, &item, target_dir) {
        // Le popup reste ouvert jusqu'au résultat : un refus s'y affiche.
        Ok(op) => {
            if let Some(popup) = model.move_popup.as_mut() {
                popup.pending = true;
                popup.error = None;
            }
            Command::FileOp { root, op }
        }
        Err(reason) => {
            if let Some(popup) = model.move_popup.as_mut() {
                popup.error = Some(reason);
            }
            Command::None
        }
    }
}

/// Opération de déplacement : sources chargés de l'élément (fraîcheur) et
/// renumérotation des frères de la source.
fn build_move(model: &Model, item: &Path, target_dir: PathBuf) -> Result<FileOp, String> {
    let parent = item.parent().unwrap_or(Path::new(""));
    let node = children_of(model, parent)
        .and_then(|nodes| nodes.iter().find(|node| node.path() == item))
        .ok_or("Élément introuvable dans l'arbre : rechargement en cours ?")?;
    if !target_dir.as_os_str().is_empty()
        && !children_of(model, target_dir.parent().unwrap_or(Path::new("")))
            .is_some_and(|nodes| nodes.iter().any(|node| node.path() == target_dir))
    {
        return Err("Dossier cible introuvable dans l'arbre".to_owned());
    }
    let mut expected = Vec::new();
    collect_sources(node, &mut expected);
    Ok(FileOp::Move {
        path: item.to_path_buf(),
        target_dir,
        expected,
        renumber: renumber_siblings(model, item),
    })
}

/// Source chargé de chaque requête et `folder.bru` sous `node` ; les
/// fichiers illisibles n'en ont pas.
fn collect_sources(node: &TreeNode, out: &mut Vec<(PathBuf, String)>) {
    match node {
        TreeNode::Request(request) => {
            if let Some(ast) = &request.ast {
                out.push((request.path.clone(), ast.raw().to_owned()));
            }
        }
        TreeNode::Folder(folder) => {
            if let Ok(Some(source)) = folder_source(folder) {
                out.push((folder.path.join(crate::writer::files::FOLDER_FILE), source));
            }
            for child in &folder.children {
                collect_sources(child, out);
            }
        }
        TreeNode::Error(_) => {}
    }
}

/// Retour d'une opération : statut, rechargement, sélection de l'élément.
pub(crate) fn file_op_done(
    model: &mut Model,
    root: &Path,
    result: Result<FileOpOutcome, FileOpError>,
) -> Command {
    // Résultat d'un déplacement demandé depuis le popup : succès, il se
    // ferme ; refus, il reste ouvert avec le message et rien n'a bougé.
    if model.move_popup.as_ref().is_some_and(|popup| popup.pending) {
        match &result {
            Ok(_) => model.move_popup = None,
            Err(error) => {
                if let Some(popup) = model.move_popup.as_mut() {
                    popup.pending = false;
                    popup.error = Some(error.to_string());
                }
                return Command::None;
            }
        }
    }
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
