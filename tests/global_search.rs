//! Tests d'intégration pour la recherche globale (Ctrl+K).
//!
//! Exercé avec les fixtures réelles de `tests/fixtures/collections/` et un terminal
//! simulé en mémoire.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::collection::{BruLoader, CollectionLoader};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::timeout;

fn fixture_parser_cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases")
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

fn ctrl_key(c: char) -> AppEvent {
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

fn spawn_run(
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    events: mpsc::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<RunResult> {
    tokio::spawn(async move {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
        let loader = Arc::new(BruLoader);
        let result = run(
            &mut terminal,
            loader,
            source,
            sender,
            events,
            "bru".into(),
            Arc::new(bruno_tui::app::clipboard::SystemClipboard::default()),
            Arc::new(bruno_tui::writer::BruWriter),
            Vec::new(),
            bruno_tui::app::mouse::MouseSetup::terminal(false),
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            false,
            false,
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

#[tokio::test]
async fn global_search_opens_filters_and_selects_request() {
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let fixture = fixture_parser_cases();
    let handle = spawn_run(fixture.clone(), sender.clone(), events);

    // 1. Charger la collection
    let collection = BruLoader.load(&fixture);
    sender
        .send(AppEvent::CollectionLoaded(collection))
        .await
        .expect("envoi collection");

    // 2. Ouvrir la recherche globale avec Ctrl+K
    sender.send(ctrl_key('k')).await.expect("envoi Ctrl+K");

    // 3. Saisir "scripted"
    for c in "scripted".chars() {
        sender
            .send(key(KeyCode::Char(c)))
            .await
            .expect("envoi saisie");
    }

    // 4. Valider avec Entrée
    sender.send(key(KeyCode::Enter)).await.expect("envoi Enter");

    // 5. Quitter avec 'q'
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi q");

    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run termine")
        .expect("tâche");

    assert!(matches!(exit, Exit::Normal));
    let screen_text = screen(&terminal).join("\n");
    // La requête "scripted" doit être sélectionnée et affichée dans le détail
    assert!(screen_text.contains("scripted"), "{screen_text}");
}

#[tokio::test]
async fn global_search_opens_and_closes_with_escape() {
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let fixture = fixture_parser_cases();
    let handle = spawn_run(fixture.clone(), sender.clone(), events);

    let collection = BruLoader.load(&fixture);
    sender
        .send(AppEvent::CollectionLoaded(collection))
        .await
        .expect("envoi collection");

    // Ouvrir avec Ctrl+K
    sender.send(ctrl_key('k')).await.expect("envoi Ctrl+K");

    // Fermer avec Échap
    sender.send(key(KeyCode::Esc)).await.expect("envoi Esc");

    // Quitter
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi q");

    let (_, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run termine")
        .expect("tâche");

    assert!(matches!(exit, Exit::Normal));
}

#[tokio::test]
async fn global_search_matches_unresolved_url_variables() {
    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let fixture = fixture_parser_cases();
    let handle = spawn_run(fixture.clone(), sender.clone(), events);

    let collection = BruLoader.load(&fixture);
    sender
        .send(AppEvent::CollectionLoaded(collection))
        .await
        .expect("envoi collection");

    // Ouvrir avec Ctrl+K
    sender.send(ctrl_key('k')).await.expect("envoi Ctrl+K");

    // Chercher la variable d'URL {{host}}
    for c in "{{host}}".chars() {
        sender
            .send(key(KeyCode::Char(c)))
            .await
            .expect("envoi saisie");
    }

    // Valider le premier résultat
    sender.send(key(KeyCode::Enter)).await.expect("envoi Enter");

    // Quitter
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi q");

    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run termine")
        .expect("tâche");

    assert!(matches!(exit, Exit::Normal));
    let screen_text = screen(&terminal).join("\n");
    assert!(screen_text.contains("https://{{host}}"), "{screen_text}");
}
