//! Runner exercé avec le faux `bru` de `tests/fixtures/fake-bru/` :
//! déterministe, sans Node ni réseau.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::time::Duration;

use bruno_tui::runner::{BruRunner, ReportErrorKind, RunError, RunEvent, RunOutcome, RunRequest};
use tokio::sync::mpsc;
use tokio::time::timeout;

const EVENT_TIMEOUT: Duration = Duration::from_secs(10);

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fake_runner() -> (BruRunner, mpsc::Receiver<RunEvent>) {
    let (tx, rx) = mpsc::channel(8);
    let runner = BruRunner::with_program(fixtures().join("fake-bru/fake-bru.sh"), tx);
    (runner, rx)
}

/// Requête dont les cibles pilotent le faux `bru` : `<mode> [paramètre]`.
fn fake_request(args: &[&str]) -> RunRequest {
    RunRequest {
        targets: args.iter().map(PathBuf::from).collect(),
        recursive: false,
        ..RunRequest::collection(fixtures().join("collections/runner-probe"))
    }
}

fn mixed_report() -> String {
    fixtures().join("reports/mixed.json").display().to_string()
}

async fn next_event(rx: &mut mpsc::Receiver<RunEvent>) -> RunEvent {
    timeout(EVENT_TIMEOUT, rx.recv())
        .await
        .expect("événement attendu avant le délai")
        .expect("canal ouvert")
}

fn pid_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("bruno-tui-{}-{name}.pid", std::process::id()))
}

async fn wait_for_pid(path: &Path) -> u32 {
    for _ in 0..200 {
        if let Ok(content) = std::fs::read_to_string(path)
            && let Ok(pid) = content.trim().parse()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("le faux bru n'a pas écrit son PID dans {}", path.display());
}

fn process_exists(pid: u32) -> bool {
    StdCommand::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[tokio::test]
async fn report_on_fd3_is_completed_with_exit_code() {
    let (runner, mut rx) = fake_runner();
    let report = mixed_report();
    let handle = runner.start(fake_request(&["report", &report]));

    let event = next_event(&mut rx).await;
    assert_eq!(event.id, handle.id());
    match event.outcome {
        RunOutcome::Completed { report, exit_code } => {
            assert_eq!(exit_code, Some(1));
            assert_eq!(report.iterations()[0].results.len(), 5);
            assert_eq!(report.failures().count(), 3);
        }
        other => panic!("issue inattendue : {other:?}"),
    }
}

/// Fichiers réguliers directement sous `dir`.
fn regular_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("répertoire lisible")
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

#[tokio::test]
async fn runner_creates_no_report_file() {
    let collection = fixtures().join("collections/runner-probe");
    let cwd = std::env::current_dir().expect("répertoire courant");
    let temp = std::env::temp_dir();
    let before = (regular_files(&collection), regular_files(&cwd));

    let (runner, mut rx) = fake_runner();
    let report = mixed_report();
    let _handle = runner.start(fake_request(&["report", &report]));
    assert!(matches!(
        next_event(&mut rx).await.outcome,
        RunOutcome::Completed { .. }
    ));

    assert_eq!((regular_files(&collection), regular_files(&cwd)), before);
    // Aucun fichier contenant le rapport dans le répertoire temporaire.
    let marker = "connect ECONNREFUSED 127.0.0.1:18799";
    let leaked = regular_files(&temp)
        .into_iter()
        .find(|path| std::fs::read_to_string(path).is_ok_and(|content| content.contains(marker)));
    assert_eq!(leaked, None);
}

#[tokio::test]
async fn no_report_with_exit_code_zero() {
    let (runner, mut rx) = fake_runner();
    let _handle = runner.start(fake_request(&["none"]));
    match next_event(&mut rx).await.outcome {
        RunOutcome::Failed(RunError::NoReport { exit_code, output }) => {
            assert_eq!(exit_code, Some(0));
            assert!(output.contains("root of a collection"), "{output}");
        }
        other => panic!("issue inattendue : {other:?}"),
    }
}

#[tokio::test]
async fn path_not_found_is_no_report_with_output() {
    let (runner, mut rx) = fake_runner();
    let _handle = runner.start(fake_request(&["path-not-found"]));
    match next_event(&mut rx).await.outcome {
        RunOutcome::Failed(RunError::NoReport { exit_code, output }) => {
            assert_eq!(exit_code, Some(5));
            assert!(output.contains("Path not found"), "{output}");
        }
        other => panic!("issue inattendue : {other:?}"),
    }
}

#[tokio::test]
async fn invalid_json_is_invalid_report() {
    let (runner, mut rx) = fake_runner();
    let _handle = runner.start(fake_request(&["invalid"]));
    match next_event(&mut rx).await.outcome {
        RunOutcome::Failed(RunError::InvalidReport { kind, .. }) => {
            assert_eq!(kind, ReportErrorKind::Structure);
        }
        other => panic!("issue inattendue : {other:?}"),
    }
}

#[tokio::test]
async fn missing_program_is_bru_not_found() {
    let (tx, mut rx) = mpsc::channel(1);
    let runner = BruRunner::with_program("bru-programme-inexistant-bruno-tui", tx);
    let _handle = runner.start(fake_request(&["none"]));
    let outcome = next_event(&mut rx).await.outcome;
    assert!(
        matches!(outcome, RunOutcome::Failed(RunError::BruNotFound)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn start_returns_immediately_and_emits_a_single_event() {
    let (runner, mut rx) = fake_runner();
    let report = mixed_report();
    let handle = runner.start(fake_request(&["slow-report", &report]));

    // `bru` dort 1 s : rien ne doit encore être arrivé.
    assert!(matches!(
        rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));

    let event = next_event(&mut rx).await;
    assert_eq!(event.id, handle.id());
    assert!(matches!(event.outcome, RunOutcome::Completed { .. }));

    // Plus aucun émetteur une fois le runner détruit et la tâche finie.
    drop(runner);
    let rest = timeout(EVENT_TIMEOUT, rx.recv())
        .await
        .expect("canal fermé");
    assert!(rest.is_none(), "un seul événement attendu : {rest:?}");
    drop(handle);
}

#[tokio::test]
async fn concurrent_runs_carry_their_own_id() {
    let (runner, mut rx) = fake_runner();
    let report = mixed_report();
    let slow = runner.start(fake_request(&["slow-report", &report]));
    let fast = runner.start(fake_request(&["none"]));
    assert_ne!(slow.id(), fast.id());

    let first = next_event(&mut rx).await;
    let second = next_event(&mut rx).await;
    assert_eq!(first.id, fast.id());
    assert!(matches!(
        first.outcome,
        RunOutcome::Failed(RunError::NoReport { .. })
    ));
    assert_eq!(second.id, slow.id());
    assert!(matches!(second.outcome, RunOutcome::Completed { .. }));
}

#[tokio::test]
async fn cancel_kills_bru_and_emits_cancelled() {
    let (runner, mut rx) = fake_runner();
    let pid_path = pid_file("cancel");
    let _ = std::fs::remove_file(&pid_path);
    let handle = runner.start(fake_request(&["sleep", &pid_path.display().to_string()]));
    let id = handle.id();
    let pid = wait_for_pid(&pid_path).await;
    assert!(process_exists(pid));

    handle.cancel();
    let event = next_event(&mut rx).await;
    let _ = std::fs::remove_file(&pid_path);

    assert_eq!(event.id, id);
    assert!(
        matches!(event.outcome, RunOutcome::Cancelled),
        "{:?}",
        event.outcome
    );
    assert!(!process_exists(pid), "le processus {pid} survit");
}

#[tokio::test]
async fn dropping_the_handle_kills_bru() {
    let (runner, mut rx) = fake_runner();
    let pid_path = pid_file("drop");
    let _ = std::fs::remove_file(&pid_path);
    let handle = runner.start(fake_request(&["sleep", &pid_path.display().to_string()]));
    let pid = wait_for_pid(&pid_path).await;

    drop(handle);
    let event = next_event(&mut rx).await;
    let _ = std::fs::remove_file(&pid_path);

    assert!(matches!(event.outcome, RunOutcome::Cancelled));
    assert!(!process_exists(pid), "le processus {pid} survit");
}
