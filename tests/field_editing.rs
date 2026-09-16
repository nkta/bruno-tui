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

    // Navigation : passer au détail (Tab), démarrer l'édition (e)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Char('e'))).await.expect("e");

    // Mode Insert (i), frappe d'un caractère, sortie d'Insert (Esc)
    sender.send(key(KeyCode::Char('i'))).await.expect("i");
    sender.send(key(KeyCode::Char('!'))).await.expect("char");
    sender.send(key(KeyCode::Esc)).await.expect("esc");

    // Lancer la sauvegarde (w)
    sender.send(key(KeyCode::Char('w'))).await.expect("w");

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
    // Ouvrir la session d'édition (e)
    sender.send(key(KeyCode::Char('e'))).await.expect("e");
    // Entrer en mode Insert (i)
    sender.send(key(KeyCode::Char('i'))).await.expect("i");

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

    // Sortir d'Insert (Esc)
    sender.send(key(KeyCode::Esc)).await.expect("esc");

    // Sauvegarder (w)
    sender.send(key(KeyCode::Char('w'))).await.expect("w");
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

    // Focus Détail (Tab) et ouvrir édition (e)
    sender.send(key(KeyCode::Tab)).await.expect("tab");
    sender.send(key(KeyCode::Char('e'))).await.expect("e");

    // Modifier l'URL en mémoire
    sender.send(key(KeyCode::Char('i'))).await.expect("i");
    sender.send(key(KeyCode::Char('X'))).await.expect("char");
    sender.send(key(KeyCode::Esc)).await.expect("esc");

    // Modifier le fichier sur disque en externe pour invalider le FileStamp
    let target_file = dir.join("simple.bru");
    tokio::time::sleep(Duration::from_millis(20)).await;
    fs::write(
        &target_file,
        "get {\n  url: https://external-change.com\n}\n",
    )
    .expect("write external");

    // Tentative de sauvegarde (w)
    sender.send(key(KeyCode::Char('w'))).await.expect("w");
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
