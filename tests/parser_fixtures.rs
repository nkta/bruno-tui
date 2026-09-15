//! Chargement de collections Bruno sur les fixtures écrites à la main de
//! `tests/fixtures/collections/parser-cases/`, et round-trip sur toutes les
//! fixtures `.bru` du dépôt.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use bruno_tui::collection::config::{find_root, read_config};
use bruno_tui::collection::{
    AuthMode, BlockBody, BodyContent, BodyKind, BruFile, BruLoader, Collection, CollectionLoader,
    ErrorNode, FolderNode, KeyValue, LoadError, ParseError, RequestNode, RequestView, TreeNode,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections")
}

fn cases() -> PathBuf {
    fixtures().join("parser-cases")
}

fn load_cases() -> Collection {
    BruLoader.load(&cases()).expect("collection chargée")
}

fn child<'a>(nodes: &'a [TreeNode], file_name: &str) -> &'a TreeNode {
    nodes
        .iter()
        .find(|node| {
            node.path()
                .file_name()
                .is_some_and(|name| name == file_name)
        })
        .unwrap_or_else(|| panic!("nœud `{file_name}` absent"))
}

fn request<'a>(nodes: &'a [TreeNode], file_name: &str) -> &'a RequestNode {
    match child(nodes, file_name) {
        TreeNode::Request(node) => node,
        other => panic!("`{file_name}` : attendu une requête, obtenu {other:?}"),
    }
}

fn folder<'a>(nodes: &'a [TreeNode], file_name: &str) -> &'a FolderNode {
    match child(nodes, file_name) {
        TreeNode::Folder(node) => node,
        other => panic!("`{file_name}` : attendu un dossier, obtenu {other:?}"),
    }
}

fn error<'a>(nodes: &'a [TreeNode], file_name: &str) -> &'a ErrorNode {
    match child(nodes, file_name) {
        TreeNode::Error(node) => node,
        other => panic!("`{file_name}` : attendu une erreur, obtenu {other:?}"),
    }
}

fn file_names(nodes: &[TreeNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| {
            node.path()
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect()
}

fn kv(key: &str, value: &str, enabled: bool) -> KeyValue {
    KeyValue {
        key: key.to_owned(),
        value: value.to_owned(),
        enabled,
    }
}

// --- Racine et configuration -------------------------------------------

#[test]
fn root_is_found_from_a_nested_request() {
    let root = find_root(&cases().join("grp/sub/deep.bru")).expect("racine trouvée");
    assert_eq!(root, fs::canonicalize(cases()).expect("chemin canonique"));

    let collection = BruLoader
        .load(&cases().join("grp/sub/deep.bru"))
        .expect("collection chargée");
    assert_eq!(collection.root, root);
    assert_eq!(collection.name, "parser-cases");
}

#[test]
fn directory_outside_any_collection() {
    let outside = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/reports");
    let error = BruLoader.load(&outside).expect_err("hors collection");
    match &error {
        LoadError::NotACollection { path } => assert_eq!(path, &outside),
        other => panic!("attendu NotACollection, obtenu {other:?}"),
    }
    assert!(
        error.to_string().contains("tests/fixtures/reports"),
        "{error}"
    );
}

#[test]
fn bruno_json_with_unknown_fields() {
    let config = read_config(&cases()).expect("bruno.json valide");
    assert_eq!(config.name.as_deref(), Some("parser-cases"));
    assert_eq!(config.ignore, ["node_modules", "tmp"]);
}

// --- Traversée et ordre ------------------------------------------------

#[test]
fn traversal_skips_ignored_non_bru_and_environments() {
    let collection = load_cases();
    let names = file_names(&collection.tree);
    for excluded in [
        "tmp",
        "notes.md",
        "environments",
        "collection.bru",
        "bruno.json",
    ] {
        assert!(
            !names.iter().any(|n| n == excluded),
            "`{excluded}` présent : {names:?}"
        );
    }

    let misc = folder(&collection.tree, "misc");
    assert_eq!(misc.name, "misc");
    assert_eq!(misc.seq, None);
    assert!(misc.meta.is_none());
    assert_eq!(file_names(&misc.children), ["z.bru"]);
    assert_eq!(misc.path, Path::new("misc"));
    assert_eq!(misc.children[0].path(), Path::new("misc/z.bru"));

    let envs: Vec<&str> = collection
        .environments
        .iter()
        .map(|env| env.as_ref().expect("environnement valide").name.as_str())
        .collect();
    assert_eq!(envs, ["local"]);
}

#[test]
fn children_are_ordered_by_seq_on_two_levels() {
    let collection = load_cases();
    assert_eq!(
        file_names(&collection.tree),
        [
            "grp",               // seq 3 (folder.bru)
            "simple-get.bru",    // seq 7
            "post-json.bru",     // seq 10
            "scripted.bru",      // seq 12
            "badmeta",           // sans seq : folder.bru invalide
            "broken.bru",        // sans seq : erreur
            "misc",              // sans seq : pas de folder.bru
            "multiline.bru",     // sans seq
            "no-method.bru",     // sans seq : erreur
            "no-seq.bru",        // sans seq
            "unknown-block.bru", // sans seq
        ]
    );

    let grp = folder(&collection.tree, "grp");
    assert_eq!(grp.name, "Groupe");
    assert_eq!(grp.seq, Some(3));
    assert_eq!(
        file_names(&grp.children),
        ["x.bru", "sub", "y.bru", "inherit.bru"]
    );

    let sub = folder(&grp.children, "sub");
    assert_eq!(sub.name, "Sous-groupe");
    assert_eq!(sub.seq, Some(5));
    assert_eq!(sub.path, Path::new("grp/sub"));
    assert_eq!(file_names(&sub.children), ["deep.bru"]);
    assert_eq!(request(&sub.children, "deep.bru").view.method, "DELETE");
}

// --- Vues typées -------------------------------------------------------

#[test]
fn simple_get_view() {
    let collection = load_cases();
    let node = request(&collection.tree, "simple-get.bru");
    let view = &node.view;
    assert_eq!(node.path, Path::new("simple-get.bru"));
    assert_eq!(view.name.as_deref(), Some("ping"));
    assert_eq!(view.kind.as_deref(), Some("http"));
    assert_eq!(view.seq, Some(7));
    assert_eq!(view.method, "GET");
    assert_eq!(view.url, "https://{{host}}/ping");
    assert_eq!(view.body, None);
    assert_eq!(view.auth, Some(AuthMode::NoAuth));
    assert!(view.headers.is_empty());
    assert!(!view.has_pre_request_script && !view.has_post_response_script);
    assert!(!view.has_tests && !view.has_assert);
}

#[test]
fn post_json_view_with_nested_braces() {
    let collection = load_cases();
    let view = &request(&collection.tree, "post-json.bru").view;
    assert_eq!(view.method, "POST");
    assert_eq!(view.headers, [kv("Content-Type", "application/json", true)]);
    let body = view.body.as_ref().expect("corps");
    assert_eq!(body.kind, BodyKind::Json);
    let BodyContent::Text(json) = &body.content else {
        panic!("attendu un corps texte, obtenu {:?}", body.content);
    };
    assert_eq!(
        json,
        "{\n  \"a\": {\n    \"b\": [\n      { \"c\": 1 }\n    ]\n  },\n  \"s\": \"}\"\n}"
    );
    let value: serde_json::Value = serde_json::from_str(json).expect("JSON valide");
    assert_eq!(value["a"]["b"][0]["c"], 1);
    assert_eq!(value["s"], "}");
}

#[test]
fn scripted_view() {
    let collection = load_cases();
    let view = &request(&collection.tree, "scripted.bru").view;
    assert!(view.has_pre_request_script);
    assert!(view.has_post_response_script);
    assert!(view.has_tests);
    assert!(view.has_assert);
    assert_eq!(view.auth, Some(AuthMode::Bearer));
    assert_eq!(
        view.headers,
        [
            kv("Accept", "application/json", true),
            kv("X-Debug", "1", false)
        ]
    );
    assert_eq!(
        view.assertions,
        [
            kv("res.status", "eq 200", true),
            kv("res.body.ok", "isTrue", true)
        ]
    );
}

#[test]
fn multiline_query_param() {
    let collection = load_cases();
    let view = &request(&collection.tree, "multiline.bru").view;
    assert_eq!(
        view.query_params,
        [
            kv("q", "line one\nline two\nline three", true),
            kv("page", "1", true)
        ]
    );
}

#[test]
fn inherited_auth_is_not_resolved() {
    let collection = load_cases();
    let grp = folder(&collection.tree, "grp");
    assert!(
        grp.meta
            .as_ref()
            .expect("folder.bru")
            .as_ref()
            .expect("valide")
            .has_auth
    );
    let view = &request(&grp.children, "inherit.bru").view;
    assert_eq!(view.auth, Some(AuthMode::Inherit));
}

#[test]
fn unknown_block_is_kept_in_place() {
    let collection = load_cases();
    let node = request(&collection.tree, "unknown-block.bru");
    let ast = node.ast.as_ref().expect("AST");
    let names: Vec<&str> = ast.blocks().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["meta", "future:thing", "get", "tests", "headers"]);
    let BlockBody::Unknown { content } = &ast.block("future:thing").expect("bloc").body else {
        panic!("attendu un bloc inconnu");
    };
    assert_eq!(
        ast.slice(content),
        "  mode: experimental\n  nested {\n    deeper: yes\n  }\n"
    );
    assert_eq!(node.view.method, "GET");
}

#[test]
fn collection_folder_and_environment_files() {
    let collection = load_cases();
    let settings = collection
        .settings
        .as_ref()
        .expect("collection.bru")
        .as_ref()
        .expect("valide");
    assert_eq!(settings.name.as_deref(), Some("parser-cases"));
    assert!(settings.has_headers);
    assert!(settings.has_auth);

    let local = collection.environments[0].as_ref().expect("valide");
    assert_eq!(local.path, Path::new("environments/local.bru"));
    assert_eq!(
        local.variables,
        [
            kv("host", "localhost:3000", true),
            kv("debug", "true", false)
        ]
    );
    assert_eq!(local.secret_names, ["token"]);
}

// --- Tolérance ---------------------------------------------------------

#[test]
fn invalid_files_do_not_stop_loading() {
    let collection = load_cases();

    let broken = error(&collection.tree, "broken.bru");
    assert_eq!(broken.path, Path::new("broken.bru"));
    assert!(matches!(
        broken.error,
        ParseError::UnclosedBlock { ref name, line: 13 } if name == "headers"
    ));
    let reason = broken.error.to_string();
    assert!(
        reason.contains("headers") && reason.contains("13"),
        "{reason}"
    );
    assert!(!reason.contains("s3cr3t"), "{reason}");

    let no_method = error(&collection.tree, "no-method.bru");
    assert!(matches!(no_method.error, ParseError::MissingMethod));

    // Les voisins valides sont chargés.
    request(&collection.tree, "simple-get.bru");
    request(&collection.tree, "no-seq.bru");

    let badmeta = folder(&collection.tree, "badmeta");
    assert_eq!(badmeta.name, "badmeta");
    assert_eq!(badmeta.seq, None);
    assert!(matches!(
        badmeta.meta,
        Some(Err(ParseError::UnclosedBlock { line: 1, .. }))
    ));
    assert_eq!(
        request(&badmeta.children, "ok.bru").view.url,
        "https://{{host}}/ok"
    );
}

// --- Round-trip ----------------------------------------------------------

/// Fichiers `.bru` volontairement invalides structurellement.
const MALFORMED: [&str; 2] = ["parser-cases/broken.bru", "parser-cases/badmeta/folder.bru"];

fn bru_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .expect("répertoire de fixtures")
        .map(|entry| entry.expect("entrée").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            bru_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "bru") {
            out.push(path);
        }
    }
}

#[test]
fn round_trip_is_byte_identical_on_every_fixture() {
    let mut files = Vec::new();
    bru_files(&cases(), &mut files);
    bru_files(&fixtures().join("runner-probe"), &mut files);
    assert!(files.len() >= 20, "fixtures trouvées : {}", files.len());

    for path in files {
        let relative = path
            .strip_prefix(fixtures())
            .expect("chemin de fixture")
            .to_string_lossy()
            .into_owned();
        let source = fs::read_to_string(&path).expect("fixture UTF-8");
        let parsed = BruFile::parse(source.clone());
        if MALFORMED.contains(&relative.as_str()) {
            assert!(parsed.is_err(), "`{relative}` devrait être rejeté");
            continue;
        }
        let file = parsed.unwrap_or_else(|e| panic!("`{relative}` : {e}"));
        assert!(file.spans_are_contiguous(), "`{relative}` : tranches");
        assert_eq!(file.raw().as_bytes(), source.as_bytes(), "`{relative}`");
        let rebuilt: String = file.nodes().iter().map(|n| file.slice(&n.span)).collect();
        assert_eq!(
            rebuilt.as_bytes(),
            fs::read(&path).expect("octets"),
            "`{relative}`"
        );
    }
}

#[test]
fn round_trip_crlf_and_missing_final_newline() {
    let lf = fs::read_to_string(cases().join("simple-get.bru")).expect("fixture");
    let reference = RequestView::from_ast(&BruFile::parse(lf.clone()).expect("LF")).expect("vue");

    for variant in [
        lf.replace('\n', "\r\n"),
        lf.trim_end_matches('\n').to_owned(),
    ] {
        let file = BruFile::parse(variant.clone()).expect("variante valide");
        assert!(file.spans_are_contiguous());
        let rebuilt: String = file.nodes().iter().map(|n| file.slice(&n.span)).collect();
        assert_eq!(rebuilt.as_bytes(), variant.as_bytes());
        assert_eq!(RequestView::from_ast(&file).expect("vue"), reference);
    }
}

// --- Lecture seule -----------------------------------------------------

type Snapshot = BTreeMap<PathBuf, (bool, u64, SystemTime, Vec<u8>)>;

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("création du répertoire de test");
    for entry in fs::read_dir(from).expect("lecture") {
        let entry = entry.expect("entrée");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copie");
        }
    }
}

fn snapshot(root: &Path, dir: &Path, out: &mut Snapshot) {
    for entry in fs::read_dir(dir).expect("lecture") {
        let path = entry.expect("entrée").path();
        let meta = fs::symlink_metadata(&path).expect("métadonnées");
        let modified = meta.modified().expect("mtime");
        let relative = path.strip_prefix(root).expect("relatif").to_path_buf();
        if meta.is_dir() {
            out.insert(relative, (true, 0, modified, Vec::new()));
            snapshot(root, &path, out);
        } else {
            let content = fs::read(&path).expect("contenu");
            out.insert(relative, (false, meta.len(), modified, content));
        }
    }
}

#[test]
fn loading_never_writes() {
    let workdir = std::env::temp_dir().join(format!("bruno-tui-parser-ro-{}", std::process::id()));
    let _ = fs::remove_dir_all(&workdir);
    copy_dir(&cases(), &workdir);

    let mut before = Snapshot::new();
    snapshot(&workdir, &workdir, &mut before);
    let collection = BruLoader.load(&workdir).expect("collection chargée");
    assert!(!collection.tree.is_empty());
    let mut after = Snapshot::new();
    snapshot(&workdir, &workdir, &mut after);

    let _ = fs::remove_dir_all(&workdir);
    assert_eq!(before.len(), after.len(), "entrées créées ou supprimées");
    assert!(before == after, "contenu ou date de modification changé");
}

// --- Abstraction CollectionLoader --------------------------------------

/// Loader de substitution : arbre fixe, sans aucun fichier.
struct FakeLoader;

impl CollectionLoader for FakeLoader {
    fn load(&self, _path: &Path) -> Result<Collection, LoadError> {
        let view = RequestView {
            name: Some("fake".into()),
            kind: None,
            seq: Some(1),
            method: "GET".into(),
            url: "http://fake".into(),
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
        };
        Ok(Collection {
            root: PathBuf::from("/fake"),
            name: "fake".into(),
            settings: None,
            tree: vec![TreeNode::Folder(FolderNode {
                path: "dir".into(),
                name: "dir".into(),
                seq: None,
                meta: None,
                children: vec![
                    TreeNode::Request(RequestNode {
                        path: "dir/a.yml".into(),
                        view,
                        ast: None,
                    }),
                    TreeNode::Error(ErrorNode {
                        path: "dir/b.yml".into(),
                        error: ParseError::MissingMethod,
                    }),
                ],
            })],
            environments: Vec::new(),
        })
    }
}

/// Code appelant qui ne connaît que l'interface.
fn summarize(loader: &dyn CollectionLoader, path: &Path) -> (String, usize, usize) {
    fn count(nodes: &[TreeNode], requests: &mut usize, errors: &mut usize) {
        for node in nodes {
            match node {
                TreeNode::Request(_) => *requests += 1,
                TreeNode::Error(_) => *errors += 1,
                TreeNode::Folder(folder) => count(&folder.children, requests, errors),
            }
        }
    }
    let collection = loader.load(path).expect("chargement");
    let (mut requests, mut errors) = (0, 0);
    count(&collection.tree, &mut requests, &mut errors);
    (collection.name, requests, errors)
}

#[test]
fn callers_are_independent_of_the_loader() {
    assert_eq!(
        summarize(&FakeLoader, Path::new("ignored")),
        ("fake".into(), 1, 1)
    );
    // parser-cases : 12 requêtes valides, 2 fichiers en erreur.
    assert_eq!(
        summarize(&BruLoader, &cases()),
        ("parser-cases".into(), 12, 2)
    );
}

#[tokio::test]
async fn loading_runs_off_the_event_loop() {
    let loaders: Vec<Arc<dyn CollectionLoader>> = vec![Arc::new(BruLoader), Arc::new(FakeLoader)];
    for loader in loaders {
        let path = cases();
        let collection = tokio::task::spawn_blocking(move || loader.load(&path))
            .await
            .expect("tâche bloquante")
            .expect("collection chargée");
        assert!(!collection.tree.is_empty());
    }
}
