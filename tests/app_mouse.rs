//! Tests d'intégration de la souris (`mouse-support`) : boucle complète,
//! capture souris simulée, aucune écriture sur le vrai terminal.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::mouse::{MouseCapture, MouseSetup};
use bruno_tui::app::run;
use bruno_tui::collection::BruLoader;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tokio::sync::mpsc;
use tokio::time::timeout;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases")
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> AppEvent {
    AppEvent::Terminal(Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
}

/// Capture simulée : enregistre les demandes, échoue à la demande.
#[derive(Default)]
struct FakeCapture {
    calls: Mutex<Vec<bool>>,
    fail: bool,
}

impl FakeCapture {
    fn calls(&self) -> Vec<bool> {
        self.calls.lock().map(|c| c.clone()).unwrap_or_default()
    }
}

impl MouseCapture for FakeCapture {
    fn set(&self, enabled: bool) -> std::io::Result<()> {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(enabled);
        }
        if self.fail {
            return Err(std::io::Error::other("échec simulé"));
        }
        Ok(())
    }
}

type RunResult = (Terminal<TestBackend>, Exit);

fn spawn_run(
    capture: Arc<FakeCapture>,
    enabled: bool,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<RunResult> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let result = run(
            &mut terminal,
            Arc::new(BruLoader),
            fixture(),
            sender,
            events,
            "bru".into(),
            Arc::new(bruno_tui::app::clipboard::SystemClipboard),
            Arc::new(bruno_tui::writer::BruWriter),
            Vec::new(),
            MouseSetup {
                capture,
                enabled,
                startup_error: None,
            },
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

fn screen(terminal: &Terminal<TestBackend>) -> String {
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

async fn quit(
    sender: &mpsc::Sender<AppEvent>,
    handle: tokio::task::JoinHandle<RunResult>,
) -> String {
    sender.send(key(KeyCode::Char('q'))).await.expect("q");
    let (terminal, exit) = timeout(Duration::from_secs(3), handle)
        .await
        .expect("délai")
        .expect("tâche");
    assert!(matches!(exit, Exit::Normal));
    screen(&terminal)
}

#[tokio::test]
async fn m_toggles_capture_through_the_injected_control() {
    let capture = Arc::new(FakeCapture::default());
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(Arc::clone(&capture), true, sender.clone(), events);
    tokio::time::sleep(Duration::from_millis(50)).await;

    sender.send(key(KeyCode::Char('M'))).await.expect("M");
    sender.send(key(KeyCode::Char('M'))).await.expect("M");
    let screen = quit(&sender, handle).await;

    assert_eq!(capture.calls(), [false, true]);
    assert!(screen.contains("Souris activée"), "{screen}");
}

#[tokio::test]
async fn capture_failure_keeps_state_and_reports_it() {
    let capture = Arc::new(FakeCapture {
        calls: Mutex::new(Vec::new()),
        fail: true,
    });
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(Arc::clone(&capture), true, sender.clone(), events);
    tokio::time::sleep(Duration::from_millis(50)).await;

    sender.send(key(KeyCode::Char('M'))).await.expect("M");
    tokio::time::sleep(Duration::from_millis(50)).await;
    // La capture est restée active : le clic sélectionne `post-json`.
    sender
        .send(mouse(MouseEventKind::Down(MouseButton::Left), 3, 4))
        .await
        .expect("clic");
    sender
        .send(mouse(MouseEventKind::Up(MouseButton::Left), 3, 4))
        .await
        .expect("relâchement");
    let screen = quit(&sender, handle).await;

    assert_eq!(capture.calls(), [false]);
    assert!(screen.contains("https://{{host}}/items"), "{screen}");
}

#[tokio::test]
async fn click_selects_a_tree_node_and_disabled_capture_ignores_it() {
    let capture = Arc::new(FakeCapture::default());
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(Arc::clone(&capture), true, sender.clone(), events);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Ligne 4 : troisième nœud du premier niveau, `post-json`.
    sender
        .send(mouse(MouseEventKind::Down(MouseButton::Left), 3, 4))
        .await
        .expect("clic");
    sender
        .send(mouse(MouseEventKind::Up(MouseButton::Left), 3, 4))
        .await
        .expect("relâchement");
    let screen = quit(&sender, handle).await;
    assert!(screen.contains("https://{{host}}/items"), "{screen}");

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = spawn_run(Arc::clone(&capture), false, sender.clone(), events);
    tokio::time::sleep(Duration::from_millis(50)).await;
    sender
        .send(mouse(MouseEventKind::Down(MouseButton::Left), 3, 4))
        .await
        .expect("clic");
    let screen = quit(&sender, handle).await;
    assert!(!screen.contains("https://{{host}}/items"), "{screen}");
}
