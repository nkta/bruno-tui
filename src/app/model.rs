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

use crate::collection::{BodyContent, BodyKind, Collection, LoadError, RequestView, TreeNode};
use crate::runner::{self, SecretString};
use crate::secrets::{Resolved, SecretLookup, SecretMapping, SecretSource, is_valid_name};
use crate::writer::{FieldEdit, FileStamp};

use super::filter::FilterState;
use super::message::TextCapture;
use super::search::SearchState;
use super::text_input::TextInput;

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
}

/// Onglet actif du panneau Réponse (`response-tabs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseTab {
    #[default]
    Body,
    Headers,
    Tests,
}

impl ResponseTab {
    pub fn next(self) -> Self {
        match self {
            ResponseTab::Body => ResponseTab::Headers,
            ResponseTab::Headers => ResponseTab::Tests,
            ResponseTab::Tests => ResponseTab::Body,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            ResponseTab::Body => ResponseTab::Tests,
            ResponseTab::Headers => ResponseTab::Body,
            ResponseTab::Tests => ResponseTab::Headers,
        }
    }
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

/// Session d'édition d'une requête, superposée au focus Détail.
#[derive(Debug, Clone)]
pub struct EditSession {
    /// Chemin de la requête éditée, relatif à la racine ; la session se
    /// ferme sans confirmation si la sélection change de nœud alors
    /// qu'elle ne porte aucune modification non enregistrée.
    pub path: PathBuf,
    pub stamp: FileStamp,
    /// Champs éditables, dans l'ordre d'affichage (D2).
    pub fields: Vec<EditableField>,
    /// Indice du champ sous le curseur, dans `fields`.
    pub cursor: usize,
    pub state: EditState,
    /// Modifications validées, une par cible touchée (fusionnées par
    /// écrasement, dans l'ordre de `FieldEdit` attendu par bru-writer :
    /// pas besoin de dédupliquer davantage, le writer le fait déjà).
    pub pending: Vec<FieldEdit>,
    /// Au moins une modification validée depuis la dernière sauvegarde.
    pub dirty: bool,
    /// Décalage horizontal, en colonnes d'affichage, de la valeur du champ
    /// en saisie (`improve-direct-editing`, D7).
    pub hscroll: u16,
}

impl EditSession {
    /// Modifications non enregistrées : validées, ou saisie en cours dont
    /// le texte diffère de la valeur au début de la saisie.
    pub fn has_unsaved(&self) -> bool {
        self.dirty || matches!(&self.state, EditState::Input(input) if input.is_modified())
    }

    /// Champ sous le curseur.
    pub fn current_field(&self) -> Option<EditableField> {
        self.fields.get(self.cursor).copied()
    }
}

/// État d'une session : choix du champ, ou saisie de sa valeur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditState {
    FieldSelect,
    Input(TextInput),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditableField {
    Url,
    HeaderValue(usize),
    QueryParamValue(usize),
    PathParamValue(usize),
    BodyText,
}

impl EditableField {
    /// Construit la liste des champs éditables pour une vue de requête (D2).
    pub fn list_for(view: &RequestView) -> Vec<Self> {
        let mut fields = vec![Self::Url];
        for index in 0..view.headers.len() {
            fields.push(Self::HeaderValue(index));
        }
        for index in 0..view.query_params.len() {
            fields.push(Self::QueryParamValue(index));
        }
        for index in 0..view.path_params.len() {
            fields.push(Self::PathParamValue(index));
        }
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

    /// Nom du champ pour l'affichage (D8).
    pub fn display_name(&self, view: &RequestView) -> String {
        match self {
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
            Self::BodyText => "Corps".to_owned(),
        }
    }
}

/// Nom d'un champ pour l'affichage (D8).
pub fn field_display_name(field: &EditableField, view: &RequestView) -> String {
    field.display_name(view)
}

/// Confirmation en attente avant une action destructive ou de fermeture (D6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingConfirm {
    /// Fermer la session en cours (Échap) malgré des modifications.
    DiscardEdit,
    /// Fermer l'application (`q` ou `Ctrl+C`) malgré une session modifiée.
    QuitWithUnsavedEdit,
}

/// Résultat d'une sauvegarde réussie (D7).
#[derive(Debug, Clone)]
pub struct SavedEdit {
    pub stamp: FileStamp,
    pub ast: crate::collection::BruFile,
    pub view: RequestView,
}

/// Valeur affichée d'un champ : tampon de saisie si en saisie sur ce
/// champ, sinon vue + éditions en attente (D3).
pub fn field_value<'a>(
    session: &'a EditSession,
    view: &'a RequestView,
    field: &EditableField,
) -> &'a str {
    if let EditState::Input(input) = &session.state
        && session.fields.get(session.cursor) == Some(field)
    {
        return input.text();
    }
    field_value_committed(session, view, field)
}

/// Valeur engagée d'un champ (hors tampon de saisie en cours) :
/// vue + éditions en attente dans `pending` (D3).
pub fn field_value_committed<'a>(
    session: &'a EditSession,
    view: &'a RequestView,
    field: &EditableField,
) -> &'a str {
    for edit in session.pending.iter().rev() {
        match (field, edit) {
            (EditableField::Url, FieldEdit::Url(v)) => return v.as_str(),
            (EditableField::HeaderValue(i), FieldEdit::HeaderValue { index, value })
                if i == index =>
            {
                return value.as_str();
            }
            (EditableField::QueryParamValue(i), FieldEdit::QueryParamValue { index, value })
                if i == index =>
            {
                return value.as_str();
            }
            (EditableField::PathParamValue(i), FieldEdit::PathParamValue { index, value })
                if i == index =>
            {
                return value.as_str();
            }
            (EditableField::BodyText, FieldEdit::BodyText(v)) => return v.as_str(),
            _ => {}
        }
    }
    match field {
        EditableField::Url => &view.url,
        EditableField::HeaderValue(i) => view.headers.get(*i).map_or("", |h| &h.value),
        EditableField::QueryParamValue(i) => view.query_params.get(*i).map_or("", |p| &p.value),
        EditableField::PathParamValue(i) => view.path_params.get(*i).map_or("", |p| &p.value),
        EditableField::BodyText => match &view.body {
            Some(body) => match &body.content {
                BodyContent::Text(text) => text.as_str(),
                _ => "",
            },
            None => "",
        },
    }
}

/// État d'activation d'un champ (en-tête ou paramètre) : vue + éditions en attente (D3).
pub fn field_enabled(session: &EditSession, view: &RequestView, field: &EditableField) -> bool {
    for edit in session.pending.iter().rev() {
        match (field, edit) {
            (EditableField::HeaderValue(i), FieldEdit::HeaderEnabled { index, enabled })
                if i == index =>
            {
                return *enabled;
            }
            (
                EditableField::QueryParamValue(i),
                FieldEdit::QueryParamEnabled { index, enabled },
            ) if i == index => {
                return *enabled;
            }
            (EditableField::PathParamValue(i), FieldEdit::PathParamEnabled { index, enabled })
                if i == index =>
            {
                return *enabled;
            }
            _ => {}
        }
    }
    match field {
        EditableField::HeaderValue(i) => view.headers.get(*i).is_none_or(|h| h.enabled),
        EditableField::QueryParamValue(i) => view.query_params.get(*i).is_none_or(|p| p.enabled),
        EditableField::PathParamValue(i) => view.path_params.get(*i).is_none_or(|p| p.enabled),
        EditableField::Url | EditableField::BodyText => true,
    }
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
    /// Taille du terminal (colonnes, lignes).
    pub size: (u16, u16),
    /// `Some` : la boucle doit s'arrêter.
    pub exit: Option<Exit>,
    /// État des exécutions de requêtes.
    pub run: RunState,
    /// État du filtre de réponse, `None` tant que `|` n'a pas été ouvert.
    pub filter: Option<FilterState>,
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
    /// Jeton de la copie en cours, s'il y en a une : un `ClipboardResult`
    /// dont le jeton ne correspond plus est ignoré (résultat en retard sur
    /// une copie plus récente).
    pub(crate) pending_clipboard_token: Option<u64>,
    /// Prochain jeton à distribuer à une copie.
    pub(crate) next_clipboard_token: u64,
    /// Session d'édition active sur la requête du focus Détail.
    pub editing: Option<EditSession>,
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

/// Message affiché temporairement dans la barre d'état.
#[derive(Debug, Clone)]
pub enum StatusMessage {
    Copied,
    ClipboardError(String),
    /// Recherche validée sans aucune correspondance.
    NoMatch,
    /// Échec lors de la sauvegarde sur disque.
    SaveError(String),
    /// Changement de requête refusé : la session d'édition porte des
    /// modifications non enregistrées.
    EditLocked,
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
            size,
            exit: None,
            run: RunState::default(),
            filter: None,
            search: None,
            detail_selection: None,
            response_selection: None,
            detail_match: None,
            response_match: None,
            last_status: None,
            pending_clipboard_token: None,
            next_clipboard_token: 0,
            editing: None,
            confirm: None,
            current_environment: None,
            environment_selected: 0,
            secrets: SecretsState::default(),
        }
    }

    /// Ce que la boucle doit capturer au clavier hors navigation normale.
    /// Conçu pour que `add-field-editing`/`add-response-filter` y ajoutent
    /// leur propre branche sans toucher à celle-ci.
    pub fn text_capture(&self) -> Option<TextCapture> {
        // Une confirmation en attente reprend la main sur le clavier : ses
        // réponses (`y`, `n`, `Entrée`, `Échap`) ne doivent pas devenir du
        // texte de saisie.
        if self.confirm.is_some() {
            return None;
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
        {
            return Some(TextCapture::Input);
        }
        if self.search.as_ref().is_some_and(SearchState::is_editing) {
            return Some(TextCapture::Search);
        }
        if self.filter.as_ref().is_some_and(|f| f.editing) {
            return Some(TextCapture::Filter);
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

/// Noms `vars:secret` de l'environnement courant, vide sans environnement
/// ou si l'environnement courant n'existe plus.
pub fn current_secret_names(model: &Model) -> &[String] {
    let (Some(collection), Some(current)) = (model.loaded(), &model.current_environment) else {
        return &[];
    };
    collection
        .environments
        .iter()
        .find_map(|entry| match entry {
            Ok(env) if &env.name == current => Some(env.secret_names.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
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

/// Lignes du panneau des variables secrètes : mappings `--secret`, puis
/// `vars:secret` de l'environnement courant, puis ajouts, sans doublon.
/// Une valeur saisie prime sur la résolution `.env`/shell.
pub fn secret_rows(model: &Model) -> Vec<SecretRow> {
    let state = &model.secrets;
    let declared = current_secret_names(model);
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

/// Surcharges `--env-var` à transmettre : chaque ligne ayant une valeur.
pub fn secret_env_vars(model: &Model) -> Vec<(String, SecretString)> {
    secret_rows(model)
        .into_iter()
        .filter_map(|row| row.value.map(|value| (row.name, value)))
        .collect()
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

    #[test]
    fn editable_field_list_for_request_view() {
        use crate::collection::{BodyContent, BodyKind, BodyView, KeyValue, RequestView};

        let make_view = |body_kind: Option<BodyKind>, headers_count: usize| RequestView {
            name: Some("test".into()),
            kind: Some("http".into()),
            seq: Some(1),
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: (0..headers_count)
                .map(|i| KeyValue {
                    key: format!("H{i}"),
                    value: format!("V{i}"),
                    enabled: true,
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
            assertions: vec![],
        };

        // URL + 2 en-têtes + corps json -> 4 champs dans l'ordre
        let view_json = make_view(Some(BodyKind::Json), 2);
        let fields = EditableField::list_for(&view_json);
        assert_eq!(
            fields,
            vec![
                EditableField::Url,
                EditableField::HeaderValue(0),
                EditableField::HeaderValue(1),
                EditableField::BodyText,
            ]
        );

        // corps formUrlEncoded -> corps exclu
        let view_form = make_view(Some(BodyKind::FormUrlEncoded), 2);
        let fields_form = EditableField::list_for(&view_form);
        assert_eq!(
            fields_form,
            vec![
                EditableField::Url,
                EditableField::HeaderValue(0),
                EditableField::HeaderValue(1),
            ]
        );

        // aucun en-tête/paramètre -> liste réduite à URL (+ corps si éditable)
        let view_no_headers = make_view(None, 0);
        let fields_no_headers = EditableField::list_for(&view_no_headers);
        assert_eq!(fields_no_headers, vec![EditableField::Url]);

        let view_only_body = make_view(Some(BodyKind::Text), 0);
        let fields_only_body = EditableField::list_for(&view_only_body);
        assert_eq!(
            fields_only_body,
            vec![EditableField::Url, EditableField::BodyText]
        );
    }

    #[test]
    fn field_value_and_field_enabled_read_pending_or_fallback() {
        use crate::collection::{BodyContent, BodyKind, BodyView, KeyValue, RequestView};
        use std::path::PathBuf;

        let view = RequestView {
            name: Some("test".into()),
            kind: Some("http".into()),
            seq: Some(1),
            method: "GET".into(),
            url: "https://initial.com".into(),
            headers: vec![
                KeyValue {
                    key: "H0".into(),
                    value: "V0".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "H1".into(),
                    value: "V1".into(),
                    enabled: false,
                },
            ],
            query_params: vec![KeyValue {
                key: "Q0".into(),
                value: "QV0".into(),
                enabled: true,
            }],
            path_params: vec![],
            body: Some(BodyView {
                kind: BodyKind::Json,
                content: BodyContent::Text("{\"init\": true}".into()),
            }),
            auth: None,
            has_pre_request_script: false,
            has_post_response_script: false,
            has_tests: false,
            has_assert: false,
            assertions: vec![],
        };

        let stamp = crate::writer::FileStamp::capture(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/simple.bru"),
        )
        .expect("stamp");

        let mut session = EditSession {
            path: PathBuf::from("req.bru"),
            stamp,
            fields: EditableField::list_for(&view),
            cursor: 0,
            state: EditState::FieldSelect,
            pending: vec![],
            dirty: false,
            hscroll: 0,
        };

        // Aucune édition en attente -> valeur et activation d'origine
        assert_eq!(
            field_value(&session, &view, &EditableField::Url),
            "https://initial.com"
        );
        assert_eq!(
            field_value(&session, &view, &EditableField::HeaderValue(0)),
            "V0"
        );
        assert!(field_enabled(
            &session,
            &view,
            &EditableField::HeaderValue(0)
        ));
        assert_eq!(
            field_value(&session, &view, &EditableField::HeaderValue(1)),
            "V1"
        );
        assert!(!field_enabled(
            &session,
            &view,
            &EditableField::HeaderValue(1)
        ));
        assert_eq!(
            field_value(&session, &view, &EditableField::BodyText),
            "{\"init\": true}"
        );

        // Une édition en attente sur HeaderValue(0)
        session.pending.push(FieldEdit::HeaderValue {
            index: 0,
            value: "V0-edited".into(),
        });
        session.pending.push(FieldEdit::HeaderEnabled {
            index: 0,
            enabled: false,
        });

        // L'indice courant 0 est édité
        assert_eq!(
            field_value(&session, &view, &EditableField::HeaderValue(0)),
            "V0-edited"
        );
        assert!(!field_enabled(
            &session,
            &view,
            &EditableField::HeaderValue(0)
        ));

        // Édition sur un autre indice (HeaderValue(0)) -> sans effet sur HeaderValue(1) ni Url
        assert_eq!(
            field_value(&session, &view, &EditableField::HeaderValue(1)),
            "V1"
        );
        assert!(!field_enabled(
            &session,
            &view,
            &EditableField::HeaderValue(1)
        ));
        assert_eq!(
            field_value(&session, &view, &EditableField::Url),
            "https://initial.com"
        );

        // Édition sur Url et BodyText
        session
            .pending
            .push(FieldEdit::Url("https://edited.com".into()));
        session
            .pending
            .push(FieldEdit::BodyText("{\"edited\": true}".into()));
        assert_eq!(
            field_value(&session, &view, &EditableField::Url),
            "https://edited.com"
        );
        assert_eq!(
            field_value(&session, &view, &EditableField::BodyText),
            "{\"edited\": true}"
        );
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
}
