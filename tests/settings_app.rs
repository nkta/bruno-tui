//! Popup « Réglages » de la collection (`docs/reglages-collection.md`) :
//! touches réelles (`to_message`), `Ctrl+S`, exécution de
//! `Command::SaveSettings` comme la boucle, puis comparaison à l'octet près
//! avec les fichiers de `tests/fixtures/collections/settings-cases/`, générés
//! par le vrai code de Bruno (`scripts/gen-settings-fixtures.sh`).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::sync::atomic::{AtomicU32, Ordering};

use bruno_tui::app::event::AppEvent;
use bruno_tui::app::message::{Message, to_message};
use bruno_tui::app::model::{EnvSeries, Model, StatusMessage};
use bruno_tui::app::update::settings::execute_save;
use bruno_tui::app::update::{Command, update};
use bruno_tui::app::view;
use bruno_tui::collection::{BruLoader, CollectionLoader, OpenCollectionLoader};
use bruno_tui::writer::bruno_config::{Json, convert_legacy_proxy};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/settings-cases")
}

fn case(name: &str, suffix: &str) -> String {
    fs::read_to_string(cases().join(format!("{name}.{suffix}.bruno.json"))).expect("fixture")
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct Ui {
    model: Model,
    root: PathBuf,
}

impl Ui {
    /// Collection jetable : `bruno.json` = `config`, un environnement `local`.
    fn new(tag: &str, config: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!(
                "tmp-settings-{tag}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("environments")).expect("dossier");
        fs::write(root.join("bruno.json"), config).expect("bruno.json");
        fs::write(
            root.join("environments/local.bru"),
            "vars {\n  host: localhost\n}\n",
        )
        .expect("environnement");
        let mut model = Model::new(root.clone(), (120, 40));
        update(&mut model, Message::CollectionLoaded(BruLoader.load(&root)));
        model.selected_collection = true;
        Self { model, root }
    }

    fn from_case(name: &str) -> Self {
        Self::new(name, &case(name, "before"))
    }

    fn run(&mut self, command: Command) {
        match command {
            Command::SaveSettings {
                root,
                stamp,
                changes,
                added_paths,
            } => {
                let result = execute_save(&root, &stamp, &changes, &added_paths);
                let next = update(&mut self.model, Message::SettingsSaved { root, result });
                self.run(next);
            }
            Command::ReloadCollection => {
                let result = BruLoader.load(&self.root);
                let next = update(&mut self.model, Message::CollectionLoaded(result));
                self.run(next);
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

    fn presses(&mut self, code: KeyCode, count: usize) {
        for _ in 0..count {
            self.press(code);
        }
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    fn save(&mut self) {
        self.press_with(KeyCode::Char('s'), KeyModifiers::CONTROL);
    }

    fn clear_field(&mut self) {
        self.press(KeyCode::End);
        self.presses(KeyCode::Backspace, 80);
    }

    /// `B` : ouvre le popup.
    fn open(&mut self) {
        self.press(KeyCode::Char('B'));
        assert!(self.model.settings.is_some(), "popup ouvert");
    }

    fn read(&self) -> String {
        fs::read_to_string(self.root.join("bruno.json")).expect("lecture")
    }

    fn status_text(&self) -> String {
        match &self.model.last_status {
            Some(StatusMessage::FileOpDone(text)) => text.clone(),
            Some(StatusMessage::FileOpRefused(text)) => format!("Refusé : {text}"),
            other => format!("{other:?}"),
        }
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

impl Drop for Ui {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.root, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&self.root);
    }
}

type Script = fn(&mut Ui);

const PART_PROXY: KeyCode = KeyCode::Tab;

fn proxy_enabled(ui: &mut Ui) {
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right); // enabled
    ui.press(KeyCode::Down); // protocole (http, inchangé)
    ui.press(KeyCode::Down); // hôte
    ui.press(KeyCode::Enter);
    ui.type_text("{{proxy_host}}");
    ui.press(KeyCode::Enter);
    ui.press(KeyCode::Down); // port
    ui.press(KeyCode::Enter);
    ui.type_text("8080");
    ui.press(KeyCode::Enter);
}

fn proxy_host_replaced(ui: &mut Ui) {
    ui.press(PART_PROXY);
    ui.presses(KeyCode::Down, 2);
    ui.press(KeyCode::Enter);
    ui.clear_field();
    ui.type_text("{{proxy_host}}");
    ui.press(KeyCode::Enter);
}

fn certs_tab(ui: &mut Ui) {
    ui.presses(KeyCode::Tab, 2);
}

fn scripts() -> Vec<(&'static str, Script)> {
    vec![
        ("proxy-add", proxy_enabled),
        ("proxy-add-final-newline", proxy_enabled),
        ("proxy-add-four-spaces", proxy_enabled),
        ("proxy-modify", proxy_host_replaced),
        ("proxy-modify-final-newline", proxy_host_replaced),
        ("proxy-auth-enabled", |ui| {
            ui.press(PART_PROXY);
            ui.presses(KeyCode::Down, 4); // case d'authentification
            ui.press(KeyCode::Char(' '));
        }),
        ("proxy-auth-disabled", |ui| {
            ui.press(PART_PROXY);
            ui.press(KeyCode::Right);
            ui.presses(KeyCode::Down, 4);
            ui.press(KeyCode::Char(' '));
        }),
        ("proxy-mode-inherit", |ui| {
            ui.press(PART_PROXY);
            ui.press(KeyCode::Left);
        }),
        ("proxy-mode-enabled", |ui| {
            ui.press(PART_PROXY);
            ui.press(KeyCode::Right);
        }),
        ("proxy-mode-disabled", |ui| {
            ui.press(PART_PROXY);
            ui.presses(KeyCode::Right, 2);
        }),
        ("proxy-port-empty", |ui| {
            ui.press(PART_PROXY);
            ui.presses(KeyCode::Down, 3);
            ui.press(KeyCode::Enter);
            ui.clear_field();
            ui.press(KeyCode::Enter);
        }),
        ("presets-add-env", |ui| {
            ui.presses(KeyCode::Down, 2);
            ui.press(KeyCode::Right);
        }),
        ("presets-remove-env", |ui| {
            ui.presses(KeyCode::Down, 2);
            ui.press(KeyCode::Right);
        }),
        ("presets-change-type", |ui| {
            ui.press(KeyCode::Left);
        }),
        ("certs-add-cert", |ui| {
            certs_tab(ui);
            ui.press(KeyCode::Char('a'));
            ui.type_text(" api.exemple.fr ");
            ui.press(KeyCode::Enter); // type
            ui.press(KeyCode::Enter); // certificat
            ui.type_text("certs/client.crt");
            ui.press(KeyCode::Enter);
            ui.type_text("certs/client.key");
            ui.press(KeyCode::Enter); // passphrase
            ui.type_text("{{certPass}}");
            ui.press(KeyCode::Enter); // ajoute
        }),
        ("certs-add-pfx", |ui| {
            certs_tab(ui);
            ui.press(KeyCode::Char('a'));
            ui.type_text("*.exemple.fr");
            ui.press(KeyCode::Enter);
            ui.press(KeyCode::Right); // pfx
            ui.press(KeyCode::Enter);
            ui.type_text("certs/client.pfx");
            ui.press(KeyCode::Enter); // passphrase vide
            ui.press(KeyCode::Enter); // ajoute
        }),
        ("certs-remove", |ui| {
            certs_tab(ui);
            ui.press(KeyCode::Char('d'));
            ui.press(KeyCode::Char('y'));
        }),
        ("certs-disable", |ui| {
            certs_tab(ui);
            ui.press(KeyCode::Char(' '));
        }),
        ("certs-enable", |ui| {
            certs_tab(ui);
            ui.press(KeyCode::Down);
            ui.press(KeyCode::Char(' '));
        }),
    ]
}

#[test]
fn every_case_matches_the_bytes_written_with_bruno_code() {
    for (name, script) in scripts() {
        let mut ui = Ui::from_case(name);
        ui.open();
        script(&mut ui);
        ui.save();
        assert!(
            ui.model.settings.is_none(),
            "{name} : popup fermé, statut {}",
            ui.status_text()
        );
        assert_eq!(ui.read(), case(name, "after"), "{name}");
    }
}

#[test]
fn every_written_file_is_read_back_by_node_json_parse() {
    if Process::new("node").arg("--version").output().is_err() {
        return;
    }
    for (name, script) in scripts() {
        let mut ui = Ui::from_case(name);
        ui.open();
        script(&mut ui);
        ui.save();
        let check = Process::new("node")
            .arg("-e")
            .arg("JSON.parse(require('fs').readFileSync(process.argv[1], 'utf8'))")
            .arg(ui.root.join("bruno.json"))
            .output()
            .expect("node");
        assert!(
            check.status.success(),
            "{name} : JSON.parse refuse le fichier"
        );
    }
}

#[test]
fn the_case_list_covers_every_fixture() {
    let mut names: Vec<String> = fs::read_dir(cases())
        .expect("dossier")
        .flatten()
        .filter_map(|e| {
            e.file_name()
                .to_string_lossy()
                .strip_suffix(".before.bruno.json")
                .map(str::to_owned)
        })
        .collect();
    names.sort();
    let mut wanted: Vec<String> = scripts().iter().map(|(n, _)| (*n).to_owned()).collect();
    wanted.sort();
    assert_eq!(names, wanted);
}

#[test]
fn nothing_is_written_when_nothing_changed() {
    let mut ui = Ui::from_case("proxy-modify");
    let before = ui.read();
    ui.open();
    ui.save();
    assert!(ui.model.settings.is_none());
    assert_eq!(ui.read(), before);
}

#[test]
fn only_modified_keys_are_touched_and_other_bytes_are_kept() {
    let mut ui = Ui::from_case("proxy-modify");
    let before = ui.read();
    ui.open();
    // Presets modifié seulement : `proxy` et `ignore` restent identiques.
    ui.press(KeyCode::Right);
    ui.save();
    let after = ui.read();
    assert!(after.contains(r#""ignore": ["node_modules", ".git"]"#));
    let proxy_at = |text: &str| text.find("\"proxy\"").zip(text.find("\"type\""));
    let (start, end) = proxy_at(&before).expect("proxy");
    assert_eq!(&before[start..end], &after[start..end]);
    assert!(after.contains("\"presets\""));
}

// --- Fraîcheur, atomicité --------------------------------------------------

#[test]
fn stale_file_refuses_the_write_and_keeps_the_popup() {
    let mut ui = Ui::from_case("proxy-add");
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right);
    let external = "{\n  \"name\": \"externe\"\n}";
    fs::write(ui.root.join("bruno.json"), external).expect("écriture externe");
    ui.save();
    assert!(ui.model.settings.is_some(), "popup toujours ouvert");
    assert_eq!(ui.read(), external, "aucun octet écrit");
    assert_eq!(
        ui.status_text(),
        "Refusé : bruno.json a changé sur le disque"
    );
    // L'écriture reste possible : plus rien n'est en cours.
    assert!(!ui.model.settings.as_ref().expect("popup").saving);
}

#[test]
fn failed_write_leaves_the_file_unchanged_and_no_temporary_file() {
    let mut ui = Ui::from_case("proxy-add");
    let original = ui.read();
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right);
    // Répertoire sans écriture : la création du temporaire échoue avant tout
    // renommage (même simulation que `no_temp_file_survives_a_simulated_write_failure`).
    fs::set_permissions(&ui.root, fs::Permissions::from_mode(0o500)).expect("chmod");
    ui.save();
    fs::set_permissions(&ui.root, fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(ui.model.settings.is_some());
    assert_eq!(ui.read(), original);
    let leftovers: Vec<String> = fs::read_dir(&ui.root)
        .expect("dossier")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    assert!(ui.status_text().starts_with("Refusé"));
}

#[test]
fn the_only_file_written_is_bruno_json() {
    let mut ui = Ui::from_case("proxy-add");
    let list = |root: &Path| {
        let mut files: Vec<String> = fs::read_dir(root)
            .expect("dossier")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        files
    };
    let before = list(&ui.root);
    let env_before = fs::read(ui.root.join("environments/local.bru")).expect("env");
    ui.open();
    proxy_enabled(&mut ui);
    ui.save();
    assert_eq!(list(&ui.root), before);
    assert_eq!(
        fs::read(ui.root.join("environments/local.bru")).expect("env"),
        env_before
    );
}

#[test]
fn bruno_json_symlink_is_refused() {
    let mut ui = Ui::from_case("proxy-add");
    let outside = ui
        .root
        .join("..")
        .join(format!("outside-{}.json", std::process::id()));
    fs::write(&outside, "{}").expect("cible");
    fs::remove_file(ui.root.join("bruno.json")).expect("retrait");
    std::os::unix::fs::symlink(&outside, ui.root.join("bruno.json")).expect("lien");
    ui.press(KeyCode::Char('B'));
    assert!(ui.model.settings.is_none());
    assert!(
        ui.status_text().contains("lien symbolique"),
        "{}",
        ui.status_text()
    );
    let _ = fs::remove_file(&outside);
}

// --- Refus -----------------------------------------------------------------

#[test]
fn invalid_json_and_non_object_roots_refuse_the_popup() {
    for (tag, text) in [("invalid", "{ \"name\": "), ("array", "[1, 2]")] {
        // La collection est chargée valide, puis le fichier est abîmé.
        let mut ui = Ui::new(tag, "{\n  \"name\": \"x\"\n}");
        fs::write(ui.root.join("bruno.json"), text).expect("écriture externe");
        ui.press(KeyCode::Char('B'));
        assert!(ui.model.settings.is_none(), "{tag}");
        assert!(
            ui.status_text()
                .starts_with("Refusé : Réglages impossibles"),
            "{tag} : {}",
            ui.status_text()
        );
    }
}

#[test]
fn duplicate_key_refuses_the_write() {
    let text = "{\n  \"name\": \"x\",\n  \"proxy\": {},\n  \"proxy\": {}\n}";
    let mut ui = Ui::new("duplicate", text);
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right);
    ui.save();
    assert!(ui.model.settings.is_some());
    assert_eq!(ui.read(), text);
    assert!(
        ui.status_text().contains("en double"),
        "{}",
        ui.status_text()
    );
}

#[test]
fn tabs_and_mixed_line_endings_are_refused() {
    for (tag, text) in [
        ("tabs", "{\n\t\"name\": \"x\"\n}"),
        ("mixed", "{\r\n  \"name\": \"x\"\n}"),
    ] {
        let mut ui = Ui::new(tag, text);
        ui.press(KeyCode::Char('B'));
        assert!(ui.model.settings.is_none(), "{tag}");
    }
}

#[test]
fn yaml_collection_is_read_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/oc-probe");
    let mut model = Model::new(root.clone(), (120, 40));
    update(
        &mut model,
        Message::CollectionLoaded(OpenCollectionLoader.load(&root)),
    );
    update(&mut model, Message::OpenSettings);
    assert!(model.settings.is_none());
    assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));
}

#[test]
fn a_run_in_progress_refuses_the_popup() {
    let mut ui = Ui::from_case("proxy-add");
    ui.model.series = Some(EnvSeries::new(PathBuf::from("."), vec!["local".into()]));
    ui.press(KeyCode::Char('B'));
    assert!(ui.model.settings.is_none());
    assert!(ui.status_text().contains("Exécution en cours"));
}

#[test]
fn b_is_ignored_while_another_popup_has_the_keyboard() {
    let mut ui = Ui::from_case("proxy-add");
    ui.press(KeyCode::Char('a')); // popup de fichier
    assert!(ui.model.file_popup.is_some());
    update(&mut ui.model, Message::OpenSettings);
    assert!(ui.model.settings.is_none());
}

#[test]
fn port_out_of_range_and_empty_domain_are_refused() {
    let mut ui = Ui::from_case("proxy-add");
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right);
    ui.presses(KeyCode::Down, 3); // port
    for bad in ["0", "65536", "80a"] {
        ui.press(KeyCode::Enter);
        ui.clear_field();
        ui.type_text(bad);
        ui.press(KeyCode::Enter);
        assert!(
            ui.model.settings.as_ref().expect("popup").input.is_some(),
            "{bad} refusé, saisie gardée"
        );
        ui.press(KeyCode::Esc);
    }
    // Certificats : domaine vide, type cert sans clé.
    ui.press(KeyCode::Tab);
    ui.press(KeyCode::Char('a'));
    ui.presses(KeyCode::Down, 4);
    ui.press(KeyCode::Enter);
    assert!(ui.screen(120, 40).contains("Domain is required"));
    let add = ui
        .model
        .settings
        .as_mut()
        .expect("popup")
        .add
        .as_mut()
        .expect("ajout");
    assert_eq!(add.error.as_deref(), Some("Domain is required"));
    add.cursor = 0;
    ui.type_text("x.fr");
    ui.presses(KeyCode::Down, 4);
    ui.press(KeyCode::Enter);
    let add = ui
        .model
        .settings
        .as_ref()
        .expect("popup")
        .add
        .as_ref()
        .expect("ajout");
    assert_eq!(
        add.error.as_deref(),
        Some("certFilePath is a required field")
    );
}

#[test]
fn absolute_certificate_path_becomes_relative_and_missing_file_only_warns() {
    let mut ui = Ui::from_case("certs-add-cert");
    ui.open();
    certs_tab(&mut ui);
    ui.press(KeyCode::Char('a'));
    ui.type_text("api.fr");
    ui.press(KeyCode::Enter);
    ui.press(KeyCode::Enter);
    let absolute = format!("{}/certs/client.crt", ui.root.display());
    ui.type_text(&absolute);
    ui.press(KeyCode::Enter);
    ui.type_text("../autre/client.key");
    ui.press(KeyCode::Enter);
    ui.press(KeyCode::Enter);
    ui.save();
    let written = ui.read();
    assert!(
        written.contains("\"certFilePath\": \"certs/client.crt\""),
        "{written}"
    );
    assert!(written.contains("\"keyFilePath\": \"../autre/client.key\""));
    let status = ui.status_text();
    assert!(
        status.contains("Avertissement : fichier introuvable : certs/client.crt"),
        "{status}"
    );
}

// --- Secrets ---------------------------------------------------------------

#[test]
fn literal_secrets_are_refused_and_never_appear_anywhere() {
    let mut ui = Ui::from_case("proxy-add");
    let original = ui.read();
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right);
    ui.presses(KeyCode::Down, 6); // mot de passe
    ui.press(KeyCode::Enter);
    ui.type_text("hunter2-secret");
    // Pendant la saisie, jamais en clair.
    assert!(!ui.screen(120, 40).contains("hunter2"));
    ui.press(KeyCode::Enter);
    let screen = ui.screen(120, 40);
    assert!(!screen.contains("hunter2"), "{screen}");
    assert!(
        screen.contains("{{proxyPassword}}"),
        "suggestion de variable"
    );
    let debug = format!("{:?} {:?}", ui.model.last_status, ui.model.settings);
    assert!(!debug.contains("hunter2"), "{debug}");
    // Passphrase d'un certificat.
    ui.press(KeyCode::Tab);
    ui.press(KeyCode::Char('a'));
    ui.type_text("a.fr");
    ui.presses(KeyCode::Down, 2);
    ui.type_text("c.crt");
    ui.press(KeyCode::Down);
    ui.type_text("c.key");
    ui.press(KeyCode::Down);
    ui.type_text("passphrase-secrete");
    assert!(!ui.screen(120, 40).contains("passphrase-secrete"));
    ui.press(KeyCode::Enter);
    let screen = ui.screen(120, 40);
    assert!(!screen.contains("passphrase-secrete"), "{screen}");
    let debug = format!("{:?}", ui.model.settings);
    assert!(!debug.contains("passphrase-secrete"), "{debug}");
    ui.press(KeyCode::Esc);
    ui.press(KeyCode::Esc);
    ui.press(KeyCode::Char('y'));
    assert!(ui.model.settings.is_none());
    assert_eq!(ui.read(), original, "rien n'a été écrit");
}

#[test]
fn variable_and_empty_secrets_are_accepted() {
    let mut ui = Ui::from_case("proxy-add");
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Right);
    ui.presses(KeyCode::Down, 6);
    ui.press(KeyCode::Enter);
    ui.type_text("{{proxy_password}}");
    ui.press(KeyCode::Enter);
    ui.save();
    assert!(ui.read().contains("\"password\": \"{{proxy_password}}\""));
}

#[test]
fn existing_literal_secret_is_masked_and_copied_unchanged() {
    let text = "{\n  \"name\": \"x\",\n  \"proxy\": {\n    \"inherit\": false,\n    \"config\": {\n      \"protocol\": \"http\",\n      \"hostname\": \"h\",\n      \"port\": 80,\n      \"auth\": {\n        \"username\": \"u\",\n        \"password\": \"s3cret-en-clair\"\n      },\n      \"bypassProxy\": \"\"\n    }\n  }\n}";
    let mut ui = Ui::new("masked", text);
    ui.open();
    ui.press(PART_PROXY);
    let screen = ui.screen(120, 40);
    assert!(screen.contains("********"), "{screen}");
    assert!(!screen.contains("s3cret"), "{screen}");
    ui.press_with(KeyCode::Char('r'), KeyModifiers::CONTROL);
    assert!(!ui.screen(120, 40).contains("s3cret"), "même révélé");
    // Modification d'un autre champ : le secret est recopié tel quel.
    ui.presses(KeyCode::Down, 2);
    ui.press(KeyCode::Enter);
    ui.type_text("2");
    ui.press(KeyCode::Enter);
    ui.save();
    let written = ui.read();
    assert!(written.contains("\"hostname\": \"h2\""));
    assert!(written.contains("\"password\": \"s3cret-en-clair\""));
}

// --- Ancien format de proxy -----------------------------------------------

#[test]
fn legacy_proxy_conversion_equals_transform_proxy_config() {
    let legacy =
        Json::parse(&fs::read_to_string(cases().join("legacy-proxy.json")).expect("fixture"))
            .expect("json");
    for key in ["true", "false", "global", "minimal"] {
        let case = legacy.get(key).expect("cas");
        let input = case.get("input").expect("entrée");
        let output = case.get("output").expect("sortie");
        assert_eq!(&convert_legacy_proxy(input), output, "enabled = {key}");
    }
}

#[test]
fn legacy_proxy_is_converted_only_when_modified() {
    let text = "{\n  \"name\": \"x\",\n  \"proxy\": {\n    \"enabled\": true,\n    \"protocol\": \"http\",\n    \"hostname\": \"h\",\n    \"port\": 8080\n  }\n}";
    let mut ui = Ui::new("legacy", text);
    ui.open();
    ui.press(KeyCode::Esc);
    assert!(ui.model.settings.is_none());
    assert_eq!(ui.read(), text, "fermé sans modifier : rien converti");
    ui.open();
    ui.press(PART_PROXY);
    ui.press(KeyCode::Down);
    ui.press(KeyCode::Right); // protocole https
    ui.save();
    let written = ui.read();
    assert!(!written.contains("\"enabled\""), "{written}");
    assert!(written.contains("\"inherit\": false"));
    assert!(written.contains("\"protocol\": \"https\""));
    assert!(
        ui.status_text()
            .contains("Proxy converti au nouveau format de Bruno")
    );
}

// --- Rendu, touche B -------------------------------------------------------

#[test]
fn popup_renders_the_three_parts_at_80_and_120_columns() {
    for width in [80, 120] {
        let mut ui = Ui::from_case("proxy-modify");
        ui.open();
        let screen = ui.screen(width, 30);
        for expected in ["Réglages — demo", "Presets", "Proxy", "Certificats"] {
            assert!(screen.contains(expected), "{width} : {expected}\n{screen}");
        }
        assert!(screen.contains("Type de requête"), "{screen}");
        ui.press(KeyCode::Tab);
        let screen = ui.screen(width, 30);
        assert!(screen.contains("Authentification"), "{screen}");
        assert!(screen.contains("old.example"), "{screen}");
        assert!(
            screen.contains("{{proxy_password}}") || screen.contains("••••"),
            "{screen}"
        );
        ui.press(KeyCode::Tab);
        let screen = ui.screen(width, 30);
        assert!(screen.contains("Aucun certificat client"), "{screen}");
    }
}

#[test]
fn certificates_list_shows_disabled_entries() {
    let mut ui = Ui::from_case("certs-enable");
    ui.open();
    certs_tab(&mut ui);
    let screen = ui.screen(100, 30);
    assert!(screen.contains("api.exemple.fr"), "{screen}");
    assert!(screen.contains("vieux.exemple.fr"), "{screen}");
    assert!(screen.contains("(désactivé)"), "{screen}");
}

#[test]
fn escape_confirms_before_discarding_modifications() {
    let mut ui = Ui::from_case("proxy-add");
    let original = ui.read();
    ui.open();
    ui.press(KeyCode::Right);
    ui.press(KeyCode::Esc);
    assert!(
        ui.screen(120, 30)
            .contains("Abandonner les modifications ? (y/n)")
    );
    ui.press(KeyCode::Char('n'));
    assert!(ui.model.settings.is_some());
    ui.press(KeyCode::Esc);
    ui.press(KeyCode::Char('y'));
    assert!(ui.model.settings.is_none());
    assert_eq!(ui.read(), original);
}

#[test]
fn deleting_a_certificate_asks_in_the_status_bar() {
    let mut ui = Ui::from_case("certs-remove");
    ui.open();
    certs_tab(&mut ui);
    ui.press(KeyCode::Char('d'));
    let screen = ui.screen(120, 30);
    assert!(
        screen.contains("Supprimer le certificat « api.exemple.fr » ? (y/n)"),
        "{screen}"
    );
}

#[test]
fn saved_settings_reload_the_collection_and_clear_the_popup() {
    let mut ui = Ui::from_case("presets-add-env");
    ui.open();
    ui.presses(KeyCode::Down, 1);
    ui.press(KeyCode::Enter);
    ui.type_text("{{base_url}}");
    ui.press(KeyCode::Enter);
    ui.save();
    assert!(ui.model.settings.is_none());
    assert!(
        ui.status_text()
            .starts_with("Réglages enregistrés (presets)")
    );
    let presets = ui
        .model
        .loaded()
        .and_then(|c| c.presets.as_deref())
        .cloned()
        .expect("presets rechargés");
    assert_eq!(presets.request_url.as_deref(), Some("{{base_url}}"));
}

// --- Préremplissage du popup `a` (D6-A) -------------------------------------

#[test]
fn presets_prefill_the_new_request_popup() {
    let text = "{\n  \"name\": \"x\",\n  \"presets\": {\n    \"requestType\": \"graphql\",\n    \"requestUrl\": \"{{base_url}}/graphql\"\n  }\n}";
    let mut ui = Ui::new("prefill", text);
    ui.press(KeyCode::Char('a'));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.kind_label(), "graphql");
    assert_eq!(popup.url.text(), "{{base_url}}/graphql");
}

#[test]
fn grpc_and_ws_presets_fall_back_to_http_with_a_mention() {
    for kind in ["grpc", "ws"] {
        let text = format!(
            "{{\n  \"name\": \"x\",\n  \"presets\": {{\n    \"requestType\": \"{kind}\",\n    \"requestUrl\": \"\"\n  }}\n}}"
        );
        let mut ui = Ui::new(&format!("fallback-{kind}"), &text);
        ui.press(KeyCode::Char('a'));
        let popup = ui.model.file_popup.as_ref().expect("popup");
        assert_eq!(popup.kind_label(), "http");
        assert!(ui.status_text().contains(kind), "{}", ui.status_text());
    }
}

#[test]
fn no_presets_leave_the_new_request_popup_untouched() {
    let mut ui = Ui::new("noprefill", "{\n  \"name\": \"x\"\n}");
    ui.press(KeyCode::Char('a'));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.kind_label(), "http");
    assert_eq!(popup.url.text(), "");
}

// --- Touche B --------------------------------------------------------------

#[test]
fn only_b_opens_the_settings() {
    for code in 0x20u8..0x7f {
        let c = char::from(code);
        let event = AppEvent::Terminal(Event::Key(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )));
        let message = to_message(event, None);
        let opens = matches!(message, Some(Message::OpenSettings));
        assert_eq!(opens, c == 'B', "touche {c:?}");
    }
}

/// Relecture par le vrai `bru` (ignoré par défaut : nécessite `bru` dans le
/// `PATH`) : la collection écrite par le TUI se charge et `bru run` produit
/// son rapport. Lancer avec `cargo test -- --ignored`.
#[test]
#[ignore = "nécessite bru"]
fn every_written_file_loads_in_the_real_bru() {
    for (name, script) in scripts() {
        let mut ui = Ui::from_case(name);
        ui.open();
        script(&mut ui);
        ui.save();
        fs::write(
            ui.root.join("ping.bru"),
            "meta {\n  name: ping\n  type: http\n  seq: 1\n}\n\nget {\n  url: http://127.0.0.1:9/\n  body: none\n  auth: none\n}\n",
        )
        .expect("requête");
        let report = ui.root.join("report.json");
        let output = Process::new("bru")
            .current_dir(&ui.root)
            .args(["run", "ping.bru", "--reporter-json"])
            .arg(&report)
            .output()
            .expect("bru");
        let text = fs::read_to_string(&report).unwrap_or_default();
        assert!(
            serde_json::from_str::<serde_json::Value>(&text).is_ok(),
            "{name} : bru n'a pas produit de rapport\n{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// En workspace, `B` ouvre les réglages de la collection de la ligne
/// sélectionnée (`docs/workspace.md` § 4).
#[test]
fn in_a_workspace_b_opens_the_settings_of_the_selected_collection() {
    use bruno_tui::app::update::refresh_rows;
    use bruno_tui::app::workspace::install;
    use bruno_tui::collection::workspace::from_paths;

    let first = Ui::new("ws-un", "{\n  \"name\": \"un\"\n}");
    let second = Ui::new("ws-deux", "{\n  \"name\": \"deux\"\n}");
    let mut model = Model::new(first.root.clone(), (120, 40));
    install(
        &mut model,
        from_paths(&[first.root.clone(), second.root.clone()]),
    );
    for (member, root) in [(1, &second.root), (0, &first.root)] {
        update(
            &mut model,
            Message::MemberLoaded {
                member,
                result: BruLoader.load(root),
            },
        );
    }
    refresh_rows(&mut model);
    let index = model
        .tree
        .rows
        .iter()
        .position(|row| row.member == 1)
        .expect("en-tête du second membre");
    model.tree.selected = index;
    model.selected_collection = true;
    // Comme après un déplacement de la sélection : la collection de la ligne devient active.
    update(
        &mut model,
        Message::Resize {
            width: 120,
            height: 40,
        },
    );
    update(&mut model, Message::OpenSettings);
    let session = model.settings.as_ref().expect("popup ouvert");
    assert_eq!(
        session.root,
        second.root.canonicalize().unwrap_or(second.root.clone())
    );
    assert_eq!(session.name, "deux");
}
