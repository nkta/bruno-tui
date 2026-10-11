//! Tests d'intégration pour le Lot C — Contenu des dossiers et de la collection.

use std::path::{Path, PathBuf};

use bruno_tui::app::message::{Message, MouseInput, MouseKind};
use bruno_tui::app::model::{Focus, Model, RequestTab};
use bruno_tui::app::update::{Command, update};
use bruno_tui::app::view::detail::{folder_tab_bar, folder_tab_is_empty, plain_lines};
use bruno_tui::app::view::hit::{Hit, hit_test};
use bruno_tui::collection::{
    BruLoader, CollectionFormat, CollectionLoader, OpenCollectionLoader, TreeNode,
};

fn fixture_model(path: &str) -> Model {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let mut model = Model::new(root.clone(), (120, 40));
    let collection = BruLoader.load(&root);
    update(&mut model, Message::CollectionLoaded(collection));
    model
}

fn oc_fixture_model(path: &str) -> Model {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path);
    let mut model = Model::new(root.clone(), (120, 40));
    let collection = OpenCollectionLoader.load(&root);
    update(&mut model, Message::CollectionLoaded(collection));
    model
}

fn select_row(model: &mut Model, path_str: &str) {
    let index = model
        .tree
        .rows
        .iter()
        .position(|row| {
            model
                .node_at(&row.address)
                .is_some_and(|n| n.path() == Path::new(path_str))
        })
        .expect("nœud trouvé");
    model.tree.selected = index;
}

#[test]
fn folder_bru_tabs_and_masked_auth() {
    let mut model = fixture_model("tests/fixtures/collections/folder-content");
    select_row(&mut model, "group");

    // Par défaut, onglet En-têtes affiché
    assert_eq!(model.shown_request_tab(), RequestTab::Headers);
    let text = plain_lines(&model).join("\n");
    assert!(text.contains("group"), "{text}");
    assert!(
        text.contains("Portée : déclaré dans ce dossier (sans héritage)"),
        "{text}"
    );
    assert!(text.contains("X-Folder: folder-value"), "{text}");

    // Onglet Auth : mot de passe masqué (********), pas en clair
    model.request_tab = RequestTab::Auth;
    assert_eq!(model.shown_request_tab(), RequestTab::Auth);
    let auth_text = plain_lines(&model).join("\n");
    assert!(auth_text.contains("Mode : basic"), "{auth_text}");
    assert!(auth_text.contains("username: myuser"), "{auth_text}");
    assert!(auth_text.contains("password: ********"), "{auth_text}");
    assert!(
        !auth_text.contains("supersecretpassword"),
        "le secret ne doit pas apparaître en clair : {auth_text}"
    );

    // Onglet Vars
    model.request_tab = RequestTab::Vars;
    let vars_text = plain_lines(&model).join("\n");
    assert!(vars_text.contains("my_folder_var: 42"), "{vars_text}");
    assert!(vars_text.contains("after_var: done"), "{vars_text}");

    // Onglet Script
    model.request_tab = RequestTab::Script;
    let script_text = plain_lines(&model).join("\n");
    assert!(script_text.contains("folder pre script"), "{script_text}");
    assert!(script_text.contains("folder post script"), "{script_text}");

    // Onglet Tests
    model.request_tab = RequestTab::Tests;
    let tests_text = plain_lines(&model).join("\n");
    assert!(tests_text.contains("folder test"), "{tests_text}");

    // Onglet Docs
    model.request_tab = RequestTab::Docs;
    let docs_text = plain_lines(&model).join("\n");
    assert!(
        docs_text.contains("Documentation du dossier group."),
        "{docs_text}"
    );
}

#[test]
fn folder_tab_navigation_with_left_and_right() {
    let mut model = fixture_model("tests/fixtures/collections/folder-content");
    select_row(&mut model, "group");
    update(&mut model, Message::NextFocus);
    assert_eq!(model.focus, Focus::Detail);

    // Départ sur Headers
    assert_eq!(model.shown_request_tab(), RequestTab::Headers);

    // Flèche droite -> Auth
    update(&mut model, Message::Right);
    assert_eq!(model.shown_request_tab(), RequestTab::Auth);

    // Flèche droite -> Vars
    update(&mut model, Message::Right);
    assert_eq!(model.shown_request_tab(), RequestTab::Vars);

    // Flèche gauche -> Auth
    update(&mut model, Message::Left);
    assert_eq!(model.shown_request_tab(), RequestTab::Auth);

    // Flèche gauche -> Headers
    update(&mut model, Message::Left);
    assert_eq!(model.shown_request_tab(), RequestTab::Headers);

    // Flèche gauche depuis Headers boucle vers Docs
    update(&mut model, Message::Left);
    assert_eq!(model.shown_request_tab(), RequestTab::Docs);
}

#[test]
fn folder_tab_bar_marks_empty_tabs() {
    let model = fixture_model("tests/fixtures/collections/folder-content");
    let meta = match &model.loaded().expect("chargé").tree[0] {
        TreeNode::Folder(f) => f.meta.as_ref().expect("présent").as_ref().expect("valide"),
        _ => panic!("dossier attendu"),
    };

    assert!(!folder_tab_is_empty(meta, RequestTab::Headers));
    assert!(!folder_tab_is_empty(meta, RequestTab::Auth));
    assert!(!folder_tab_is_empty(meta, RequestTab::Vars));
    assert!(!folder_tab_is_empty(meta, RequestTab::Script));
    assert!(!folder_tab_is_empty(meta, RequestTab::Tests));
    assert!(!folder_tab_is_empty(meta, RequestTab::Docs));

    let bar = folder_tab_bar(meta, RequestTab::Headers, 100);
    assert!(bar.spans.iter().any(|s| s.content == "En-têtes"));
    assert!(bar.spans.iter().any(|s| s.content == "Auth"));
}

#[test]
fn root_collection_selection_via_c_and_display() {
    let mut model = fixture_model("tests/fixtures/collections/folder-content");
    assert!(!model.selected_collection);

    // Touche 'c' (Message::Rename) bascule la sélection sur la collection
    update(&mut model, Message::Rename);
    assert!(model.selected_collection);

    // Panneau Détail affiche le contenu de collection.bru
    assert_eq!(model.shown_request_tab(), RequestTab::Headers);
    let text = plain_lines(&model).join("\n");
    assert!(text.contains("folder-content-tests"), "{text}");
    assert!(
        text.contains("Portée : déclaré au niveau collection (sans héritage)"),
        "{text}"
    );
    assert!(text.contains("X-Root: root-value"), "{text}");

    // Auth masquée de la collection
    model.request_tab = RequestTab::Auth;
    let auth_text = plain_lines(&model).join("\n");
    assert!(auth_text.contains("Mode : bearer"), "{auth_text}");
    assert!(auth_text.contains("token: ********"), "{auth_text}");
    assert!(
        !auth_text.contains("root-secret-123"),
        "le token ne doit pas apparaître en clair"
    );

    // Docs de la collection
    model.request_tab = RequestTab::Docs;
    let docs_text = plain_lines(&model).join("\n");
    assert!(
        docs_text.contains("Documentation de la collection racine."),
        "{docs_text}"
    );

    // Naviguer au clavier (Up / Down) désélectionne la collection et revient à l'arbre
    update(&mut model, Message::Down);
    assert!(!model.selected_collection);

    // Ré-appuyer sur 'c' réactive la sélection
    update(&mut model, Message::Rename);
    assert!(model.selected_collection);

    // Ré-appuyer sur 'c' désactive la sélection
    update(&mut model, Message::Rename);
    assert!(!model.selected_collection);
}

#[test]
fn run_selected_on_collection_is_recursive() {
    let mut model = fixture_model("tests/fixtures/collections/folder-content");
    update(&mut model, Message::Rename);
    assert!(model.selected_collection);

    // 'r' lance la collection entière récursivement
    let cmd = update(&mut model, Message::RunSelected);
    match cmd {
        Command::StartRun {
            request, target, ..
        } => {
            assert_eq!(target, Path::new(""));
            assert_eq!(request.targets, Vec::<PathBuf>::new());
            assert!(request.recursive);
        }
        other => panic!("StartRun attendu, obtenu {other:?}"),
    }
}

#[test]
fn mouse_click_on_tree_header_selects_collection() {
    let mut model = fixture_model("tests/fixtures/collections/folder-content");
    model.mouse.capture = true;
    assert!(!model.selected_collection);

    let areas = bruno_tui::app::view::layout_for(model.size, None).expect("layout");
    // Clic sur l'en-tête de l'arbre
    let hit = hit_test(&model, areas.tree.x + 5, areas.tree.y);
    assert_eq!(hit, Some(Hit::TreeHeader));

    update(
        &mut model,
        Message::Mouse(MouseInput {
            kind: MouseKind::Press,
            column: areas.tree.x + 5,
            row: areas.tree.y,
        }),
    );
    assert!(model.selected_collection);
}

#[test]
fn read_only_no_editing_on_folder_or_collection() {
    let mut model = fixture_model("tests/fixtures/collections/folder-content");
    select_row(&mut model, "group");
    update(&mut model, Message::NextFocus);
    assert_eq!(model.focus, Focus::Detail);

    // 'e' (StartEdit) sur un dossier ne fait rien
    update(&mut model, Message::StartEdit);
    assert!(model.editing.is_none());

    // Sur la collection
    update(&mut model, Message::FocusTree);
    update(&mut model, Message::Rename);
    assert!(model.selected_collection);
    update(&mut model, Message::NextFocus);
    update(&mut model, Message::StartEdit);
    assert!(model.editing.is_none());
}

#[test]
fn opencollection_folder_and_collection_display() {
    let mut model = oc_fixture_model("tests/fixtures/collections/oc-probe");
    assert_eq!(
        model.loaded().map(|c| c.format),
        Some(CollectionFormat::OpenCollection)
    );

    // Sélection de la collection via 'c'
    update(&mut model, Message::Rename);
    assert!(model.selected_collection);
    let root_text = plain_lines(&model).join("\n");
    assert!(
        root_text.contains("Portée : déclaré au niveau collection (sans héritage)"),
        "{root_text}"
    );

    // Dossier data
    update(&mut model, Message::Rename); // retour à l'arbre
    select_row(&mut model, "data");
    let folder_text = plain_lines(&model).join("\n");
    assert!(
        folder_text.contains("Portée : déclaré dans ce dossier (sans héritage)"),
        "{folder_text}"
    );
}
