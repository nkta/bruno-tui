//! Boucle de l'interface exercée avec un terminal simulé en mémoire et des
//! événements injectés, sans terminal réel.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases")
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

/// Loader bloqué tant que la barrière n'est pas levée.
struct GateLoader {
    gate: Mutex<std_mpsc::Receiver<()>>,
    finished: Arc<AtomicBool>,
}

impl GateLoader {
    fn new() -> (Arc<Self>, std_mpsc::Sender<()>, Arc<AtomicBool>) {
        let (open, gate) = std_mpsc::channel();
        let finished = Arc::new(AtomicBool::new(false));
        let loader = Arc::new(Self {
            gate: Mutex::new(gate),
            finished: Arc::clone(&finished),
        });
        (loader, open, finished)
    }
}

impl CollectionLoader for GateLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError> {
        if let Ok(gate) = self.gate.lock() {
            let _ = gate.recv();
        }
        self.finished.store(true, Ordering::SeqCst);
        Err(LoadError::NotACollection {
            path: path.to_path_buf(),
        })
    }
}

type RunResult = (Terminal<TestBackend>, Exit);

fn spawn_run(
    loader: Arc<dyn CollectionLoader>,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<RunResult> {
    spawn_run_sized(loader, sender, events, 100)
}

/// Comme `spawn_run`, avec une largeur de terminal choisie : le titre
/// affiche le chemin absolu de la fixture sans repli à la ligne, dont la
/// longueur dépend de l'emplacement du dépôt (utile pour les tests qui
/// inspectent la première image, avant tout redimensionnement).
fn spawn_run_sized(
    loader: Arc<dyn CollectionLoader>,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
    width: u16,
) -> tokio::task::JoinHandle<RunResult> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(width, 30)).expect("terminal");
        let result = run(&mut terminal, loader, fixture(), sender, events).await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

#[tokio::test]
async fn first_frame_shows_loading_then_q_quits() {
    let (loader, open, _) = GateLoader::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    // Largeur calculée sur le chemin réel de la fixture : le titre ne
    // replie pas à la ligne, sa longueur dépend de l'emplacement du dépôt.
    let width = (fixture().display().to_string().len() + 30) as u16;
    let handle = spawn_run_sized(loader, sender.clone(), events, width);

    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());

    assert!(matches!(exit, Exit::Normal));
    let lines = screen(&terminal);
    assert!(lines[0].contains("Chargement de"), "{}", lines[0]);
    assert!(lines[0].contains("parser-cases"), "{}", lines[0]);
}

#[tokio::test]
async fn loaded_collection_is_rendered_and_navigable() {
    let (loader, open, _) = GateLoader::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(loader, sender.clone(), events);

    // Le chargement réel est injecté avant les touches, dans l'ordre du canal.
    let collection = BruLoader.load(&fixture());
    sender
        .send(AppEvent::CollectionLoaded(collection))
        .await
        .expect("envoi");
    for code in [KeyCode::Right, KeyCode::Right, KeyCode::Char('q')] {
        sender.send(key(code)).await.expect("envoi");
    }
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());

    assert!(matches!(exit, Exit::Normal));
    let lines = screen(&terminal);
    assert!(
        lines[0].contains("bruno-tui · parser-cases"),
        "{}",
        lines[0]
    );
    assert!(lines[2].contains("▾ Groupe"), "{}", lines[2]);
    assert!(lines[3].contains("GET    x"), "{}", lines[3]);
    let detail = lines.join("\n");
    assert!(detail.contains("GET https://{{host}}/x"), "{detail}");
}

#[tokio::test]
async fn quit_does_not_wait_for_a_blocked_load() {
    let (loader, open, finished) = GateLoader::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(loader, sender.clone(), events);

    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (_, exit) = timeout(Duration::from_secs(1), handle)
        .await
        .expect("run doit retourner en moins d'une seconde")
        .expect("tâche");
    assert!(matches!(exit, Exit::Normal));
    assert!(
        !finished.load(Ordering::SeqCst),
        "le chargement ne doit pas être terminé"
    );

    // Libère le loader pour que le runtime de test puisse s'arrêter.
    open.send(()).expect("barrière");
}

#[tokio::test]
async fn failed_load_keeps_the_interface_open() {
    let (loader, open, _) = GateLoader::new();
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(loader, sender.clone(), events);

    let failure = BruLoader.load(&fixture().join("../../reports"));
    assert!(matches!(failure, Err(LoadError::NotACollection { .. })));
    sender
        .send(AppEvent::CollectionLoaded(failure))
        .await
        .expect("envoi");
    sender.send(key(KeyCode::Down)).await.expect("envoi");
    // Une implémentation correcte ne se ferme jamais d'elle-même.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !handle.is_finished(),
        "l'interface ne doit pas se fermer seule"
    );

    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, _) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    let text = screen(&terminal).join("\n");
    assert!(
        text.contains("Impossible de charger la collection"),
        "{text}"
    );
    assert!(text.contains("reports"), "{text}");
}
