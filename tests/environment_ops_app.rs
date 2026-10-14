//! Opérations sur les environnements depuis l'interface (`docs/environnements.md`) :
//! touches réelles (`to_message`) dans le panneau Environnement, popups,
//! confirmation `y`/`n`, puis exécution de `Command::FileOp` comme la boucle
//! (corbeille dans un `XDG_DATA_HOME` temporaire) et rechargement.

use std::fs;
use std::path::{Path, PathBuf};

use bruno_tui::app::event::AppEvent;
use bruno_tui::app::message::{Message, to_message};
use bruno_tui::app::model::{EnvSeries, Focus, Model, StatusMessage};
use bruno_tui::app::update::{Command, update};
use bruno_tui::app::view;
use bruno_tui::collection::{BruLoader, CollectionFormat, CollectionLoader};
use bruno_tui::writer::files;
use bruno_tui::writer::trash::Trash;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

const DEV: &str = "vars {\n  host: dev.example\n}\n";
const PROD: &str = "vars {\n  host: prod.example\n}\n";

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/collections/writer-cases/files/collection")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("copie");
    for entry in fs::read_dir(from).expect("lecture") {
        let entry = entry.expect("entrée");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copie fichier");
        }
    }
}

struct Ui {
    model: Model,
    root: PathBuf,
    data: PathBuf,
}

impl Ui {
    fn base(tag: &str) -> PathBuf {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("tmp-file-ops-app-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        base
    }

    /// Copie de la mini-collection avec les environnements `envs`
    /// (`nom`, contenu), panneau Environnement ouvert (`E`).
    fn new(tag: &str, envs: &[(&str, &str)]) -> Self {
        let base = Self::base(tag);
        let root = base.join("collection");
        copy_dir(&fixture(), &root);
        fs::create_dir_all(root.join("environments")).expect("dossier");
        for (name, content) in envs {
            fs::write(root.join(format!("environments/{name}.bru")), content).expect("env");
        }
        let mut model = Model::new(root.clone(), (120, 40));
        update(&mut model, Message::CollectionLoaded(BruLoader.load(&root)));
        let mut ui = Self {
            model,
            root,
            data: base.join("data"),
        };
        ui.press(KeyCode::Char('E'));
        ui
    }

    /// Exécute une commande comme la boucle : opération de fichier, puis
    /// rechargement.
    fn run(&mut self, command: Command) {
        match command {
            Command::FileOp { root, op } => {
                let trash = Trash::at(self.data.join("Trash"));
                let result = files::execute(&root, &op, || Ok(trash), "2026-10-14T12:00:00");
                let next = update(&mut self.model, Message::FileOpDone { root, result });
                self.run(next);
            }
            Command::ReloadCollection => {
                let result = BruLoader.load(&self.root);
                let next = update(&mut self.model, Message::CollectionLoaded(result));
                self.run(next);
            }
            Command::SaveEnvironment {
                path,
                ast,
                stamp,
                edits,
            } => {
                // La boucle écrit hors thread principal ; ici, directement.
                bruno_tui::writer::write_environment(&self.root.join(path), &ast, &stamp, &edits)
                    .expect("écriture de l'environnement");
            }
            Command::Batch(commands) => commands.into_iter().for_each(|c| self.run(c)),
            _ => {}
        }
    }

    fn press_with(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let event = AppEvent::Terminal(Event::Key(KeyEvent::new(code, modifiers)));
        if let Some(message) = to_message(event, self.model.text_capture()) {
            let command = update(&mut self.model, message);
            self.run(command);
        }
    }

    fn press(&mut self, code: KeyCode) {
        self.press_with(code, KeyModifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    fn save(&mut self) {
        self.press_with(KeyCode::Char('s'), KeyModifiers::CONTROL);
    }

    fn exists(&self, path: &str) -> bool {
        self.root.join(path).exists()
    }

    /// Place le curseur du panneau sur l'environnement `name`.
    fn pick(&mut self, name: &str) {
        let index = self
            .model
            .loaded()
            .expect("collection")
            .environments
            .iter()
            .position(|e| e.as_ref().is_ok_and(|env| env.name == name))
            .expect("environnement");
        self.model.environment_selected = index + 1;
    }

    /// Efface le champ texte sous le curseur.
    fn clear_field(&mut self) {
        self.press(KeyCode::End);
        for _ in 0..80 {
            self.press(KeyCode::Backspace);
        }
    }

    fn refused(&self, text: &str) -> bool {
        self.status() == Some(StatusMessage::FileOpRefused(text.into()))
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).expect("lecture")
    }

    fn status(&self) -> Option<StatusMessage> {
        self.model.last_status.clone()
    }

    fn screen(&self, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| view::view(&self.model, frame))
            .expect("rendu");
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[test]
fn a_creates_an_environment_with_exact_content_and_selects_it() {
    let mut ui = Ui::new("create", &[("dev", DEV)]);
    ui.press(KeyCode::Char('a'));
    assert_eq!(
        ui.model.file_popup.as_ref().expect("popup").title(),
        " Nouvel environnement "
    );
    ui.type_text("staging");
    ui.press(KeyCode::Enter);

    assert!(ui.model.file_popup.is_none());
    assert_eq!(ui.read("environments/staging.bru"), "vars {\n}\n");
    let selected = ui.model.environment_selected;
    let names: Vec<String> = ui
        .model
        .loaded()
        .expect("collection")
        .environments
        .iter()
        .flatten()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names[selected - 1], "staging");
    assert_eq!(ui.model.focus, Focus::EnvironmentPicker);
}

#[test]
fn a_taken_name_is_refused_in_the_popup_without_any_suffix() {
    let mut ui = Ui::new("taken", &[("dev", DEV), ("Été", DEV)]);
    ui.press(KeyCode::Char('a'));
    ui.type_text("DEV");
    ui.press(KeyCode::Enter);
    let popup = ui.model.file_popup.as_ref().expect("popup reste ouvert");
    assert_eq!(
        popup.error.as_deref(),
        Some("Un environnement de ce nom existe déjà")
    );
    assert!(!ui.exists("environments/DEV.bru"));

    ui.clear_field();
    ui.type_text("été");
    ui.press(KeyCode::Enter);
    assert!(ui.model.file_popup.is_some(), "Unicode : été = Été");
    assert!(!ui.exists("environments/été.bru"));
    assert!(!ui.exists("environments/DEV 1.bru"));
}

#[test]
fn f2_renames_and_follows_the_active_environment_and_extends_of_siblings() {
    let child = "extends: dev\n\nvars {\n  a: 1\n}\n";
    let mut ui = Ui::new("rename", &[("dev", DEV), ("child", child), ("prod", PROD)]);
    ui.pick("dev");
    ui.press(KeyCode::Enter);
    assert_eq!(ui.model.current_environment.as_deref(), Some("dev"));
    ui.press(KeyCode::F(2));
    assert_eq!(
        ui.model.file_popup.as_ref().expect("popup").title(),
        " Renommer l'environnement — dev "
    );
    ui.clear_field();
    ui.type_text("integration");
    ui.press(KeyCode::Enter);

    assert!(!ui.exists("environments/dev.bru"));
    assert_eq!(ui.read("environments/integration.bru"), DEV);
    assert_eq!(
        ui.read("environments/child.bru"),
        "extends: integration\n\nvars {\n  a: 1\n}\n"
    );
    assert_eq!(ui.read("environments/prod.bru"), PROD);
    assert_eq!(ui.model.current_environment.as_deref(), Some("integration"));
}

#[test]
fn f2_on_a_taken_name_is_refused() {
    let mut ui = Ui::new("rename-taken", &[("dev", DEV), ("prod", PROD)]);
    ui.pick("dev");
    ui.press(KeyCode::F(2));
    ui.clear_field();
    ui.type_text("PROD");
    ui.press(KeyCode::Enter);
    assert!(ui.model.file_popup.as_ref().expect("popup").error.is_some());
    assert!(ui.exists("environments/dev.bru"));
    assert_eq!(ui.read("environments/prod.bru"), PROD);
}

#[test]
fn rename_is_refused_when_the_file_changed_on_disk() {
    let mut ui = Ui::new("rename-stale", &[("dev", DEV)]);
    ui.pick("dev");
    ui.press(KeyCode::F(2));
    ui.clear_field();
    ui.type_text("qa");
    // Écriture externe entre l'ouverture du popup et la validation.
    fs::write(ui.root.join("environments/dev.bru"), PROD).expect("externe");
    ui.press(KeyCode::Enter);
    assert!(ui.exists("environments/dev.bru"));
    assert!(!ui.exists("environments/qa.bru"));
    assert_eq!(ui.read("environments/dev.bru"), PROD);
    assert!(matches!(ui.status(), Some(StatusMessage::FileOpRefused(_))));
}

#[test]
fn p_clones_byte_for_byte_and_prefills_copie() {
    let source = "vars {\n  b: 2\n  a: 1\n}\n\ncolor: #123456\n";
    let mut ui = Ui::new("clone", &[("dev", source)]);
    ui.pick("dev");
    ui.press(KeyCode::Char('p'));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.title(), " Cloner l'environnement — dev ");
    assert_eq!(popup.name.text(), "dev copie");
    ui.press(KeyCode::Enter);
    assert_eq!(ui.read("environments/dev copie.bru"), source);
    assert_eq!(ui.read("environments/dev.bru"), source);
}

#[test]
fn p_never_suffixes_a_taken_name_silently() {
    let mut ui = Ui::new("clone-taken", &[("dev", DEV), ("dev copie", PROD)]);
    ui.pick("dev");
    ui.press(KeyCode::Char('p'));
    ui.press(KeyCode::Enter);
    assert!(ui.model.file_popup.as_ref().expect("popup").error.is_some());
    assert!(!ui.exists("environments/dev copie 2.bru"));
    assert!(!ui.exists("environments/dev copie 1.bru"));
    assert_eq!(ui.read("environments/dev copie.bru"), PROD);
}

#[test]
fn d_then_n_cancels_and_d_then_y_trashes_and_resets_the_active_environment() {
    let mut ui = Ui::new("trash", &[("dev", DEV), ("prod", PROD)]);
    ui.pick("dev");
    ui.press(KeyCode::Enter);
    assert_eq!(ui.model.current_environment.as_deref(), Some("dev"));

    ui.press(KeyCode::Char('d'));
    assert!(ui.model.confirm.is_some());
    ui.press(KeyCode::Char('n'));
    assert!(ui.model.confirm.is_none());
    assert!(ui.exists("environments/dev.bru"));
    assert_eq!(ui.model.current_environment.as_deref(), Some("dev"));

    ui.press(KeyCode::Char('d'));
    ui.press(KeyCode::Char('y'));
    assert!(!ui.exists("environments/dev.bru"));
    assert!(ui.data.join("Trash/files/dev.bru").exists());
    assert_eq!(ui.model.current_environment, None);
    assert!(ui.exists("environments/prod.bru"));
}

#[test]
fn trash_is_refused_when_the_file_changed_on_disk() {
    let mut ui = Ui::new("trash-stale", &[("dev", DEV)]);
    ui.pick("dev");
    ui.press(KeyCode::Char('d'));
    fs::write(ui.root.join("environments/dev.bru"), PROD).expect("externe");
    ui.press(KeyCode::Char('y'));
    assert!(ui.exists("environments/dev.bru"));
    assert!(!ui.data.join("Trash").exists());
}

#[test]
fn operations_are_refused_on_aucun() {
    let mut ui = Ui::new("aucun", &[("dev", DEV)]);
    ui.model.environment_selected = 0;
    for key in [
        KeyCode::F(2),
        KeyCode::Char('p'),
        KeyCode::Char('d'),
        KeyCode::Char('e'),
    ] {
        ui.model.last_status = None;
        ui.press(key);
        assert!(ui.refused("Aucun n'est pas un environnement"), "{key:?}");
        assert!(ui.model.file_popup.is_none());
        assert!(ui.model.confirm.is_none());
        assert!(ui.model.environment_editing.is_none());
    }
}

#[test]
fn operations_are_refused_on_a_yaml_collection() {
    let mut ui = Ui::new("yaml", &[("dev", DEV)]);
    ui.model.loaded_mut().expect("collection").format = CollectionFormat::OpenCollection;
    ui.pick("dev");
    for key in [
        KeyCode::Char('a'),
        KeyCode::F(2),
        KeyCode::Char('p'),
        KeyCode::Char('d'),
    ] {
        ui.model.last_status = None;
        ui.press(key);
        assert_eq!(
            ui.status(),
            Some(StatusMessage::ReadOnlyCollection),
            "{key:?}"
        );
        assert!(ui.model.file_popup.is_none());
        assert!(ui.model.confirm.is_none());
    }
    assert_eq!(ui.read("environments/dev.bru"), DEV);
}

#[test]
fn operations_are_refused_during_a_run() {
    let mut ui = Ui::new("running", &[("dev", DEV)]);
    ui.model.series = Some(EnvSeries::new(PathBuf::new(), vec!["dev".into()]));
    ui.pick("dev");
    for key in [
        KeyCode::Char('a'),
        KeyCode::F(2),
        KeyCode::Char('p'),
        KeyCode::Char('d'),
    ] {
        ui.model.last_status = None;
        ui.press(key);
        assert!(
            ui.refused("Exécution en cours : opération sur les fichiers refusée"),
            "{key:?}"
        );
        assert!(ui.model.file_popup.is_none());
        assert!(ui.model.confirm.is_none());
    }
    assert!(ui.exists("environments/dev.bru"));
}

#[test]
fn space_toggles_a_variable_in_the_edit_popup_and_ctrl_s_writes_the_tilde() {
    let mut ui = Ui::new("space", &[("dev", "vars {\n  host: a\n  port: 80\n}\n")]);
    ui.pick("dev");
    ui.press(KeyCode::Char('e'));
    assert!(ui.model.environment_editing.is_some());
    ui.press(KeyCode::Char(' '));
    assert!(!ui.model.environment_editing.as_ref().unwrap().variables[0].enabled);
    ui.save();
    assert_eq!(
        ui.read("environments/dev.bru"),
        "vars {\n  ~host: a\n  port: 80\n}\n"
    );
}

#[test]
fn m_and_b_do_not_open_their_popups_from_the_environment_panel() {
    let mut ui = Ui::new("no-conflict", &[("dev", DEV)]);
    ui.pick("dev");
    ui.press(KeyCode::Char('m'));
    assert!(ui.model.move_popup.is_none());
    assert!(ui.model.file_popup.is_none());
    ui.press(KeyCode::Char('B'));
    assert!(ui.model.settings.is_none());
    assert_eq!(ui.model.focus, Focus::EnvironmentPicker);
    assert_eq!(ui.read("environments/dev.bru"), DEV);
}

#[test]
fn popups_render_at_80_and_120_columns() {
    for width in [80u16, 120] {
        let mut ui = Ui::new(&format!("render-{width}"), &[("dev", DEV)]);
        ui.pick("dev");
        for (key, title) in [
            (KeyCode::Char('a'), "Nouvel environnement"),
            (KeyCode::F(2), "Renommer l'environnement — dev"),
            (KeyCode::Char('p'), "Cloner l'environnement — dev"),
        ] {
            ui.press(key);
            let screen = ui.screen(width, 30);
            assert!(
                screen.contains(title),
                "{width} colonnes : {title}\n{screen}"
            );
            assert!(
                screen.contains("Entrée"),
                "{width} colonnes : aide du popup"
            );
            ui.press(KeyCode::Esc);
            assert!(ui.model.file_popup.is_none());
        }
        ui.press(KeyCode::Char('d'));
        let screen = ui.screen(width, 30);
        assert!(
            screen.contains("Mettre l'environnement « dev » à la corbeille ? (y/n)"),
            "{width} colonnes\n{screen}"
        );
    }
}
