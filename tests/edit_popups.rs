//! Édition par popups (`docs/edition-popups.md` § 6.1) : pour chaque popup,
//! ouverture depuis son onglet, modification au clavier, `Ctrl+S`, puis
//! comparaison octet à octet avec le fichier attendu des tests du writer
//! (`tests/fixtures/collections/writer-cases/*.after.bru`).
//!
//! Les touches passent par la vraie table de `to_message`, sur `update`
//! seul ; l'écriture demandée par `Command::SaveEdit` est faite ici avec
//! `BruWriter`, comme le ferait la boucle, puis son résultat est renvoyé à
//! `update` (`Message::EditSaved`), ce qui ferme le popup.

use std::fs;
use std::path::{Path, PathBuf};

use bruno_tui::app::event::AppEvent;
use bruno_tui::app::message::{Message, to_message};
use bruno_tui::app::model::{
    EditScope, EditState, EditableField, Model, PendingConfirm, RequestTab, SavedEdit,
};
use bruno_tui::app::update::{Command, refresh_rows, update};
use bruno_tui::collection::{BruFile, BruLoader, CollectionLoader, FileMeta, RequestView};
use bruno_tui::writer::{BruWriter, RequestWriter};
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases")
}

/// Collection temporaire : `bruno.json` et chaque cas copié à sa place
/// (`(cas, destination)`, ex. `("folder-ops.bru", "grp/folder.bru")`).
struct Ui {
    model: Model,
    root: PathBuf,
}

impl Ui {
    fn new(tag: &str, files: &[(&str, &str)]) -> Self {
        let root = std::env::temp_dir().join(format!(
            "bruno-tui-edit-popups-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("répertoire de test");
        fs::copy(cases().join("bruno.json"), root.join("bruno.json")).expect("bruno.json");
        for (case, destination) in files {
            let target = root.join(destination);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).expect("dossier");
            }
            fs::copy(cases().join(case), &target).expect("copie du cas");
        }
        let mut model = Model::new(root.clone(), (120, 40));
        update(&mut model, Message::CollectionLoaded(BruLoader.load(&root)));
        Self { model, root }
    }

    /// Sélectionne le nœud de chemin `path` (relatif à la racine) et donne
    /// le focus au détail.
    fn select(&mut self, path: &str) {
        let target = Path::new(path);
        for ancestor in target.ancestors().skip(1) {
            if !ancestor.as_os_str().is_empty() {
                self.model.tree.expanded.insert(ancestor.to_path_buf());
            }
        }
        refresh_rows(&mut self.model);
        let index = self
            .model
            .tree
            .rows
            .iter()
            .position(|row| {
                self.model
                    .node_at(&row.address)
                    .is_some_and(|node| node.path() == target)
            })
            .unwrap_or_else(|| panic!("`{path}` introuvable"));
        self.model.tree.selected = index;
        self.model.selected_collection = false;
        self.model.focus = bruno_tui::app::model::Focus::Detail;
    }

    /// Sélectionne la collection (`collection.bru`).
    fn select_collection(&mut self) {
        self.model.selected_collection = true;
        self.model.focus = bruno_tui::app::model::Focus::Detail;
    }

    fn event(&mut self, code: KeyCode, modifiers: KeyModifiers) -> Command {
        let event = AppEvent::Terminal(Event::Key(KeyEvent::new(code, modifiers)));
        match to_message(event, self.model.text_capture()) {
            Some(message) => update(&mut self.model, message),
            None => Command::None,
        }
    }

    fn key(&mut self, code: KeyCode) -> Command {
        self.event(code, KeyModifiers::NONE)
    }

    fn keys(&mut self, code: KeyCode, times: usize) {
        for _ in 0..times {
            self.key(code);
        }
    }

    fn chars(&mut self, text: &str) {
        for c in text.chars() {
            self.key(KeyCode::Char(c));
        }
    }

    /// Affiche l'onglet `tab` et ouvre son popup (`Entrée`).
    fn open(&mut self, tab: RequestTab) {
        self.model.request_tab = tab;
        self.key(KeyCode::Enter);
        let scope = self.model.editing.as_ref().map(|s| s.scope);
        assert_eq!(scope, Some(EditScope::Tab(tab)), "popup {tab:?} ouvert");
    }

    fn field(&self) -> Option<EditableField> {
        self.model.editing.as_ref()?.current_field()
    }

    /// `Ctrl+S` : écrit ce que demande `Command::SaveEdit` et renvoie le
    /// résultat à `update`. Le popup doit être fermé ensuite.
    fn save(&mut self) {
        let command = self.event(KeyCode::Char('s'), KeyModifiers::CONTROL);
        self.write(command);
    }

    fn write(&mut self, command: Command) {
        let Command::SaveEdit {
            path,
            ast,
            stamp,
            edits,
        } = command
        else {
            panic!("sauvegarde attendue, obtenu {command:?}");
        };
        let full_path = self.root.join(&path);
        let new_stamp = BruWriter
            .write_request(&full_path, &ast, &stamp, &edits)
            .expect("écriture");
        let text = fs::read_to_string(&full_path).expect("relecture");
        let ast = BruFile::parse(text).expect("AST");
        let view = RequestView::from_ast(&ast)
            .unwrap_or_else(|_| FileMeta::from_ast(ast.clone()).to_request_view());
        update(
            &mut self.model,
            Message::EditSaved {
                path,
                result: Ok(SavedEdit {
                    stamp: new_stamp,
                    ast,
                    view,
                }),
            },
        );
        assert!(self.model.editing.is_none(), "Ctrl+S ferme le popup");
    }

    /// Le fichier `destination` est identique à `<cas>.after.bru`.
    fn assert_matches(&self, destination: &str, case: &str) {
        let expected = fs::read_to_string(cases().join(case)).expect("fichier attendu");
        let written = fs::read_to_string(self.root.join(destination)).expect("relecture");
        assert_eq!(written, expected, "{destination} contre {case}");
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// --- Requête (`m`) -----------------------------------------------------------

#[test]
fn request_popup_changes_the_method_with_arrows() {
    let mut ui = Ui::new("method", &[("method.bru", "method.bru")]);
    ui.select("method.bru");
    ui.key(KeyCode::Char('m'));
    assert_eq!(ui.field(), Some(EditableField::Method));
    ui.key(KeyCode::Right);
    ui.save();
    ui.assert_matches("method.bru", "method.after.bru");
}

#[test]
fn request_popup_edits_the_url_from_any_tab() {
    let mut ui = Ui::new("url", &[("simple.bru", "simple.bru")]);
    ui.select("simple.bru");
    // `m` ouvre le popup Requête depuis n'importe quel onglet.
    ui.model.request_tab = RequestTab::Docs;
    ui.key(KeyCode::Char('m'));
    ui.key(KeyCode::Down);
    assert_eq!(ui.field(), Some(EditableField::Url));
    ui.key(KeyCode::Enter);
    ui.keys(KeyCode::Backspace, 4);
    ui.chars("pong");
    ui.save();
    ui.assert_matches("simple.bru", "simple.after.bru");
    // L'onglet affiché n'a pas changé.
    assert_eq!(ui.model.request_tab, RequestTab::Docs);
}

// --- Params ------------------------------------------------------------------

#[test]
fn params_popup_adds_disables_and_removes_query_params() {
    let mut ui = Ui::new("query-sync", &[("query-sync.bru", "query-sync.bru")]);
    ui.select("query-sync.bru");
    ui.open(RequestTab::Params);
    ui.key(KeyCode::Char('a'));
    ui.chars("sort");
    ui.key(KeyCode::Tab);
    ui.chars("asc");
    ui.key(KeyCode::Enter);
    assert_eq!(ui.field(), Some(EditableField::QueryParamValue(2)));
    ui.key(KeyCode::Up);
    ui.key(KeyCode::Char(' '));
    ui.key(KeyCode::Up);
    ui.key(KeyCode::Char('d'));
    ui.save();
    ui.assert_matches("query-sync.bru", "query-sync.after.bru");
}

#[test]
fn params_popup_adds_a_path_param_in_its_part() {
    let mut ui = Ui::new(
        "add-path",
        &[("add-path-between.bru", "add-path-between.bru")],
    );
    ui.select("add-path-between.bru");
    ui.open(RequestTab::Params);
    ui.key(KeyCode::Tab);
    assert_eq!(
        ui.field(),
        Some(EditableField::AddRow(
            bruno_tui::writer::EntrySection::PathParams
        ))
    );
    ui.key(KeyCode::Char('a'));
    ui.chars("id");
    ui.key(KeyCode::Tab);
    ui.chars("42");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("add-path-between.bru", "add-path-between.after.bru");
}

// --- En-têtes ----------------------------------------------------------------

#[test]
fn headers_popup_adds_an_entry() {
    let mut ui = Ui::new("add-header", &[("add-header.bru", "add-header.bru")]);
    ui.select("add-header.bru");
    ui.open(RequestTab::Headers);
    ui.key(KeyCode::Char('a'));
    ui.chars("X-Request-Id");
    ui.key(KeyCode::Tab);
    ui.chars("{{requestId}}");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("add-header.bru", "add-header.after.bru");
}

#[test]
fn headers_popup_renames_a_disabled_key() {
    let mut ui = Ui::new("rename", &[("rename-disabled.bru", "rename-disabled.bru")]);
    ui.select("rename-disabled.bru");
    ui.open(RequestTab::Headers);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Char('c'));
    ui.keys(KeyCode::Backspace, "X-Debug".len());
    ui.chars("X-Verbose");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("rename-disabled.bru", "rename-disabled.after.bru");
}

/// Deux popups l'un après l'autre (En-têtes, puis Params) : deux
/// écritures successives donnent le même fichier qu'une seule.
#[test]
fn headers_then_params_popups_remove_entries() {
    let mut ui = Ui::new("remove", &[("remove-entry.bru", "remove-entry.bru")]);
    ui.select("remove-entry.bru");
    ui.open(RequestTab::Headers);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Char('d'));
    ui.save();
    ui.open(RequestTab::Params);
    ui.key(KeyCode::Tab);
    assert_eq!(ui.field(), Some(EditableField::PathParamValue(0)));
    ui.key(KeyCode::Char('d'));
    ui.save();
    ui.assert_matches("remove-entry.bru", "remove-entry.after.bru");
}

// --- Vars, Assert, Settings ----------------------------------------------------

#[test]
fn vars_popup_edits_removes_and_adds_in_both_parts() {
    let mut ui = Ui::new("vars", &[("vars-ops.bru", "vars-ops.bru")]);
    ui.select("vars-ops.bru");
    ui.open(RequestTab::Vars);
    ui.key(KeyCode::Enter);
    ui.keys(KeyCode::Backspace, "secret123".len());
    ui.chars("updated_token");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Char('d'));
    // La partie Post-réponse est vide : sa ligne d'ajout prend le curseur.
    assert_eq!(
        ui.field(),
        Some(EditableField::AddRow(
            bruno_tui::writer::EntrySection::VarsPostResponse
        ))
    );
    ui.key(KeyCode::Char('a'));
    ui.chars("res_id");
    ui.key(KeyCode::Tab);
    ui.chars("res.body.id");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("vars-ops.bru", "vars-ops.after.bru");
}

#[test]
fn assert_popup_edits_disables_and_adds() {
    let mut ui = Ui::new("assert", &[("assert-ops.bru", "assert-ops.bru")]);
    ui.select("assert-ops.bru");
    ui.open(RequestTab::Assert);
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Backspace);
    ui.chars("1");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Char(' '));
    ui.key(KeyCode::Char('a'));
    ui.chars("res.headers['content-type']");
    ui.key(KeyCode::Tab);
    ui.chars("matches json");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("assert-ops.bru", "assert-ops.after.bru");
}

#[test]
fn settings_popup_edits_values_only() {
    let mut ui = Ui::new("settings", &[("settings-ops.bru", "settings-ops.bru")]);
    ui.select("settings-ops.bru");
    ui.open(RequestTab::Settings);
    // Ni ajout, ni renommage, ni suppression, ni bascule.
    for key in ['a', 'c', 'd', ' '] {
        ui.key(KeyCode::Char(key));
        let session = ui.model.editing.as_ref().expect("popup");
        assert_eq!(session.state, EditState::FieldSelect, "{key}");
        assert!(!session.dirty, "{key}");
    }
    ui.key(KeyCode::Enter);
    ui.keys(KeyCode::Backspace, "true".len());
    ui.chars("false");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Backspace);
    ui.chars("5000");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("settings-ops.bru", "settings-ops.after.bru");
}

// --- Auth --------------------------------------------------------------------

/// Paramètres d'auth affichés par le popup : `(clé, valeur)`.
fn auth_rows(ui: &Ui) -> Vec<(String, String)> {
    ui.model
        .editing
        .as_ref()
        .expect("popup")
        .preview
        .auth_params()
        .iter()
        .map(|kv| (kv.key.clone(), kv.value.clone()))
        .collect()
}

fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Changer de mode remplace aussitôt les lignes ; revenir au mode
/// d'origine retrouve les valeurs ; `Ctrl+S` demande confirmation avant de
/// supprimer l'ancien bloc non vide (`n` n'écrit rien, `y` écrit).
#[test]
fn auth_popup_switches_mode_restores_values_and_confirms() {
    let mut ui = Ui::new(
        "auth-switch",
        &[("auth-mode-switch.bru", "auth-mode-switch.bru")],
    );
    let before = fs::read(ui.root.join("auth-mode-switch.bru")).expect("lecture");
    ui.select("auth-mode-switch.bru");
    ui.open(RequestTab::Auth);
    assert_eq!(ui.field(), Some(EditableField::AuthMode));
    let original = rows(&[("username", "alice"), ("password", "secretpassword")]);
    assert_eq!(auth_rows(&ui), original);

    ui.key(KeyCode::Right);
    assert_eq!(auth_rows(&ui), rows(&[("token", "")]));
    let fields = ui.model.editing.as_ref().map(|s| s.fields.clone());
    assert_eq!(
        fields,
        Some(vec![
            EditableField::AuthMode,
            EditableField::AuthParamValue(0)
        ])
    );
    ui.key(KeyCode::Left);
    assert_eq!(auth_rows(&ui), original);
    ui.key(KeyCode::Right);

    let command = ui.event(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(matches!(command, Command::None));
    assert_eq!(
        ui.model.confirm,
        Some(PendingConfirm::DeleteAuthBlock {
            former_mode: "basic".into()
        })
    );
    ui.key(KeyCode::Char('n'));
    assert!(ui.model.editing.is_some());
    assert_eq!(
        fs::read(ui.root.join("auth-mode-switch.bru")).expect("lecture"),
        before
    );

    ui.event(KeyCode::Char('s'), KeyModifiers::CONTROL);
    let command = ui.key(KeyCode::Char('y'));
    ui.write(command);
    ui.assert_matches("auth-mode-switch.bru", "auth-mode-switch.after.bru");
}

#[test]
fn auth_popup_keeps_a_plaintext_token_on_g() {
    let mut ui = Ui::new("auth-param", &[("auth-param.bru", "auth-param.bru")]);
    ui.select("auth-param.bru");
    ui.open(RequestTab::Auth);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.keys(KeyCode::Backspace, "initial_token".len());
    ui.chars("updated_token");
    ui.key(KeyCode::Enter);
    assert!(matches!(
        ui.model.editing.as_ref().map(|s| &s.state),
        Some(EditState::PlaintextSecretPrompt { .. })
    ));
    ui.key(KeyCode::Char('g'));
    ui.save();
    ui.assert_matches("auth-param.bru", "auth-param.after.bru");
}

/// Secret en clair : `Échap` revient à la saisie avec le texte tapé,
/// `Entrée` le remplace par la variable suggérée.
#[test]
fn auth_popup_replaces_a_plaintext_token_by_a_variable() {
    let mut ui = Ui::new("auth-replace", &[("auth-param.bru", "auth-param.bru")]);
    ui.select("auth-param.bru");
    ui.open(RequestTab::Auth);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.chars("X");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Esc);
    match ui.model.editing.as_ref().map(|s| &s.state) {
        Some(EditState::Input(input)) => assert_eq!(input.text(), "initial_tokenX"),
        other => panic!("saisie attendue : {other:?}"),
    }
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Enter);
    assert_eq!(auth_rows(&ui), rows(&[("token", "{{bearerToken}}")]));
    ui.save();
    let written = fs::read_to_string(ui.root.join("auth-param.bru")).expect("relecture");
    assert!(
        written.contains("auth:bearer {\n  token: {{bearerToken}}\n}"),
        "{written}"
    );
}

/// Mode apikey : clé, valeur (variable, sans question), puis placement
/// choisi avec `→`.
#[test]
fn auth_popup_sets_apikey_with_its_placement() {
    let mut ui = Ui::new("auth-apikey", &[("auth-apikey.bru", "auth-apikey.bru")]);
    ui.select("auth-apikey.bru");
    ui.open(RequestTab::Auth);
    // none, inherit, basic, bearer, digest, ntlm, apikey
    ui.keys(KeyCode::Right, 6);
    assert_eq!(
        auth_rows(&ui),
        rows(&[("key", ""), ("value", ""), ("placement", "")])
    );
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.chars("X-API-KEY");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.chars("{{secretKey}}");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Right);
    ui.save();
    ui.assert_matches("auth-apikey.bru", "auth-apikey.after.bru");
}

#[test]
fn auth_popup_on_a_folder() {
    let mut ui = Ui::new("folder-auth", &[("folder-auth.bru", "grp/folder.bru")]);
    ui.select("grp");
    ui.open(RequestTab::Auth);
    ui.keys(KeyCode::Right, 2);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.chars("admin");
    ui.key(KeyCode::Enter);
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.chars("{{secretPassword}}");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("grp/folder.bru", "folder-auth.after.bru");
}

/// Collection : pas de mode `inherit` dans la liste.
#[test]
fn auth_popup_on_the_collection() {
    let mut ui = Ui::new(
        "collection-auth",
        &[("collection-auth.bru", "collection.bru")],
    );
    ui.select_collection();
    ui.open(RequestTab::Auth);
    // none, basic, bearer
    ui.keys(KeyCode::Right, 2);
    assert_eq!(auth_rows(&ui), rows(&[("token", "")]));
    ui.key(KeyCode::Down);
    ui.key(KeyCode::Enter);
    ui.chars("{{collToken}}");
    ui.key(KeyCode::Enter);
    ui.save();
    ui.assert_matches("collection.bru", "collection-auth.after.bru");
}

// --- Fermeture -------------------------------------------------------------

/// `Échap` sans changement ferme aussitôt ; avec changement, il demande
/// confirmation : `n` garde le popup, `y` l'abandonne sans rien écrire.
#[test]
fn escape_closes_or_asks_before_discarding() {
    let mut ui = Ui::new("escape", &[("simple.bru", "simple.bru")]);
    let before = fs::read(ui.root.join("simple.bru")).expect("lecture");
    ui.select("simple.bru");
    ui.key(KeyCode::Char('m'));
    ui.key(KeyCode::Esc);
    assert!(ui.model.editing.is_none());
    assert!(ui.model.confirm.is_none());

    ui.key(KeyCode::Char('m'));
    ui.key(KeyCode::Right);
    ui.key(KeyCode::Esc);
    assert_eq!(ui.model.confirm, Some(PendingConfirm::DiscardEdit));
    ui.key(KeyCode::Char('n'));
    assert!(ui.model.editing.as_ref().is_some_and(|s| s.dirty));
    ui.key(KeyCode::Esc);
    ui.key(KeyCode::Char('y'));
    assert!(ui.model.editing.is_none());
    assert_eq!(
        fs::read(ui.root.join("simple.bru")).expect("lecture"),
        before
    );

    // `Ctrl+S` sans changement ferme le popup sans écrire.
    ui.key(KeyCode::Char('m'));
    let command = ui.event(KeyCode::Char('s'), KeyModifiers::CONTROL);
    assert!(matches!(command, Command::None));
    assert!(ui.model.editing.is_none());
}
