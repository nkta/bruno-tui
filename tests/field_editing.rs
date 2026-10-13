//! Tests d'intégration pour l'édition de champ et la sauvegarde.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use bruno_tui::app::clipboard::{Clipboard, ClipboardError};
use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::collection::{BruFile, BruLoader};
use bruno_tui::writer::{BruWriter, FieldEdit, FileStamp, RequestWriter, WriteError};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::timeout;

fn writer_cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases")
}

fn workdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "bruno-tui-field-editing-{tag}-{}",
        std::process::id()
    ));
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

fn screen(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect()
        })
        .collect()
}

type RunResult = (Terminal<TestBackend>, Exit);

struct SpyClipboard {
    copied_tx: Mutex<std_mpsc::Sender<String>>,
}

impl SpyClipboard {
    fn new() -> (Arc<Self>, std_mpsc::Receiver<String>) {
        let (tx, rx) = std_mpsc::channel();
        (
            Arc::new(Self {
                copied_tx: Mutex::new(tx),
            }),
            rx,
        )
    }
}

impl Clipboard for SpyClipboard {
    fn set_text(&self, text: String) -> Result<(), ClipboardError> {
        if let Ok(tx) = self.copied_tx.lock() {
            let _ = tx.send(text);
        }
        Ok(())
    }
}

fn spawn_run(
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
    writer: Arc<dyn RequestWriter>,
    clipboard: Arc<dyn Clipboard>,
) -> tokio::task::JoinHandle<RunResult> {
    spawn_run_with_editor(source, sender, events, writer, clipboard, None)
}

fn spawn_run_with_editor(
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
    writer: Arc<dyn RequestWriter>,
    clipboard: Arc<dyn Clipboard>,
    editor: Option<String>,
) -> tokio::task::JoinHandle<RunResult> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let result = run(
            &mut terminal,
            Arc::new(BruLoader),
            source,
            sender,
            events,
            "bru".into(),
            clipboard,
            writer,
            Vec::new(),
            bruno_tui::app::mouse::MouseSetup::terminal(false),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            false,
            false,
            editor,
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

struct BarrierWriter {
    started: Mutex<std_mpsc::Sender<()>>,
    unblock: Mutex<std_mpsc::Receiver<()>>,
    inner: BruWriter,
}

impl BarrierWriter {
    fn new() -> (Arc<Self>, std_mpsc::Receiver<()>, std_mpsc::Sender<()>) {
        let (started_tx, started_rx) = std_mpsc::channel();
        let (unblock_tx, unblock_rx) = std_mpsc::channel();
        let writer = Arc::new(Self {
            started: Mutex::new(started_tx),
            unblock: Mutex::new(unblock_rx),
            inner: BruWriter,
        });
        (writer, started_rx, unblock_tx)
    }
}

impl RequestWriter for BarrierWriter {
    fn write_request(
        &self,
        path: &Path,
        ast: &BruFile,
        stamp: &FileStamp,
        edits: &[FieldEdit],
    ) -> Result<FileStamp, WriteError> {
        if let Ok(started) = self.started.lock() {
            let _ = started.send(());
        }
        if let Ok(unblock) = self.unblock.lock() {
            let _ = unblock.recv();
        }
        self.inner.write_request(path, ast, stamp, edits)
    }
}

#[tokio::test]
async fn save_does_not_block_the_interface_during_slow_write() {
    let dir = workdir("barrier");
    copy_single_case("simple.bru", &dir);

    let (writer, started_rx, unblock_tx) = BarrierWriter::new();
    let (clipboard, yank_rx) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(dir.clone(), sender.clone(), events, writer, clipboard);

    // Attente du chargement initial
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Navigation : passer au détail (Tab), ouvrir le popup Requête (m)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Char('m'))).await.expect("m");

    // Méthode -> URL, puis saisie de l'URL (Entrée), frappe d'un caractère,
    // validation (Tab)
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    sender.send(key(KeyCode::Char('!'))).await.expect("char");
    sender.send(key(KeyCode::Tab)).await.expect("tab");

    // Lancer la sauvegarde (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");

    // Attendre la notification que write_request a démarré
    tokio::task::spawn_blocking(move || {
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("write started");
    })
    .await
    .expect("join blocking");

    // Pendant que l'écriture est bloquée par la barrière : l'interface doit
    // rester réactive ! Le popup, modal, attend le résultat : on le ferme
    // (Échap, puis y pour abandonner), puis on envoie 'y' (Yank) dans le
    // détail, qui produit un effet immédiat via le SpyClipboard.
    sender.send(key(KeyCode::Esc)).await.expect("esc");
    sender.send(key(KeyCode::Char('y'))).await.expect("discard");
    sender.send(key(KeyCode::Char('y'))).await.expect("yank");

    let copied_text = tokio::task::spawn_blocking(move || {
        yank_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("yank received during slow write")
    })
    .await
    .expect("join yank");

    assert!(
        !copied_text.is_empty(),
        "l'interface a traité la commande Yank pendant l'écriture asynchrone"
    );

    // Débloquer l'écriture
    unblock_tx.send(()).expect("unblock");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Quitter
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join handle");

    assert!(matches!(exit, Exit::Normal));

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn save_url_matches_expected_after_file_byte_for_byte() {
    let dir = workdir("byte-identical");
    copy_single_case("simple.bru", &dir);

    let (clipboard, _) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Aller sur le focus Détail (Tab)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    // Ouvrir le popup Requête (m)
    sender.send(key(KeyCode::Char('m'))).await.expect("m");
    // Méthode -> URL, puis commencer la saisie de l'URL (Entrée)
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Remplacer "ping" par "pong" à la fin de "https://{{host}}/ping"
    for _ in 0..4 {
        sender
            .send(key(KeyCode::Backspace))
            .await
            .expect("backspace");
    }
    for c in "pong".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }

    // Valider et enregistrer en une touche (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Quitter (q)
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // Vérifier la comparaison octet à octet avec simple.after.bru
    let rewritten_path = dir.join("simple.bru");
    let expected_bytes = fs::read(writer_cases().join("simple.after.bru")).expect("after file");
    let actual_bytes = fs::read(&rewritten_path).expect("rewritten file");
    assert_eq!(actual_bytes, expected_bytes);

    // Vérifier que le détail affiché montre la nouvelle valeur
    let lines = screen(&terminal);
    let found_url = lines.iter().any(|l| l.contains("https://{{host}}/pong"));
    assert!(
        found_url,
        "l'écran doit afficher la nouvelle URL après sauvegarde"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn stale_file_refusal_keeps_screen_state_and_shows_conflict_message() {
    let dir = workdir("stale-refusal");
    copy_single_case("simple.bru", &dir);

    let (clipboard, _) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Focus Détail (Tab) et ouvrir le popup Requête (m)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Char('m'))).await.expect("m");

    // Modifier l'URL en mémoire (validée par Entrée) ; Méthode -> URL
    sender.send(key(KeyCode::Down)).await.expect("down");
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    sender.send(key(KeyCode::Char('X'))).await.expect("char");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Modifier le fichier sur disque en externe pour invalider le FileStamp
    let target_file = dir.join("simple.bru");
    tokio::time::sleep(Duration::from_millis(20)).await;
    fs::write(
        &target_file,
        "get {\n  url: https://external-change.com\n}\n",
    )
    .expect("write external");

    // Tentative de sauvegarde (Ctrl+S)
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Tenter de quitter (q) -> doit demander confirmation car session toujours modifiée !
    sender.send(key(KeyCode::Char('q'))).await.expect("q");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Confirmer la sortie (y)
    sender.send(key(KeyCode::Char('y'))).await.expect("y");
    let (terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));

    // Le contenu externe n'a PAS été écrasé
    let current_content = fs::read_to_string(&target_file).expect("read");
    assert!(current_content.contains("external-change.com"));

    // L'écran a conservé les modifications locales non enregistrées
    let lines = screen(&terminal);
    let found_edit = lines.iter().any(|l| l.contains("pingX"));
    assert!(
        found_edit,
        "l'écran doit avoir conservé l'état de l'édition locale malgré l'erreur de sauvegarde"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn cancelled_input_and_closed_session_never_write() {
    let dir = workdir("cancel-no-write");
    copy_single_case("simple.bru", &dir);
    let before = fs::read(dir.join("simple.bru")).expect("lecture initiale");

    let (clipboard, _) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Popup Requête (m), curseur sur l'URL.
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Char('m'))).await.expect("m");
    sender.send(key(KeyCode::Down)).await.expect("down");

    // Saisie annulée par Échap : `q` tapé est du texte, pas une sortie.
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    for c in "qr/".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }
    sender.send(key(KeyCode::Esc)).await.expect("esc");

    // Saisie validée puis popup fermé en abandonnant (Échap, y).
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    sender.send(key(KeyCode::Char('Z'))).await.expect("char");
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Esc)).await.expect("esc");
    sender.send(key(KeyCode::Char('y'))).await.expect("y");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Popup fermé, rien en attente : `q` quitte sans confirmation.
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");
    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("timeout")
        .expect("join");

    assert!(matches!(exit, Exit::Normal));
    let after = fs::read(dir.join("simple.bru")).expect("lecture finale");
    assert_eq!(after, before, "aucune écriture sans Ctrl+S");

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn add_delete_rename_then_save_matches_expected_after_file() {
    let dir = workdir("entry-flow");
    copy_single_case("session-flow.bru", &dir);

    let (clipboard, _) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
    );
    tokio::time::sleep(Duration::from_millis(50)).await;

    let send = |event: AppEvent| {
        let sender = sender.clone();
        async move { sender.send(event).await.expect("envoi") }
    };

    // Détail, popup Params (onglet affiché). Positions : ligne d'ajout
    // des paramètres de requête (aucun), puis `id` (paramètre de chemin).
    send(key(KeyCode::Tab)).await;
    send(key(KeyCode::Enter)).await;

    // Ajout du paramètre de requête `page: 2` depuis sa ligne d'ajout ; un
    // `q` tapé dans la clé est du texte et ne ferme pas l'application.
    send(key(KeyCode::Char('a'))).await;
    send(key(KeyCode::Char('q'))).await;
    send(key(KeyCode::Backspace)).await;
    for c in "page".chars() {
        send(key(KeyCode::Char(c))).await;
    }
    send(key(KeyCode::Tab)).await;
    send(key(KeyCode::Char('2'))).await;
    send(key(KeyCode::Enter)).await;

    // Curseur sur `page` : `Tab` passe aux paramètres de chemin, sur `id`,
    // renommé en `userId`.
    send(key(KeyCode::Tab)).await;
    send(key(KeyCode::Char('c'))).await;
    send(key(KeyCode::Backspace)).await;
    send(key(KeyCode::Backspace)).await;
    for c in "userId".chars() {
        send(key(KeyCode::Char(c))).await;
    }
    send(key(KeyCode::Enter)).await;

    // Enregistrer ferme le popup.
    send(ctrl('s')).await;
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Popup En-têtes (→ ×2 depuis Params) : supprimer `X-Debug`.
    send(key(KeyCode::Right)).await;
    send(key(KeyCode::Right)).await;
    send(key(KeyCode::Enter)).await;
    send(key(KeyCode::Down)).await;
    send(key(KeyCode::Char('d'))).await;

    send(ctrl('s')).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    send(key(KeyCode::Char('q'))).await;

    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("l'application se ferme sans confirmation après la sauvegarde")
        .expect("join handle");
    assert!(matches!(exit, Exit::Normal));

    let expected =
        fs::read_to_string(writer_cases().join("session-flow.after.bru")).expect("fixture after");
    let written = fs::read_to_string(dir.join("session-flow.bru")).expect("relecture");
    assert_eq!(written, expected);

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn environment_editing_flow_saves_to_disk_and_secrets_never_appear() {
    let dir = workdir("env-editing-flow");
    fs::create_dir_all(dir.join("environments")).expect("dir");
    fs::write(
        dir.join("bruno.json"),
        r#"{"name": "test-env", "type": "collection", "version": "1"}"#,
    )
    .expect("bruno.json");
    let initial_content = "vars {\n  host: https://staging.example.com\n  port: 8080\n}\n\nvars:secret [\n  apiKey,\n  jwtToken\n]\n";
    fs::write(dir.join("environments/staging.bru"), initial_content).expect("staging.bru");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let (clipboard, _rx) = SpyClipboard::new();
    let handle = spawn_run(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Ouvrir le panneau Environnement ('E')
    sender.send(key(KeyCode::Char('E'))).await.expect("send");
    // 2. Descendre sur 'staging'
    sender.send(key(KeyCode::Down)).await.expect("send");
    // 3. 'e' pour ouvrir le popup d'édition
    // (`add-environment-panel-and-edit-popup`).
    sender.send(key(KeyCode::Char('e'))).await.expect("send");

    // 4. Entrée pour éditer la variable 'host'
    sender.send(key(KeyCode::Enter)).await.expect("send");
    // Effacer et taper une nouvelle valeur
    for _ in 0..30 {
        sender.send(key(KeyCode::Backspace)).await.expect("send");
    }
    for c in "https://prod.example.com".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("send");
    }
    // Valider la saisie avec Entrée
    sender.send(key(KeyCode::Enter)).await.expect("send");

    // 5. Sauvegarder avec Ctrl+S
    sender.send(ctrl('s')).await.expect("send");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 6. Quitter avec 'q'
    sender.send(key(KeyCode::Char('q'))).await.expect("send");

    let (terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("fermeture normale")
        .expect("join handle");
    assert!(matches!(exit, Exit::Normal));

    let screen_str = screen(&terminal).join("\n");
    // Confirmer qu'une variable secrète n'apparaît jamais dans cette vue
    assert!(!screen_str.contains("apiKey"), "{screen_str}");
    assert!(!screen_str.contains("jwtToken"), "{screen_str}");

    // 7. Relire le fichier sur disque pour confirmer l'écriture
    let written = fs::read_to_string(dir.join("environments/staging.bru")).expect("lecture");
    assert!(
        written.contains("host: https://prod.example.com"),
        "{written}"
    );
    assert!(written.contains("port: 8080"), "{written}");
    assert!(
        written.contains("vars:secret [\n  apiKey,\n  jwtToken\n]"),
        "{written}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn folder_and_collection_kv_editing_and_save() {
    let dir = workdir("folder-collection-edit");
    fs::write(
        dir.join("bruno.json"),
        r#"{"version":"1","name":"test","type":"collection"}"#,
    )
    .expect("bruno.json");
    fs::write(
        dir.join("collection.bru"),
        "headers {\n  X-Root: root-val\n}\n",
    )
    .expect("collection.bru");
    fs::create_dir_all(dir.join("grp")).expect("grp dir");
    fs::write(
        dir.join("grp/folder.bru"),
        "meta {\n  name: grp\n}\n\nheaders {\n  X-Folder: old-folder-val\n}\n",
    )
    .expect("folder.bru");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let (clipboard, _rx) = SpyClipboard::new();
    let handle = spawn_run(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // 1. Sur le dossier grp dans l'arbre : passer au détail (Tab)
    sender.send(key(KeyCode::Tab)).await.expect("send");
    // 2. Ouvrir la session d'édition ('e')
    sender.send(key(KeyCode::Char('e'))).await.expect("send");
    // 3. Entrée sur le champ Header(0) pour modifier la valeur
    sender.send(key(KeyCode::Enter)).await.expect("send");
    for _ in 0..15 {
        sender.send(key(KeyCode::Backspace)).await.expect("send");
    }
    for c in "new-folder-val".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("send");
    }
    sender.send(key(KeyCode::Enter)).await.expect("send");
    // Sauvegarder avec Ctrl+S
    sender.send(ctrl('s')).await.expect("send");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // 4. Fermer la session propre avec Échap, revenir à l'arbre
    sender.send(key(KeyCode::Esc)).await.expect("send");
    sender.send(key(KeyCode::Esc)).await.expect("send");

    // 5. Sélectionner la collection avec 'c'
    sender.send(key(KeyCode::Char('c'))).await.expect("send");
    // Passer au détail (Tab)
    sender.send(key(KeyCode::Tab)).await.expect("send");
    // Ouvrir la session d'édition ('e')
    sender.send(key(KeyCode::Char('e'))).await.expect("send");
    // Entrée sur le champ Header(0)
    sender.send(key(KeyCode::Enter)).await.expect("send");
    for _ in 0..15 {
        sender.send(key(KeyCode::Backspace)).await.expect("send");
    }
    for c in "new-root-val".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("send");
    }
    sender.send(key(KeyCode::Enter)).await.expect("send");
    // Sauvegarder avec Ctrl+S
    sender.send(ctrl('s')).await.expect("send");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Quitter
    sender.send(key(KeyCode::Esc)).await.expect("send");
    sender.send(key(KeyCode::Char('q'))).await.expect("send");

    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("fermeture normale")
        .expect("join handle");
    assert!(matches!(exit, Exit::Normal));

    // Vérifier les fichiers sauvegardés sur disque
    let folder_content = fs::read_to_string(dir.join("grp/folder.bru")).expect("read folder");
    assert!(
        folder_content.contains("X-Folder: new-folder-val"),
        "{folder_content}"
    );

    let coll_content = fs::read_to_string(dir.join("collection.bru")).expect("read collection");
    assert!(
        coll_content.contains("X-Root: new-root-val"),
        "{coll_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn external_text_editor_flow_with_fake_editor() {
    let dir = workdir("fake-editor");
    copy_single_case("simple.bru", &dir);

    let log_path = dir.join("temp_seen.log");
    let fake_editor_script = format!(
        "sh -c 'echo \"$1\" > \"{}\"; echo \"console.log(\\\"hello\\\");\" > \"$1\"' --",
        log_path.display()
    );

    let (clipboard, _) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run_with_editor(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
        Some(fake_editor_script),
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Aller sur le focus Détail (Tab), afficher l'onglet Script (→ ×5
    // depuis Params) et ouvrir son popup (Entrée) : curseur sur le script
    // pré-requête.
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    for _ in 0..5 {
        sender.send(key(KeyCode::Right)).await.expect("right");
    }
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Appuyer sur Entrée pour lancer le faux éditeur externe
    sender
        .send(key(KeyCode::Enter))
        .await
        .expect("enter to edit");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Sauvegarder la requête avec Ctrl+S
    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Quitter l'application avec 'q'
    sender.send(key(KeyCode::Char('q'))).await.expect("quit");

    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("fermeture normale")
        .expect("join handle");
    assert!(matches!(exit, Exit::Normal));

    // 1. Vérifier que le faux éditeur a bien été appelé et que le fichier temporaire est supprimé
    let temp_file_path_str = fs::read_to_string(&log_path).expect("lecture temp_seen.log");
    let temp_file_path = PathBuf::from(temp_file_path_str.trim());
    assert!(
        !temp_file_path.exists(),
        "Le fichier temporaire {} aurait dû être supprimé !",
        temp_file_path.display()
    );

    // 2. Vérifier que simple.bru sur le disque contient bien le script réindenté
    let saved_content = fs::read_to_string(dir.join("simple.bru")).expect("lecture simple.bru");
    assert!(
        saved_content.contains("script:pre-request {\n  console.log(\"hello\");\n}"),
        "Contenu attendu non trouvé dans simple.bru:\n{saved_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn external_text_editor_flow_on_folder_bru() {
    let dir = workdir("fake-editor-folder");
    let source_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases");
    fs::copy(source_dir.join("bruno.json"), dir.join("bruno.json")).expect("bruno.json");
    fs::create_dir_all(dir.join("grp")).expect("grp dir");
    fs::copy(
        source_dir.join("grp/folder.bru"),
        dir.join("grp/folder.bru"),
    )
    .expect("folder.bru");

    let fake_editor_script = "sh -c 'echo \"console.log(\\\"folder-test\\\");\" > \"$1\"' --";

    let (clipboard, _) = SpyClipboard::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run_with_editor(
        dir.clone(),
        sender.clone(),
        events,
        Arc::new(BruWriter),
        clipboard,
        Some(fake_editor_script.to_string()),
    );

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Aller sur le dossier grp dans le détail, afficher son onglet Script
    // (→ ×3 depuis En-têtes) et ouvrir son popup : curseur sur le script
    // pré-requête.
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    for _ in 0..3 {
        sender.send(key(KeyCode::Right)).await.expect("right");
    }
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    sender
        .send(key(KeyCode::Enter))
        .await
        .expect("enter to edit");
    tokio::time::sleep(Duration::from_millis(150)).await;

    sender.send(ctrl('s')).await.expect("ctrl+s");
    tokio::time::sleep(Duration::from_millis(150)).await;

    sender.send(key(KeyCode::Char('q'))).await.expect("quit");

    let (_terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("fermeture")
        .expect("join");
    assert!(matches!(exit, Exit::Normal));

    let saved_content = fs::read_to_string(dir.join("grp/folder.bru")).expect("lecture folder.bru");
    assert!(
        saved_content.contains("script:pre-request {\n  console.log(\"folder-test\");\n}"),
        "Contenu attendu non trouvé dans folder.bru:\n{saved_content}"
    );

    let _ = fs::remove_dir_all(&dir);
}
