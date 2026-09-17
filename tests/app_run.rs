//! Intégration de l'exécution de requêtes dans la boucle `app`, avec le
//! faux `bru` de `tests/fixtures/fake-bru/`. Style de `tests/app_shell.rs`.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc as std_mpsc};
use std::time::Duration;

use bruno_tui::app::event::{AppEvent, EVENT_BUFFER};
use bruno_tui::app::model::Exit;
use bruno_tui::app::run;
use bruno_tui::app::view::tree::FAILURE_MARK;
use bruno_tui::collection::{
    BruLoader, Collection, CollectionLoader, LoadError, RequestNode, RequestView, TreeNode,
};
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

fn fake_bru() -> PathBuf {
    fixtures().join("fake-bru/fake-bru.sh")
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
            Vec::new(),
        )
        .await;
        let Ok(exit) = result;
        (terminal, exit)
    })
}

/// Bloqué tant que la barrière n'est pas levée (même patron que
/// `tests/app_shell.rs`) : la collection réelle n'est jamais utilisée dans
/// ces tests, elle est injectée à la main sur le canal, mais le chargement
/// interne de `run()` doit rester en attente jusqu'à la fin du test pour ne
/// jamais produire un second `AppEvent::CollectionLoaded`. La barrière est
/// levée avant la fin de chaque test pour que le runtime de test puisse
/// s'arrêter (`spawn_blocking` ne se termine pas tout seul sinon).
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

/// Lance `run()` avec une collection injectée directement sur le canal
/// d'événements, sans jamais attendre le chargement réel du `GateLoader`.
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

/// Requête synthétique, sans fichier réel derrière : `path` pilote
/// directement la cible passée au faux `bru`.
fn stub_request(path: &str) -> TreeNode {
    TreeNode::Request(RequestNode {
        path: path.into(),
        ast: None,
        view: RequestView {
            name: Some(path.to_owned()),
            kind: Some("http".to_owned()),
            seq: None,
            method: "GET".to_owned(),
            url: "http://127.0.0.1:1/x".to_owned(),
            headers: Vec::new(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            body: None,
            auth: None,
            has_pre_request_script: false,
            has_post_response_script: false,
            has_tests: false,
            has_assert: false,
            assertions: Vec::new(),
        },
    })
}

fn stub_collection(root: PathBuf, paths: &[&str]) -> Collection {
    Collection {
        root,
        name: "stub".to_owned(),
        settings: None,
        tree: paths.iter().map(|path| stub_request(path)).collect(),
        environments: Vec::new(),
    }
}

/// Répertoire temporaire dédié à un test : jamais la collection de fixture
/// suivie par git, pour ne jamais y écrire ni interférer avec un autre test
/// qui vérifie l'absence de fichier de rapport.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("bruno-tui-app-run-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&path).expect("répertoire temporaire");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Fichier de PID écrit par le mode `sleep` du faux `bru` quand il est
/// invoqué avec une seule cible : la boucle `app` ne fournit jamais qu'une
/// seule cible par exécution (D4), donc `$2` vaut toujours le nom du drapeau
/// suivant construit par `RunRequest::bru_args` pour une requête simple non
/// récursive sans environnement : `--reporter-json`. Fixe et déterministe
/// tant que cette construction ne change pas.
fn sleep_pid_file(dir: &Path) -> PathBuf {
    dir.join("--reporter-json")
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
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[tokio::test]
async fn launching_a_request_shows_its_result() {
    // `ok` (seq 1) est le premier nœud de `runner-probe` : sélectionné par
    // défaut, sans navigation.
    let collection = BruLoader.load(&runner_probe()).expect("collection chargée");
    // Terminal haut : le texte du détail (requête + section « Résultat »)
    // dépasse largement une hauteur usuelle, et le test vérifie à la fois le
    // verdict et le message d'assertion sans faire défiler.
    let (sender, handle, open) =
        spawn_app(collection, runner_probe(), fake_bru(), (100, 150)).await;

    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    // Le faux `bru` répond quasi instantanément (pas de sommeil) : cette
    // marge suffit très largement, sans risque de fausse détection de
    // blocage (contrairement à `slow-report`, testé séparément).
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Le message d'assertion est dans l'onglet Tests du panneau Réponse
    // (`response-tabs`), pas dans l'onglet Corps actif par défaut :
    // Tab, Tab pour y porter le focus, puis deux fois → pour l'atteindre.
    sender.send(key(KeyCode::Tab)).await.expect("envoi");
    sender.send(key(KeyCode::Tab)).await.expect("envoi");
    sender.send(key(KeyCode::Right)).await.expect("envoi");
    sender.send(key(KeyCode::Right)).await.expect("envoi");
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");

    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    let screen = screen(&terminal).join("\n");
    assert!(screen.contains("Verdict : échec"), "{screen}");
    assert!(screen.contains("expected 200 to equal 404"), "{screen}");
    assert!(screen.contains(FAILURE_MARK), "{screen}");
}

#[tokio::test]
async fn quit_does_not_wait_for_a_running_execution() {
    let temp = TempDir::new("slow-report");
    let collection = stub_collection(temp.0.clone(), &["slow-report"]);
    let (sender, handle, open) = spawn_app(collection, temp.0.clone(), fake_bru(), (100, 30)).await;

    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    sender.send(key(KeyCode::Down)).await.expect("envoi");
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");

    // Le faux `bru` dort 1 s en mode `slow-report` : l'interface doit se
    // fermer bien avant.
    let (_, exit) = timeout(Duration::from_millis(500), handle)
        .await
        .expect("run doit retourner sans attendre l'exécution")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
}

#[tokio::test]
async fn cancelling_a_running_execution_kills_the_process() {
    let temp = TempDir::new("cancel");
    let pid_path = sleep_pid_file(&temp.0);
    let collection = stub_collection(temp.0.clone(), &["sleep"]);
    let (sender, handle, open) = spawn_app(collection, temp.0.clone(), fake_bru(), (100, 30)).await;

    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    let pid = wait_for_pid(&pid_path).await;
    assert!(process_exists(pid));

    sender.send(ctrl('x')).await.expect("envoi");
    // Laisse le temps à l'annulation d'être traitée et au processus de
    // mourir avant de vérifier.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!process_exists(pid), "le processus {pid} survit");

    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    let screen = screen(&terminal).join("\n");
    assert!(screen.contains("annulée"), "{screen}");
}

#[tokio::test]
async fn only_one_execution_runs_at_a_time() {
    let temp = TempDir::new("one-at-a-time");
    let pid_path = sleep_pid_file(&temp.0);
    let collection = stub_collection(temp.0.clone(), &["sleep", "other"]);
    let (sender, handle, open) = spawn_app(collection, temp.0.clone(), fake_bru(), (100, 30)).await;

    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    let pid = wait_for_pid(&pid_path).await;

    // Change la sélection puis relance : sans effet tant que la première
    // exécution est en cours.
    sender.send(key(KeyCode::Down)).await.expect("envoi");
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(process_exists(pid), "la première exécution doit continuer");
    // Un second lancement aurait réécrit le même fichier de PID.
    let content = std::fs::read_to_string(&pid_path).expect("fichier de PID");
    assert_eq!(content.trim().parse::<u32>().expect("PID"), pid);

    sender.send(ctrl('x')).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (_, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    assert!(!process_exists(pid));
}

async fn run_to_failure_message(name: &str, bru_program: PathBuf) -> String {
    let temp = TempDir::new(name);
    let collection = stub_collection(temp.0.clone(), &["none"]);
    let (sender, handle, open) =
        spawn_app(collection, temp.0.clone(), bru_program, (100, 30)).await;

    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    screen(&terminal).join("\n")
}

#[tokio::test]
async fn no_report_is_reported_distinctly() {
    // Le nœud sélectionné se nomme `none` : le faux `bru`, invoqué avec
    // cette seule cible, n'écrit aucun rapport.
    let screen = run_to_failure_message("none", fake_bru()).await;
    assert!(screen.contains("sans produire de rapport"), "{screen}");
}

#[tokio::test]
async fn invalid_report_is_reported_distinctly_without_its_content() {
    // Réutilise la même fixture, avec une collection dont la seule requête
    // se nomme `invalid` pour cibler ce mode du faux `bru`.
    let temp = TempDir::new("invalid");
    let collection = stub_collection(temp.0.clone(), &["invalid"]);
    let (sender, handle, open) = spawn_app(collection, temp.0.clone(), fake_bru(), (100, 30)).await;

    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    let screen = screen(&terminal).join("\n");
    assert!(screen.contains("non conforme"), "{screen}");
    assert!(!screen.contains("iterationIndex"), "{screen}");
}

#[tokio::test]
async fn missing_bru_program_is_reported_distinctly() {
    let screen = run_to_failure_message(
        "missing-program",
        PathBuf::from("bru-programme-inexistant-bruno-tui"),
    )
    .await;
    assert!(screen.contains("introuvable"), "{screen}");
}

fn secret_probe() -> PathBuf {
    fixtures().join("collections/secret-probe")
}

/// Choisit l'environnement `CI`, seul environnement de `secret-probe`.
async fn choose_ci(sender: &mpsc::Sender<AppEvent>) {
    for code in [KeyCode::Char('E'), KeyCode::Down, KeyCode::Enter] {
        sender.send(key(code)).await.expect("envoi");
    }
}

#[tokio::test]
async fn vars_secret_is_resolved_from_dotenv_and_passed_to_bru() {
    let collection = BruLoader.load(&secret_probe()).expect("collection chargée");
    let (sender, handle, open) = spawn_app(collection, secret_probe(), fake_bru(), (120, 30)).await;
    // Laisse la résolution asynchrone du `.env` revenir dans la boucle.
    tokio::time::sleep(Duration::from_millis(300)).await;
    choose_ci(&sender).await;
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Panneau ouvert à la fin pour vérifier la source affichée.
    sender.send(key(KeyCode::Char('S'))).await.expect("envoi");
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    let screen = screen(&terminal).join("\n");
    // Le faux `bru` n'a servi un rapport que s'il a reçu la surcharge.
    assert!(!screen.contains("sans produire de rapport"), "{screen}");
    assert!(!screen.contains("en attente"), "{screen}");
    assert!(
        screen.contains("oktaClientSecret  .env (OKTA_CLIENT_SECRET)"),
        "{screen}"
    );
    assert!(!screen.contains("fixture-value"), "{screen}");
}

#[tokio::test]
async fn missing_vars_secret_opens_the_panel_before_running() {
    // Même collection, mais racine sans `.env` : rien n'est trouvé.
    let temp = TempDir::new("secret-missing");
    let mut collection = BruLoader.load(&secret_probe()).expect("collection chargée");
    collection.root = temp.0.clone();
    let (sender, handle, open) = spawn_app(collection, temp.0.clone(), fake_bru(), (120, 30)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    choose_ci(&sender).await;
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    // Saisie masquée de la valeur, puis lancement depuis le panneau.
    sender.send(key(KeyCode::Enter)).await.expect("envoi");
    for c in "fixture-value".chars() {
        sender.send(key(KeyCode::Char(c))).await.expect("envoi");
    }
    sender.send(key(KeyCode::Enter)).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(100)).await;
    sender.send(key(KeyCode::Char('r'))).await.expect("envoi");
    tokio::time::sleep(Duration::from_millis(300)).await;
    sender.send(key(KeyCode::Char('q'))).await.expect("envoi");
    let (terminal, exit) = timeout(Duration::from_secs(5), handle)
        .await
        .expect("run retourne")
        .expect("tâche");
    let _ = open.send(());
    assert!(matches!(exit, Exit::Normal));
    let screen = screen(&terminal).join("\n");
    assert!(!screen.contains("sans produire de rapport"), "{screen}");
    assert!(!screen.contains("fixture-value"), "{screen}");
}
