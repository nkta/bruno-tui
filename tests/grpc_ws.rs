//! Requêtes gRPC et WebSocket (lecture seule) : chargement des sondes
//! `grpc-ws-probe` (`.bru`) et `oc-grpc-ws-probe` (YAML), écrites selon
//! les sources de Bruno 4.2.1 (voir leurs README).

use std::path::{Path, PathBuf};

use bruno_tui::app::message::Message;
use bruno_tui::app::model::{ActiveRun, Focus, Model, RequestTab, StatusMessage};
use bruno_tui::app::update::{Command, update};
use bruno_tui::app::view::view;
use bruno_tui::collection::{
    AuthMode, AutoLoader, BodyContent, Collection, CollectionLoader, RequestNode, TreeNode,
};
use bruno_tui::runner::report::Report;
use bruno_tui::runner::{RunEvent, RunId, RunOutcome};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections")
}

fn load(name: &str) -> Collection {
    AutoLoader
        .load(&fixtures().join(name))
        .expect("collection chargée")
}

fn request<'a>(collection: &'a Collection, path: &str) -> &'a RequestNode {
    collection
        .tree
        .iter()
        .find_map(|node| match node {
            TreeNode::Request(request) if request.path == Path::new(path) => Some(request),
            _ => None,
        })
        .unwrap_or_else(|| panic!("requête {path} absente : {:?}", collection.tree))
}

fn body_text(request: &RequestNode) -> String {
    match request.view.body.as_ref().map(|body| &body.content) {
        Some(BodyContent::Text(text)) => text.clone(),
        other => panic!("corps texte attendu : {other:?}"),
    }
}

#[test]
fn bru_probe_loads_grpc_and_websocket_requests() {
    let collection = load("grpc-ws-probe");
    assert!(
        collection
            .tree
            .iter()
            .all(|node| !matches!(node, TreeNode::Error(_))),
        "{:?}",
        collection.tree
    );

    let grpc = request(&collection, "say-hello.bru");
    assert!(grpc.view.is_grpc());
    assert_eq!(grpc.view.method, "GRPC");
    assert_eq!(grpc.view.url, "grpc://localhost:50051");
    assert_eq!(grpc.view.auth, Some(AuthMode::NoAuth));
    assert!(grpc.view.auth_params().is_empty());
    assert_eq!(grpc.view.headers.len(), 1);
    assert_eq!(grpc.view.headers[0].key, "x-custom-meta");
    assert_eq!(body_text(grpc), "▸ message 1\n{\n  \"name\": \"world\"\n}");
    let details = grpc.view.grpc().expect("champs gRPC");
    assert_eq!(details.method.as_deref(), Some("/hello.Greeter/SayHello"));
    assert_eq!(details.method_type.as_deref(), Some("unary"));
    assert!(grpc.view.has_tests);

    let ws = request(&collection, "echo.bru");
    assert!(ws.view.is_ws());
    assert_eq!(ws.view.method, "WS");
    assert_eq!(ws.view.url, "ws://localhost:8080/ws");
    assert_eq!(ws.view.headers[0].key, "Sec-WebSocket-Protocol");
    assert!(body_text(ws).starts_with("▸ hello message (json, sélectionné)\n{"));
    assert_eq!(
        ws.view.script_pre_request(),
        Some("bru.setVar(\"connecting\", true);")
    );

    assert!(request(&collection, "ping.bru").view.is_http());
}

#[test]
fn yaml_probe_loads_grpc_and_websocket_requests() {
    let collection = load("oc-grpc-ws-probe");
    assert!(collection.notices.is_empty(), "{:?}", collection.notices);

    let grpc = request(&collection, "say hello.yml");
    assert!(grpc.view.is_grpc());
    assert_eq!(grpc.view.method, "GRPC");
    assert_eq!(grpc.view.auth, Some(AuthMode::Bearer));
    // Paramètres d'auth (lot auth) lus aussi pour gRPC, jamais résolus.
    assert_eq!(grpc.view.auth_params().len(), 1);
    assert_eq!(grpc.view.auth_params()[0].key, "token");
    assert_eq!(grpc.view.auth_params()[0].value, "{{token}}");
    assert_eq!(grpc.view.headers.len(), 2);
    assert!(!grpc.view.headers[1].enabled);
    let details = grpc.view.grpc().expect("champs gRPC");
    assert_eq!(details.proto_path.as_deref(), Some("./hello.proto"));
    assert_eq!(
        details.before_call_start.as_deref(),
        Some("bru.setVar(\"started\", true);")
    );
    assert!(grpc.view.has_tests);
    assert_eq!(grpc.view.docs(), Some("Requête gRPC de test"));

    let ws = request(&collection, "echo.yml");
    assert!(ws.view.is_ws());
    assert_eq!(ws.view.method, "WS");
    assert_eq!(
        body_text(ws),
        "▸ hello message (json, sélectionné)\n{\n  \"type\": \"greet\"\n}\n\n▸ bye (text)\nbye"
    );
    assert!(ws.view.has_pre_request_script);
}

// --- Modèle et rendu ---------------------------------------------------

fn loaded_model(name: &str) -> Model {
    let mut model = Model::new(fixtures().join(name), (120, 40));
    update(&mut model, Message::CollectionLoaded(Ok(load(name))));
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

fn select(model: &mut Model, path: &str) {
    for _ in 0..20 {
        if model
            .selected_node()
            .is_some_and(|node| node.path() == Path::new(path))
        {
            return;
        }
        update(model, Message::Down);
    }
    panic!("{path} introuvable dans l'arbre");
}

#[test]
fn tree_shows_the_type_instead_of_the_method() {
    for (name, grpc, ws) in [
        ("grpc-ws-probe", "say-hello.bru", "echo.bru"),
        ("oc-grpc-ws-probe", "say hello.yml", "echo.yml"),
    ] {
        let mut model = loaded_model(name);
        let screen = render(&model);
        assert!(screen.contains("GRPC   say hello"), "{screen}");
        assert!(screen.contains("WS     echo"), "{screen}");
        assert!(screen.contains("GET    "), "{screen}");

        select(&mut model, grpc);
        let screen = render(&model);
        assert!(screen.contains("grpc://localhost:50051"), "{screen}");
        assert!(screen.contains("Lecture seule, non exécutable"), "{screen}");
        // Ni bouton « Lancer » ni flèche de choix de méthode.
        assert!(!screen.contains("Lancer (r)"), "{screen}");
        assert!(
            screen.contains("GRPC    grpc://localhost:50051"),
            "{screen}"
        );
        select(&mut model, ws);
        let screen = render(&model);
        assert!(screen.contains("ws://localhost:8080/ws"), "{screen}");
        assert!(screen.contains("WS echo"), "{screen}");
        assert!(screen.contains("Lecture seule, non exécutable"), "{screen}");
        select(
            &mut model,
            if grpc.ends_with(".bru") {
                "ping.bru"
            } else {
                "ping.yml"
            },
        );
        assert!(render(&model).contains("Lancer (r)"));
    }
}

#[test]
fn detail_tabs_show_what_the_grpc_file_declares() {
    let mut model = loaded_model("oc-grpc-ws-probe");
    select(&mut model, "say hello.yml");
    model.focus = Focus::Detail;

    model.request_tab = RequestTab::Params;
    update(&mut model, Message::ToggleZoom);
    let screen = render(&model);
    assert!(screen.contains("Appel gRPC"), "{screen}");
    assert!(screen.contains("/hello.Greeter/SayHello"), "{screen}");
    assert!(screen.contains("Type : unary"), "{screen}");
    assert!(screen.contains("Fichier proto : ./hello.proto"), "{screen}");

    model.request_tab = RequestTab::Headers;
    let screen = render(&model);
    assert!(screen.contains("Métadonnées"), "{screen}");
    assert!(screen.contains("x-custom-meta"), "{screen}");

    model.request_tab = RequestTab::Body;
    let screen = render(&model);
    assert!(screen.contains("Messages (grpc)"), "{screen}");
    assert!(screen.contains("▸ message 1"), "{screen}");

    model.request_tab = RequestTab::Script;
    let screen = render(&model);
    assert!(screen.contains("Script before-call-start"), "{screen}");
    assert!(
        screen.contains("bru.setVar(\"started\", true);"),
        "{screen}"
    );
    assert!(screen.contains("Script after-call-end"), "{screen}");

    model.request_tab = RequestTab::Auth;
    let screen = render(&model);
    assert!(
        screen.contains("Bearer") || screen.contains("bearer"),
        "{screen}"
    );
}

#[test]
fn run_is_refused_on_grpc_and_websocket_requests() {
    for (name, path, kind) in [
        ("grpc-ws-probe", "say-hello.bru", "gRPC"),
        ("grpc-ws-probe", "echo.bru", "WebSocket"),
        ("oc-grpc-ws-probe", "say hello.yml", "gRPC"),
        ("oc-grpc-ws-probe", "echo.yml", "WebSocket"),
    ] {
        let mut model = loaded_model(name);
        select(&mut model, path);
        for focus in [Focus::Tree, Focus::Detail] {
            model.focus = focus;
            model.last_status = None;
            let command = update(&mut model, Message::RunSelected);
            assert!(matches!(command, Command::None), "{path}");
            assert!(model.run.active.is_none());
            assert_eq!(model.last_status, Some(StatusMessage::NotExecutable(kind)));
        }
        let screen = render(&model);
        assert!(
            screen.contains(&format!("Requête {kind} : type non exécutable par bru run")),
            "{screen}"
        );
    }

    // La requête HTTP voisine se lance normalement.
    let mut model = loaded_model("grpc-ws-probe");
    select(&mut model, "ping.bru");
    let command = update(&mut model, Message::RunSelected);
    assert!(matches!(command, Command::StartRun { .. }));
}

#[test]
fn editing_is_refused_on_grpc_and_websocket_requests() {
    for (path, kind) in [("say-hello.bru", "gRPC"), ("echo.bru", "WebSocket")] {
        let mut model = loaded_model("grpc-ws-probe");
        select(&mut model, path);
        model.focus = Focus::Detail;
        for message in [Message::StartEdit, Message::Enter] {
            model.last_status = None;
            update(&mut model, message);
            assert!(model.editing.is_none(), "{path}");
            assert_eq!(
                model.last_status,
                Some(StatusMessage::ReadOnlyRequestType(kind))
            );
        }
        let screen = render(&model);
        assert!(
            screen.contains("l'édition ne gère que les requêtes HTTP"),
            "{screen}"
        );
    }
}

#[test]
fn campaign_report_marks_grpc_and_websocket_in_error_and_keeps_http() {
    let report: Report =
        serde_json::from_str(include_str!("fixtures/reports/grpc-ws.json")).expect("grpc-ws.json");
    let mut model = loaded_model("grpc-ws-probe");
    model.run.active = Some(ActiveRun {
        id: RunId(1),
        target: PathBuf::new(),
        recursive: true,
        is_campaign: true,
        handle: None,
    });
    update(
        &mut model,
        Message::RunFinished(RunEvent {
            id: RunId(1),
            outcome: RunOutcome::Completed {
                report,
                exit_code: Some(1),
                output: String::new(),
            },
        }),
    );
    assert_eq!(model.run.outcomes.len(), 3);
    let campaign = model.campaign.as_ref().expect("campagne");
    assert_eq!(campaign.total, 3);
    assert_eq!(campaign.failed, 2);

    // Le panneau Réponse de la requête gRPC affiche l'erreur de `bru`.
    select(&mut model, "say-hello.bru");
    model.focus = Focus::Tree;
    let screen = render(&model);
    assert!(screen.contains("Unsupported protocol grpc:"), "{screen}");
}
