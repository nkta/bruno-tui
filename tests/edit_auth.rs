//! Tests d'intégration pour l'édition de l'authentification (mode et paramètres).
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bruno_tui::app::clipboard::{Clipboard, ClipboardError};
use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::collection::BruLoader;
use bruno_tui::writer::BruWriter;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::timeout;

struct DummyClipboard;

impl Clipboard for DummyClipboard {
    fn set_text(&self, _text: String) -> Result<(), ClipboardError> {
        Ok(())
    }
}

fn writer_cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases")
}

fn workdir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("bruno-tui-edit-auth-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("répertoire de test");
    dir
}

fn copy_single_case(case_file: &str, target_dir: &Path) {
    let source_dir = writer_cases();
    fs::copy(source_dir.join("bruno.json"), target_dir.join("bruno.json"))
        .expect("copie bruno.json");
    fs::copy(source_dir.join(case_file), target_dir.join(case_file)).expect("copie case_file");
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn ctrl(c: char) -> AppEvent {
    AppEvent::Terminal(Event::Key(KeyEvent::new(
        KeyCode::Char(c),
        KeyModifiers::CONTROL,
    )))
}

fn spawn_run(
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<(Terminal<TestBackend>, Exit)> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(120, 35)).expect("terminal");
        let result = run(
            &mut terminal,
            Arc::new(BruLoader),
            source,
            sender,
            events,
            "bru".into(),
            Arc::new(DummyClipboard),
            Arc::new(BruWriter),
            Vec::new(),
            bruno_tui::app::mouse::MouseSetup::terminal(false),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            false,
            false,
            None,
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

#[tokio::test]
async fn edit_auth_mode_and_token_on_request_and_save() {
    let dir = workdir("request-bearer");
    copy_single_case("simple.bru", &dir);

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(dir.clone(), sender.clone(), events);

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Passer au détail (Tab) et ouvrir l'édition (Entrée)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 2. Naviguer vers AuthMode :
    // Dans simple.bru : Method (0), Url (1), Headers (2..N, AddRow), Query/Path AddRow, AuthMode
    // Utiliser Down pour atteindre AuthMode ou naviguer dans les champs
    // Dans simple.bru :
    // fields: [Method, Url, AddRow(Headers), AddRow(QueryParams), AddRow(PathParams), AuthMode, ...]
    // Donc: Down 5 fois amène à AuthMode.
    for _ in 0..5 {
        sender.send(key(KeyCode::Down)).await.expect("down");
    }

    // 3. Ouvrir AuthModePicker (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Modes: ["none", "inherit", "basic", "bearer", ...]
    // Descendre de 3 crans pour sélectionner "bearer"
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Down)).await.expect("down");

    // Valider le mode bearer (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 4. Naviguer vers le champ token (un cran vers le bas)
    sender.send(key(KeyCode::Down)).await.expect("down");

    // Ouvrir la saisie du token (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Saisir "secret-token-xyz"
    for c in "secret-token-xyz".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }

    // Valider la saisie (Tab) -> ouvre PlaintextSecretPrompt
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    // Accepter de garder la valeur en clair ('g')
    sender
        .send(key(KeyCode::Char('g')))
        .await
        .expect("keep plaintext");

    // 5. Sauvegarder (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Quitter (q)
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // 6. Vérifier le fichier sauvegardé sur disque
    let file_content = fs::read_to_string(dir.join("simple.bru")).expect("read simple.bru");
    assert!(
        file_content.contains("auth: bearer"),
        "le bloc de méthode doit déclarer auth: bearer:\n{file_content}"
    );
    assert!(
        file_content.contains("auth:bearer {\n  token: secret-token-xyz\n}"),
        "le bloc auth:bearer doit contenir le token saisi:\n{file_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn edit_auth_apikey_placement_via_picker_and_save() {
    let dir = workdir("request-apikey");
    copy_single_case("simple.bru", &dir);

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(dir.clone(), sender.clone(), events);

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Passer au détail et ouvrir l'édition
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Naviguer vers AuthMode (5 crans)
    for _ in 0..5 {
        sender.send(key(KeyCode::Down)).await.expect("down");
    }

    // Ouvrir AuthModePicker
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Modes: ["none", "inherit", "basic", "bearer", "digest", "ntlm", "apikey"] -> 6 crans vers le bas
    for _ in 0..6 {
        sender.send(key(KeyCode::Down)).await.expect("down");
    }
    // Valider le mode apikey
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Les paramètres apikey sont: key, value, placement.
    // Descendre de 3 crans pour atteindre placement
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Down)).await.expect("down");

    // Entrée ouvre ApiKeyPlacementPicker
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Options: ["header", "queryparams"] -> descendre de 1 cran pour choisir "queryparams"
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Sauvegarder (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Quitter (q)
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // Vérifier le fichier sauvegardé sur disque
    let file_content = fs::read_to_string(dir.join("simple.bru")).expect("read simple.bru");
    assert!(
        file_content.contains("auth: apikey"),
        "le bloc get doit déclarer auth: apikey:\n{file_content}"
    );
    assert!(
        file_content.contains("placement: queryparams"),
        "placement doit valoir queryparams:\n{file_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn edit_auth_mode_on_folder_and_save() {
    let dir = workdir("folder-basic");
    fs::copy(writer_cases().join("bruno.json"), dir.join("bruno.json")).expect("copie bruno.json");
    fs::create_dir_all(dir.join("grp")).expect("grp dir");
    fs::write(
        dir.join("grp/folder.bru"),
        "meta {\n  name: grp\n}\n\nheaders {\n  X-Custom: 1\n}\n",
    )
    .expect("folder.bru");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(dir.clone(), sender.clone(), events);

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Passer au détail (Tab) et ouvrir l'édition (Entrée)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 2. Dans un dossier avec 1 header :
    // fields: [HeaderValue(0), AddRow(Headers), AuthMode, ...]
    // Descendre de 2 crans pour atteindre AuthMode
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Down)).await.expect("down");

    // 3. Ouvrir AuthModePicker (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Modes pour dossier: ["none", "inherit", "basic", ...]
    // Descendre de 2 crans pour choisir "basic"
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 4. Descendre sur username (1 cran) et saisir "admin"
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    for c in "admin".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }
    sender.send(key(KeyCode::Tab)).await.expect("tab");

    // 5. Descendre sur password (1 cran) et saisir "secret"
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    for c in "secret".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }
    // Valider la saisie (Tab) -> ouvre PlaintextSecretPrompt
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    // Accepter de garder la valeur en clair ('g')
    sender
        .send(key(KeyCode::Char('g')))
        .await
        .expect("keep plaintext");

    // 6. Sauvegarder (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Quitter
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // Vérifier folder.bru
    let folder_content = fs::read_to_string(dir.join("grp/folder.bru")).expect("read folder.bru");
    assert!(
        folder_content.contains("auth {\n  mode: basic\n}"),
        "folder.bru doit contenir le bloc auth {{ mode: basic }}:\n{folder_content}"
    );
    assert!(
        folder_content.contains("username: admin"),
        "folder.bru doit contenir username: admin:\n{folder_content}"
    );
    assert!(
        folder_content.contains("password: secret"),
        "folder.bru doit contenir password: secret:\n{folder_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn edit_auth_mode_change_prompts_confirmation_and_deletes_old_block() {
    let dir = workdir("confirm-delete-block");
    let bru_json = r#"{
  "name": "confirm-test",
  "version": "1"
}"#;
    fs::write(dir.join("bruno.json"), bru_json).expect("write bruno.json");

    let initial_bru = r#"meta {
  name: basic-req
  type: http
  seq: 1
}

get {
  url: https://{{host}}/api
  body: none
  auth: basic
}

auth:basic {
  username: alice
  password: secretpassword
}
"#;
    fs::write(dir.join("basic-req.bru"), initial_bru).expect("write basic-req.bru");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(dir.clone(), sender.clone(), events);

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Ouvrir l'édition
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 2. Naviguer vers AuthMode (5 crans vers le bas)
    for _ in 0..5 {
        sender.send(key(KeyCode::Down)).await.expect("down");
    }

    // 3. Ouvrir AuthModePicker (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Modes: ["none", "inherit", "basic", "bearer", ...]
    // Le mode initial est basic (index 2). 1 cran vers le bas pour sélectionner bearer (index 3).
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 4. Ctrl+S pour sauvegarder -> doit demander confirmation
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 5. Refuser avec 'n'
    sender.send(key(KeyCode::Char('n'))).await.expect("n");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Vérifier que le fichier n'a PAS été modifié sur disque
    let content_after_n = fs::read_to_string(dir.join("basic-req.bru")).expect("read file");
    assert!(
        content_after_n.contains("auth: basic"),
        "le mode basic doit toujours être présent après refus:\n{content_after_n}"
    );
    assert!(
        content_after_n.contains("username: alice"),
        "les identifiants d'origine doivent être conservés après refus:\n{content_after_n}"
    );

    // 6. Ctrl+S à nouveau puis accepter avec 'y'
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(100)).await;

    sender.send(key(KeyCode::Char('y'))).await.expect("y");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Quitter
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // 7. Vérifier que le fichier sur disque a été correctement sauvegardé
    let content_after_save = fs::read_to_string(dir.join("basic-req.bru")).expect("read file");
    assert!(
        content_after_save.contains("auth: bearer"),
        "le nouveau mode bearer doit être écrit:\n{content_after_save}"
    );
    assert!(
        content_after_save.contains("auth:bearer {\n  token: \n}"),
        "le nouveau bloc bearer vide doit être présent:\n{content_after_save}"
    );
    assert!(
        !content_after_save.contains("auth:basic"),
        "l'ancien bloc auth:basic doit avoir été supprimé:\n{content_after_save}"
    );
    assert!(
        !content_after_save.contains("alice"),
        "les anciennes valeurs doivent avoir été supprimées:\n{content_after_save}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn edit_auth_plaintext_secret_replacement_writes_variable_reference() {
    let dir = workdir("secret-replacement");
    let bru_json = r#"{
  "name": "secret-test",
  "version": "1"
}"#;
    fs::write(dir.join("bruno.json"), bru_json).expect("write bruno.json");

    let initial_bru = r#"meta {
  name: test-req
  type: http
  seq: 1
}

get {
  url: https://{{host}}/api
  body: none
  auth: bearer
}

auth:bearer {
  token: 
}
"#;
    fs::write(dir.join("test-req.bru"), initial_bru).expect("write test-req.bru");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(dir.clone(), sender.clone(), events);

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Ouvrir l'édition
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 2. Naviguer vers AuthMode (5 crans), puis vers token (1 cran supplémentaire = 6 crans)
    for _ in 0..6 {
        sender.send(key(KeyCode::Down)).await.expect("down");
    }

    // 3. Ouvrir la saisie du token (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 4. Saisir une valeur sensible en clair
    for c in "my-super-secret-token".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }

    // 5. Valider la saisie (Tab) -> ouvre PlaintextSecretPrompt
    sender.send(key(KeyCode::Tab)).await.expect("tab");

    // 6. Choisir Remplacer par la variable suggérée (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // 7. Sauvegarder (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 8. Quitter
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // 9. Vérifier le fichier sur disque
    let content_after_save = fs::read_to_string(dir.join("test-req.bru")).expect("read file");
    assert!(
        content_after_save.contains("token: {{bearerToken}}"),
        "le fichier doit contenir la référence {{bearerToken}}:\n{content_after_save}"
    );
    assert!(
        !content_after_save.contains("my-super-secret-token"),
        "la valeur en clair ne doit JAMAIS être écrite sur le disque:\n{content_after_save}"
    );

    let _ = fs::remove_dir_all(&dir);
}
