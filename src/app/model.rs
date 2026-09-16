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
use crate::runner;
use crate::writer::{FieldEdit, FileStamp};

use super::filter::FilterState;
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

/// Session d'édition d'une requête, superposée au focus Détail.
#[derive(Debug, Clone)]
pub struct EditSession {
    /// Chemin de la requête éditée, relatif à la racine ; la session se
    /// ferme sans confirmation si la sélection change de nœud.
    pub path: PathBuf,
    pub stamp: FileStamp,
    /// Champs éditables, dans l'ordre d'affichage (D2).
    pub fields: Vec<EditableField>,
    /// Indice du champ sous le curseur, dans `fields`.
    pub cursor: usize,
    pub mode: EditMode,
    /// Modifications en attente, une par cible touchée (fusionnées par
    /// écrasement, dans l'ordre de `FieldEdit` attendu par bru-writer :
    /// pas besoin de dédupliquer davantage, le writer le fait déjà).
    pub pending: Vec<FieldEdit>,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditMode {
    Normal,
    /// Curseur de texte en indice de caractère (pas d'octet) et tampon de
    /// saisie en cours.
    Insert {
        text_cursor: usize,
        buffer: String,
    },
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

/// Valeur affichée d'un champ : tampon de saisie si en mode Insert sur ce
/// champ, sinon vue + éditions en attente (D3).
pub fn field_value<'a>(
    session: &'a EditSession,
    view: &'a RequestView,
    field: &EditableField,
) -> &'a str {
    if let EditMode::Insert { buffer, .. } = &session.mode
        && session.fields.get(session.cursor) == Some(field)
    {
        return buffer.as_str();
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
    /// Session d'édition active sur la requête du focus Détail.
    pub editing: Option<EditSession>,
    /// Confirmation en attente avant de fermer l'édition ou l'application.
    pub confirm: Option<PendingConfirm>,
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
            filter: None,
            search: None,
            detail_selection: None,
            detail_match: None,
            last_status: None,
            pending_clipboard_token: None,
            next_clipboard_token: 0,
            editing: None,
            confirm: None,
        }
    }

    /// Ce que la boucle doit capturer au clavier hors navigation normale.
    /// Conçu pour que `add-field-editing`/`add-response-filter` y ajoutent
    /// leur propre branche sans toucher à celle-ci.
    pub fn text_capture(&self) -> Option<TextCapture> {
        if self
            .editing
            .as_ref()
            .is_some_and(|s| matches!(s.mode, EditMode::Insert { .. }))
        {
            return Some(TextCapture::Insert);
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
            mode: EditMode::Normal,
            pending: vec![],
            dirty: false,
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
    }
}
