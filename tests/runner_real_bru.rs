//! Runner exercé contre le vrai `bru` sur la collection de fixture.
//!
//! Ignoré par défaut : nécessite `bru` (Node) et `python3` dans le `PATH`,
//! et le port local 18765 libre. Lancer avec `cargo test -- --ignored`.
#![cfg(unix)]

use std::collections::BTreeSet;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use bruno_tui::runner::report::ResultStatus;
use bruno_tui::runner::{BruRunner, RunOutcome, RunRequest};
use tokio::sync::mpsc;
use tokio::time::timeout;

const PORT: u16 = 18765;

fn collection() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/runner-probe")
}

/// Serveur HTTP local servant `www/`, arrêté à la destruction.
struct HttpServer(Child);

impl HttpServer {
    fn start() -> Self {
        let mut child = Command::new("python3")
            .args([
                "-m",
                "http.server",
                &PORT.to_string(),
                "--bind",
                "127.0.0.1",
            ])
            .arg("--directory")
            .arg(collection().join("www"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("python3 requis");
        for _ in 0..100 {
            if TcpStream::connect(("127.0.0.1", PORT)).is_ok() {
                return Self(child);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("le serveur HTTP de fixture n'a pas démarré");
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Fichiers réguliers sous `dir`, récursivement si demandé.
fn regular_files(dir: &Path, recursive: bool) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(kind) if kind.is_file() => {
                files.insert(path);
            }
            Ok(kind) if kind.is_dir() && recursive => files.extend(regular_files(&path, true)),
            _ => {}
        }
    }
    files
}

#[tokio::test]
#[ignore = "nécessite bru, python3 et le port 18765"]
async fn real_bru_run_on_fixture_collection() {
    let _server = HttpServer::start();
    let cwd = std::env::current_dir().expect("répertoire courant");
    let collection_before = regular_files(&collection(), true);
    let cwd_before = regular_files(&cwd, false);

    let (tx, mut rx) = mpsc::channel(1);
    let runner = BruRunner::new(tx);
    let handle = runner.start(RunRequest::collection(collection()));

    let event = timeout(Duration::from_secs(60), rx.recv())
        .await
        .expect("bru doit terminer en moins de 60 s")
        .expect("canal ouvert");
    assert_eq!(event.id, handle.id());

    let RunOutcome::Completed { report, exit_code } = event.outcome else {
        panic!("issue inattendue : {:?}", event.outcome);
    };
    assert_eq!(exit_code, Some(1));
    assert_eq!(report.iterations().len(), 1);

    let results = &report.iterations()[0].results;
    let verdict = |name: &str| {
        results
            .iter()
            .find(|result| result.name == name)
            .unwrap_or_else(|| panic!("résultat {name} absent"))
    };
    assert!(verdict("down").is_failure());
    assert_eq!(verdict("down").status, ResultStatus::Error);
    assert!(verdict("ok").is_failure());
    assert!(verdict("json").is_failure());
    assert!(!verdict("green").is_failure());
    assert!(!verdict("skip").is_failure());
    assert_eq!(verdict("skip").status, ResultStatus::Skipped);

    // Aucun fichier de rapport créé, ni dans la collection ni dans le cwd.
    assert_eq!(regular_files(&collection(), true), collection_before);
    assert_eq!(regular_files(&cwd, false), cwd_before);
}

/// `secret-probe` : l'environnement `CI` déclare `vars:secret
/// [ oktaClientSecret ]`, le `.env` porte `OKTA_CLIENT_SECRET`, et le script
/// pré-requête vérifie la valeur lue par `bru.getEnvVar` puis saute la
/// requête (aucun réseau).
#[tokio::test]
#[ignore = "nécessite bru"]
async fn real_bru_receives_vars_secret_found_automatically() {
    use bruno_tui::app::message::Message;
    use bruno_tui::app::model::Model;
    use bruno_tui::app::update::{Command, update};
    use bruno_tui::collection::{BruLoader, CollectionLoader};
    use bruno_tui::secrets;

    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/secret-probe");
    let files_before = regular_files(&root, true);

    // Sans `--secret` : la recherche automatique suffit.
    let mut model = Model::new(root.clone(), (100, 30));
    let loaded = BruLoader.load(&root);
    let Command::ResolveSecrets {
        root: resolve_root,
        lookups,
    } = update(&mut model, Message::CollectionLoaded(loaded))
    else {
        panic!("résolution des variables secrètes attendue");
    };
    let resolved = secrets::resolve(&resolve_root, &lookups, &|key| std::env::var_os(key));
    update(
        &mut model,
        Message::SecretsResolved {
            root: resolve_root,
            resolved,
        },
    );
    model.current_environment = Some("CI".into());
    let Command::StartRun { request, .. } = update(&mut model, Message::RunSelected) else {
        panic!("exécution attendue sans proposition de saisie");
    };
    assert_eq!(request.env_vars.len(), 1);

    let (tx, mut rx) = mpsc::channel(1);
    let runner = BruRunner::new(tx);
    let _handle = runner.start(request);
    let event = timeout(Duration::from_secs(60), rx.recv())
        .await
        .expect("bru doit terminer en moins de 60 s")
        .expect("canal ouvert");
    let RunOutcome::Completed { report, .. } = event.outcome else {
        panic!("issue inattendue : {:?}", event.outcome);
    };
    let result = &report.iterations()[0].results[0];
    assert_eq!(result.pre_request_test_results.len(), 1);
    assert!(!result.is_failure(), "le test pré-requête doit passer");
    assert_eq!(regular_files(&root, true), files_before);
}
