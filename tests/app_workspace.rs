//! Mode workspace dans la boucle `app` : chargement de chaque collection,
//! changement de collection active et `bru run` lancé depuis la bonne
//! racine (faux `bru` qui note son répertoire de travail).
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::app::workspace::{Launch, resolve_launch};
use bruno_tui::collection::AutoLoader;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use tokio::sync::mpsc;
use tokio::time::timeout;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
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

/// Dossier temporaire propre au test, supprimé à la fin.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("bruno-tui-app-ws-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("répertoire temporaire");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Copie du workspace de démonstration, chemins réécrits en absolu.
fn demo_workspace(dir: &Path) -> PathBuf {
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).expect("dossier");
    let text =
        std::fs::read_to_string(fixtures().join("workspaces/demo/workspace.yml")).expect("fixture");
    let collections = fixtures().join("collections");
    let text = text
        .replace("/BRUNO_TUI_FIXTURES", fixtures().to_str().expect("UTF-8"))
        .replace("../../collections", collections.to_str().expect("UTF-8"));
    std::fs::write(ws.join("workspace.yml"), text).expect("écriture");
    ws
}

/// Faux `bru` qui note « répertoire courant | arguments » dans `log`, puis
/// sert le rapport réel `mixed.json`.
fn recording_bru(dir: &Path, log: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("record-bru.sh");
    let report = fixtures().join("reports/mixed.json");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$(pwd)|$*\" >> '{}'\ncat '{}' >&3\nexit 1\n",
            log.display(),
            report.display()
        ),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    script
}

fn log_lines(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn wait_log(log: &Path, count: usize) -> Vec<String> {
    for _ in 0..400 {
        let lines = log_lines(log);
        if lines.len() >= count {
            return lines;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("le faux bru n'a pas été lancé {count} fois");
}

#[tokio::test]
async fn each_collection_runs_from_its_own_root() {
    let tmp = TempDir::new("roots");
    let ws_dir = demo_workspace(&tmp.0);
    let log = tmp.0.join("bru.log");
    let bru = recording_bru(&tmp.0, &log);
    let launch = resolve_launch(std::slice::from_ref(&ws_dir)).expect("workspace");
    assert!(matches!(launch, Launch::Workspace(_)));

    let (sender, events) = mpsc::channel(EVENT_BUFFER);
    let handle = tokio::spawn({
        let sender = sender.clone();
        async move {
            let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("terminal");
            let result = run(
                &mut terminal,
                Arc::new(AutoLoader),
                launch,
                sender,
                events,
                bru.into(),
                Arc::new(bruno_tui::app::clipboard::SystemClipboard::default()),
                Arc::new(bruno_tui::writer::BruWriter),
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
        }
    });

    // Laisse charger les quatre collections, puis parcourt l'arbre.
    tokio::time::sleep(Duration::from_millis(500)).await;
    for code in [
        KeyCode::Down,  // en-tête « Multi-env »
        KeyCode::Right, // déplié
        KeyCode::Down,  // data
    ] {
        sender.send(key(code)).await.expect("envoi");
    }
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    let lines = wait_log(&log, 1).await;
    let multi_env =
        std::fs::canonicalize(fixtures().join("collections/multi-env-probe")).expect("canonique");
    let (cwd, args) = lines[0].split_once('|').expect("ligne");
    assert_eq!(Path::new(cwd), multi_env, "{lines:?}");
    assert!(args.contains("data.bru"), "{args}");
    // La requête est unitaire : les options de campagne (dont
    // --workspace-path) ne s'y appliquent pas.
    assert!(!args.contains("--workspace-path"), "{args}");

    // Fin de l'exécution, puis retour à la première collection.
    tokio::time::sleep(Duration::from_millis(500)).await;
    for code in [KeyCode::Home, KeyCode::Right, KeyCode::Down] {
        sender.send(key(code)).await.expect("envoi");
    }
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    let lines = wait_log(&log, 2).await;
    let runner_probe =
        std::fs::canonicalize(fixtures().join("collections/runner-probe")).expect("canonique");
    let (cwd, _) = lines[1].split_once('|').expect("ligne");
    assert_eq!(Path::new(cwd), runner_probe, "{lines:?}");

    // Campagne sur l'en-tête : --workspace-path vaut le dossier du workspace.
    tokio::time::sleep(Duration::from_millis(500)).await;
    sender.send(key(KeyCode::Home)).await.expect("envoi");
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    let lines = wait_log(&log, 3).await;
    let (cwd, args) = lines[2].split_once('|').expect("ligne");
    assert_eq!(Path::new(cwd), runner_probe);
    assert!(
        args.contains(&format!("--workspace-path {}", ws_dir.display())),
        "{args}"
    );

    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("arrêt")
        .expect("tâche");
    assert!(matches!(exit, Exit::Normal));
    let text = screen(&terminal);
    assert!(text.contains("Workspace : Demo"), "{text}");
    assert!(text.contains("✗ Absente — pas une collection"), "{text}");
}
