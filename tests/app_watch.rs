//! Tests d'intégration pour le rechargement automatique de la collection.
//!
//! Exécute `run()` sur une copie temporaire d'une collection de test
//! (`tests/fixtures/collections/parser-cases`) pour vérifier la surveillance
//! réelle du système de fichiers et le comportement de `--no-watch`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::collection::BruLoader;
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

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type RunResult = (Terminal<TestBackend>, Exit);

fn spawn_run(
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
    watch: bool,
) -> tokio::task::JoinHandle<RunResult> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("terminal");
        let result = run(
            &mut terminal,
            Arc::new(BruLoader),
            source,
            sender,
            events,
            "bru".into(),
            Arc::new(bruno_tui::app::clipboard::SystemClipboard::default()),
            Arc::new(bruno_tui::writer::BruWriter),
            Vec::new(),
            bruno_tui::app::mouse::MouseSetup::terminal(false),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            watch,
            false,
            None,
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

#[tokio::test]
async fn watch_auto_reloads_when_file_is_modified() {
    let temp_root = std::env::temp_dir().join(format!(
        "bruno_tui_watch_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("epoch")
            .as_nanos()
    ));
    let _guard = TempDirGuard(temp_root.clone());
    copy_dir_all(&fixture(), &temp_root).expect("copie fixture");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(temp_root.clone(), sender.clone(), events, true);

    // Attente du chargement initial et démarrage du watcher
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Modification d'un fichier .bru dans la collection temporaire
    let target_file = temp_root.join("simple-get.bru");
    let content = std::fs::read_to_string(&target_file).expect("lecture");
    let modified = content.replace("https://{{host}}/ping", "https://{{host}}/ping-updated");
    std::fs::write(&target_file, modified).expect("écriture");

    // Attente du délai de débounce (~300ms) et du rechargement
    tokio::time::sleep(Duration::from_millis(600)).await;

    // Fermeture propre avec 'q'
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi q");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");

    assert!(matches!(exit, Exit::Normal));
    let lines = screen(&terminal);
    let text = lines.join("\n");
    assert!(
        text.contains("Collection rechargée") || text.contains("ping-updated"),
        "le rechargement doit être visible à l'écran : {text}"
    );
}

#[tokio::test]
async fn no_watch_option_disables_filesystem_watching() {
    let temp_root = std::env::temp_dir().join(format!(
        "bruno_tui_no_watch_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("epoch")
            .as_nanos()
    ));
    let _guard = TempDirGuard(temp_root.clone());
    copy_dir_all(&fixture(), &temp_root).expect("copie fixture");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    // watch: false
    let handle = spawn_run(temp_root.clone(), sender.clone(), events, false);

    tokio::time::sleep(Duration::from_millis(300)).await;

    let target_file = temp_root.join("simple-get.bru");
    let content = std::fs::read_to_string(&target_file).expect("lecture");
    let modified = content.replace("https://{{host}}/ping", "https://{{host}}/ping-nowatch");
    std::fs::write(&target_file, modified).expect("écriture");

    tokio::time::sleep(Duration::from_millis(600)).await;

    sender.send(key(KeyCode::Char('q'))).await.expect("envoi q");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");

    assert!(matches!(exit, Exit::Normal));
    let lines = screen(&terminal);
    let text = lines.join("\n");
    assert!(
        !text.contains("Collection rechargée"),
        "ne doit pas afficher Collection rechargée avec --no-watch"
    );
    assert!(
        !text.contains("ping-nowatch"),
        "ne doit pas recharger le fichier modifié avec --no-watch"
    );
}
