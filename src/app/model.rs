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

use crate::collection::{
    BodyContent, BodyKind, Collection, KeyValue, LoadError, RequestView, TreeNode,
};
use crate::runner::{
    self, RunnerOptionsConfig, RunnerOptionsError, SandboxMode, SecretString, default_reports_dir,
};
use crate::secrets::{Resolved, SecretLookup, SecretMapping, SecretSource, is_valid_name};
use crate::writer::{EntrySection, EnvironmentVarEdit, FieldEdit, FileStamp};

use super::filter::FilterState;
use super::global_search::GlobalSearchPopup;
use super::message::TextCapture;
use super::search::SearchState;
use super::text_input::TextInput;
use super::tree_filter::TreeFilterState;

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
    Response,
    Diagnostics,
    History,
    EnvironmentPicker,
    Secrets,
    Campaign,
}

/// Onglet actif du panneau Réponse (`response-tabs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseTab {
    #[default]
    Body,
    Headers,
    Tests,
    Timeline,
    Console,
}

impl ResponseTab {
    pub fn next(self) -> Self {
        match self {
            ResponseTab::Body => ResponseTab::Headers,
            ResponseTab::Headers => ResponseTab::Tests,
            ResponseTab::Tests => ResponseTab::Timeline,
            ResponseTab::Timeline => ResponseTab::Console,
            ResponseTab::Console => ResponseTab::Body,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            ResponseTab::Body => ResponseTab::Console,
            ResponseTab::Headers => ResponseTab::Body,
            ResponseTab::Tests => ResponseTab::Headers,
            ResponseTab::Timeline => ResponseTab::Tests,
            ResponseTab::Console => ResponseTab::Timeline,
        }
    }
}

/// Onglet affiché du détail d'une requête, comme les onglets de requête
/// de Bruno bureau. Pendant une session d'édition, l'onglet affiché suit
/// le champ sous le curseur ([`Model::shown_request_tab`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestTab {
    #[default]
    Params,
    Body,
    Headers,
    Auth,
    Vars,
    Script,
    Assert,
    Tests,
    Docs,
    Settings,
}

impl RequestTab {
    /// Tous les onglets, dans l'ordre d'affichage.
    pub const ALL: [RequestTab; 10] = [
        RequestTab::Params,
        RequestTab::Body,
        RequestTab::Headers,
        RequestTab::Auth,
        RequestTab::Vars,
        RequestTab::Script,
        RequestTab::Assert,
        RequestTab::Tests,
        RequestTab::Docs,
        RequestTab::Settings,
    ];

    /// Libellé de l'onglet.
    pub fn label(self) -> &'static str {
        match self {
            RequestTab::Params => "Params",
            RequestTab::Body => "Corps",
            RequestTab::Headers => "En-têtes",
            RequestTab::Auth => "Auth",
            RequestTab::Vars => "Vars",
            RequestTab::Script => "Script",
            RequestTab::Assert => "Assert",
            RequestTab::Tests => "Tests",
            RequestTab::Docs => "Docs",
            RequestTab::Settings => "Settings",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Onglet qui affiche `field` ; `None` pour la méthode et l'URL,
    /// affichées en tête du détail quel que soit l'onglet.
    pub fn of_field(field: EditableField) -> Option<Self> {
        match field {
            EditableField::Method | EditableField::Url => None,
            EditableField::HeaderValue(_) | EditableField::AddRow(EntrySection::Headers) => {
                Some(RequestTab::Headers)
            }
            EditableField::QueryParamValue(_)
            | EditableField::PathParamValue(_)
            | EditableField::AddRow(EntrySection::QueryParams | EntrySection::PathParams) => {
                Some(RequestTab::Params)
            }
            EditableField::BodyText => Some(RequestTab::Body),
        }
    }
}

/// État du popup d'aide des raccourcis clavier (`help-popup`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HelpPopup {
    /// Ligne de défilement du contenu du popup d'aide.
    pub scroll: u16,
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

/// Statut d'une requête au sein d'une campagne de tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampaignItemStatus {
    Passed,
    Failed,
    Skipped,
}

/// Filtre actif sur la liste des requêtes de la campagne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CampaignFilter {
    #[default]
    Failed,
    Passed,
    Skipped,
    All,
}

impl CampaignFilter {
    /// Fait tourner le filtre : Échecs -> Réussis -> Ignorés -> Tous -> Échecs.
    pub fn next(self) -> Self {
        match self {
            Self::Failed => Self::Passed,
            Self::Passed => Self::Skipped,
            Self::Skipped => Self::All,
            Self::All => Self::Failed,
        }
    }

    /// Libellé affiché dans l'en-tête du panneau.
    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Tous",
            Self::Failed => "Échecs",
            Self::Passed => "Réussis",
            Self::Skipped => "Ignorés",
        }
    }
}

/// Une requête exécutée au sein d'une campagne.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignItem {
    /// Chemin du fichier de la requête (relatif à la racine de la collection).
    pub path: PathBuf,
    /// Nom déclaré de la requête.
    pub name: String,
    /// Statut du verdict.
    pub status: CampaignItemStatus,
    /// Code HTTP s'il existe.
    pub http_code: Option<u16>,
    /// Durée d'exécution en secondes.
    pub duration_secs: f64,
    /// Raison d'échec ou de saut si présente.
    pub reason: Option<String>,
    /// Index de l'itération (0-indexed).
    pub iteration_index: u32,
}

/// Résumé de la dernière campagne d'exécution récursive de TNR.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignSummary {
    /// Dossier cible de la campagne.
    pub target: PathBuf,
    /// Nombre total de requêtes exécutées.
    pub total: usize,
    /// Nombre de requêtes réussies.
    pub passed: usize,
    /// Nombre de requêtes en échec.
    pub failed: usize,
    /// Nombre de requêtes ignorées (skipped).
    pub skipped: usize,
    /// Durée totale d'exécution en secondes.
    pub duration_secs: f64,
    /// Toutes les requêtes exécutées dans la campagne.
    pub items: Vec<CampaignItem>,
    /// Filtre de statut actif.
    pub filter: CampaignFilter,
    /// Liste ordonnée des requêtes en échec.
    pub failures: Vec<CampaignFailure>,
    /// Nombre total d'itérations de la campagne.
    pub total_iterations: usize,
}

impl CampaignSummary {
    /// Renvoie les requêtes correspondant au filtre actif.
    pub fn filtered_items(&self) -> Vec<CampaignItem> {
        if self.items.is_empty() && !self.failures.is_empty() {
            let legacy_items: Vec<CampaignItem> = self
                .failures
                .iter()
                .map(|f| CampaignItem {
                    path: f.path.clone(),
                    name: f.name.clone(),
                    status: CampaignItemStatus::Failed,
                    http_code: f.http_code,
                    duration_secs: 0.0,
                    reason: Some(f.reason.clone()),
                    iteration_index: 0,
                })
                .collect();
            return match self.filter {
                CampaignFilter::All | CampaignFilter::Failed => legacy_items,
                CampaignFilter::Passed | CampaignFilter::Skipped => Vec::new(),
            };
        }

        self.items
            .iter()
            .filter(|item| match self.filter {
                CampaignFilter::All => true,
                CampaignFilter::Failed => item.status == CampaignItemStatus::Failed,
                CampaignFilter::Passed => item.status == CampaignItemStatus::Passed,
                CampaignFilter::Skipped => item.status == CampaignItemStatus::Skipped,
            })
            .cloned()
            .collect()
    }
}

/// Vue du panneau Campagne : la liste des requêtes ou le tableau comparatif
/// d'une campagne multi-environnements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CampaignView {
    #[default]
    List,
    Comparison,
}

/// Verdict d'une requête pour un environnement, toutes itérations
/// confondues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonCell {
    Passed,
    Failed,
    Skipped,
    /// Requête absente du rapport de cet environnement.
    Absent,
}

impl ComparisonCell {
    /// Symbole affiché dans le tableau.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Passed => "✓",
            Self::Failed => "✗",
            Self::Skipped => "–",
            Self::Absent => "·",
        }
    }

    /// Verdict d'un résultat, même règle que la liste de la campagne.
    fn of(result: &runner::report::RequestResult) -> Self {
        if result.is_failure() {
            Self::Failed
        } else if result.skipped || result.status == runner::report::ResultStatus::Skipped {
            Self::Skipped
        } else {
            Self::Passed
        }
    }

    /// Cumul de deux itérations : un échec l'emporte, puis une réussite.
    fn merge(self, other: Self) -> Self {
        use ComparisonCell::*;
        match (self, other) {
            (Failed, _) | (_, Failed) => Failed,
            (Passed, _) | (_, Passed) => Passed,
            (Skipped, _) | (_, Skipped) => Skipped,
            (Absent, Absent) => Absent,
        }
    }
}

/// Ligne du tableau comparatif : une requête, une cellule par environnement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonRow {
    /// Chemin de la requête, relatif à la racine (`test.filename`).
    pub path: PathBuf,
    pub name: String,
    pub cells: Vec<ComparisonCell>,
}

impl ComparisonRow {
    /// Vrai si le verdict diffère entre au moins deux environnements.
    pub fn diverges(&self) -> bool {
        self.cells.windows(2).any(|pair| pair[0] != pair[1])
    }
}

/// Filtre des lignes du tableau comparatif (touche `f`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComparisonFilter {
    #[default]
    All,
    Divergent,
    Failed,
    Passed,
    Skipped,
}

impl ComparisonFilter {
    /// Tous -> Divergences -> Échecs -> Réussis -> Ignorés -> Tous.
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Divergent,
            Self::Divergent => Self::Failed,
            Self::Failed => Self::Passed,
            Self::Passed => Self::Skipped,
            Self::Skipped => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Tous",
            Self::Divergent => "Divergences",
            Self::Failed => "Échecs",
            Self::Passed => "Réussis",
            Self::Skipped => "Ignorés",
        }
    }

    /// Vrai si la ligne passe le filtre : divergente, ou ayant au moins une
    /// cellule du verdict filtré.
    pub fn accepts(self, row: &ComparisonRow) -> bool {
        let any = |cell: ComparisonCell| row.cells.contains(&cell);
        match self {
            Self::All => true,
            Self::Divergent => row.diverges(),
            Self::Failed => any(ComparisonCell::Failed),
            Self::Passed => any(ComparisonCell::Passed),
            Self::Skipped => any(ComparisonCell::Skipped),
        }
    }
}

/// Tableau comparatif de la dernière série multi-environnements. Les
/// résultats restent en mémoire, jamais sur disque.
#[derive(Debug, Clone)]
pub struct CampaignComparison {
    /// Cible commune de la série.
    pub target: PathBuf,
    /// Colonnes, dans l'ordre d'exécution.
    pub environments: Vec<String>,
    /// Erreur d'exécution d'un environnement sans rapport, par colonne.
    pub errors: Vec<Option<String>>,
    /// Lignes, dans l'ordre de première apparition dans les rapports.
    pub rows: Vec<ComparisonRow>,
    /// Résultat de chaque requête, par colonne, pour le panneau Réponse.
    pub outcomes: Vec<HashMap<PathBuf, RequestOutcome>>,
    pub filter: ComparisonFilter,
}

impl CampaignComparison {
    /// Construit le tableau à partir des issues d'une série.
    pub fn build(target: PathBuf, runs: Vec<EnvRun>) -> Self {
        let columns = runs.len();
        let mut environments = Vec::with_capacity(columns);
        let mut errors = Vec::with_capacity(columns);
        let mut outcomes = Vec::with_capacity(columns);
        let mut rows: Vec<ComparisonRow> = Vec::new();
        let mut row_index: HashMap<PathBuf, usize> = HashMap::new();
        for (column, run) in runs.into_iter().enumerate() {
            let mut env_outcomes = HashMap::new();
            match run.outcome {
                EnvRunOutcome::Completed {
                    report,
                    exit_code,
                    console,
                } => {
                    errors.push(None);
                    let total_iterations = report.iterations().len() as u32;
                    for iteration in report.0 {
                        for result in iteration.results {
                            let path = PathBuf::from(&result.test.filename);
                            let index = *row_index.entry(path.clone()).or_insert_with(|| {
                                rows.push(ComparisonRow {
                                    path: path.clone(),
                                    name: result.name.clone(),
                                    cells: vec![ComparisonCell::Absent; columns],
                                });
                                rows.len() - 1
                            });
                            let cell = &mut rows[index].cells[column];
                            *cell = cell.merge(ComparisonCell::of(&result));
                            // Dernière itération affichée, comme `RunState::outcomes`.
                            env_outcomes.insert(
                                path,
                                RequestOutcome::with_console(
                                    result,
                                    exit_code,
                                    std::sync::Arc::clone(&console),
                                    true,
                                )
                                .with_iteration(iteration.iteration_index + 1, total_iterations)
                                .with_environment(Some(run.environment.clone())),
                            );
                        }
                    }
                }
                EnvRunOutcome::Failed(error) => errors.push(Some(error)),
            }
            environments.push(run.environment);
            outcomes.push(env_outcomes);
        }
        Self {
            target,
            environments,
            errors,
            rows,
            outcomes,
            filter: ComparisonFilter::default(),
        }
    }

    /// Lignes passant le filtre actif.
    pub fn filtered_rows(&self) -> Vec<&ComparisonRow> {
        self.rows
            .iter()
            .filter(|row| self.filter.accepts(row))
            .collect()
    }

    /// Nombre de requêtes dont le verdict diffère entre environnements.
    pub fn divergent_count(&self) -> usize {
        self.rows.iter().filter(|row| row.diverges()).count()
    }
}

/// Information sur une requête en échec lors d'une campagne.
#[derive(Debug, Clone, PartialEq)]
pub struct CampaignFailure {
    /// Chemin du fichier de la requête (relatif à la racine de la collection).
    pub path: PathBuf,
    /// Nom déclaré de la requête.
    pub name: String,
    /// Code de réponse HTTP s'il existe.
    pub http_code: Option<u16>,
    /// Première raison d'échec constatée.
    pub reason: String,
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
    /// `RequestResult::test.filename` (avec l'extension `.bru` ou `.yml`) — aucune
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
    /// Indique s'il s'agit d'une campagne de tests (récursive ou relance des échecs).
    pub is_campaign: bool,
    /// `None` juste après une annulation déjà demandée : un second appui sur
    /// la touche d'annulation reste sans effet (idempotent).
    pub handle: Option<runner::RunHandle>,
}

/// Résultat conservé pour une requête, issu d'un rapport valide — que le
/// verdict de la requête soit un succès ou un échec.
#[derive(Debug, Clone)]
pub struct RequestOutcome {
    pub result: runner::report::RequestResult,
    pub exit_code: Option<i32>,
    /// Sortie stdout + stderr de `bru`, débarrassée des codes ANSI.
    /// Conservée en mémoire vive uniquement (jamais sur disque ni dans l'historique).
    /// Partagée entre toutes les requêtes d'une même exécution : une seule
    /// copie, même pour une campagne de plusieurs centaines de requêtes.
    pub console: std::sync::Arc<str>,
    /// Indique si la console provient d'une exécution récursive (globale à la campagne).
    pub is_global_console: bool,
    /// Mention d'itération (N, M) quand il y a plusieurs itérations.
    pub iteration: Option<(u32, u32)>,
    /// Environnement de l'exécution, renseigné pour une série
    /// multi-environnements (affiché sur la bordure du panneau Réponse).
    pub environment: Option<String>,
}

impl RequestOutcome {
    pub fn new(result: runner::report::RequestResult, exit_code: Option<i32>) -> Self {
        Self {
            result,
            exit_code,
            console: std::sync::Arc::from(""),
            is_global_console: false,
            iteration: None,
            environment: None,
        }
    }

    pub fn with_console(
        result: runner::report::RequestResult,
        exit_code: Option<i32>,
        console: std::sync::Arc<str>,
        is_global_console: bool,
    ) -> Self {
        Self {
            result,
            exit_code,
            console,
            is_global_console,
            iteration: None,
            environment: None,
        }
    }

    pub fn with_environment(mut self, environment: Option<String>) -> Self {
        self.environment = environment;
        self
    }

    pub fn with_iteration(mut self, current: u32, total: u32) -> Self {
        if total > 1 {
            self.iteration = Some((current, total));
        }
        self
    }
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

/// Sélection visuelle de lignes dans le détail ou la réponse. Au clavier,
/// ancrée au sommet du panneau au moment de son activation, la borne
/// mobile se déduisant du défilement courant (`update::selection_range`).
/// À la souris, les deux bornes sont fixes (`mouse-support`, D6).
#[derive(Debug, Clone, Copy)]
pub struct DetailSelection {
    pub anchor: u16,
    /// Borne fixe posée par un glisser ; `None` pour une sélection clavier.
    pub head: Option<u16>,
}

/// Panneau dans lequel un glisser a commencé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragPanel {
    Detail,
    Response,
}

/// Appui du bouton gauche en cours dans le détail ou la réponse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drag {
    pub panel: DragPanel,
    /// Ligne logique sous l'appui.
    pub anchor: u16,
    /// Le pointeur a atteint une autre ligne : ce n'est plus un clic.
    pub moved: bool,
}

/// État souris (`mouse-support`, D5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MouseState {
    /// Capture effectivement appliquée au terminal.
    pub capture: bool,
    pub drag: Option<Drag>,
    /// Horodatage et indice de la dernière entrée cliquée dans le panneau
    /// Environnement, pour détecter un double-clic
    /// (`add-environment-panel-and-edit-popup`, design D4).
    pub last_environment_click: Option<(std::time::Instant, usize)>,
}

/// Session d'édition d'une requête, superposée au focus Détail.
#[derive(Debug, Clone)]
pub struct EditSession {
    /// Chemin de la requête éditée, relatif à la racine ; la session se
    /// ferme sans confirmation si la sélection change de nœud alors
    /// qu'elle ne porte aucune modification non enregistrée.
    pub path: PathBuf,
    pub stamp: FileStamp,
    /// Positions du curseur de champ, dans l'ordre d'affichage, calculées
    /// depuis `preview`.
    pub fields: Vec<EditableField>,
    /// Indice du champ sous le curseur, dans `fields`.
    pub cursor: usize,
    pub state: EditState,
    /// Cible de la saisie en cours ; significative seulement en `Input`
    /// (`add-entry-management`, D7).
    pub target: InputTarget,
    /// Journal ordonné des modifications validées, appliqué tel quel par
    /// `bru-writer` : un indice y désigne la position courante après les
    /// modifications précédentes.
    pub pending: Vec<FieldEdit>,
    /// Vue de la requête après application de `pending`, telle que
    /// `bru-writer` l'écrirait (`writer::preview`).
    pub preview: RequestView,
    /// Au moins une modification validée depuis la dernière sauvegarde.
    pub dirty: bool,
    /// Décalage horizontal, en colonnes d'affichage, de la valeur du champ
    /// en saisie (`improve-direct-editing`, D7).
    pub hscroll: u16,
}

impl EditSession {
    /// Ouvre une session propre sur une requête chargée.
    pub fn new(path: PathBuf, stamp: FileStamp, view: RequestView) -> Self {
        Self {
            path,
            stamp,
            fields: EditableField::list_for(&view),
            cursor: 0,
            state: EditState::FieldSelect,
            target: InputTarget::Field,
            pending: Vec::new(),
            preview: view,
            dirty: false,
            hscroll: 0,
        }
    }

    /// Modifications non enregistrées : validées, ou saisie en cours dont
    /// le texte diffère de la valeur au début de la saisie. La saisie de la
    /// valeur d'une nouvelle entrée compte toujours : sa clé est déjà
    /// validée.
    pub fn has_unsaved(&self) -> bool {
        self.dirty
            || match &self.state {
                EditState::FieldSelect | EditState::MethodPicker { .. } => false,
                EditState::Input(input) => {
                    input.is_modified() || matches!(self.target, InputTarget::NewValue { .. })
                }
            }
    }

    /// Champ sous le curseur.
    pub fn current_field(&self) -> Option<EditableField> {
        self.fields.get(self.cursor).copied()
    }

    /// Remplace l'aperçu et recalcule les positions du curseur, bornant le
    /// curseur à la dernière.
    pub fn set_preview(&mut self, view: RequestView) {
        self.fields = EditableField::list_for(&view);
        self.preview = view;
        self.cursor = self.cursor.min(self.fields.len().saturating_sub(1));
    }
}

/// État d'une session d'édition d'environnement : sélection de la
/// variable, saisie de sa valeur, ou ajout d'une variable en deux temps
/// (`add-environment-entry-management`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentEditState {
    Select,
    Input(TextInput),
    /// Ajout d'une variable : saisie de la clé.
    AddingKey(TextInput),
    /// Ajout d'une variable : clé déjà validée, saisie de la valeur.
    AddingValue {
        key: String,
        input: TextInput,
    },
}

impl EnvironmentEditState {
    /// Le `TextInput` porté par cet état, s'il y en a un — couvre les
    /// trois variantes de saisie de texte, pour ne pas tripler le même
    /// branchement dans `update.rs` (design D3).
    pub fn text_input_mut(&mut self) -> Option<&mut TextInput> {
        match self {
            EnvironmentEditState::Input(input)
            | EnvironmentEditState::AddingKey(input)
            | EnvironmentEditState::AddingValue { input, .. } => Some(input),
            EnvironmentEditState::Select => None,
        }
    }
}

/// Session d'édition des variables d'un environnement.
#[derive(Debug, Clone)]
pub struct EnvironmentEditSession {
    /// Nom de l'environnement (ex: "local", "staging").
    pub name: String,
    /// Chemin du fichier `.bru` de l'environnement, relatif à la racine de la collection.
    pub path: PathBuf,
    pub stamp: FileStamp,
    /// Variables en mémoire, dans l'ordre du fichier.
    pub variables: Vec<KeyValue>,
    /// Indice de la variable sous le curseur, dans `variables`.
    pub cursor: usize,
    pub state: EnvironmentEditState,
    /// Au moins une variable a été modifiée par rapport à l'état initial.
    pub dirty: bool,
    /// Journal ordonné des modifications validées (valeur, ajout,
    /// suppression), appliqué tel quel par `bru-writer` : un indice y
    /// désigne la position dans `variables` telle qu'elle résulte des
    /// modifications précédentes de la même liste — même principe que
    /// `EditSession.pending` pour une requête
    /// (`add-environment-entry-management`, design D3).
    pub pending: Vec<EnvironmentVarEdit>,
}

impl EnvironmentEditSession {
    /// Ouvre une session sur un environnement chargé.
    pub fn new(name: String, path: PathBuf, stamp: FileStamp, variables: Vec<KeyValue>) -> Self {
        Self {
            name,
            path,
            stamp,
            variables,
            cursor: 0,
            state: EnvironmentEditState::Select,
            dirty: false,
            pending: Vec::new(),
        }
    }

    /// Variable actuellement sous le curseur.
    pub fn current_variable(&self) -> Option<&KeyValue> {
        self.variables.get(self.cursor)
    }

    /// Démarre la saisie de la valeur de la variable sélectionnée si elle est activée.
    /// Retourne `true` si la saisie démarre, `false` si la variable est désactivée ou inexistante.
    pub fn begin_input(&mut self) -> bool {
        let Some(var) = self.variables.get(self.cursor) else {
            return false;
        };
        if !var.enabled {
            return false;
        }
        self.state = EnvironmentEditState::Input(TextInput::new(&var.value, false));
        true
    }

    /// Valide la saisie en cours : met à jour la valeur en mémoire et marque
    /// la session modifiée si la valeur a changé.
    pub fn commit_input(&mut self) -> bool {
        let EnvironmentEditState::Input(input) = &self.state else {
            return false;
        };
        let text = input.text().to_owned();
        if let Some(var) = self.variables.get_mut(self.cursor)
            && var.value != text
        {
            var.value = text.clone();
            self.dirty = true;
            self.pending.push(EnvironmentVarEdit::Value {
                index: self.cursor,
                value: text,
            });
        }
        self.state = EnvironmentEditState::Select;
        true
    }

    /// Annule la saisie en cours et restaure l'état de sélection, sans
    /// rien modifier — que ce soit une saisie de valeur ou un ajout en
    /// cours (`AddingKey`/`AddingValue`) : dans tous les cas, `Échap`
    /// abandonne l'ajout entier sans rien ajouter
    /// (`add-environment-entry-management`).
    pub fn cancel_input(&mut self) {
        self.state = EnvironmentEditState::Select;
    }

    /// Démarre l'ajout d'une variable : saisie de la clé, texte vide.
    pub fn begin_add(&mut self) {
        self.state = EnvironmentEditState::AddingKey(TextInput::new("", false));
    }

    /// Fait passer l'ajout de la saisie de la clé à celle de la valeur ;
    /// `key` est supposée déjà validée par l'appelant (`update.rs`, qui
    /// seul a accès à l'AST pour la vérifier via `bru-writer`). Sans
    /// effet hors de `AddingKey`.
    pub fn advance_add_to_value(&mut self, key: String) {
        if matches!(self.state, EnvironmentEditState::AddingKey(_)) {
            self.state = EnvironmentEditState::AddingValue {
                key,
                input: TextInput::new("", false),
            };
        }
    }

    /// Termine l'ajout en cours : ajoute la variable en fin de liste,
    /// activée, curseur dessus, session modifiée. Sans effet hors de
    /// `AddingValue`.
    pub fn finish_add(&mut self) {
        let EnvironmentEditState::AddingValue { key, input } = &self.state else {
            return;
        };
        let key = key.clone();
        let value = input.text().to_owned();
        self.variables.push(KeyValue {
            key: key.clone(),
            value: value.clone(),
            enabled: true,
        });
        self.cursor = self.variables.len() - 1;
        self.dirty = true;
        self.pending.push(EnvironmentVarEdit::Add {
            key,
            value,
            enabled: true,
        });
        self.state = EnvironmentEditState::Select;
    }

    /// Retire la variable sous le curseur, immédiatement, sans
    /// confirmation ; le curseur reste au même indice, borné à la
    /// dernière position. Sans effet si la liste est vide.
    pub fn remove_current(&mut self) -> bool {
        if self.cursor >= self.variables.len() {
            return false;
        }
        self.variables.remove(self.cursor);
        self.dirty = true;
        self.pending
            .push(EnvironmentVarEdit::Remove { index: self.cursor });
        self.cursor = self.cursor.min(self.variables.len().saturating_sub(1));
        true
    }

    /// Indique si la session a des modifications non enregistrées.
    pub fn has_unsaved(&self) -> bool {
        self.dirty
            || match &self.state {
                EnvironmentEditState::Select => false,
                EnvironmentEditState::Input(input) | EnvironmentEditState::AddingKey(input) => {
                    input.is_modified()
                }
                EnvironmentEditState::AddingValue { .. } => true,
            }
    }
}

/// État d'une session : choix du champ, saisie de sa valeur, ou sélecteur
/// de méthode ouvert (`add-method-editing`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditState {
    FieldSelect,
    Input(TextInput),
    /// Sélecteur de méthode ouvert, `selected` est l'indice présélectionné
    /// dans `crate::collection::ast::METHODS`.
    MethodPicker {
        selected: usize,
    },
}

/// Ce que produit la validation de la saisie en cours.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputTarget {
    /// Valeur du champ sous le curseur (URL, entrée, corps).
    Field,
    /// Clé d'une nouvelle entrée ; `return_cursor` est la position d'où
    /// l'ajout a commencé.
    NewKey {
        section: EntrySection,
        return_cursor: usize,
    },
    /// Valeur d'une nouvelle entrée dont la clé est validée.
    NewValue {
        section: EntrySection,
        key: String,
        return_cursor: usize,
    },
    /// Clé de l'entrée d'indice `index` de la section.
    RenameKey { section: EntrySection, index: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableField {
    /// Nom du bloc de méthode HTTP (`add-method-editing`).
    Method,
    Url,
    HeaderValue(usize),
    QueryParamValue(usize),
    PathParamValue(usize),
    /// Ligne « + Ajouter » d'une section.
    AddRow(EntrySection),
    BodyText,
}

impl EditableField {
    /// Positions du curseur pour une vue de requête : la méthode, l'URL,
    /// chaque section suivie de sa ligne d'ajout, puis le corps s'il est
    /// éditable.
    pub fn list_for(view: &RequestView) -> Vec<Self> {
        let mut fields = vec![Self::Method, Self::Url];
        for index in 0..view.headers.len() {
            fields.push(Self::HeaderValue(index));
        }
        fields.push(Self::AddRow(EntrySection::Headers));
        for index in 0..view.query_params.len() {
            fields.push(Self::QueryParamValue(index));
        }
        fields.push(Self::AddRow(EntrySection::QueryParams));
        for index in 0..view.path_params.len() {
            fields.push(Self::PathParamValue(index));
        }
        fields.push(Self::AddRow(EntrySection::PathParams));
        if let Some(body) = &view.body
            && matches!(
                body.kind,
                BodyKind::Json
                    | BodyKind::Text
                    | BodyKind::Xml
                    | BodyKind::Sparql
                    | BodyKind::Graphql
            )
        {
            fields.push(Self::BodyText);
        }
        fields
    }

    /// Entrée existante désignée : section et indice.
    pub fn entry(&self) -> Option<(EntrySection, usize)> {
        match self {
            Self::HeaderValue(index) => Some((EntrySection::Headers, *index)),
            Self::QueryParamValue(index) => Some((EntrySection::QueryParams, *index)),
            Self::PathParamValue(index) => Some((EntrySection::PathParams, *index)),
            Self::Method | Self::Url | Self::AddRow(_) | Self::BodyText => None,
        }
    }

    /// Section d'une entrée ou d'une ligne d'ajout.
    pub fn section(&self) -> Option<EntrySection> {
        match self {
            Self::AddRow(section) => Some(*section),
            other => other.entry().map(|(section, _)| section),
        }
    }

    /// Nom du champ pour l'affichage.
    pub fn display_name(&self, view: &RequestView) -> String {
        match self {
            Self::Method => "Méthode".to_owned(),
            Self::Url => "Url".to_owned(),
            Self::HeaderValue(index) => format!(
                "En-tête {}",
                view.headers.get(*index).map_or("", |h| &h.key)
            ),
            Self::QueryParamValue(index) => format!(
                "Paramètre de requête {}",
                view.query_params.get(*index).map_or("", |p| &p.key)
            ),
            Self::PathParamValue(index) => format!(
                "Paramètre de chemin {}",
                view.path_params.get(*index).map_or("", |p| &p.key)
            ),
            Self::AddRow(section) => add_row_label(*section).to_owned(),
            Self::BodyText => "Corps".to_owned(),
        }
    }
}

/// Libellé de la ligne d'ajout d'une section.
pub fn add_row_label(section: EntrySection) -> &'static str {
    match section {
        EntrySection::Headers => "+ Ajouter un en-tête",
        EntrySection::QueryParams => "+ Ajouter un paramètre de requête",
        EntrySection::PathParams => "+ Ajouter un paramètre de chemin",
    }
}

/// Nom d'une section pour la barre d'aide.
pub fn section_label(section: EntrySection) -> &'static str {
    match section {
        EntrySection::Headers => "En-têtes",
        EntrySection::QueryParams => "Paramètres de requête",
        EntrySection::PathParams => "Paramètres de chemin",
    }
}

/// Entrées d'une section d'une vue.
pub fn section_entries(
    view: &RequestView,
    section: EntrySection,
) -> &[crate::collection::KeyValue] {
    match section {
        EntrySection::Headers => &view.headers,
        EntrySection::QueryParams => &view.query_params,
        EntrySection::PathParams => &view.path_params,
    }
}

/// Nom d'un champ pour l'affichage (D8).
pub fn field_display_name(field: &EditableField, view: &RequestView) -> String {
    field.display_name(view)
}

/// Confirmation en attente avant une action destructive ou sensible (D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingConfirm {
    /// Fermer la session en cours (Échap) malgré des modifications.
    DiscardEdit,
    /// Fermer l'application (`q` ou `Ctrl+C`) malgré une session modifiée.
    QuitWithUnsavedEdit,
    /// Copier la commande curl avec les secrets en clair (en-têtes d'auth).
    CopyUnmaskedCurl,
}

/// Résultat d'une sauvegarde réussie (D7).
#[derive(Debug, Clone)]
pub struct SavedEdit {
    pub stamp: FileStamp,
    pub ast: crate::collection::BruFile,
    pub view: RequestView,
}

/// Résultat d'une sauvegarde réussie d'un environnement.
#[derive(Debug, Clone)]
pub struct SavedEnvironment {
    pub stamp: FileStamp,
    pub ast: crate::collection::BruFile,
}

/// Valeur affichée d'un champ : tampon de saisie si la valeur de ce champ
/// est en saisie, sinon valeur de l'aperçu.
pub fn field_value<'a>(session: &'a EditSession, field: &EditableField) -> &'a str {
    if let EditState::Input(input) = &session.state
        && session.target == InputTarget::Field
        && session.fields.get(session.cursor) == Some(field)
    {
        return input.text();
    }
    field_value_committed(session, field)
}

/// Valeur validée d'un champ (hors tampon de saisie en cours), lue dans
/// l'aperçu des modifications validées.
pub fn field_value_committed<'a>(session: &'a EditSession, field: &EditableField) -> &'a str {
    let view = &session.preview;
    match field {
        EditableField::Method => &view.method,
        EditableField::Url => &view.url,
        EditableField::AddRow(_) => "",
        EditableField::BodyText => match &view.body {
            Some(body) => match &body.content {
                BodyContent::Text(text) => text.as_str(),
                _ => "",
            },
            None => "",
        },
        entry => entry
            .entry()
            .and_then(|(section, index)| section_entries(view, section).get(index))
            .map_or("", |kv| kv.value.as_str()),
    }
}

/// État d'activation d'un champ (en-tête ou paramètre), lu dans l'aperçu.
pub fn field_enabled(session: &EditSession, field: &EditableField) -> bool {
    field
        .entry()
        .and_then(|(section, index)| section_entries(&session.preview, section).get(index))
        .is_none_or(|kv| kv.enabled)
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
    /// Première ligne affichée de la réponse.
    pub response_scroll: u16,
    /// Onglet actif du panneau Réponse.
    pub response_tab: ResponseTab,
    /// Onglet choisi du détail d'une requête (voir
    /// [`Model::shown_request_tab`] pour l'onglet réellement affiché).
    pub request_tab: RequestTab,
    /// Taille du terminal (colonnes, lignes).
    pub size: (u16, u16),
    /// `Some` : la boucle doit s'arrêter.
    pub exit: Option<Exit>,
    /// État des exécutions de requêtes.
    pub run: RunState,
    /// État du filtre de réponse, `None` tant que `|` n'a pas été ouvert.
    pub filter: Option<FilterState>,
    /// État du filtre de l'arbre Collection, `None` tant que `f` n'a pas été ouvert.
    pub tree_filter: Option<TreeFilterState>,
    /// État de la recherche, `None` tant que `/` n'a jamais été pressé.
    pub search: Option<SearchState>,
    /// Sélection visuelle active dans le détail.
    pub detail_selection: Option<DetailSelection>,
    /// Sélection visuelle active dans la réponse.
    pub response_selection: Option<DetailSelection>,
    /// Ligne et position en octets de la dernière correspondance de
    /// recherche trouvée dans le détail, pour la surbrillance.
    pub detail_match: Option<(u16, Range<usize>)>,
    /// Ligne et position en octets de la dernière correspondance de
    /// recherche trouvée dans la réponse, pour la surbrillance.
    pub response_match: Option<(u16, Range<usize>)>,
    /// Dernier statut à afficher dans la barre d'état (copie, recherche
    /// sans résultat), en plus des rappels habituels. Persiste jusqu'au
    /// prochain statut, aucune minuterie.
    pub last_status: Option<StatusMessage>,
    /// Capture souris et glisser en cours.
    pub mouse: MouseState,
    /// Jeton de la copie en cours, s'il y en a une : un `ClipboardResult`
    /// dont le jeton ne correspond plus est ignoré (résultat en retard sur
    /// une copie plus récente).
    pub(crate) pending_clipboard_token: Option<u64>,
    /// Origine de la copie en cours : la sélection visuelle n'est levée
    /// qu'une fois la copie réussie (`search-and-yank`, `mouse-support`).
    pub(crate) pending_clipboard_source: Option<ClipboardSource>,
    /// Prochain jeton à distribuer à une copie.
    pub(crate) next_clipboard_token: u64,
    /// Session d'édition active sur la requête du focus Détail.
    pub editing: Option<EditSession>,
    /// Session d'édition active sur l'environnement du focus Sélection d'environnement.
    pub environment_editing: Option<EnvironmentEditSession>,
    /// Confirmation en attente avant de fermer l'édition ou l'application.
    pub confirm: Option<PendingConfirm>,
    /// Nom de l'environnement courant, `None` = « Aucun ».
    pub current_environment: Option<String>,
    /// Indice sélectionné dans le panneau de sélection d'environnement :
    /// 0 = « Aucun », 1..=N = les entrées de `Collection.environments`.
    /// N'a de sens que pendant que `Focus::EnvironmentPicker` est actif.
    pub environment_selected: usize,
    /// Variables secrètes transmises à `bru` (`secret-env-vars`).
    pub secrets: SecretsState,
    /// Popup d'aide des raccourcis clavier, superposé à l'écran courant.
    pub help: Option<HelpPopup>,
    /// Résumé de la dernière campagne d'exécution récursive.
    pub campaign: Option<CampaignSummary>,
    /// Indice sélectionné dans le panneau Campagne.
    pub campaign_selected: usize,
    /// Mode plein écran actif sur le panneau focalisé (`feat/panel-zoom`).
    pub zoom: bool,
    /// Options de campagne mémorisées pour la session (`feat-runner-options`).
    pub runner_options: RunnerOptionsConfig,
    /// Popup de configuration des options de campagne.
    pub runner_options_popup: Option<RunnerOptionsPopup>,
    /// Popup de recherche globale par nom, méthode, chemin et URL (`Ctrl+K`).
    pub global_search: Option<GlobalSearchPopup>,
    /// Répertoire par défaut de destination des rapports JUnit et HTML.
    pub reports_dir: PathBuf,
    /// Chemins des rapports exportés lors de la dernière exécution récursive.
    pub last_reports: Vec<PathBuf>,
    /// Empreintes des derniers fichiers enregistrés par le TUI lui-même (`Ctrl+S`).
    pub last_saved_stamps: HashMap<PathBuf, FileStamp>,
    /// Un rechargement de collection est en attente de la fermeture de la session d'édition.
    pub pending_reload: bool,
    /// Série multi-environnements en cours d'enchaînement.
    pub series: Option<EnvSeries>,
    /// Tableau comparatif de la dernière campagne multi-environnements.
    pub comparison: Option<CampaignComparison>,
    /// Vue active du panneau Campagne (`t`).
    pub campaign_view: CampaignView,
    /// Colonne (environnement) sélectionnée dans le tableau comparatif.
    pub comparison_column: usize,
}

/// État des variables secrètes. Les valeurs ne sont jamais formatées :
/// elles vivent dans des `SecretString` dont `Debug` est masqué.
#[derive(Debug, Default)]
pub struct SecretsState {
    /// Déclarations `--secret`, immuables pendant la session.
    pub mappings: Vec<SecretMapping>,
    /// Dernière résolution depuis `.env` et le shell.
    pub resolved: Vec<Resolved>,
    /// Valeurs saisies dans le panneau, par nom.
    pub typed: Vec<(String, SecretString)>,
    /// Noms ajoutés depuis le panneau (`a`), dans l'ordre d'ajout.
    pub added: Vec<String>,
    /// Ligne sélectionnée dans le panneau.
    pub selected: usize,
    /// Saisie en cours dans le panneau.
    pub input: Option<SecretInput>,
    /// Refus à afficher dans le panneau jusqu'à la prochaine action.
    pub error: Option<SecretError>,
    /// Exécution retenue par la proposition de saisie au lancement :
    /// cible et mode récursif.
    pub pending_run: Option<(PathBuf, bool)>,
    /// Environnements pour lesquels la proposition a déjà été traitée.
    pub acknowledged: HashSet<String>,
    /// Environnement dont le panneau propose les secrets, à la place de
    /// l'environnement courant : un environnement d'une série en attente.
    pub prompt_env: Option<String>,
    /// Environnements de la série retenue avec `pending_run`, vide pour une
    /// exécution sur l'environnement courant.
    pub pending_envs: Vec<String>,
}

/// Saisie en cours dans le panneau des variables secrètes.
#[derive(Debug)]
pub enum SecretInput {
    /// Nom d'une variable à ajouter, saisi en clair.
    Name(String),
    /// Valeur masquée de `name` ; `adding` si le nom vient d'être saisi et
    /// n'est ajouté qu'à la validation d'une valeur non vide.
    Value {
        name: String,
        buffer: SecretString,
        adding: bool,
    },
}

/// Refus d'un nom saisi dans le panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretError {
    InvalidName,
    DuplicateName,
}

/// Ligne du panneau des variables secrètes, calculée à la demande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretRow {
    pub name: String,
    pub source: SecretSource,
    pub value: Option<SecretString>,
    /// Nom ajouté depuis le panneau.
    pub added: bool,
    /// Nom déclaré en `vars:secret` par l'environnement courant.
    pub declared: bool,
}

/// Origine d'une copie dans le presse-papiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardSource {
    /// Sélection visuelle d'un panneau (Détail ou Réponse).
    Selection(Focus),
    /// Commande curl générée.
    Curl { masked: bool },
}

/// Message affiché temporairement dans la barre d'état.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusMessage {
    Copied,
    /// Commande curl copiée dans le presse-papiers (`masked` indique si les secrets sont masqués).
    CopiedCurl {
        masked: bool,
    },
    /// Aucune requête exécutée à copier en commande curl.
    NoCurlResult,
    ClipboardError(String),
    /// Recherche validée sans aucune correspondance.
    NoMatch,
    /// Échec lors de la sauvegarde sur disque.
    SaveError(String),
    /// Changement de requête refusé : la session d'édition porte des
    /// modifications non enregistrées.
    EditLocked,
    /// Capture souris activée (`true`) ou désactivée (`false`).
    MouseCapture(bool),
    /// Échec d'activation ou de désactivation de la capture souris.
    MouseCaptureError(String),
    /// Modification refusée par `bru-writer` ; le texte décrit la nature
    /// du refus sans jamais citer la saisie.
    EditRefused(String),
    /// Aucune réponse exploitable à ouvrir dans un éditeur externe.
    NoResponseBody,
    /// Échec lors du lancement de l'éditeur externe.
    EditorError(String),
    /// Fin d'une campagne d'exécution récursive.
    CampaignFinished(String),
    /// Aucune requête en échec dans la collection.
    NoFailedRequests,
    /// Options de campagne enregistrées pour la session.
    RunnerOptionsSaved,
    /// Un fichier surveillé a été modifié sur disque alors qu'une session
    /// d'édition avec des modifications non enregistrées est ouverte.
    FileModifiedOnDisk,
    /// La collection a été rechargée suite à une modification sur disque.
    CollectionReloaded,
    /// Le rechargement automatique a échoué ; l'ancienne collection reste affichée.
    ReloadFailed(String),
    /// Édition ou enregistrement refusé : la collection est au format
    /// OpenCollection (YAML), que le writer ne sait pas écrire.
    ReadOnlyCollection,
    /// Cellule du tableau comparatif sans résultat pour cet environnement.
    NoEnvironmentResult(String),
}

/// Série de campagnes multi-environnements : un `bru run` par
/// environnement, lancés l'un après l'autre, jamais en parallèle.
#[derive(Debug)]
pub struct EnvSeries {
    /// Cible commune à toutes les exécutions de la série.
    pub target: PathBuf,
    /// Environnements de la série, dans l'ordre d'exécution.
    pub environments: Vec<String>,
    /// Indice, dans `environments`, de l'exécution en cours.
    pub current: usize,
    /// Issues déjà reçues, dans l'ordre de `environments`.
    pub runs: Vec<EnvRun>,
    /// Annulation demandée (`Ctrl+X`) : la série n'enchaîne plus.
    pub cancelled: bool,
}

impl EnvSeries {
    pub fn new(target: PathBuf, environments: Vec<String>) -> Self {
        Self {
            target,
            environments,
            current: 0,
            runs: Vec::new(),
            cancelled: false,
        }
    }

    /// Environnement de l'exécution en cours.
    pub fn current_environment(&self) -> Option<&str> {
        self.environments.get(self.current).map(String::as_str)
    }
}

/// Issue de l'exécution d'une série sur un environnement.
#[derive(Debug, Clone)]
pub struct EnvRun {
    pub environment: String,
    pub outcome: EnvRunOutcome,
}

/// Rapport reçu, ou motif de l'absence de rapport.
#[derive(Debug, Clone)]
pub enum EnvRunOutcome {
    Completed {
        report: runner::Report,
        exit_code: Option<i32>,
        console: std::sync::Arc<str>,
    },
    /// `bru` n'a produit aucun rapport exploitable (message d'erreur).
    Failed(String),
}

/// Champ interactif dans le popup d'options de campagne.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerOptionsField {
    /// Case à cocher d'un environnement de la collection.
    Environment(usize),
    Tags,
    ExcludeTags,
    Delay,
    Bail,
    TestsOnly,
    Insecure,
    Cacert,
    IgnoreTruststore,
    ClientCertConfig,
    NoProxy,
    DisableCookies,
    CsvFilePath,
    JsonFilePath,
    IterationCount,
    Parallel,
    Sandbox,
    Verbose,
    ReporterJunit,
    ReporterHtml,
    SkipHeaders,
    /// Fichier d'environnement (`--env-file`), après les cases d'environnement.
    EnvFile,
    GlobalEnv,
    WorkspacePath,
    SaveButton,
    RunButton,
}

impl RunnerOptionsField {
    pub const STATIC_COUNT: usize = 25;
    /// Indice fixe avant lequel s'insèrent les cases d'environnement.
    pub const ENVIRONMENTS_AT: usize = 20;

    /// Champ fixe d'indice `index`, après les cases d'environnement.
    pub fn from_static_index(index: usize) -> Self {
        match index {
            0 => Self::Tags,
            1 => Self::ExcludeTags,
            2 => Self::Delay,
            3 => Self::Bail,
            4 => Self::TestsOnly,
            5 => Self::Insecure,
            6 => Self::Cacert,
            7 => Self::IgnoreTruststore,
            8 => Self::ClientCertConfig,
            9 => Self::NoProxy,
            10 => Self::DisableCookies,
            11 => Self::CsvFilePath,
            12 => Self::JsonFilePath,
            13 => Self::IterationCount,
            14 => Self::Parallel,
            15 => Self::Sandbox,
            16 => Self::Verbose,
            17 => Self::ReporterJunit,
            18 => Self::ReporterHtml,
            19 => Self::SkipHeaders,
            20 => Self::EnvFile,
            21 => Self::GlobalEnv,
            22 => Self::WorkspacePath,
            23 => Self::SaveButton,
            _ => Self::RunButton,
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(
            self,
            Self::Tags
                | Self::ExcludeTags
                | Self::Delay
                | Self::Cacert
                | Self::ClientCertConfig
                | Self::CsvFilePath
                | Self::JsonFilePath
                | Self::IterationCount
                | Self::EnvFile
                | Self::GlobalEnv
                | Self::WorkspacePath
        )
    }

    pub fn is_toggle(&self) -> bool {
        matches!(
            self,
            Self::Environment(_)
                | Self::Bail
                | Self::TestsOnly
                | Self::Insecure
                | Self::IgnoreTruststore
                | Self::NoProxy
                | Self::DisableCookies
                | Self::Parallel
                | Self::Sandbox
                | Self::Verbose
                | Self::ReporterJunit
                | Self::ReporterHtml
                | Self::SkipHeaders
        )
    }
}

/// État du popup d'options de campagne (TNR).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerOptionsPopup {
    /// Environnements valides de la collection, dans son ordre.
    pub available_environments: Vec<String>,
    /// Environnements cochés pour la campagne.
    pub selected_environments: Vec<String>,
    pub env_file_input: TextInput,
    pub global_env_input: TextInput,
    pub workspace_path_input: TextInput,
    pub tags_input: TextInput,
    pub exclude_tags_input: TextInput,
    pub delay_input: TextInput,
    pub bail: bool,
    pub tests_only: bool,
    pub insecure: bool,
    pub cacert_input: TextInput,
    pub ignore_truststore: bool,
    pub client_cert_config_input: TextInput,
    pub noproxy: bool,
    pub disable_cookies: bool,
    pub csv_file_path_input: TextInput,
    pub json_file_path_input: TextInput,
    pub iteration_count_input: TextInput,
    pub parallel: bool,
    pub sandbox: SandboxMode,
    pub verbose: bool,
    pub reporter_junit: bool,
    pub reporter_html: bool,
    pub skip_headers: bool,
    pub cursor: usize,
    pub editing_text: bool,
    pub error: Option<String>,
}

impl RunnerOptionsPopup {
    /// Construit le popup à partir de la configuration actuelle et de la collection chargée.
    pub fn from_config(config: &RunnerOptionsConfig, collection: Option<&Collection>) -> Self {
        let available_environments = collection
            .map(|c| {
                c.environments
                    .iter()
                    .flatten()
                    .map(|e| e.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            available_environments,
            selected_environments: config.environments.clone(),
            env_file_input: TextInput::new(
                &config
                    .env_file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                false,
            ),
            global_env_input: TextInput::new(
                config.global_env.as_deref().unwrap_or_default(),
                false,
            ),
            workspace_path_input: TextInput::new(
                &config
                    .workspace_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                false,
            ),
            tags_input: TextInput::new(&config.tags.join(", "), false),
            exclude_tags_input: TextInput::new(&config.exclude_tags.join(", "), false),
            delay_input: TextInput::new(
                &config.delay_ms.map(|d| d.to_string()).unwrap_or_default(),
                false,
            ),
            bail: config.bail,
            tests_only: config.tests_only,
            insecure: config.insecure,
            cacert_input: TextInput::new(
                &config
                    .cacert
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                false,
            ),
            ignore_truststore: config.ignore_truststore,
            client_cert_config_input: TextInput::new(
                &config
                    .client_cert_config
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                false,
            ),
            noproxy: config.noproxy,
            disable_cookies: config.disable_cookies,
            csv_file_path_input: TextInput::new(
                &config
                    .csv_file_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                false,
            ),
            json_file_path_input: TextInput::new(
                &config
                    .json_file_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                false,
            ),
            iteration_count_input: TextInput::new(
                &config
                    .iteration_count
                    .map(|c| c.to_string())
                    .unwrap_or_default(),
                false,
            ),
            parallel: config.parallel,
            sandbox: config.sandbox,
            verbose: config.verbose,
            reporter_junit: config.export_junit,
            reporter_html: config.export_html,
            skip_headers: config.skip_headers,
            cursor: 0,
            editing_text: false,
            error: None,
        }
    }

    /// Nombre de champs navigables : une case par environnement, puis les champs fixes.
    pub fn total_fields(&self) -> usize {
        self.available_environments.len() + RunnerOptionsField::STATIC_COUNT
    }

    /// Champ d'indice `index` dans l'ordre de navigation.
    /// Les cases d'environnement s'insèrent avant le champ fixe
    /// [`RunnerOptionsField::ENVIRONMENTS_AT`], en fin de popup.
    pub fn field_at(&self, index: usize) -> RunnerOptionsField {
        let at = RunnerOptionsField::ENVIRONMENTS_AT;
        let count = self.available_environments.len();
        if index < at {
            RunnerOptionsField::from_static_index(index)
        } else if index < at + count {
            RunnerOptionsField::Environment(index - at)
        } else {
            RunnerOptionsField::from_static_index(index - count)
        }
    }

    /// Valide et convertit la saisie du popup en [`RunnerOptionsConfig`].
    pub fn to_config(&self) -> Result<RunnerOptionsConfig, RunnerOptionsError> {
        let tags = crate::runner::parse_tags(self.tags_input.text())?;
        let exclude_tags = crate::runner::parse_tags(self.exclude_tags_input.text())?;
        let delay_ms = crate::runner::parse_delay(self.delay_input.text())?;
        let cacert = crate::runner::parse_existing_file_path(self.cacert_input.text())?;
        if self.ignore_truststore && cacert.is_none() {
            return Err(RunnerOptionsError::IgnoreTruststoreWithoutCacert);
        }
        let client_cert_config =
            crate::runner::parse_existing_file_path(self.client_cert_config_input.text())?;
        let csv_file_path =
            crate::runner::parse_existing_file_path(self.csv_file_path_input.text())?;
        let json_file_path =
            crate::runner::parse_existing_file_path(self.json_file_path_input.text())?;
        if csv_file_path.is_some() && json_file_path.is_some() {
            return Err(RunnerOptionsError::MutuallyExclusiveDataFiles);
        }
        let iteration_count =
            crate::runner::parse_iteration_count(self.iteration_count_input.text())?;
        if self.parallel && csv_file_path.is_none() && json_file_path.is_none() {
            return Err(RunnerOptionsError::ParallelWithoutDataFile);
        }
        // Ordre de la collection, quel que soit l'ordre des coches.
        let environments: Vec<String> = self
            .available_environments
            .iter()
            .filter(|name| self.selected_environments.contains(name))
            .cloned()
            .collect();
        let env_file = crate::runner::parse_env_file_path(self.env_file_input.text())?;
        if env_file.is_some() && !environments.is_empty() {
            return Err(RunnerOptionsError::EnvFileWithEnvironments);
        }
        let global_env = crate::runner::parse_global_env(self.global_env_input.text());
        let workspace_path = crate::runner::parse_workspace_path(self.workspace_path_input.text())?;
        Ok(RunnerOptionsConfig {
            environments,
            env_file,
            global_env,
            workspace_path,
            tags,
            exclude_tags,
            delay_ms,
            bail: self.bail,
            tests_only: self.tests_only,
            insecure: self.insecure,
            cacert,
            ignore_truststore: self.ignore_truststore,
            client_cert_config,
            noproxy: self.noproxy,
            disable_cookies: self.disable_cookies,
            csv_file_path,
            json_file_path,
            iteration_count,
            parallel: self.parallel,
            sandbox: self.sandbox,
            verbose: self.verbose,
            export_junit: self.reporter_junit,
            export_html: self.reporter_html,
            skip_headers: self.skip_headers,
        })
    }

    pub fn current_field(&self) -> RunnerOptionsField {
        self.field_at(self.cursor)
    }

    /// Bascule la valeur du champ booléen courant s'il s'agit d'un toggle.
    pub fn toggle_current(&mut self) {
        match self.current_field() {
            RunnerOptionsField::Environment(idx) => {
                if let Some(env_name) = self.available_environments.get(idx) {
                    if let Some(pos) = self
                        .selected_environments
                        .iter()
                        .position(|e| e == env_name)
                    {
                        self.selected_environments.remove(pos);
                    } else {
                        self.selected_environments.push(env_name.clone());
                    }
                }
            }
            RunnerOptionsField::Bail => self.bail = !self.bail,
            RunnerOptionsField::TestsOnly => self.tests_only = !self.tests_only,
            RunnerOptionsField::Insecure => self.insecure = !self.insecure,
            RunnerOptionsField::IgnoreTruststore => {
                self.ignore_truststore = !self.ignore_truststore
            }
            RunnerOptionsField::NoProxy => self.noproxy = !self.noproxy,
            RunnerOptionsField::DisableCookies => self.disable_cookies = !self.disable_cookies,
            RunnerOptionsField::Parallel => self.parallel = !self.parallel,
            RunnerOptionsField::Sandbox => self.sandbox = self.sandbox.toggle(),
            RunnerOptionsField::Verbose => self.verbose = !self.verbose,
            RunnerOptionsField::ReporterJunit => self.reporter_junit = !self.reporter_junit,
            RunnerOptionsField::ReporterHtml => self.reporter_html = !self.reporter_html,
            RunnerOptionsField::SkipHeaders => self.skip_headers = !self.skip_headers,
            _ => {}
        }
    }

    /// Référence mutable vers le tampon de saisie du champ actif si textuel.
    pub fn active_text_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.current_field() {
            RunnerOptionsField::Tags => Some(&mut self.tags_input),
            RunnerOptionsField::ExcludeTags => Some(&mut self.exclude_tags_input),
            RunnerOptionsField::Delay => Some(&mut self.delay_input),
            RunnerOptionsField::Cacert => Some(&mut self.cacert_input),
            RunnerOptionsField::ClientCertConfig => Some(&mut self.client_cert_config_input),
            RunnerOptionsField::CsvFilePath => Some(&mut self.csv_file_path_input),
            RunnerOptionsField::JsonFilePath => Some(&mut self.json_file_path_input),
            RunnerOptionsField::IterationCount => Some(&mut self.iteration_count_input),
            RunnerOptionsField::EnvFile => Some(&mut self.env_file_input),
            RunnerOptionsField::GlobalEnv => Some(&mut self.global_env_input),
            RunnerOptionsField::WorkspacePath => Some(&mut self.workspace_path_input),
            _ => None,
        }
    }

    pub fn prev_field(&mut self) {
        let total = self.total_fields();
        self.cursor = if self.cursor == 0 {
            total.saturating_sub(1)
        } else {
            self.cursor - 1
        };
        self.editing_text = false;
        self.error = None;
    }

    pub fn next_field(&mut self) {
        let total = self.total_fields();
        if total > 0 {
            self.cursor = (self.cursor + 1) % total;
        }
        self.editing_text = false;
        self.error = None;
    }
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
            response_scroll: 0,
            response_tab: ResponseTab::Body,
            request_tab: RequestTab::Params,
            size,
            exit: None,
            run: RunState::default(),
            filter: None,
            tree_filter: None,
            search: None,
            detail_selection: None,
            response_selection: None,
            detail_match: None,
            response_match: None,
            last_status: None,
            mouse: MouseState::default(),
            pending_clipboard_token: None,
            pending_clipboard_source: None,
            next_clipboard_token: 0,
            editing: None,
            environment_editing: None,
            confirm: None,
            current_environment: None,
            environment_selected: 0,
            secrets: SecretsState::default(),
            help: None,
            campaign: None,
            campaign_selected: 0,
            zoom: false,
            runner_options: RunnerOptionsConfig::default(),
            runner_options_popup: None,
            global_search: None,
            reports_dir: default_reports_dir(),
            last_reports: Vec::new(),
            last_saved_stamps: HashMap::new(),
            pending_reload: false,
            series: None,
            comparison: None,
            campaign_view: CampaignView::List,
            comparison_column: 0,
        }
    }

    /// Onglet affiché du détail de la requête sélectionnée : pendant une
    /// session d'édition sur cette requête, celui du champ sous le curseur
    /// (la méthode et l'URL gardent l'onglet choisi) ; sinon l'onglet
    /// choisi.
    pub fn shown_request_tab(&self) -> RequestTab {
        let session_tab = match self.selected_node() {
            Some(TreeNode::Request(request)) => self
                .editing
                .as_ref()
                .filter(|session| session.path == request.path)
                .and_then(EditSession::current_field)
                .and_then(RequestTab::of_field),
            _ => None,
        };
        session_tab.unwrap_or(self.request_tab)
    }

    /// Panneau actuellement zoomé en plein écran s'il y en a un
    /// (`feat/panel-zoom`).
    pub fn zoomed_panel(&self) -> Option<Focus> {
        if self.zoom {
            match self.focus {
                Focus::Tree | Focus::Detail | Focus::Response => Some(self.focus),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Ce que la boucle doit capturer au clavier hors navigation normale.
    /// Conçu pour que `add-field-editing`/`add-response-filter` y ajoutent
    /// leur propre branche sans toucher à celle-ci.
    pub fn text_capture(&self) -> Option<TextCapture> {
        // Le popup d'aide intercepte les touches tant qu'il est ouvert :
        // aucune saisie de texte n'est active par-dessus.
        if self.help.is_some() {
            return None;
        }
        // Une confirmation en attente reprend la main sur le clavier : ses
        // réponses (`y`, `n`, `Entrée`, `Échap`) ne doivent pas devenir du
        // texte de saisie.
        if self.confirm.is_some() {
            return None;
        }
        if self.global_search.is_some() {
            return Some(TextCapture::GlobalSearch);
        }
        match &self.secrets.input {
            Some(SecretInput::Name(_)) => return Some(TextCapture::SecretName),
            Some(SecretInput::Value { .. }) => return Some(TextCapture::SecretValue),
            None => {}
        }
        if self
            .editing
            .as_ref()
            .is_some_and(|s| matches!(s.state, EditState::Input(_)))
            || self.environment_editing.as_ref().is_some_and(|s| {
                matches!(
                    s.state,
                    EnvironmentEditState::Input(_)
                        | EnvironmentEditState::AddingKey(_)
                        | EnvironmentEditState::AddingValue { .. }
                )
            })
            || self
                .runner_options_popup
                .as_ref()
                .is_some_and(|p| p.editing_text)
        {
            return Some(TextCapture::Input);
        }
        if self
            .editing
            .as_ref()
            .is_some_and(|s| matches!(s.state, EditState::MethodPicker { .. }))
        {
            return Some(TextCapture::MethodPicker);
        }
        if self.search.as_ref().is_some_and(SearchState::is_editing) {
            return Some(TextCapture::Search);
        }
        if self.filter.as_ref().is_some_and(|f| f.editing) {
            return Some(TextCapture::Filter);
        }
        if self.tree_filter.as_ref().is_some_and(|f| f.editing) {
            return Some(TextCapture::TreeFilter);
        }
        None
    }

    /// Collection chargée, s'il y en a une.
    pub fn loaded(&self) -> Option<&Collection> {
        match &self.collection {
            CollectionState::Loaded(collection) => Some(collection),
            _ => None,
        }
    }

    /// Collection chargée en accès mutable, s'il y en a une.
    pub fn loaded_mut(&mut self) -> Option<&mut Collection> {
        match &mut self.collection {
            CollectionState::Loaded(collection) => Some(collection),
            _ => None,
        }
    }

    /// Remplace l'AST et la vue d'une requête identifiée par son chemin (D7, Risks).
    pub fn replace_request_node(
        &mut self,
        path: &std::path::Path,
        ast: crate::collection::BruFile,
        view: RequestView,
    ) -> bool {
        let Some(collection) = self.loaded_mut() else {
            return false;
        };
        let mut ast = Some(ast);
        let mut view = Some(view);
        replace_request_in_tree(&mut collection.tree, path, &mut ast, &mut view)
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

    /// Corps brut de la réponse sélectionnée en texte pour l'éditeur externe.
    pub fn selected_response_body(&self) -> Option<String> {
        selected_response_body(self)
    }
}

/// Retourne le corps brut de la réponse sélectionnée en texte : chaîne telle
/// quelle, autre valeur mise en forme via [`crate::app::filter::pretty_print`],
/// ou `None` si aucune requête n'est sélectionnée, si la requête n'a pas de
/// résultat d'exécution, ou si son corps de réponse est `null`.
pub fn selected_response_body(model: &Model) -> Option<String> {
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        return None;
    };
    let outcome = model.run.outcomes.get(&request.path)?;
    match &outcome.result.response.data {
        serde_json::Value::Null => None,
        serde_json::Value::String(text) => Some(text.clone()),
        data => Some(super::filter::pretty_print(data)),
    }
}

/// Remplace l'AST et la vue de la requête désignée par `path` dans l'arbre.
/// Retourne `true` si le nœud a été trouvé et remplacé (D7, Risks).
pub fn replace_request_in_tree(
    tree: &mut [TreeNode],
    path: &std::path::Path,
    ast: &mut Option<crate::collection::BruFile>,
    view: &mut Option<RequestView>,
) -> bool {
    for node in tree.iter_mut() {
        match node {
            TreeNode::Request(req) if req.path == path => {
                req.ast = ast.take();
                if let Some(v) = view.take() {
                    req.view = v;
                }
                return true;
            }
            TreeNode::Folder(folder) => {
                if replace_request_in_tree(&mut folder.children, path, ast, view) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Nom de l'environnement à l'indice donné dans le panneau de sélection :
/// 0 est toujours « Aucun » (`Some(None)`) ; 1..=N couvre les entrées de
/// `Collection.environments` dans l'ordre, `Some(Some(nom))` si l'entrée
/// est valide, `None` si elle est en erreur ou si l'indice est hors
/// limites — dans les deux cas, sans effet pour l'appelant (D3, D4).
pub fn environment_name_at(collection: &Collection, index: usize) -> Option<Option<&str>> {
    if index == 0 {
        return Some(None);
    }
    match collection.environments.get(index - 1) {
        Some(Ok(env)) => Some(Some(env.name.as_str())),
        Some(Err(_)) | None => None,
    }
}

/// Noms `vars:secret` de l'environnement donné, vide sans environnement
/// ou si l'environnement donné n'existe plus.
pub fn secret_names_for_env<'a>(model: &'a Model, env_name: Option<&str>) -> &'a [String] {
    let (Some(collection), Some(target_env)) = (model.loaded(), env_name) else {
        return &[];
    };
    collection
        .environments
        .iter()
        .find_map(|entry| match entry {
            Ok(env) if env.name == target_env => Some(env.secret_names.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

/// Environnement dont le panneau des secrets montre les lignes :
/// l'environnement d'une série en attente, sinon l'environnement courant.
pub fn secrets_panel_environment(model: &Model) -> Option<&str> {
    model
        .secrets
        .prompt_env
        .as_deref()
        .or(model.current_environment.as_deref())
}

/// Recherche d'un nom : la clé explicite de son `--secret` s'il en a une,
/// sinon la recherche automatique (nom exact puis `UPPER_SNAKE_CASE`).
pub fn secret_lookup(mappings: &[SecretMapping], name: &str) -> SecretLookup {
    match mappings.iter().find(|mapping| mapping.name == name) {
        Some(mapping) => mapping.lookup(),
        None => SecretLookup::automatic(name),
    }
}

/// Recherches à lancer pour une collection : mappings `--secret`, puis
/// `vars:secret` de tous les environnements valides, sans doublon. La
/// source d'un nom ne dépend pas de l'environnement courant : une seule
/// résolution par chargement suffit.
pub fn secret_lookups(mappings: &[SecretMapping], collection: &Collection) -> Vec<SecretLookup> {
    let mut names: Vec<&str> = mappings.iter().map(|m| m.name.as_str()).collect();
    for env in collection.environments.iter().flatten() {
        for name in &env.secret_names {
            if is_valid_name(name) && !names.contains(&name.as_str()) {
                names.push(name);
            }
        }
    }
    names
        .into_iter()
        .map(|name| secret_lookup(mappings, name))
        .collect()
}

/// Lignes des variables secrètes pour un environnement donné : mappings
/// `--secret`, puis `vars:secret` de cet environnement, puis ajouts, sans
/// doublon. Une valeur saisie prime sur la résolution `.env`/shell.
pub fn secret_rows_for_env(model: &Model, env_name: Option<&str>) -> Vec<SecretRow> {
    let state = &model.secrets;
    let declared = secret_names_for_env(model, env_name);
    let mut names: Vec<&str> = Vec::new();
    let candidates = state
        .mappings
        .iter()
        .map(|mapping| mapping.name.as_str())
        .chain(declared.iter().map(String::as_str))
        .chain(state.added.iter().map(String::as_str));
    for name in candidates {
        // Un nom `vars:secret` inutilisable en `nom=valeur` est écarté.
        if is_valid_name(name) && !names.contains(&name) {
            names.push(name);
        }
    }
    names
        .into_iter()
        .map(|name| {
            let is_mapped = state.mappings.iter().any(|m| m.name == name);
            let is_declared = declared.iter().any(|d| d == name);
            let added = state.added.iter().any(|a| a == name);
            let typed = state.typed.iter().find(|(n, _)| n == name);
            let (source, value) = match typed {
                Some((_, value)) => (SecretSource::Typed, Some(value.clone())),
                None => match state.resolved.iter().find(|r| r.name == name) {
                    Some(resolved) if is_mapped || is_declared => {
                        (resolved.source.clone(), resolved.value.clone())
                    }
                    // Résolution pas encore arrivée, ou nom sans clé.
                    _ => {
                        let keys = if is_mapped || is_declared {
                            secret_lookup(&state.mappings, name).keys
                        } else {
                            Vec::new()
                        };
                        (SecretSource::Missing { keys }, None)
                    }
                },
            };
            SecretRow {
                name: name.to_owned(),
                source,
                value,
                added,
                declared: is_declared,
            }
        })
        .collect()
}

/// Lignes du panneau des variables secrètes : mappings `--secret`, puis
/// `vars:secret` de l'environnement du panneau
/// ([`secrets_panel_environment`]), puis ajouts, sans doublon.
/// Une valeur saisie prime sur la résolution `.env`/shell.
pub fn secret_rows(model: &Model) -> Vec<SecretRow> {
    secret_rows_for_env(model, secrets_panel_environment(model))
}

/// Surcharges `--env-var` à transmettre pour un environnement donné : chaque ligne ayant une valeur.
pub fn secret_env_vars_for_env(
    model: &Model,
    env_name: Option<&str>,
) -> Vec<(String, SecretString)> {
    secret_rows_for_env(model, env_name)
        .into_iter()
        .filter_map(|row| row.value.map(|value| (row.name, value)))
        .collect()
}

/// Surcharges `--env-var` à transmettre : chaque ligne ayant une valeur.
pub fn secret_env_vars(model: &Model) -> Vec<(String, SecretString)> {
    secret_env_vars_for_env(model, model.current_environment.as_deref())
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
    use std::path::Path;

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

    fn make_view(body_kind: Option<BodyKind>, headers_count: usize) -> RequestView {
        use crate::collection::{BodyView, KeyValue};
        RequestView {
            name: Some("test".into()),
            kind: Some("http".into()),
            seq: Some(1),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: (0..headers_count)
                .map(|i| KeyValue {
                    key: format!("H{i}"),
                    value: format!("V{i}"),
                    enabled: i % 2 == 0,
                })
                .collect(),
            query_params: vec![],
            path_params: vec![],
            body: body_kind.map(|k| BodyView {
                kind: k,
                content: BodyContent::Text("{}".into()),
            }),
            auth: None,
            has_pre_request_script: false,
            has_post_response_script: false,
            has_tests: false,
            has_assert: false,
            extra: Box::default(),
        }
    }

    #[test]
    fn editable_field_list_for_request_view() {
        use crate::writer::EntrySection::{Headers, PathParams, QueryParams};

        // Méthode + URL + 2 en-têtes + corps json -> 8 positions dans l'ordre
        let fields = EditableField::list_for(&make_view(Some(BodyKind::Json), 2));
        assert_eq!(
            fields,
            vec![
                EditableField::Method,
                EditableField::Url,
                EditableField::HeaderValue(0),
                EditableField::HeaderValue(1),
                EditableField::AddRow(Headers),
                EditableField::AddRow(QueryParams),
                EditableField::AddRow(PathParams),
                EditableField::BodyText,
            ]
        );

        // corps formUrlEncoded -> corps exclu
        let fields_form = EditableField::list_for(&make_view(Some(BodyKind::FormUrlEncoded), 0));
        assert_eq!(
            fields_form,
            vec![
                EditableField::Method,
                EditableField::Url,
                EditableField::AddRow(Headers),
                EditableField::AddRow(QueryParams),
                EditableField::AddRow(PathParams),
            ]
        );

        assert_eq!(EditableField::HeaderValue(1).entry(), Some((Headers, 1)));
        assert_eq!(EditableField::AddRow(PathParams).entry(), None);
        assert_eq!(
            EditableField::AddRow(PathParams).section(),
            Some(PathParams)
        );
        assert_eq!(EditableField::Url.section(), None);
    }

    #[test]
    fn field_value_and_enabled_read_the_preview_and_the_input_buffer() {
        use crate::collection::KeyValue;
        use std::path::PathBuf;

        let view = make_view(Some(BodyKind::Json), 2);
        let stamp = crate::writer::FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/simple.bru"),
        )
        .expect("stamp");
        let mut session = EditSession::new(PathBuf::from("req.bru"), stamp, view.clone());

        assert_eq!(
            field_value(&session, &EditableField::Url),
            "https://example.com"
        );
        assert_eq!(field_value(&session, &EditableField::HeaderValue(1)), "V1");
        assert!(field_enabled(&session, &EditableField::HeaderValue(0)));
        assert!(!field_enabled(&session, &EditableField::HeaderValue(1)));
        assert_eq!(field_value(&session, &EditableField::BodyText), "{}");

        // Aperçu : second en-tête supprimé, en-tête ajouté.
        let mut preview = view;
        preview.headers.remove(1);
        preview.headers.push(KeyValue {
            key: "X-Added".into(),
            value: "added".into(),
            enabled: true,
        });
        session.cursor = 99;
        session.set_preview(preview);
        assert_eq!(session.cursor, session.fields.len() - 1);
        assert!(session.fields.contains(&EditableField::HeaderValue(1)));
        assert!(!session.fields.contains(&EditableField::HeaderValue(2)));
        assert_eq!(
            field_value(&session, &EditableField::HeaderValue(1)),
            "added"
        );
        assert_eq!(
            session
                .current_field()
                .map(|f| f.display_name(&session.preview)),
            Some("Corps".to_owned())
        );

        // En saisie sur un champ : le tampon est affiché pour ce champ seul.
        session.cursor = session
            .fields
            .iter()
            .position(|f| *f == EditableField::Url)
            .expect("URL présente");
        session.state = EditState::Input(TextInput::new("https://typed", false));
        assert_eq!(field_value(&session, &EditableField::Url), "https://typed");
        assert_eq!(
            field_value_committed(&session, &EditableField::Url),
            "https://example.com"
        );
        // Saisie d'une clé : aucun champ existant n'affiche le tampon.
        session.target = InputTarget::NewKey {
            section: crate::writer::EntrySection::Headers,
            return_cursor: 0,
        };
        assert_eq!(
            field_value(&session, &EditableField::Url),
            "https://example.com"
        );
        assert!(!session.has_unsaved());
        session.target = InputTarget::NewValue {
            section: crate::writer::EntrySection::Headers,
            key: "K".into(),
            return_cursor: 0,
        };
        session.state = EditState::Input(TextInput::new("", false));
        assert!(session.has_unsaved(), "clé validée d'un ajout en cours");
    }

    #[test]
    fn replace_request_node_modifies_only_target_node() {
        let mut model = loaded_model((100, 30));
        let target_path = Path::new("simple-get.bru");
        let other_path = Path::new("multiline.bru");

        // Récupérer l'état initial des deux nœuds
        let (initial_target_url, initial_other_url) = {
            let collection = model.loaded().expect("collection");
            let target_node = collection
                .tree
                .iter()
                .find_map(|n| match n {
                    TreeNode::Request(r) if r.path == target_path => Some(r.view.url.clone()),
                    _ => None,
                })
                .expect("target request");
            let other_node = collection
                .tree
                .iter()
                .find_map(|n| match n {
                    TreeNode::Request(r) if r.path == other_path => Some(r.view.url.clone()),
                    _ => None,
                })
                .expect("other request");
            (target_node, other_node)
        };

        // Créer un nouvel AST et RequestView pour le nœud cible
        let new_source = "get {\n  url: https://new-ping.example.com\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");
        let new_view = RequestView::from_ast(&new_ast).expect("view");

        // Remplacement du nœud cible
        let replaced = model.replace_request_node(target_path, new_ast.clone(), new_view.clone());
        assert!(replaced);

        // Vérifier que seul le nœud cible a changé et que l'autre est intact
        {
            let collection = model.loaded().expect("collection");
            let updated_target = collection
                .tree
                .iter()
                .find_map(|n| match n {
                    TreeNode::Request(r) if r.path == target_path => Some(r),
                    _ => None,
                })
                .expect("target request");
            assert_eq!(updated_target.view.url, "https://new-ping.example.com");
            assert_eq!(updated_target.ast.as_ref(), Some(&new_ast));
            assert_ne!(updated_target.view.url, initial_target_url);

            let untouched_other = collection
                .tree
                .iter()
                .find_map(|n| match n {
                    TreeNode::Request(r) if r.path == other_path => Some(r),
                    _ => None,
                })
                .expect("other request");
            assert_eq!(untouched_other.view.url, initial_other_url);
        }

        // Tenter de remplacer un chemin inexistant retourne false
        let dummy_path = Path::new("nonexistent.bru");
        assert!(!model.replace_request_node(dummy_path, new_ast, new_view));
    }

    #[test]
    fn new_model_starts_with_empty_history() {
        let model = Model::new(PathBuf::from("test"), (100, 30));
        assert!(model.history.is_empty());
        assert_eq!(model.diagnostics_selected, 0);
        assert_eq!(model.history_selected, 0);
        assert_eq!(model.current_environment, None);
        assert_eq!(model.environment_selected, 0);
    }

    /// La fixture `parser-cases/environments/` porte, dans l'ordre
    /// alphabétique : `local` (valide), `malformed` (en erreur),
    /// `staging` (valide).
    #[test]
    fn environment_name_at_resolves_none_valid_error_and_out_of_bounds() {
        let model = loaded_model((100, 30));
        let collection = model.loaded().expect("collection chargée");

        // Indice 0 : toujours « Aucun ».
        assert_eq!(environment_name_at(collection, 0), Some(None));
        // Indice 1 : premier environnement, valide (`local`).
        assert_eq!(environment_name_at(collection, 1), Some(Some("local")));
        // Indice 2 : second environnement, en erreur (`malformed`).
        assert_eq!(environment_name_at(collection, 2), None);
        // Indice 3 : troisième environnement, valide (`staging`).
        assert_eq!(environment_name_at(collection, 3), Some(Some("staging")));
        // Indice hors limites.
        assert_eq!(environment_name_at(collection, 4), None);
    }

    #[test]
    fn secret_rows_union_order_and_environment_changes() {
        use crate::secrets::SecretMapping;
        let mut model = loaded_model((100, 30));
        // Aucun environnement, aucune déclaration : ensemble vide.
        assert!(secret_rows(&model).is_empty());
        assert!(secret_env_vars(&model).is_empty());

        model.secrets.mappings = vec![SecretMapping {
            name: "oktaClientSecret".into(),
            key: None,
        }];
        model.current_environment = Some("local".into());
        model.secrets.added = vec!["extra".into(), "token".into()];
        let names: Vec<String> = secret_rows(&model).into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["oktaClientSecret", "token", "extra"]);
        let token = &secret_rows(&model)[1];
        assert!(token.declared);
        assert_eq!(
            token.source,
            SecretSource::Missing {
                keys: vec!["token".into(), "TOKEN".into()]
            }
        );

        // Changer d'environnement retire `token` sans perdre sa saisie.
        model.secrets.added.clear();
        model.secrets.typed = vec![("token".into(), SecretString::new("s3cr3t"))];
        model.current_environment = Some("staging".into());
        assert!(secret_rows(&model).iter().all(|r| r.name != "token"));
        model.current_environment = Some("local".into());
        let token = secret_rows(&model).remove(1);
        assert_eq!(token.source, SecretSource::Typed);
        assert_eq!(
            secret_env_vars(&model)
                .iter()
                .map(|(n, v)| (n.as_str(), v.expose()))
                .collect::<Vec<_>>(),
            [("token", "s3cr3t")]
        );
        assert!(!format!("{model:?}").contains("s3cr3t"));
    }

    #[test]
    fn typed_value_overrides_resolution() {
        use crate::secrets::Resolved;
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model.secrets.resolved = vec![Resolved {
            name: "token".into(),
            source: SecretSource::Shell {
                key: "TOKEN".into(),
            },
            value: Some(SecretString::new("shell")),
        }];
        assert_eq!(
            secret_rows(&model)[0].source,
            SecretSource::Shell {
                key: "TOKEN".into()
            }
        );
        model.secrets.typed = vec![("token".into(), SecretString::new("typed"))];
        let row = secret_rows(&model).remove(0);
        assert_eq!(row.source, SecretSource::Typed);
        assert_eq!(row.value.as_ref().map(SecretString::expose), Some("typed"));
    }

    #[test]
    fn environment_edit_session_initializes_with_variables_in_order() {
        let model = loaded_model((100, 30));
        let collection = model.loaded().expect("collection");
        let local_env = collection
            .environments
            .iter()
            .find_map(|e| match e {
                Ok(env) if env.name == "local" => Some(env),
                _ => None,
            })
            .expect("local environment");

        let stamp = FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/parser-cases/environments/local.bru"),
        )
        .expect("stamp");

        let session = EnvironmentEditSession::new(
            local_env.name.clone(),
            local_env.path.clone(),
            stamp,
            local_env.variables.clone(),
        );

        assert_eq!(session.name, "local");
        assert_eq!(session.cursor, 0);
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert!(!session.dirty);
        assert_eq!(session.variables, local_env.variables);
        assert_eq!(session.variables.len(), 2);
        assert_eq!(session.variables[0].key, "host");
        assert_eq!(session.variables[0].value, "localhost:3000");
        assert!(session.variables[0].enabled);
        assert_eq!(session.variables[1].key, "debug");
        assert_eq!(session.variables[1].value, "true");
        assert!(!session.variables[1].enabled);
    }

    #[test]
    fn environment_edit_session_input_commit_and_cancel() {
        let stamp = FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/parser-cases/environments/local.bru"),
        )
        .expect("stamp");

        let variables = vec![
            KeyValue {
                key: "host".into(),
                value: "localhost".into(),
                enabled: true,
            },
            KeyValue {
                key: "debug".into(),
                value: "true".into(),
                enabled: false,
            },
        ];

        let mut session = EnvironmentEditSession::new(
            "local".into(),
            PathBuf::from("environments/local.bru"),
            stamp,
            variables,
        );

        // Début de la saisie sur 'host'
        assert!(session.begin_input());
        assert!(
            matches!(&session.state, EnvironmentEditState::Input(input) if input.text() == "localhost")
        );
        assert!(!session.dirty);

        // Simulation de modification du texte
        if let EnvironmentEditState::Input(input) = &mut session.state {
            input.insert('8');
        }
        assert!(session.has_unsaved());

        // Annulation de la saisie
        session.cancel_input();
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert_eq!(session.variables[0].value, "localhost");
        assert!(!session.dirty);
        assert!(!session.has_unsaved());

        // Nouvelle saisie et validation
        assert!(session.begin_input());
        if let EnvironmentEditState::Input(input) = &mut session.state {
            *input = TextInput::new("", false);
            for c in "staging.example.com".chars() {
                input.insert(c);
            }
        }
        assert!(session.commit_input());
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert_eq!(session.variables[0].value, "staging.example.com");
        assert!(session.dirty);
        assert!(session.has_unsaved());
    }

    #[test]
    fn environment_edit_session_refuses_input_on_disabled_variable() {
        let stamp = FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/parser-cases/environments/local.bru"),
        )
        .expect("stamp");

        let variables = vec![
            KeyValue {
                key: "host".into(),
                value: "localhost".into(),
                enabled: true,
            },
            KeyValue {
                key: "debug".into(),
                value: "true".into(),
                enabled: false,
            },
        ];

        let mut session = EnvironmentEditSession::new(
            "local".into(),
            PathBuf::from("environments/local.bru"),
            stamp,
            variables,
        );

        // Curseur sur la variable désactivée 'debug'
        session.cursor = 1;
        assert_eq!(
            session.current_variable().map(|v| v.key.as_str()),
            Some("debug")
        );

        // begin_input doit refuser
        assert!(!session.begin_input());
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert!(!session.dirty);
    }

    fn sample_env_session() -> EnvironmentEditSession {
        let stamp = FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/parser-cases/environments/local.bru"),
        )
        .expect("stamp");
        let variables = vec![
            KeyValue {
                key: "host".into(),
                value: "localhost".into(),
                enabled: true,
            },
            KeyValue {
                key: "debug".into(),
                value: "true".into(),
                enabled: false,
            },
        ];
        EnvironmentEditSession::new(
            "local".into(),
            PathBuf::from("environments/local.bru"),
            stamp,
            variables,
        )
    }

    #[test]
    fn environment_edit_session_full_add_sequence() {
        let mut session = sample_env_session();

        session.begin_add();
        assert!(matches!(session.state, EnvironmentEditState::AddingKey(_)));
        assert!(
            !session.has_unsaved(),
            "clé pas encore tapée : rien à perdre"
        );

        if let EnvironmentEditState::AddingKey(input) = &mut session.state {
            for c in "region".chars() {
                input.insert(c);
            }
        }
        assert!(session.has_unsaved(), "clé en cours de saisie");
        session.advance_add_to_value("region".into());
        assert!(matches!(
            &session.state,
            EnvironmentEditState::AddingValue { key, .. } if key == "region"
        ));
        assert!(!session.dirty, "pas encore ajoutée");

        if let EnvironmentEditState::AddingValue { input, .. } = &mut session.state {
            for c in "eu-west".chars() {
                input.insert(c);
            }
        }
        session.finish_add();

        assert_eq!(session.state, EnvironmentEditState::Select);
        assert!(session.dirty);
        assert_eq!(session.cursor, 2);
        assert_eq!(session.variables.len(), 3);
        assert_eq!(session.variables[2].key, "region");
        assert_eq!(session.variables[2].value, "eu-west");
        assert!(session.variables[2].enabled);
        assert_eq!(
            session.pending,
            vec![crate::writer::EnvironmentVarEdit::Add {
                key: "region".into(),
                value: "eu-west".into(),
                enabled: true,
            }]
        );
    }

    #[test]
    fn environment_edit_session_cancel_input_abandons_add_at_either_step() {
        // Abandon pendant la saisie de la clé.
        let mut session = sample_env_session();
        session.begin_add();
        if let EnvironmentEditState::AddingKey(input) = &mut session.state {
            input.insert('x');
        }
        session.cancel_input();
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert_eq!(session.variables.len(), 2);
        assert!(session.pending.is_empty());

        // Abandon pendant la saisie de la valeur : la clé déjà validée
        // n'ajoute rien non plus.
        session.begin_add();
        session.advance_add_to_value("region".into());
        if let EnvironmentEditState::AddingValue { input, .. } = &mut session.state {
            input.insert('x');
        }
        session.cancel_input();
        assert_eq!(session.state, EnvironmentEditState::Select);
        assert_eq!(session.variables.len(), 2);
        assert!(session.pending.is_empty());
    }

    #[test]
    fn environment_edit_session_remove_current_bounds_cursor() {
        let mut session = sample_env_session();
        session.variables.push(KeyValue {
            key: "region".into(),
            value: "eu".into(),
            enabled: true,
        });
        session.cursor = 2; // "region", la dernière

        assert!(session.remove_current());
        assert_eq!(session.variables.len(), 2);
        assert_eq!(session.cursor, 1, "borné à la dernière position restante");
        assert!(session.dirty);
        assert_eq!(
            session.pending,
            vec![crate::writer::EnvironmentVarEdit::Remove { index: 2 }]
        );
    }

    #[test]
    fn environment_edit_session_remove_current_on_empty_list_does_nothing() {
        let mut session = sample_env_session();
        session.variables.clear();
        assert!(!session.remove_current());
        assert!(!session.dirty);
    }

    #[test]
    fn selected_response_body_coverage() {
        use crate::app::test_support::{runner_probe_model, select};

        let mut model = runner_probe_model();

        // Dossier sélectionné : aucun corps
        select(&mut model, "folder");
        assert_eq!(selected_response_body(&model), None);

        // Requête sélectionnée avec corps null (folder/down.bru dans mixed.json)
        select(&mut model, "folder/down.bru");
        assert_eq!(selected_response_body(&model), None);

        // Requête sélectionnée avec corps texte brut (ok.bru dans mixed.json)
        select(&mut model, "ok.bru");
        assert_eq!(
            selected_response_body(&model),
            Some("<html><body>probe</body></html>\n".to_string())
        );

        // Requête sélectionnée avec corps JSON structuré (json.bru dans mixed.json)
        select(&mut model, "json.bru");
        let body = selected_response_body(&model).expect("corps JSON attendu");
        assert!(body.contains("{\n"));
        assert!(body.contains("\"a\": ["));

        // Requête jamais exécutée
        model.run.outcomes.clear();
        assert_eq!(selected_response_body(&model), None);

        // Aucune sélection
        model.tree.rows.clear();
        assert_eq!(selected_response_body(&model), None);
    }

    #[test]
    fn campaign_filter_cycle_and_labels() {
        let filter = CampaignFilter::Failed;
        assert_eq!(filter.label(), "Échecs");

        let filter = filter.next();
        assert_eq!(filter, CampaignFilter::Passed);
        assert_eq!(filter.label(), "Réussis");

        let filter = filter.next();
        assert_eq!(filter, CampaignFilter::Skipped);
        assert_eq!(filter.label(), "Ignorés");

        let filter = filter.next();
        assert_eq!(filter, CampaignFilter::All);
        assert_eq!(filter.label(), "Tous");

        let filter = filter.next();
        assert_eq!(filter, CampaignFilter::Failed);
    }

    #[test]
    fn campaign_summary_filtered_items_behavior() {
        let items = vec![
            CampaignItem {
                path: PathBuf::from("a.bru"),
                name: "a".into(),
                status: CampaignItemStatus::Passed,
                http_code: Some(200),
                duration_secs: 0.1,
                reason: None,
                iteration_index: 0,
            },
            CampaignItem {
                path: PathBuf::from("b.bru"),
                name: "b".into(),
                status: CampaignItemStatus::Failed,
                http_code: Some(500),
                duration_secs: 0.2,
                reason: Some("error".into()),
                iteration_index: 0,
            },
            CampaignItem {
                path: PathBuf::from("c.bru"),
                name: "c".into(),
                status: CampaignItemStatus::Skipped,
                http_code: None,
                duration_secs: 0.0,
                reason: None,
                iteration_index: 0,
            },
        ];

        let mut summary = CampaignSummary {
            target: PathBuf::from("grp"),
            total: 3,
            passed: 1,
            failed: 1,
            skipped: 1,
            duration_secs: 0.3,
            items,
            filter: CampaignFilter::Failed,
            failures: vec![CampaignFailure {
                path: PathBuf::from("b.bru"),
                name: "b".into(),
                http_code: Some(500),
                reason: "error".into(),
            }],
            total_iterations: 1,
        };

        // Filtre par défaut : Échecs
        let filtered = summary.filtered_items();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, PathBuf::from("b.bru"));
        assert_eq!(filtered[0].status, CampaignItemStatus::Failed);

        // Filtre Réussis
        summary.filter = CampaignFilter::Passed;
        let filtered = summary.filtered_items();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, PathBuf::from("a.bru"));
        assert_eq!(filtered[0].status, CampaignItemStatus::Passed);

        // Filtre Ignorés
        summary.filter = CampaignFilter::Skipped;
        let filtered = summary.filtered_items();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, PathBuf::from("c.bru"));
        assert_eq!(filtered[0].status, CampaignItemStatus::Skipped);

        // Filtre Tous
        summary.filter = CampaignFilter::All;
        let filtered = summary.filtered_items();
        assert_eq!(filtered.len(), 3);
        assert_eq!(filtered[0].path, PathBuf::from("a.bru"));
        assert_eq!(filtered[1].path, PathBuf::from("b.bru"));
        assert_eq!(filtered[2].path, PathBuf::from("c.bru"));
    }

    #[test]
    fn campaign_summary_failures_fallback_when_items_empty() {
        let summary = CampaignSummary {
            target: PathBuf::from("grp"),
            total: 1,
            passed: 0,
            failed: 1,
            skipped: 0,
            duration_secs: 0.1,
            items: Vec::new(),
            filter: CampaignFilter::Failed,
            failures: vec![CampaignFailure {
                path: PathBuf::from("legacy.bru"),
                name: "legacy".into(),
                http_code: Some(502),
                reason: "bad gateway".into(),
            }],
            total_iterations: 1,
        };

        let filtered = summary.filtered_items();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].path, PathBuf::from("legacy.bru"));
        assert_eq!(filtered[0].status, CampaignItemStatus::Failed);
        assert_eq!(filtered[0].reason, Some("bad gateway".into()));
    }

    fn fixture_report(json: &str) -> runner::Report {
        serde_json::from_str(json).expect("rapport de fixture")
    }

    fn env_run(environment: &str, report: runner::Report) -> EnvRun {
        EnvRun {
            environment: environment.into(),
            outcome: EnvRunOutcome::Completed {
                report,
                exit_code: Some(0),
                console: std::sync::Arc::from("console"),
            },
        }
    }

    fn multi_env_comparison() -> CampaignComparison {
        CampaignComparison::build(
            PathBuf::new(),
            vec![
                env_run(
                    "up",
                    fixture_report(include_str!(
                        "../../tests/fixtures/reports/multi-env-up.json"
                    )),
                ),
                env_run(
                    "down",
                    fixture_report(include_str!(
                        "../../tests/fixtures/reports/multi-env-down.json"
                    )),
                ),
            ],
        )
    }

    #[test]
    fn comparison_is_built_from_real_reports() {
        use ComparisonCell::*;
        let comparison = multi_env_comparison();
        assert_eq!(comparison.environments, ["up", "down"]);
        assert_eq!(comparison.errors, [None, None]);
        let rows: Vec<(&Path, &[ComparisonCell])> = comparison
            .rows
            .iter()
            .map(|row| (row.path.as_path(), row.cells.as_slice()))
            .collect();
        assert_eq!(
            rows,
            [
                (Path::new("data.bru"), &[Passed, Failed][..]),
                (Path::new("static.bru"), &[Passed, Passed][..]),
                (Path::new("skip.bru"), &[Skipped, Skipped][..]),
            ]
        );
        assert_eq!(comparison.divergent_count(), 1);
        // Résultats par environnement conservés pour le panneau Réponse.
        let down = &comparison.outcomes[1][Path::new("data.bru")];
        assert_eq!(down.environment.as_deref(), Some("down"));
        assert!(down.result.is_failure());
        assert!(
            !comparison.outcomes[0][Path::new("data.bru")]
                .result
                .is_failure()
        );
    }

    #[test]
    fn comparison_filters_cycle_and_select_rows() {
        let mut comparison = multi_env_comparison();
        let paths = |c: &CampaignComparison| -> Vec<String> {
            c.filtered_rows()
                .iter()
                .map(|row| row.path.display().to_string())
                .collect()
        };
        assert_eq!(comparison.filter, ComparisonFilter::All);
        assert_eq!(paths(&comparison).len(), 3);
        comparison.filter = comparison.filter.next();
        assert_eq!(comparison.filter.label(), "Divergences");
        assert_eq!(paths(&comparison), ["data.bru"]);
        comparison.filter = comparison.filter.next();
        assert_eq!(comparison.filter, ComparisonFilter::Failed);
        assert_eq!(paths(&comparison), ["data.bru"]);
        comparison.filter = comparison.filter.next();
        assert_eq!(comparison.filter, ComparisonFilter::Passed);
        assert_eq!(paths(&comparison), ["data.bru", "static.bru"]);
        comparison.filter = comparison.filter.next();
        assert_eq!(comparison.filter, ComparisonFilter::Skipped);
        assert_eq!(paths(&comparison), ["skip.bru"]);
        comparison.filter = comparison.filter.next();
        assert_eq!(comparison.filter, ComparisonFilter::All);
    }

    #[test]
    fn a_request_missing_from_an_environment_is_absent_and_diverges() {
        let comparison = CampaignComparison::build(
            PathBuf::new(),
            vec![
                env_run(
                    "up",
                    fixture_report(include_str!(
                        "../../tests/fixtures/reports/multi-env-up.json"
                    )),
                ),
                env_run(
                    "csv",
                    fixture_report(include_str!(
                        "../../tests/fixtures/reports/csv-iterations.json"
                    )),
                ),
                EnvRun {
                    environment: "cassé".into(),
                    outcome: EnvRunOutcome::Failed("bru introuvable".into()),
                },
            ],
        );
        assert_eq!(comparison.errors[2].as_deref(), Some("bru introuvable"));
        let req = comparison
            .rows
            .iter()
            .find(|row| row.path == Path::new("req.bru"))
            .expect("req.bru");
        assert_eq!(
            req.cells,
            [
                ComparisonCell::Absent,
                ComparisonCell::Passed,
                ComparisonCell::Absent
            ]
        );
        assert!(req.diverges());
        assert!(comparison.outcomes[2].is_empty());
    }

    /// Avec des itérations CSV, une cellule résume toutes les itérations :
    /// ✗ dès qu'une seule échoue.
    #[test]
    fn a_cell_summarises_all_iterations() {
        let report = || {
            fixture_report(include_str!(
                "../../tests/fixtures/reports/csv-iterations.json"
            ))
        };
        let mut failing = report();
        assert_eq!(failing.0.len(), 2);
        // Seule la seconde itération échoue sur cet environnement.
        failing.0[1].results[0].error = Some("échec simulé".into());
        let comparison = CampaignComparison::build(
            PathBuf::new(),
            vec![env_run("a", report()), env_run("b", failing)],
        );
        assert_eq!(comparison.rows.len(), 1);
        assert_eq!(
            comparison.rows[0].cells,
            [ComparisonCell::Passed, ComparisonCell::Failed]
        );
        // Le résultat conservé est celui de la dernière itération.
        assert_eq!(
            comparison.outcomes[1][Path::new("req.bru")].iteration,
            Some((2, 2))
        );
    }

    #[test]
    fn cell_merge_prefers_failure_then_success() {
        use ComparisonCell::*;
        assert_eq!(Passed.merge(Failed), Failed);
        assert_eq!(Skipped.merge(Passed), Passed);
        assert_eq!(Absent.merge(Skipped), Skipped);
        assert_eq!(Absent.merge(Absent), Absent);
        assert_eq!(
            [Passed, Failed, Skipped, Absent].map(ComparisonCell::symbol),
            ["✓", "✗", "–", "·"]
        );
    }
}
