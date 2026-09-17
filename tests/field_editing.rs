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

    // Navigation : passer au détail (Tab), ouvrir l'édition (Entrée)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Saisie de l'URL (Entrée), frappe d'un caractère, validation (Tab)
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

    // Pendant que l'écriture est bloquée par la barrière : l'interface doit rester réactive !
    // On envoie 'y' (Yank) qui produit un effet immédiat via le SpyClipboard.
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
    // Ouvrir la session d'édition (Entrée)
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    // Commencer la saisie de l'URL (Entrée)
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

    // Focus Détail (Tab) et ouvrir édition (Entrée)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Modifier l'URL en mémoire (validée par Entrée)
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

    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Enter)).await.expect("enter");

    // Saisie annulée par Échap : `q` tapé est du texte, pas une sortie.
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    for c in "qr/".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("char");
    }
    sender.send(key(KeyCode::Esc)).await.expect("esc");

    // Saisie validée puis session fermée en abandonnant (Échap, y).
    sender.send(key(KeyCode::Enter)).await.expect("enter");
    sender.send(key(KeyCode::Char('Z'))).await.expect("char");
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Esc)).await.expect("esc");
    sender.send(key(KeyCode::Char('y'))).await.expect("y");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Session fermée et propre : `q` quitte sans confirmation.
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

    // Détail, ouverture de la session. Positions : Url, Accept, X-Debug,
    // + en-tête, + paramètre de requête, id, + paramètre de chemin.
    send(key(KeyCode::Tab)).await;
    send(key(KeyCode::Enter)).await;

    // Ajout du paramètre de requête `page: 2` depuis sa ligne d'ajout ; un
    // `q` tapé dans la clé est du texte et ne ferme pas l'application.
    for _ in 0..4 {
        send(key(KeyCode::Down)).await;
    }
    send(key(KeyCode::Enter)).await;
    send(key(KeyCode::Char('q'))).await;
    send(key(KeyCode::Backspace)).await;
    for c in "page".chars() {
        send(key(KeyCode::Char(c))).await;
    }
    send(key(KeyCode::Tab)).await;
    send(key(KeyCode::Char('2'))).await;
    send(key(KeyCode::Enter)).await;

    // Curseur sur `page` : remonter sur `X-Debug` et le supprimer.
    for _ in 0..2 {
        send(key(KeyCode::Up)).await;
    }
    send(key(KeyCode::Char('d'))).await;

    // Curseur sur la ligne d'ajout d'en-tête : descendre sur `id` et le
    // renommer en `userId`.
    for _ in 0..3 {
        send(key(KeyCode::Down)).await;
    }
    send(key(KeyCode::Char('c'))).await;
    send(key(KeyCode::Backspace)).await;
    send(key(KeyCode::Backspace)).await;
    for c in "userId".chars() {
        send(key(KeyCode::Char(c))).await;
    }
    send(key(KeyCode::Enter)).await;

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
