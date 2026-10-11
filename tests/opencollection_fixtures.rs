//! Collections OpenCollection (YAML) : détection du format, parsing de la
//! fixture `oc-probe` (générée par `bru import openapi`, voir son README),
//! rendu de l'arbre et du détail, refus de l'édition et appariement du
//! rapport réel `opencollection.json` avec les nœuds de l'arbre.

use std::fs;
use std::path::{Path, PathBuf};

use bruno_tui::app::message::Message;
use bruno_tui::app::model::{ActiveRun, Focus, Model, StatusMessage};
use bruno_tui::app::update::{Command, update};
use bruno_tui::app::view::view;
use bruno_tui::collection::{
    AuthMode, AutoLoader, BodyContent, BodyKind, BruLoader, Collection, CollectionFormat,
    CollectionLoader, EnvironmentExtends, KeyValue, LoadError, OpenCollectionLoader, ParseError,
    RequestNode, TreeNode,
};
use bruno_tui::runner::report::Report;
use bruno_tui::runner::{RunEvent, RunId, RunOutcome};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const REPORT: &str = include_str!("fixtures/reports/opencollection.json");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections")
}

fn oc_probe() -> PathBuf {
    fixtures().join("oc-probe")
}

fn load() -> Collection {
    AutoLoader.load(&oc_probe()).expect("collection chargée")
}

fn kv(key: &str, value: &str, enabled: bool) -> KeyValue {
    KeyValue {
        key: key.into(),
        value: value.into(),
        enabled,
    }
}

fn find<'a>(nodes: &'a [TreeNode], path: &str) -> Option<&'a TreeNode> {
    for node in nodes {
        if node.path() == Path::new(path) {
            return Some(node);
        }
        if let TreeNode::Folder(folder) = node
            && let Some(found) = find(&folder.children, path)
        {
            return Some(found);
        }
    }
    None
}

fn request<'a>(collection: &'a Collection, path: &str) -> &'a RequestNode {
    match find(&collection.tree, path) {
        Some(TreeNode::Request(request)) => request,
        other => panic!("requête {path} attendue, obtenu {other:?}"),
    }
}

fn requests(nodes: &[TreeNode], out: &mut Vec<PathBuf>) {
    for node in nodes {
        match node {
            TreeNode::Request(request) => out.push(request.path.clone()),
            TreeNode::Folder(folder) => requests(&folder.children, out),
            TreeNode::Error(_) => {}
        }
    }
}

/// Répertoire temporaire propre au test, jamais la fixture suivie par git.
fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bruno-tui-oc-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("répertoire temporaire");
    dir
}

// --- Détection du format ----------------------------------------------

#[test]
fn auto_loader_detects_opencollection_from_root_and_nested_paths() {
    let collection = load();
    assert_eq!(collection.format, CollectionFormat::OpenCollection);
    assert!(!collection.format.is_editable());
    assert_eq!(collection.name, "oc-probe");

    let nested = AutoLoader
        .load(&oc_probe().join("data/nested/secure.yml"))
        .expect("depuis un fichier imbriqué");
    assert_eq!(nested.format, CollectionFormat::OpenCollection);
    assert_eq!(nested.root, collection.root);

    let bru = AutoLoader
        .load(&fixtures().join("parser-cases"))
        .expect("collection .bru");
    assert_eq!(bru.format, CollectionFormat::Bru);
    assert!(bru.format.is_editable());
}

#[test]
fn opencollection_yml_wins_over_bruno_json_in_the_same_directory() {
    let dir = workdir("both");
    fs::write(dir.join("bruno.json"), r#"{"name":"bru"}"#).expect("bruno.json");
    fs::write(dir.join("opencollection.yml"), "info:\n  name: yml\n").expect("yml");
    fs::write(
        dir.join("a.bru"),
        "meta {\n  name: a\n}\n\nget {\n  url: x\n}\n",
    )
    .expect("a.bru");
    fs::write(dir.join("b.yml"), "info:\n  name: b\n  type: http\n").expect("b.yml");

    let collection = AutoLoader.load(&dir).expect("chargement");
    assert_eq!(collection.format, CollectionFormat::OpenCollection);
    assert_eq!(collection.name, "yml");
    let names: Vec<_> = collection.tree.iter().map(TreeNode::name).collect();
    assert_eq!(names, ["b"]);

    // Les loaders explicites restent utilisables.
    let bru = BruLoader.load(&dir).expect("bru");
    assert_eq!(bru.format, CollectionFormat::Bru);
    let names: Vec<_> = bru.tree.iter().map(TreeNode::name).collect();
    assert_eq!(names, ["a"]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn outside_any_collection_and_invalid_root_file() {
    let dir = workdir("none");
    match AutoLoader.load(&dir) {
        Err(LoadError::NotACollection { path }) => assert_eq!(path, dir),
        other => panic!("NotACollection attendu, obtenu {other:?}"),
    }
    assert!(matches!(
        OpenCollectionLoader.load(&dir),
        Err(LoadError::NotACollection { .. })
    ));

    fs::write(dir.join("opencollection.yml"), "info: [\n  token: s3cr3t\n").expect("yml");
    let error = AutoLoader.load(&dir).expect_err("yml invalide");
    assert!(matches!(error, LoadError::InvalidOpenCollection { .. }));
    assert!(!error.to_string().contains("s3cr3t"), "{error}");
    let _ = fs::remove_dir_all(&dir);
}

// --- Parsing de la fixture --------------------------------------------

#[test]
fn tree_order_and_folders_follow_the_bru_loader_rules() {
    let collection = load();
    let root: Vec<_> = collection.tree.iter().map(TreeNode::name).collect();
    assert_eq!(root, ["data", "pages"]);

    let Some(TreeNode::Folder(data)) = find(&collection.tree, "data") else {
        panic!("dossier data");
    };
    let children: Vec<_> = data.children.iter().map(TreeNode::name).collect();
    assert_eq!(children, ["data", "create data", "nested"]);

    let Some(TreeNode::Folder(nested)) = find(&collection.tree, "data/nested") else {
        panic!("dossier nested");
    };
    assert_eq!(nested.seq, Some(3));
    let meta = nested
        .meta
        .as_ref()
        .expect("folder.yml")
        .as_ref()
        .expect("valide");
    assert!(meta.has_headers);
    assert!(meta.has_auth);
    assert!(meta.has_vars);
    assert!(meta.has_pre_request_script);
    assert!(!meta.has_tests);
    assert!(meta.ast.is_none());

    // `auth: inherit` dans folder.yml : pas d'auth propre au dossier.
    let Some(TreeNode::Folder(pages)) = find(&collection.tree, "pages") else {
        panic!("dossier pages");
    };
    let meta = pages.meta.as_ref().expect("meta").as_ref().expect("valide");
    assert!(!meta.has_auth);

    let settings = collection
        .settings
        .as_ref()
        .expect("opencollection.yml")
        .as_ref()
        .expect("valide");
    assert_eq!(settings.name.as_deref(), Some("oc-probe"));
    assert!(settings.has_headers);
    assert!(collection.notices.is_empty(), "{:?}", collection.notices);
}

#[test]
fn imported_requests_match_the_openapi_spec() {
    let collection = load();
    let index = request(&collection, "pages/index.yml");
    let view = &index.view;
    assert!(index.ast.is_none());
    assert_eq!(view.name.as_deref(), Some("index"));
    assert_eq!(view.kind.as_deref(), Some("http"));
    assert_eq!(view.seq, Some(1));
    assert_eq!(view.method, "GET");
    assert_eq!(view.url, "{{baseUrl}}/index.html");
    assert_eq!(view.headers, [kv("X-Trace", "abc", true)]);
    assert_eq!(view.query_params, [kv("lang", "fr", true)]);
    assert!(view.path_params.is_empty());
    assert_eq!(view.auth, Some(AuthMode::Inherit));
    assert_eq!(view.body, None);
    assert_eq!(
        view.settings(),
        [
            kv("encodeUrl", "true", true),
            kv("timeout", "0", true),
            kv("followRedirects", "true", true),
            kv("maxRedirects", "5", true),
            kv("forwardAuthorizationHeader", "true", true),
        ]
    );

    let create = &request(&collection, "data/create data.yml").view;
    assert_eq!(create.method, "POST");
    let body = create.body.as_ref().expect("corps");
    assert_eq!(body.kind, BodyKind::Json);
    assert_eq!(
        body.content,
        BodyContent::Text("{\n  \"name\": \"probe\"\n}".into())
    );
}

#[test]
fn hand_written_request_covers_every_displayed_tab() {
    let collection = load();
    let view = &request(&collection, "data/nested/secure.yml").view;
    assert_eq!(view.url, "{{baseUrl}}/data.json?page=1");
    assert_eq!(
        view.headers,
        [
            kv("Accept", "application/json", true),
            kv("X-Debug", "1", false)
        ]
    );
    assert_eq!(
        view.query_params,
        [kv("page", "1", true), kv("verbose", "true", false)]
    );
    assert_eq!(view.auth, Some(AuthMode::Bearer));
    assert!(view.has_pre_request_script);
    assert!(view.has_post_response_script);
    assert!(view.has_tests);
    assert!(view.has_assert);
    assert_eq!(
        view.assertions(),
        [
            kv("res.status", "eq 200", true),
            kv("res.body", "isJson", true)
        ]
    );
    assert_eq!(
        view.vars_pre_request(),
        [kv("reqVar", "abc", true), kv("offVar", "off", false)]
    );
    assert_eq!(
        view.vars_post_response(),
        [kv("itemName", "res.body.name", true)]
    );
    assert_eq!(
        view.docs(),
        Some("Requête de démonstration avec auth bearer.")
    );
    assert!(view.settings().contains(&kv("timeout", "5000", true)));
}

#[test]
fn environments_use_file_stems_and_keep_secrets_out() {
    let collection = load();
    let envs: Vec<_> = collection
        .environments
        .iter()
        .map(|env| env.as_ref().expect("environnement valide"))
        .collect();
    let names: Vec<_> = envs.iter().map(|env| env.name.as_str()).collect();
    // Nom de fichier sans extension : c'est ce que `bru run --env` attend.
    assert_eq!(names, ["Environment 1", "local", "staging"]);

    let local = envs[1];
    assert_eq!(local.path, Path::new("environments/local.yml"));
    assert_eq!(
        local.variables,
        [
            kv("baseUrl", "http://127.0.0.1:18765", true),
            kv("debug", "true", false)
        ]
    );
    assert_eq!(local.secret_names, ["token"]);
    assert_eq!(local.color.as_deref(), Some("#22aa55"));
    assert!(local.ast.is_none());

    let staging = envs[2];
    assert_eq!(
        staging.extends,
        Some(EnvironmentExtends::Single("local".into()))
    );
    assert_eq!(staging.color.as_deref(), Some("#ffaa00"));
}

#[test]
fn broken_files_become_error_nodes_and_unknown_fields_notices() {
    let dir = workdir("broken");
    fs::write(
        dir.join("opencollection.yml"),
        "info:\n  name: c\nfuture: 1\n",
    )
    .expect("yml");
    fs::write(dir.join("ok.yml"), "info:\n  type: http\n  seq: 1\n").expect("ok");
    fs::write(dir.join("broken.yml"), "info: [\n").expect("broken");
    fs::write(dir.join("ws.yml"), "info:\n  type: websocket\n").expect("ws");
    fs::write(
        dir.join("extra.yml"),
        "info:\n  type: http\n  seq: 2\nhttp:\n  url: x\n  mystery: s3cr3t\n",
    )
    .expect("extra");

    let collection = AutoLoader.load(&dir).expect("chargement");
    let mut errors = Vec::new();
    for node in &collection.tree {
        if let TreeNode::Error(error) = node {
            errors.push((error.path.clone(), error.error.to_string()));
        }
    }
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(matches!(
        find(&collection.tree, "broken.yml"),
        Some(TreeNode::Error(e)) if matches!(e.error, ParseError::InvalidYaml { .. })
    ));
    assert!(matches!(
        find(&collection.tree, "ws.yml"),
        Some(TreeNode::Error(e)) if matches!(e.error, ParseError::UnsupportedItemType { .. })
    ));

    let notices: Vec<_> = collection
        .notices
        .iter()
        .map(|n| (n.path.to_string_lossy().into_owned(), n.message.clone()))
        .collect();
    assert!(
        notices.contains(&(
            "opencollection.yml".into(),
            "clé `future` inconnue, ignorée".into()
        )),
        "{notices:?}"
    );
    assert!(
        notices
            .iter()
            .any(|(path, message)| path == "extra.yml" && message.contains("http.mystery")),
        "{notices:?}"
    );
    assert!(!format!("{notices:?}").contains("s3cr3t"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ignore_list_of_the_bruno_extension_is_honoured() {
    let dir = workdir("ignore");
    fs::write(
        dir.join("opencollection.yml"),
        "info:\n  name: c\nextensions:\n  bruno:\n    ignore:\n      - tmp\n",
    )
    .expect("yml");
    fs::create_dir_all(dir.join("tmp")).expect("tmp");
    fs::write(dir.join("tmp/x.yml"), "info:\n  type: http\n").expect("x");
    fs::write(dir.join("kept.yml"), "info:\n  type: http\n").expect("kept");
    let collection = AutoLoader.load(&dir).expect("chargement");
    let names: Vec<_> = collection.tree.iter().map(TreeNode::name).collect();
    assert_eq!(names, ["kept"]);
    let _ = fs::remove_dir_all(&dir);
}

// --- Modèle, rendu et édition ------------------------------------------

fn loaded_model() -> Model {
    let mut model = Model::new(oc_probe(), (120, 40));
    update(&mut model, Message::CollectionLoaded(Ok(load())));
    model
}

fn render(model: &Model) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("terminal");
    terminal.draw(|frame| view(model, frame)).expect("dessin");
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

/// Sélectionne le nœud de chemin `path` en dépliant ses ancêtres au clavier.
fn select(model: &mut Model, path: &str) {
    for _ in 0..50 {
        let Some(node) = model.selected_node() else {
            break;
        };
        let current = node.path().to_path_buf();
        if current == Path::new(path) {
            return;
        }
        let is_ancestor = Path::new(path).starts_with(&current);
        if is_ancestor && matches!(node, TreeNode::Folder(_)) {
            update(model, Message::Right);
        }
        update(model, Message::Down);
    }
    panic!("{path} introuvable dans l'arbre");
}

#[test]
fn tree_and_detail_render_like_a_bru_collection() {
    let mut model = loaded_model();
    let screen = render(&model);
    assert!(screen.contains("oc-probe"), "{screen}");
    assert!(screen.contains("lecture seule"), "{screen}");
    assert!(
        screen.contains("data") && screen.contains("pages"),
        "{screen}"
    );

    select(&mut model, "data/nested/secure.yml");
    let screen = render(&model);
    assert!(screen.contains("secure"), "{screen}");
    assert!(screen.contains("{{baseUrl}}/data.json?page=1"), "{screen}");
    assert!(screen.contains("GET"), "{screen}");
}

#[test]
fn editing_is_refused_with_a_clear_message() {
    let mut model = loaded_model();
    select(&mut model, "pages/index.yml");
    model.focus = Focus::Detail;

    update(&mut model, Message::StartEdit);
    assert!(model.editing.is_none());
    assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));

    model.last_status = None;
    update(&mut model, Message::Enter);
    assert!(model.editing.is_none());
    assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));

    model.last_status = None;
    let command = update(&mut model, Message::SaveEdit);
    assert!(matches!(command, Command::None));
    assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));
    let screen = render(&model);
    assert!(screen.contains("lecture seule"), "{screen}");

    // Popup d'environnement : refus aussi.
    model.focus = Focus::EnvironmentPicker;
    model.environment_selected = 2;
    model.last_status = None;
    update(&mut model, Message::StartEdit);
    assert!(model.environment_editing.is_none());
    assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));

    model.last_status = None;
    let command = update(&mut model, Message::SaveEdit);
    assert!(matches!(command, Command::None));
    assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));
}

// --- Exécution et rapport ----------------------------------------------

#[test]
fn run_targets_are_yml_paths_relative_to_the_root() {
    let mut model = loaded_model();
    select(&mut model, "data/nested/secure.yml");
    match update(&mut model, Message::RunSelected) {
        Command::StartRun { request, target } => {
            assert_eq!(target, Path::new("data/nested/secure.yml"));
            assert_eq!(request.targets, [PathBuf::from("data/nested/secure.yml")]);
            assert_eq!(request.collection_root, load().root);
            assert!(!request.recursive);
        }
        other => panic!("StartRun attendu, obtenu {other:?}"),
    }
}

#[test]
fn real_report_results_match_tree_nodes() {
    let report: Report = serde_json::from_str(REPORT).expect("opencollection.json");
    let collection = load();
    let mut paths = Vec::new();
    requests(&collection.tree, &mut paths);

    let results: Vec<_> = report.iterations()[0].results.iter().collect();
    assert_eq!(results.len(), 4);
    for result in &results {
        // Pour une collection YAML, `bru` garde l'extension dans `path`
        // comme dans `test.filename`.
        assert_eq!(result.path, result.test.filename);
        assert!(
            paths.contains(&PathBuf::from(&result.test.filename)),
            "{} absent de l'arbre {paths:?}",
            result.test.filename
        );
    }
    let secure = results
        .iter()
        .find(|r| r.test.filename == "data/nested/secure.yml")
        .expect("secure");
    assert_eq!(secure.name, "secure");
    assert!(!secure.is_failure());
    assert_eq!(secure.assertion_results.len(), 2);
    assert_eq!(secure.test_results.len(), 1);

    let mut model = loaded_model();
    model.run.active = Some(ActiveRun {
        id: RunId(1),
        target: PathBuf::new(),
        recursive: true,
        handle: None,
    });
    update(
        &mut model,
        Message::RunFinished(RunEvent {
            id: RunId(1),
            outcome: RunOutcome::Completed {
                report,
                exit_code: Some(0),
            },
        }),
    );
    assert_eq!(model.run.outcomes.len(), 4);
    for path in &paths {
        assert!(
            model.run.outcomes.contains_key(path),
            "{} sans résultat",
            path.display()
        );
    }
    let campaign = model.campaign.as_ref().expect("campagne");
    assert_eq!(campaign.total, 4);
    assert_eq!(campaign.failed, 0);
}
