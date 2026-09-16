//! Test d'intégration pour les diagnostics, l'historique et le rejeu.
//!
//! Style de `tests/app_shell.rs` et `tests/app_run.rs`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::collection::{BruLoader, Collection, CollectionLoader, LoadError};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::timeout;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn runner_probe() -> PathBuf {
    fixtures().join("collections/runner-probe")
}

fn parser_cases() -> PathBuf {
    fixtures().join("collections/parser-cases")
}

fn fake_bru() -> PathBuf {
    fixtures().join("fake-bru/fake-bru.sh")
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

fn spawn_run(
    loader: Arc<dyn CollectionLoader>,
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
    bru_program: PathBuf,
    size: (u16, u16),
) -> tokio::task::JoinHandle<RunResult> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(size.0, size.1)).expect("terminal");
        let result = run(
            &mut terminal,
            loader,
            source,
            sender,
            events,
            bru_program.into(),
            Arc::new(bruno_tui::app::clipboard::SystemClipboard),
            Arc::new(bruno_tui::writer::BruWriter),
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

struct GateLoader(Mutex<std_mpsc::Receiver<()>>);

impl GateLoader {
    fn new() -> (Arc<Self>, std_mpsc::Sender<()>) {
        let (open, gate) = std_mpsc::channel();
        (Arc::new(Self(Mutex::new(gate))), open)
    }
}

impl CollectionLoader for GateLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError> {
        if let Ok(gate) = self.0.lock() {
            let _ = gate.recv();
        }
        Err(LoadError::NotACollection {
            path: path.to_path_buf(),
        })
    }
}

async fn spawn_app(
    collection: Collection,
    root: PathBuf,
    bru_program: PathBuf,
    size: (u16, u16),
) -> (
    mpsc::Sender<AppEvent>,
    tokio::task::JoinHandle<RunResult>,
    std_mpsc::Sender<()>,
) {
    let (loader, open) = GateLoader::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(loader, root, sender.clone(), events, bru_program, size);
    sender
        .send(AppEvent::CollectionLoaded(Ok(collection)))
        .await
        .expect("envoi");
    (sender, handle, open)
}

#[tokio::test]
async fn execute_request_shows_in_history_panel_and_replays_successfully() {
    let collection = BruLoader.load(&runner_probe()).expect("collection chargée");
    let (sender, handle, open) = spawn_app(collection, runner_probe(), fake_bru(), (120, 30)).await;

    // 1. Lance la première exécution sur la requête par défaut ('ok')
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 2. Bascule sur le panneau d'historique avec 'H'
    sender.send(key(KeyCode::Char('H'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Rejoue l'entrée sélectionnée dans l'historique avec 'r'
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 4. Quitte l'application avec 'q' alors que le focus est sur l'historique
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");

    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));

    let screen = screen(&terminal).join("\n");
    // Le panneau d'historique doit être affiché
    assert!(
        screen.contains("Historique"),
        "le titre du panneau d'historique doit être présent : {screen}"
    );
    // L'historique doit contenir 2 entrées pour ok.bru suite au rejeu
    assert_eq!(
        screen.matches("ok.bru").count(),
        2,
        "les 2 exécutions doivent figurer dans l'historique : {screen}"
    );
    assert!(
        screen.contains("échec"),
        "le verdict doit être affiché : {screen}"
    );
    assert!(
        screen.contains("r rejouer") && screen.contains("Échap arbre"),
        "la barre d'état doit afficher les rappels de l'historique : {screen}"
    );
}

#[tokio::test]
async fn diagnostics_panel_opens_and_displays_errors() {
    let collection = BruLoader.load(&parser_cases()).expect("collection chargée");
    let (sender, handle, open) = spawn_app(collection, parser_cases(), fake_bru(), (120, 30)).await;

    // Ouvre le panneau des diagnostics avec 'D'
    sender.send(key(KeyCode::Char('D'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Quitte l'application avec 'q' avec le panneau de diagnostics ouvert
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");

    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));

    let screen = screen(&terminal).join("\n");
    // Le badge d'erreur ⚠ 3 doit être présent dans le bandeau de titre
    assert!(
        screen.contains("⚠ 3"),
        "le badge d'erreur doit être visible : {screen}"
    );
    // Le panneau de diagnostics doit afficher les erreurs
    assert!(
        screen.contains("Diagnostics"),
        "le panneau Diagnostics doit être rendu : {screen}"
    );
    assert!(
        screen.contains("badmeta") || screen.contains("broken.bru"),
        "les fichiers en erreur doivent être listés : {screen}"
    );
    assert!(
        screen.contains("→ aller au nœud"),
        "la barre d'état des diagnostics doit être visible : {screen}"
    );
}

#[tokio::test]
async fn diagnostics_panel_cross_navigates_to_tree() {
    let collection = BruLoader.load(&parser_cases()).expect("collection chargée");
    let (sender, handle, open) = spawn_app(collection, parser_cases(), fake_bru(), (120, 30)).await;

    // 1. Ouvre le panneau des diagnostics avec 'D'
    sender.send(key(KeyCode::Char('D'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. Navigue avec flèche droite vers l'arbre
    sender.send(key(KeyCode::Right)).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 3. Quitte avec 'q'
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");

    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));

    let screen = screen(&terminal).join("\n");
    // Après navigation croisée avec Right, on est revenu sur l'arbre (focus Tree)
    assert!(
        screen.contains("Collection") && screen.contains("Détail"),
        "l'écran de collection/arbre doit être affiché : {screen}"
    );
    // badmeta doit être déplié et sélectionné
    assert!(
        screen.contains("badmeta"),
        "badmeta doit être présent dans l'arbre : {screen}"
    );
}
