//! Plusieurs collections ouvertes ensemble (`docs/workspace.md`).
//!
//! Le `Model` garde dans ses champs l'état de la collection **active**,
//! celle de la ligne sélectionnée. Les autres collections attendent dans
//! des emplacements ([`CollectionSlot`]) échangés par `std::mem::swap`
//! quand la sélection change de collection : le reste de l'application ne
//! voit jamais qu'une collection.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::collection::workspace::{self, Workspace, WorkspaceError};
use crate::secrets::SecretMapping;
use crate::writer::FileStamp;

use super::model::{
    CampaignComparison, CampaignSummary, CampaignView, CollectionState, EnvSeries, HistoryEntry,
    Model, RequestOutcome, RunFailure, SecretsState,
};

/// Ce que l'interface ouvre au lancement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launch {
    /// Une collection, ou un fichier ou dossier qu'elle contient.
    Single(PathBuf),
    /// Plusieurs collections : `workspace.yml` ou plusieurs chemins.
    Workspace(Workspace),
}

impl From<PathBuf> for Launch {
    fn from(path: PathBuf) -> Self {
        Self::Single(path)
    }
}

/// Interprète les chemins de la ligne de commande : un dossier contenant
/// `workspace.yml` ouvre le workspace, plusieurs chemins forment un
/// workspace sans nom, sinon une collection comme avant.
pub fn resolve_launch(paths: &[PathBuf]) -> Result<Launch, WorkspaceError> {
    match paths {
        [] => Ok(Launch::Single(PathBuf::from("."))),
        [path] if workspace::is_workspace_dir(path) => {
            workspace::read_workspace(path).map(Launch::Workspace)
        }
        [path] => Ok(Launch::Single(path.clone())),
        several => Ok(Launch::Workspace(workspace::from_paths(several))),
    }
}

/// État du workspace : membres, emplacements des collections inactives et
/// collection active.
#[derive(Debug)]
pub struct WorkspaceState {
    /// `info.name` ; `None` pour un workspace sans nom.
    pub name: Option<String>,
    /// Dossier contenant `workspace.yml` ; `None` sans fichier.
    pub path: Option<PathBuf>,
    pub members: Vec<MemberInfo>,
    /// En-tête de collection déplié, par membre.
    pub open: Vec<bool>,
    /// État de chaque collection. L'emplacement de la collection active
    /// est vide : son état est dans les champs du `Model`.
    pub slots: Vec<CollectionSlot>,
    /// Indice du membre actif.
    pub active: usize,
}

/// Membre du workspace tel que déclaré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    /// `collections[].name`, s'il est renseigné.
    pub name: Option<String>,
    /// Racine déclarée (non canonique).
    pub root: PathBuf,
}

/// État propre à une collection, rangé tant qu'elle n'est pas active.
/// Chaque champ a son homonyme dans le `Model` (voir [`swap_slot`]).
#[derive(Debug)]
pub struct CollectionSlot {
    pub source: PathBuf,
    pub collection: CollectionState,
    pub expanded: HashSet<PathBuf>,
    pub outcomes: HashMap<PathBuf, RequestOutcome>,
    pub last_failure: Option<RunFailure>,
    pub current_environment: Option<String>,
    pub environment_selected: usize,
    /// Secrets saisis et résolus : en mémoire vive seulement, comme dans
    /// le `Model`.
    pub secrets: SecretsState,
    pub history: VecDeque<HistoryEntry>,
    pub history_selected: usize,
    pub diagnostics_selected: usize,
    pub campaign: Option<CampaignSummary>,
    pub campaign_selected: usize,
    pub campaign_view: CampaignView,
    pub comparison_column: usize,
    pub series: Option<EnvSeries>,
    pub comparison: Option<CampaignComparison>,
    pub last_reports: Vec<PathBuf>,
    pub last_saved_stamps: HashMap<PathBuf, FileStamp>,
    pub selected_collection: bool,
    pub pending_reload: bool,
}

impl CollectionSlot {
    /// Emplacement d'une collection pas encore chargée.
    pub fn new(source: PathBuf, mappings: Vec<SecretMapping>) -> Self {
        Self {
            source,
            collection: CollectionState::Loading,
            expanded: HashSet::new(),
            outcomes: HashMap::new(),
            last_failure: None,
            current_environment: None,
            environment_selected: 0,
            secrets: SecretsState {
                mappings,
                ..SecretsState::default()
            },
            history: VecDeque::new(),
            history_selected: 0,
            diagnostics_selected: 0,
            campaign: None,
            campaign_selected: 0,
            campaign_view: CampaignView::List,
            comparison_column: 0,
            series: None,
            comparison: None,
            last_reports: Vec::new(),
            last_saved_stamps: HashMap::new(),
            selected_collection: false,
            pending_reload: false,
        }
    }
}

impl WorkspaceState {
    /// État initial : toutes les collections en chargement, la première
    /// active, repliées sauf s'il n'y en a qu'une.
    pub fn new(workspace: Workspace, mappings: &[SecretMapping]) -> Self {
        let members: Vec<MemberInfo> = workspace
            .members
            .into_iter()
            .map(|member| MemberInfo {
                name: member.name,
                root: member.root,
            })
            .collect();
        let slots = members
            .iter()
            .map(|member| CollectionSlot::new(member.root.clone(), mappings.to_vec()))
            .collect();
        let single = members.len() == 1;
        Self {
            name: workspace.name,
            path: workspace.path,
            open: vec![single; members.len()],
            members,
            slots,
            active: 0,
        }
    }
}

/// Passe le `Model` en mode workspace : l'état du premier membre est placé
/// dans les champs du `Model`.
pub fn install(model: &mut Model, workspace: Workspace) {
    let mut state = WorkspaceState::new(workspace, &model.secrets.mappings);
    if let Some(first) = state.slots.first_mut() {
        swap_slot(model, first);
    }
    model.workspace = Some(state);
    // La sélection part de l'en-tête de la première collection.
    model.selected_collection = true;
}

/// Échange les champs propres à la collection active du `Model` avec un
/// emplacement.
pub fn swap_slot(model: &mut Model, slot: &mut CollectionSlot) {
    use std::mem::swap;
    swap(&mut model.source, &mut slot.source);
    swap(&mut model.collection, &mut slot.collection);
    swap(&mut model.tree.expanded, &mut slot.expanded);
    swap(&mut model.run.outcomes, &mut slot.outcomes);
    swap(&mut model.run.last_failure, &mut slot.last_failure);
    swap(
        &mut model.current_environment,
        &mut slot.current_environment,
    );
    swap(
        &mut model.environment_selected,
        &mut slot.environment_selected,
    );
    swap(&mut model.secrets, &mut slot.secrets);
    swap(&mut model.history, &mut slot.history);
    swap(&mut model.history_selected, &mut slot.history_selected);
    swap(
        &mut model.diagnostics_selected,
        &mut slot.diagnostics_selected,
    );
    swap(&mut model.campaign, &mut slot.campaign);
    swap(&mut model.campaign_selected, &mut slot.campaign_selected);
    swap(&mut model.campaign_view, &mut slot.campaign_view);
    swap(&mut model.comparison_column, &mut slot.comparison_column);
    swap(&mut model.series, &mut slot.series);
    swap(&mut model.comparison, &mut slot.comparison);
    swap(&mut model.last_reports, &mut slot.last_reports);
    swap(&mut model.last_saved_stamps, &mut slot.last_saved_stamps);
    swap(
        &mut model.selected_collection,
        &mut slot.selected_collection,
    );
    swap(&mut model.pending_reload, &mut slot.pending_reload);
}

/// Rend `index` membre actif : l'état de l'ancien membre actif est rangé
/// dans son emplacement, celui du nouveau passe dans le `Model`. Ne touche
/// ni aux lignes ni à la sélection de l'arbre. Sans effet hors workspace,
/// pour un indice hors limites ou déjà actif.
pub fn swap_active(model: &mut Model, index: usize) {
    let Some(mut state) = model.workspace.take() else {
        return;
    };
    let old = state.active;
    if index != old && index < state.slots.len() {
        // L'emplacement de l'ancien actif est vide : il reçoit l'état du
        // `Model`, qui reçoit l'emplacement vide, échangé à son tour avec
        // l'état du nouveau membre.
        swap_slot(model, &mut state.slots[old]);
        swap_slot(model, &mut state.slots[index]);
        state.active = index;
    }
    model.workspace = Some(state);
}

/// Change de collection active à la demande de l'utilisateur : le filtre
/// de l'arbre, propre à la collection quittée, est levé avant l'échange.
pub fn activate(model: &mut Model, index: usize) {
    if active_member(model) == Some(index) {
        return;
    }
    if let Some(filter) = model.tree_filter.take() {
        model.tree.expanded = filter.saved_expanded;
    }
    swap_active(model, index);
}

/// Déplie l'en-tête de la collection active, avant de sélectionner un de
/// ses nœuds (recherche, diagnostics, historique...).
pub fn open_active(model: &mut Model) {
    if let Some(state) = model.workspace.as_mut()
        && let Some(open) = state.open.get_mut(state.active)
    {
        *open = true;
    }
}

/// Membre actif, `None` hors workspace.
pub fn active_member(model: &Model) -> Option<usize> {
    model.workspace.as_ref().map(|state| state.active)
}

/// Vrai si une exécution (ou une série d'exécutions) est en cours : la
/// sélection ne peut alors pas quitter la collection active.
pub fn run_in_progress(model: &Model) -> bool {
    model.run.active.is_some() || model.series.is_some()
}

/// État de chargement d'un membre, qu'il soit actif ou rangé.
pub fn member_collection(model: &Model, index: usize) -> Option<&CollectionState> {
    let state = model.workspace.as_ref()?;
    if index == state.active {
        Some(&model.collection)
    } else {
        state.slots.get(index).map(|slot| &slot.collection)
    }
}

/// Dossiers dépliés d'un membre.
pub fn member_expanded(model: &Model, index: usize) -> Option<&HashSet<PathBuf>> {
    let state = model.workspace.as_ref()?;
    if index == state.active {
        Some(&model.tree.expanded)
    } else {
        state.slots.get(index).map(|slot| &slot.expanded)
    }
}

/// Derniers résultats d'un membre.
pub fn member_outcomes(model: &Model, index: usize) -> Option<&HashMap<PathBuf, RequestOutcome>> {
    let state = model.workspace.as_ref()?;
    if index == state.active {
        Some(&model.run.outcomes)
    } else {
        state.slots.get(index).map(|slot| &slot.outcomes)
    }
}

/// Nom affiché d'un membre : `collections[].name`, sinon le nom de la
/// collection chargée, sinon le dernier composant du chemin.
pub fn member_name(model: &Model, index: usize) -> String {
    let Some(state) = model.workspace.as_ref() else {
        return String::new();
    };
    let Some(member) = state.members.get(index) else {
        return String::new();
    };
    if let Some(name) = &member.name {
        return name.clone();
    }
    if let Some(CollectionState::Loaded(collection)) = member_collection(model, index) {
        return collection.name.clone();
    }
    member
        .root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| member.root.display().to_string())
}

/// Membre dont la collection chargée a cette racine.
pub fn member_of_root(model: &Model, root: &Path) -> Option<usize> {
    let state = model.workspace.as_ref()?;
    (0..state.members.len()).find(|&index| {
        matches!(
            member_collection(model, index),
            Some(CollectionState::Loaded(collection)) if collection.root == root
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    #[test]
    fn one_collection_is_unchanged() {
        let path = fixtures().join("collections/runner-probe");
        assert_eq!(
            resolve_launch(std::slice::from_ref(&path)).expect("valide"),
            Launch::Single(path)
        );
        assert_eq!(
            resolve_launch(&[]).expect("valide"),
            Launch::Single(".".into())
        );
    }

    #[test]
    fn workspace_directory_and_several_paths() {
        let dir = fixtures().join("workspaces/demo");
        let Launch::Workspace(ws) = resolve_launch(&[dir]).expect("valide") else {
            panic!("workspace attendu");
        };
        assert_eq!(ws.name.as_deref(), Some("Demo"));
        assert_eq!(ws.members.len(), 4);

        let a = fixtures().join("collections/runner-probe");
        let b = fixtures().join("collections/multi-env-probe");
        let Launch::Workspace(ws) = resolve_launch(&[a.clone(), b]).expect("valide") else {
            panic!("workspace attendu");
        };
        assert_eq!(ws.name, None);
        assert_eq!(ws.path, None);
        assert_eq!(ws.members[0].root, a);
    }

    fn loaded_name(model: &Model) -> String {
        model.loaded().map(|c| c.name.clone()).unwrap_or_default()
    }

    #[test]
    fn install_puts_the_first_member_in_the_model() {
        let model = crate::app::test_support::workspace_model();
        let state = model.workspace.as_ref().expect("workspace");
        assert_eq!(state.active, 0);
        assert_eq!(state.open, [false, false]);
        assert_eq!(member_name(&model, 1), "multi-env-probe");
    }

    #[test]
    fn each_collection_keeps_its_own_state() {
        use crate::runner::SecretString;
        let mut model = crate::app::test_support::workspace_model();
        let first = loaded_name(&model);
        model.current_environment = Some("local".into());
        model
            .secrets
            .typed
            .push(("token".into(), SecretString::new("valeur-a")));
        model.tree.expanded.insert("folder".into());
        model.history_enabled = true;

        activate(&mut model, 1);
        assert_eq!(active_member(&model), Some(1));
        assert_ne!(loaded_name(&model), first);
        assert_eq!(model.current_environment, None);
        assert!(model.secrets.typed.is_empty());
        assert!(model.tree.expanded.is_empty());
        assert_eq!(model.source, fixtures().join("collections/multi-env-probe"));
        model.current_environment = Some("up".into());
        model
            .secrets
            .typed
            .push(("token".into(), SecretString::new("valeur-b")));

        activate(&mut model, 0);
        assert_eq!(loaded_name(&model), first);
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        assert_eq!(model.secrets.typed[0].1.expose(), "valeur-a");
        assert!(model.tree.expanded.contains(Path::new("folder")));
        assert!(model.history_enabled, "réglage de session, non échangé");

        activate(&mut model, 1);
        assert_eq!(model.current_environment.as_deref(), Some("up"));
        assert_eq!(model.secrets.typed[0].1.expose(), "valeur-b");
    }

    #[test]
    fn activation_out_of_range_or_outside_a_workspace_is_ignored() {
        let mut model = crate::app::test_support::workspace_model();
        activate(&mut model, 7);
        assert_eq!(active_member(&model), Some(0));
        let mut single = Model::new(".".into(), (100, 30));
        activate(&mut single, 1);
        assert_eq!(active_member(&single), None);
    }

    #[test]
    fn member_lookups_follow_the_active_member() {
        let mut model = crate::app::test_support::workspace_model();
        let root_b = match member_collection(&model, 1) {
            Some(CollectionState::Loaded(c)) => c.root.clone(),
            _ => panic!("chargée"),
        };
        assert_eq!(member_of_root(&model, &root_b), Some(1));
        activate(&mut model, 1);
        assert_eq!(member_of_root(&model, &root_b), Some(1));
        assert!(member_expanded(&model, 0).is_some());
        assert!(member_outcomes(&model, 2).is_none());
    }

    #[test]
    fn run_in_progress_covers_series() {
        let mut model = crate::app::test_support::workspace_model();
        assert!(!run_in_progress(&model));
        model.series = Some(EnvSeries::new(PathBuf::new(), vec!["up".into()]));
        assert!(run_in_progress(&model));
    }

    #[test]
    fn invalid_workspace_file_is_an_error() {
        let dir = std::env::temp_dir().join(format!("bruno-tui-ws-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dossier");
        std::fs::write(dir.join("workspace.yml"), "collections: [\n  : x").expect("écriture");
        let error = resolve_launch(std::slice::from_ref(&dir)).expect_err("YAML invalide");
        assert!(matches!(error, WorkspaceError::InvalidYaml { .. }));
        std::fs::remove_dir_all(&dir).expect("nettoyage");
    }
}

/// Arbre multi-collection : en-têtes, navigation, changement de collection
/// active et rendu.
#[cfg(test)]
mod tree_tests {
    use super::*;
    use crate::app::message::Message;
    use crate::app::model::{ActiveRun, StatusMessage};
    use crate::app::test_support::{render, selected_name, workspace_model};
    use crate::app::update::update;
    use crate::runner::RunId;

    fn names(model: &Model) -> Vec<String> {
        model
            .tree
            .rows
            .iter()
            .map(|row| match model.row_node(row) {
                Some(node) => format!(
                    "{}{}",
                    "  ".repeat(row.depth),
                    crate::app::view::tree::display_name(node)
                ),
                None => format!("# {}", member_name(model, row.member)),
            })
            .collect()
    }

    fn loaded_name(model: &Model) -> String {
        model.loaded().map(|c| c.name.clone()).unwrap_or_default()
    }

    #[test]
    fn headers_are_folded_at_start_and_unfold_with_right() {
        let mut model = workspace_model();
        assert_eq!(names(&model), ["# runner-probe", "# multi-env-probe"]);
        assert!(
            model.selected_collection,
            "en-tête = collection sélectionnée"
        );
        update(&mut model, Message::Right);
        let rows = names(&model);
        assert_eq!(rows[0], "# runner-probe");
        assert!(rows[1].starts_with("  "), "nœuds indentés : {rows:?}");
        assert_eq!(rows.last().map(String::as_str), Some("# multi-env-probe"));
        update(&mut model, Message::Right);
        assert_eq!(model.tree.selected, 1, "→ sur un en-tête déplié entre");
        assert!(!model.selected_collection);
        update(&mut model, Message::Left);
        update(&mut model, Message::Left);
        assert_eq!(model.tree.selected, 0, "← remonte à l'en-tête");
        update(&mut model, Message::Left);
        assert_eq!(names(&model), ["# runner-probe", "# multi-env-probe"]);
    }

    #[test]
    fn moving_to_another_collection_activates_it() {
        let mut model = workspace_model();
        let first = loaded_name(&model);
        model.current_environment = Some("local".into());
        update(&mut model, Message::Down);
        assert_eq!(active_member(&model), Some(1));
        assert_ne!(loaded_name(&model), first);
        assert_eq!(model.current_environment, None);
        assert!(model.selected_collection);
        update(&mut model, Message::Right);
        update(&mut model, Message::Down);
        assert_eq!(selected_name(&model), "data");
        update(&mut model, Message::Home);
        assert_eq!(active_member(&model), Some(0));
        assert_eq!(model.current_environment.as_deref(), Some("local"));
        // Les deux collections restent dépliées ensemble.
        update(&mut model, Message::Right);
        let rows = names(&model);
        assert!(rows.contains(&"  data".to_owned()), "{rows:?}");
        assert!(rows.contains(&"  ok".to_owned()), "{rows:?}");
    }

    #[test]
    fn selection_cannot_leave_the_collection_during_a_run() {
        let mut model = workspace_model();
        model.run.active = Some(ActiveRun {
            id: RunId(1),
            target: "ok.bru".into(),
            recursive: false,
            is_campaign: false,
            handle: None,
        });
        update(&mut model, Message::Down);
        assert_eq!(active_member(&model), Some(0));
        assert_eq!(model.tree.selected, 0);
        assert!(matches!(model.last_status, Some(StatusMessage::RunLocked)));
        update(&mut model, Message::End);
        assert_eq!(active_member(&model), Some(0));
        model.run.active = None;
        update(&mut model, Message::End);
        assert_eq!(active_member(&model), Some(1));
    }

    #[test]
    fn tree_panel_shows_headers_and_workspace_title() {
        let mut model = workspace_model();
        update(&mut model, Message::Right);
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Collections"), "{screen}");
        assert!(screen.contains("▾ runner-probe"), "{screen}");
        assert!(screen.contains("▸ multi-env-probe"), "{screen}");
        if let Some(state) = model.workspace.as_mut() {
            state.name = Some("Demo".into());
        }
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("Workspace : Demo"), "{screen}");
    }

    #[test]
    fn failed_member_shows_an_error_line() {
        let mut model = workspace_model();
        if let Some(state) = model.workspace.as_mut() {
            state.slots[1].collection =
                CollectionState::Failed(crate::collection::LoadError::NotACollectionRoot {
                    path: "/absent".into(),
                });
        }
        let screen = render(&model, 100, 30).join("\n");
        assert!(screen.contains("✗ multi-env-probe"), "{screen}");
        update(&mut model, Message::Down);
        assert_eq!(active_member(&model), Some(1));
        update(&mut model, Message::Right);
        assert_eq!(model.tree.rows.len(), 2, "rien à déplier");
    }

    #[test]
    fn search_opens_the_active_collection() {
        let mut model = workspace_model();
        crate::app::test_support::select(&mut model, "folder/down.bru");
        assert_eq!(selected_name(&model), "down");
        assert_eq!(active_member(&model), Some(0));
    }
}
