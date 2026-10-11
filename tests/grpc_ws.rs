//! Requêtes gRPC et WebSocket (lecture seule) : chargement des sondes
//! `grpc-ws-probe` (`.bru`) et `oc-grpc-ws-probe` (YAML), écrites selon
//! les sources de Bruno 4.2.1 (voir leurs README).

use std::path::{Path, PathBuf};

use bruno_tui::collection::{
    AuthMode, AutoLoader, BodyContent, Collection, CollectionLoader, RequestNode, TreeNode,
};

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
