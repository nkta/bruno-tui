//! Opérations de fichiers de la phase 3 (`docs/edition-phase3.md`) sur une
//! copie de `tests/fixtures/collections/writer-cases/files/collection` :
//! octets avant / après, contenus identiques à ceux de `jsonToBru`
//! (références `*.bruno.bru` générées par
//! `scripts/gen-files-bruno-fixtures.sh`), collisions, fraîcheur,
//! confinement à la collection et corbeille.

use std::fs;
use std::path::{Path, PathBuf};

use bruno_tui::writer::files::{
    self, FileOp, FileOpError, NewRequest, RequestKind, SeqChange, new_request_content,
};
use bruno_tui::writer::names::NameError;
use bruno_tui::writer::trash::Trash;

fn cases() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/writer-cases/files")
}

fn reference(name: &str) -> String {
    fs::read_to_string(cases().join(name)).expect("référence Bruno")
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("copie");
    for entry in fs::read_dir(from).expect("lecture") {
        let entry = entry.expect("entrée");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copie fichier");
        }
    }
}

/// Copie fraîche de la mini-collection, et un `XDG_DATA_HOME` à côté.
fn workspace(tag: &str) -> (PathBuf, PathBuf) {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("tmp-file-ops-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let root = base.join("collection");
    copy_dir(&cases().join("collection"), &root);
    (root, base.join("data"))
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).expect("lecture")
}

fn original(path: &str) -> String {
    fs::read_to_string(cases().join("collection").join(path)).expect("fixture")
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("répertoire")
        .map(|entry| {
            entry
                .expect("entrée")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

fn http(name: &str, filename: &str) -> NewRequest {
    NewRequest {
        name: name.into(),
        filename: filename.into(),
        kind: RequestKind::Http,
        method: "get".into(),
        url: "{{base_url}}/posts/1".into(),
        seq: 3,
    }
}

#[test]
fn new_request_contents_match_json_to_bru() {
    let cases = [
        (http("Lire un article", "x"), "new-request-http.bruno.bru"),
        (
            NewRequest {
                name: "Lister".into(),
                filename: "x".into(),
                kind: RequestKind::Graphql,
                method: "post".into(),
                url: String::new(),
                seq: 1,
            },
            "new-request-graphql.bruno.bru",
        ),
        (
            NewRequest {
                name: "Supprimer: tout".into(),
                filename: "x".into(),
                kind: RequestKind::Http,
                method: "delete".into(),
                url: "https://api.example.com/items".into(),
                seq: 12,
            },
            "new-request-delete.bruno.bru",
        ),
        (
            NewRequest {
                name: "Commentaires".into(),
                filename: "x".into(),
                kind: RequestKind::Http,
                method: "get".into(),
                url: "{{base_url}}/posts/:id/comments?full=1&flag&a=1=2&full=2#haut".into(),
                seq: 2,
            },
            "new-request-params.bruno.bru",
        ),
        (
            NewRequest {
                name: "OData".into(),
                filename: "x".into(),
                kind: RequestKind::Http,
                method: "get".into(),
                url: "https://api.example.com/odata/Items(id=:key)/:sub?$top=5".into(),
                seq: 4,
            },
            "new-request-odata.bruno.bru",
        ),
        (
            NewRequest {
                name: "Encodé".into(),
                filename: "x".into(),
                kind: RequestKind::Graphql,
                method: "post".into(),
                url: "{{base_url}}/café/:nom élève".into(),
                seq: 1,
            },
            "new-request-encoded.bruno.bru",
        ),
    ];
    for (request, fixture) in cases {
        assert_eq!(
            new_request_content(&request).expect("contenu"),
            reference(fixture),
            "{fixture}"
        );
    }
    assert_eq!(
        files::new_folder_content("Posts", 2),
        reference("new-folder.bruno.bru")
    );
    assert_eq!(
        files::folder_meta_content("Articles", None),
        reference("folder-renamed.bruno.bru")
    );
    assert_eq!(
        files::folder_meta_content("NoMeta", Some(1)),
        reference("folder-resequenced.bruno.bru")
    );
    assert_eq!(
        files::folder_meta_content("NoMeta copie", Some(5)),
        reference("folder-cloned.bruno.bru")
    );
}

#[test]
fn create_request_writes_bruno_content_and_suffixes_collisions() {
    let (root, _) = workspace("create-request");
    let request = http("Lire un article", "Lire un article");
    let first = files::create_request(&root, Path::new(""), &request).expect("création");
    assert_eq!(first, PathBuf::from("Lire un article.bru"));
    assert_eq!(
        read(&root, "Lire un article.bru"),
        reference("new-request-http.bruno.bru")
    );

    let second = files::create_request(&root, Path::new(""), &request).expect("collision 1");
    let third = files::create_request(&root, Path::new(""), &request).expect("collision 2");
    assert_eq!(second, PathBuf::from("Lire un article 1.bru"));
    assert_eq!(third, PathBuf::from("Lire un article 2.bru"));

    let nested =
        files::create_request(&root, Path::new("Posts"), &http("A", "a")).expect("dossier");
    assert_eq!(nested, PathBuf::from("Posts/a.bru"));
    // Aucun fichier temporaire ne reste.
    assert!(names_in(&root).iter().all(|name| !name.starts_with('.')));
    assert!(
        names_in(&root.join("Posts"))
            .iter()
            .all(|name| !name.starts_with('.'))
    );
}

#[test]
fn create_request_refuses_invalid_names_and_params() {
    let (root, _) = workspace("create-refused");
    let before = names_in(&root);
    let refused = [
        (http("A", "a/b"), "nom"),
        (http("A", " a"), "nom"),
        (http("A", "folder"), "réservé"),
        (http("A", "CON"), "réservé"),
        (http("", "a"), "nom affiché"),
        (
            NewRequest {
                url: "http://moi@h/posts/:id".into(),
                ..http("A", "a")
            },
            "URL incertaine",
        ),
        (
            NewRequest {
                url: "{{base_url}}/posts?{{k}}=1".into(),
                ..http("A", "a")
            },
            "clé à guillemets",
        ),
        (
            NewRequest {
                method: "fetch".into(),
                ..http("A", "a")
            },
            "méthode",
        ),
    ];
    for (request, why) in refused {
        assert!(
            files::create_request(&root, Path::new(""), &request).is_err(),
            "{why} : {request:?}"
        );
    }
    let error = files::create_request(&root, Path::new(""), &http("A", "a:b")).expect_err("refus");
    assert_eq!(
        error.to_string(),
        "Caractère spécial interdit dans le nom : « : »."
    );
    assert_eq!(names_in(&root), before);
}

#[test]
fn create_folder_writes_folder_bru_and_suffixes_collisions() {
    let (root, _) = workspace("create-folder");
    let created =
        files::create_folder(&root, Path::new(""), "Posts", "Posts", 2).expect("création");
    assert_eq!(created, PathBuf::from("Posts 1"));
    assert_eq!(
        read(&root, "Posts 1/folder.bru"),
        reference("new-folder.bruno.bru")
    );
    assert_eq!(names_in(&root.join("Posts 1")), ["folder.bru"]);
    let again = files::create_folder(&root, Path::new(""), "Posts", "Posts", 2).expect("collision");
    assert_eq!(again, PathBuf::from("Posts 2"));
    assert!(matches!(
        files::create_folder(&root, Path::new(""), "X", "a.", 1),
        Err(FileOpError::Name(NameError::InvalidCharacter(_)))
    ));
}

#[test]
fn rename_request_changes_only_the_name_line_then_the_file() {
    let (root, _) = workspace("rename-request");
    let source = original("get-post.bru");
    let renamed = files::rename_request(
        &root,
        Path::new("get-post.bru"),
        &source,
        "Lire un billet",
        Some("Lire un billet"),
    )
    .expect("renommage");
    assert_eq!(renamed, PathBuf::from("Lire un billet.bru"));
    assert!(!root.join("get-post.bru").exists());
    assert_eq!(
        read(&root, "Lire un billet.bru"),
        source.replace("name: Lire un article", "name: Lire un billet")
    );

    // Case décochée : seul `meta.name` change.
    let list = original("list-posts.bru");
    let kept = files::rename_request(&root, Path::new("list-posts.bru"), &list, "Tous", None)
        .expect("nom seul");
    assert_eq!(kept, PathBuf::from("list-posts.bru"));
    assert_eq!(
        read(&root, "list-posts.bru"),
        list.replace("name: Lister les articles", "name: Tous")
    );

    // Nom de fichier déjà pris : suffixe, comme `getUniqueRenamePath`.
    let current = read(&root, "Lire un billet.bru");
    let suffixed = files::rename_request(
        &root,
        Path::new("Lire un billet.bru"),
        &current,
        "Lire un billet",
        Some("list-posts"),
    )
    .expect("collision");
    assert_eq!(suffixed, PathBuf::from("list-posts 1.bru"));
    assert_eq!(
        read(&root, "list-posts.bru"),
        list.replace("Lister les articles", "Tous")
    );
}

#[test]
fn rename_folder_rewrites_or_creates_folder_bru() {
    let (root, _) = workspace("rename-folder");
    let folder = original("Posts/folder.bru");
    let renamed = files::rename_folder(
        &root,
        Path::new("Posts"),
        Some(&folder),
        "Billets",
        Some("Billets"),
    )
    .expect("renommage");
    assert_eq!(renamed, PathBuf::from("Billets"));
    assert_eq!(
        read(&root, "Billets/folder.bru"),
        folder.replace("name: Articles", "name: Billets")
    );
    assert_eq!(
        read(&root, "Billets/create.bru"),
        original("Posts/create.bru")
    );

    let created = files::rename_folder(&root, Path::new("NoMeta"), None, "Articles", None)
        .expect("folder.bru créé");
    assert_eq!(created, PathBuf::from("NoMeta"));
    assert_eq!(
        read(&root, "NoMeta/folder.bru"),
        reference("folder-renamed.bruno.bru")
    );
}

#[test]
fn clone_request_is_identical_except_name_and_seq() {
    let (root, _) = workspace("clone-request");
    let source = original("get-post.bru");
    let cloned = files::clone_request(
        &root,
        Path::new("get-post.bru"),
        &source,
        "Lire un article copie",
        "Lire un article copie",
        3,
    )
    .expect("clone");
    assert_eq!(cloned, PathBuf::from("Lire un article copie.bru"));
    assert_eq!(
        read(&root, "Lire un article copie.bru"),
        source
            .replace("name: Lire un article", "name: Lire un article copie")
            .replace("seq: 1", "seq: 3")
    );
    assert_eq!(read(&root, "get-post.bru"), source);

    let again = files::clone_request(
        &root,
        Path::new("get-post.bru"),
        &source,
        "Lire un article copie",
        "Lire un article copie",
        4,
    )
    .expect("collision");
    assert_eq!(again, PathBuf::from("Lire un article copie 1.bru"));
}

#[test]
fn clone_folder_copies_recursively() {
    let (root, _) = workspace("clone-folder");
    let cloned = files::clone_folder(
        &root,
        Path::new("Posts"),
        "Articles copie",
        "Posts copie",
        5,
    )
    .expect("clone");
    assert_eq!(cloned, PathBuf::from("Posts copie"));
    assert_eq!(
        read(&root, "Posts copie/folder.bru"),
        original("Posts/folder.bru")
            .replace("name: Articles", "name: Articles copie")
            .replace("seq: 3", "seq: 5")
    );
    assert_eq!(
        read(&root, "Posts copie/create.bru"),
        original("Posts/create.bru")
    );

    let no_meta = files::clone_folder(
        &root,
        Path::new("NoMeta"),
        "NoMeta copie",
        "NoMeta copie",
        5,
    )
    .expect("clone sans folder.bru");
    assert_eq!(
        read(&root, &format!("{}/folder.bru", no_meta.display())),
        reference("folder-cloned.bruno.bru")
    );
    assert_eq!(read(&root, "NoMeta copie/x.bru"), original("NoMeta/x.bru"));
    assert!(!root.join("NoMeta/folder.bru").exists());

    let again = files::clone_folder(
        &root,
        Path::new("Posts"),
        "Articles copie",
        "Posts copie",
        6,
    )
    .expect("collision");
    assert_eq!(again, PathBuf::from("Posts copie 1"));
    assert!(names_in(&root).iter().all(|name| !name.starts_with('.')));
}

#[test]
fn resequence_writes_only_changed_files() {
    let (root, _) = workspace("resequence");
    let list = original("list-posts.bru");
    let folder = original("Posts/folder.bru");
    let get = original("get-post.bru");
    let before = fs::metadata(root.join("get-post.bru"))
        .and_then(|meta| meta.modified())
        .expect("date");
    std::thread::sleep(std::time::Duration::from_millis(20));
    files::resequence(
        &root,
        &[
            SeqChange {
                path: "get-post.bru".into(),
                is_folder: false,
                seq: 1,
                expected: Some(get.clone()),
            },
            SeqChange {
                path: "Posts".into(),
                is_folder: true,
                seq: 2,
                expected: Some(folder.clone()),
            },
            SeqChange {
                path: "list-posts.bru".into(),
                is_folder: false,
                seq: 3,
                expected: Some(list.clone()),
            },
            SeqChange {
                path: "NoMeta".into(),
                is_folder: true,
                seq: 1,
                expected: None,
            },
        ],
    )
    .expect("réordonnancement");
    assert_eq!(read(&root, "get-post.bru"), get);
    assert_eq!(
        read(&root, "list-posts.bru"),
        list.replace("seq: 2", "seq: 3")
    );
    assert_eq!(
        read(&root, "Posts/folder.bru"),
        folder.replace("seq: 3", "seq: 2")
    );
    assert_eq!(
        read(&root, "NoMeta/folder.bru"),
        reference("folder-resequenced.bruno.bru")
    );
    // Le fichier dont le `seq` ne change pas n'a pas été réécrit.
    let after = fs::metadata(root.join("get-post.bru"))
        .and_then(|meta| meta.modified())
        .expect("date");
    assert_eq!(before, after);
}

#[test]
fn stale_files_are_refused_and_left_untouched() {
    let (root, _) = workspace("stale");
    let loaded = original("get-post.bru");
    fs::write(root.join("get-post.bru"), "changé par un tiers").expect("tiers");
    for result in [
        files::rename_request(&root, Path::new("get-post.bru"), &loaded, "B", Some("b")),
        files::clone_request(&root, Path::new("get-post.bru"), &loaded, "B", "b", 9),
    ] {
        assert!(
            matches!(result, Err(FileOpError::Stale { .. })),
            "{result:?}"
        );
    }
    let error = files::resequence(
        &root,
        &[SeqChange {
            path: "get-post.bru".into(),
            is_folder: false,
            seq: 5,
            expected: Some(loaded),
        }],
    )
    .expect_err("refus");
    assert!(matches!(error, FileOpError::Stale { .. }));
    assert_eq!(read(&root, "get-post.bru"), "changé par un tiers");
    assert!(!root.join("b.bru").exists());

    // `folder.bru` apparu depuis le chargement.
    fs::write(root.join("NoMeta/folder.bru"), "meta {\n  name: Tiers\n}\n").expect("tiers");
    assert!(matches!(
        files::rename_folder(&root, Path::new("NoMeta"), None, "B", None),
        Err(FileOpError::Stale { .. })
    ));
}

#[test]
fn writes_never_leave_the_collection() {
    let (root, data) = workspace("outside");
    let outside = data.join("ailleurs");
    fs::create_dir_all(&outside).expect("ailleurs");
    std::os::unix::fs::symlink(&outside, root.join("lien")).expect("lien");

    assert!(matches!(
        files::create_request(&root, Path::new("../ailleurs"), &http("A", "a")),
        Err(FileOpError::Outside { .. })
    ));
    assert!(matches!(
        files::create_request(&root, Path::new("lien"), &http("A", "a")),
        Err(FileOpError::Outside { .. })
    ));
    assert!(matches!(
        files::create_folder(&root, Path::new("/tmp"), "A", "a", 1),
        Err(FileOpError::Outside { .. })
    ));
    let trash = Trash::at(data.join("Trash"));
    assert!(matches!(
        files::trash_item(
            &root,
            Path::new("lien/../../x"),
            &trash,
            "2026-10-14T00:00:00"
        ),
        Err(FileOpError::Outside { .. })
    ));
    assert!(matches!(
        files::trash_item(&root, Path::new(""), &trash, "2026-10-14T00:00:00"),
        Err(FileOpError::Outside { .. })
    ));
    assert_eq!(fs::read_dir(&outside).expect("ailleurs").count(), 0);
}

#[test]
fn trash_moves_requests_and_folders_without_deleting() {
    let (root, data) = workspace("trash");
    let date = "2026-10-14T10:11:12";
    let outcome = files::execute(
        &root,
        &FileOp::Trash {
            path: "get-post.bru".into(),
            renumber: Ok(Vec::new()),
        },
        || Trash::from_env_values(Some(data.clone().into_os_string()), None),
        date,
    )
    .expect("corbeille");
    assert_eq!(outcome.select, None);
    assert_eq!(outcome.message, "Mis à la corbeille : get-post.bru");
    assert!(!root.join("get-post.bru").exists());
    assert_eq!(
        fs::read_to_string(data.join("Trash/files/get-post.bru")).expect("copie"),
        original("get-post.bru")
    );
    let info = fs::read_to_string(data.join("Trash/info/get-post.bru.trashinfo")).expect("info");
    assert!(info.starts_with("[Trash Info]\nPath=/"));
    assert!(info.ends_with(&format!("/collection/get-post.bru\nDeletionDate={date}\n")));

    files::execute(
        &root,
        &FileOp::Trash {
            path: "Posts".into(),
            renumber: Ok(Vec::new()),
        },
        || Ok(Trash::at(data.join("Trash"))),
        date,
    )
    .expect("dossier");
    assert_eq!(
        fs::read_to_string(data.join("Trash/files/Posts/create.bru")).expect("copie"),
        original("Posts/create.bru")
    );
}

#[test]
fn execute_reports_the_element_to_select() {
    let (root, data) = workspace("execute");
    let trash = || Ok(Trash::at(data.join("Trash")));
    let outcome = files::execute(
        &root,
        &FileOp::CreateFolder {
            dir: PathBuf::new(),
            name: "Nouveau".into(),
            dirname: "Nouveau".into(),
            seq: 5,
        },
        trash,
        "",
    )
    .expect("dossier");
    assert_eq!(outcome.select, Some(PathBuf::from("Nouveau")));
    assert_eq!(outcome.message, "Dossier créé : Nouveau");
}

/// Paramètres d'URL : chaque cas de la table produite par le vrai code de
/// Bruno (`scripts/bruno-new-items.js --url-params`) est reproduit à
/// l'identique, ou refusé quand l'issue de `new URL` n'est pas certaine.
#[test]
fn url_params_match_bruno_or_are_refused() {
    use bruno_tui::writer::url_params::bruno_url_params;
    // Cas dont l'issue de `new URL` n'est pas reproduite : refus.
    const REFUSED: [&str; 5] = [
        "http://user@h/:a",
        "http://h/./:a",
        "http://h/a\\:b",
        "http://127.1/:a",
        "http://exämple.com/:a",
    ];
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/url-params/bruno-url-params.json");
    let table: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).expect("table")).expect("JSON");
    let cases = table.as_array().expect("tableau");
    assert!(cases.len() >= 15);
    for case in cases {
        let url = case["url"].as_str().expect("url");
        let result = bruno_url_params(url);
        if REFUSED.contains(&url) {
            assert!(result.is_err(), "{url} : refus attendu");
            continue;
        }
        let params = result.unwrap_or_else(|error| panic!("{url} : {error}"));
        let query: Vec<(String, Option<String>)> = params
            .query
            .into_iter()
            .map(|p| (p.name, p.value))
            .collect();
        let expected_query: Vec<(String, Option<String>)> = case["query"]
            .as_array()
            .expect("query")
            .iter()
            .map(|p| {
                (
                    p["name"].as_str().expect("name").to_owned(),
                    p["value"].as_str().map(str::to_owned),
                )
            })
            .collect();
        assert_eq!(query, expected_query, "{url}");
        let expected_path: Vec<String> = case["path"]
            .as_array()
            .expect("path")
            .iter()
            .map(|p| p.as_str().expect("nom").to_owned())
            .collect();
        assert_eq!(params.path, expected_path, "{url}");
    }
}

#[test]
fn created_request_carries_bruno_params() {
    let (root, _) = workspace("create-params");
    let request = NewRequest {
        url: "{{base_url}}/posts/:id/comments?full=1&flag&a=1=2&full=2#haut".into(),
        ..http("Commentaires", "comments")
    };
    files::create_request(&root, Path::new(""), &request).expect("création");
    let written = read(&root, "comments.bru");
    assert!(written.contains("params:query {\n  full: 1\n  flag: \n  a: 1=2\n  full: 2\n}\n"));
    assert!(written.contains("params:path {\n  id: \n}\n"));
    let view = bruno_tui::collection::RequestView::from_ast(
        &bruno_tui::collection::BruFile::parse(written).expect("AST"),
    )
    .expect("vue");
    assert_eq!(view.query_params.len(), 4);
    assert_eq!(view.path_params[0].key, "id");
}

/// La corbeille est faite même si la renumérotation échoue ou est
/// impossible ; le statut le dit.
#[test]
fn trash_reports_a_failed_or_refused_renumbering() {
    let (root, data) = workspace("trash-renumber");
    let trash = || Ok(Trash::at(data.join("Trash")));
    let outcome = files::execute(
        &root,
        &FileOp::Trash {
            path: "get-post.bru".into(),
            renumber: Ok(vec![SeqChange {
                path: "list-posts.bru".into(),
                is_folder: false,
                seq: 1,
                expected: Some("source chargée périmée".into()),
            }]),
        },
        trash,
        "2026-10-14T00:00:00",
    )
    .expect("corbeille faite");
    assert!(!root.join("get-post.bru").exists());
    assert!(
        outcome
            .message
            .starts_with("Mis à la corbeille : get-post.bru ; renumérotation des seq en échec"),
        "{}",
        outcome.message
    );
    assert_eq!(read(&root, "list-posts.bru"), original("list-posts.bru"));

    let outcome = files::execute(
        &root,
        &FileOp::Trash {
            path: "list-posts.bru".into(),
            renumber: Err("raison".into()),
        },
        || Ok(Trash::at(data.join("Trash"))),
        "2026-10-14T00:00:00",
    )
    .expect("corbeille faite");
    assert_eq!(
        outcome.message,
        "Mis à la corbeille : list-posts.bru ; seq non renumérotés : raison"
    );
}

/// Source chargé d'un fichier de la fixture, pour la vérification de
/// fraîcheur d'un déplacement.
fn loaded(path: &str) -> (PathBuf, String) {
    (PathBuf::from(path), original(path))
}

#[test]
fn move_request_keeps_the_bytes_and_suffixes_collisions() {
    let (root, _) = workspace("move-request");
    let moved = files::move_item(
        &root,
        Path::new("get-post.bru"),
        Path::new("Posts"),
        &[loaded("get-post.bru")],
    )
    .expect("déplacement");
    assert_eq!(moved, PathBuf::from("Posts/get-post.bru"));
    assert!(!root.join("get-post.bru").exists());
    // Octets identiques, `seq` compris (décision 3).
    assert_eq!(read(&root, "Posts/get-post.bru"), original("get-post.bru"));

    // Même nom dans la cible : suffixes ` 1`, ` 2`, comme Bruno.
    for expected in ["Posts/get-post 1.bru", "Posts/get-post 2.bru"] {
        fs::copy(root.join("Posts/get-post.bru"), root.join("list-posts.bru")).expect("copie");
        fs::rename(root.join("list-posts.bru"), root.join("get-post.bru")).expect("nom");
        let moved = files::move_item(&root, Path::new("get-post.bru"), Path::new("Posts"), &[])
            .expect("collision");
        assert_eq!(moved, PathBuf::from(expected));
        assert_eq!(read(&root, expected), original("get-post.bru"));
    }
    assert_eq!(
        names_in(&root.join("Posts")),
        [
            "create.bru",
            "folder.bru",
            "get-post 1.bru",
            "get-post 2.bru",
            "get-post.bru"
        ]
    );

    // Vers la racine : un dossier vers sa racine.
    let back = files::move_item(&root, Path::new("Posts/create.bru"), Path::new(""), &[])
        .expect("vers la racine");
    assert_eq!(back, PathBuf::from("create.bru"));
    assert_eq!(read(&root, "create.bru"), original("Posts/create.bru"));
    assert!(names_in(&root).iter().all(|name| !name.starts_with('.')));
}

#[test]
fn move_folder_keeps_its_content_and_suffixes_the_directory() {
    let (root, _) = workspace("move-folder");
    fs::create_dir_all(root.join("Posts/Sub/Deep")).expect("sous-dossiers");
    fs::write(root.join("Posts/Sub/y.bru"), original("get-post.bru")).expect("y");
    fs::write(
        root.join("Posts/Sub/Deep/z.bru"),
        original("list-posts.bru"),
    )
    .expect("z");
    fs::write(root.join("Posts/notes.txt"), "pas un .bru").expect("notes");

    let moved = files::move_item(
        &root,
        Path::new("Posts"),
        Path::new("NoMeta"),
        &[loaded("Posts/folder.bru"), loaded("Posts/create.bru")],
    )
    .expect("déplacement");
    assert_eq!(moved, PathBuf::from("NoMeta/Posts"));
    assert!(!root.join("Posts").exists());
    assert_eq!(
        read(&root, "NoMeta/Posts/folder.bru"),
        original("Posts/folder.bru")
    );
    assert_eq!(
        read(&root, "NoMeta/Posts/create.bru"),
        original("Posts/create.bru")
    );
    assert_eq!(
        read(&root, "NoMeta/Posts/Sub/y.bru"),
        original("get-post.bru")
    );
    assert_eq!(
        read(&root, "NoMeta/Posts/Sub/Deep/z.bru"),
        original("list-posts.bru")
    );
    assert_eq!(read(&root, "NoMeta/Posts/notes.txt"), "pas un .bru");

    // Le suffixe va sur le nom du répertoire ; `meta.name` ne change pas.
    fs::create_dir(root.join("Posts")).expect("nouveau Posts");
    let again =
        files::move_item(&root, Path::new("Posts"), Path::new("NoMeta"), &[]).expect("collision");
    assert_eq!(again, PathBuf::from("NoMeta/Posts 1"));
    fs::create_dir(root.join("Posts")).expect("encore");
    let third =
        files::move_item(&root, Path::new("Posts"), Path::new("NoMeta"), &[]).expect("collision");
    assert_eq!(third, PathBuf::from("NoMeta/Posts 2"));
    assert_eq!(
        names_in(&root.join("NoMeta")),
        ["Posts", "Posts 1", "Posts 2", "x.bru"]
    );
}

#[test]
fn move_refusals_leave_the_disk_untouched() {
    let (root, _) = workspace("move-refusals");
    fs::create_dir_all(root.join("Posts/Sub")).expect("sous-dossier");
    let before = names_in(&root);
    let to =
        |path: &str, target: &str| files::move_item(&root, Path::new(path), Path::new(target), &[]);

    // Dans lui-même ou un descendant.
    assert!(matches!(
        to("Posts", "Posts"),
        Err(FileOpError::IntoItself { .. })
    ));
    assert!(matches!(
        to("Posts", "Posts/Sub"),
        Err(FileOpError::IntoItself { .. })
    ));
    // Même dossier (racine ou sous-dossier).
    assert!(matches!(
        to("get-post.bru", ""),
        Err(FileOpError::SameDirectory { .. })
    ));
    assert!(matches!(
        to("Posts/create.bru", "Posts"),
        Err(FileOpError::SameDirectory { .. })
    ));
    assert!(matches!(
        to("Posts", ""),
        Err(FileOpError::SameDirectory { .. })
    ));
    // Fichiers qui ne se déplacent jamais seuls.
    for path in ["bruno.json", "Posts/folder.bru"] {
        assert!(
            matches!(to(path, "NoMeta"), Err(FileOpError::NotMovable { .. })),
            "{path}"
        );
    }
    // La racine elle-même, sortie de la collection, cible absente.
    assert!(matches!(to("", "NoMeta"), Err(FileOpError::Outside { .. })));
    assert!(matches!(
        to("../escape.bru", "NoMeta"),
        Err(FileOpError::Outside { .. })
    ));
    assert!(matches!(
        to("get-post.bru", "../elsewhere"),
        Err(FileOpError::Outside { .. })
    ));
    assert!(matches!(
        to("get-post.bru", "Absent"),
        Err(FileOpError::NotFound { .. })
    ));
    assert!(matches!(
        to("absent.bru", "Posts"),
        Err(FileOpError::NotFound { .. })
    ));
    // Cible qui est un fichier.
    assert!(matches!(
        to("get-post.bru", "list-posts.bru"),
        Err(FileOpError::NotFound { .. })
    ));
    assert_eq!(names_in(&root), before);
    assert_eq!(read(&root, "get-post.bru"), original("get-post.bru"));
}

#[test]
fn move_refuses_stale_files() {
    let (root, _) = workspace("move-stale");
    fs::write(root.join("Posts/create.bru"), "changé par un tiers").expect("tiers");
    let result = files::move_item(
        &root,
        Path::new("Posts"),
        Path::new("NoMeta"),
        &[loaded("Posts/folder.bru"), loaded("Posts/create.bru")],
    );
    assert!(
        matches!(result, Err(FileOpError::Stale { .. })),
        "{result:?}"
    );
    assert!(root.join("Posts/folder.bru").exists());
    assert!(!root.join("NoMeta/Posts").exists());
    assert_eq!(read(&root, "Posts/create.bru"), "changé par un tiers");
}

#[cfg(unix)]
#[test]
fn move_refuses_symlinks_in_the_item_or_the_target() {
    use std::os::unix::fs::symlink;
    let (root, base) = {
        let (root, data) = workspace("move-symlink");
        let base = data.parent().expect("base").to_path_buf();
        (root, base)
    };
    fs::write(base.join("outside.bru"), "meta {\n  name: Dehors\n}\n").expect("dehors");
    // Lien dans un dossier à déplacer.
    symlink(base.join("outside.bru"), root.join("Posts/lien.bru")).expect("lien");
    assert!(matches!(
        files::move_item(&root, Path::new("Posts"), Path::new("NoMeta"), &[]),
        Err(FileOpError::SymlinkMove { .. })
    ));
    // Requête qui est un lien : vers l'extérieur, `inside` le refuse déjà ;
    // vers l'intérieur, c'est le refus des liens.
    symlink(base.join("outside.bru"), root.join("lien.bru")).expect("lien");
    assert!(matches!(
        files::move_item(&root, Path::new("lien.bru"), Path::new("NoMeta"), &[]),
        Err(FileOpError::Outside { .. })
    ));
    symlink(root.join("get-post.bru"), root.join("lien-interne.bru")).expect("lien");
    assert!(matches!(
        files::move_item(
            &root,
            Path::new("lien-interne.bru"),
            Path::new("NoMeta"),
            &[]
        ),
        Err(FileOpError::SymlinkMove { .. })
    ));
    // Dossier cible qui est un lien, vers l'intérieur ou l'extérieur.
    symlink(root.join("NoMeta"), root.join("LienInterne")).expect("lien");
    symlink(&base, root.join("LienExterne")).expect("lien");
    assert!(matches!(
        files::move_item(
            &root,
            Path::new("get-post.bru"),
            Path::new("LienInterne"),
            &[]
        ),
        Err(FileOpError::SymlinkMove { .. })
    ));
    assert!(matches!(
        files::move_item(
            &root,
            Path::new("get-post.bru"),
            Path::new("LienExterne"),
            &[]
        ),
        Err(FileOpError::Outside { .. })
    ));
    assert!(root.join("get-post.bru").exists());
    assert!(root.join("Posts/create.bru").exists());
    assert!(base.join("outside.bru").exists());
    assert!(!root.join("NoMeta/get-post.bru").exists());
}

#[test]
fn execute_move_reports_the_final_name_and_renumbers() {
    let (root, data) = workspace("move-execute");
    fs::write(root.join("Posts/get-post.bru"), "déjà là").expect("collision");
    let renumber = |expected: &str| {
        Ok(vec![SeqChange {
            path: "list-posts.bru".into(),
            is_folder: false,
            seq: 1,
            expected: Some(expected.into()),
        }])
    };
    let op = |renumber| FileOp::Move {
        path: "get-post.bru".into(),
        target_dir: "Posts".into(),
        expected: vec![loaded("get-post.bru")],
        renumber,
    };
    let outcome = files::execute(
        &root,
        &op(renumber(&original("list-posts.bru"))),
        || Ok(Trash::at(data.join("Trash"))),
        "2026-10-14T00:00:00",
    )
    .expect("déplacement");
    assert_eq!(outcome.select, Some(PathBuf::from("Posts/get-post 1.bru")));
    assert_eq!(outcome.message, "Déplacé vers Posts : get-post 1.bru");
    assert_eq!(
        read(&root, "Posts/get-post 1.bru"),
        original("get-post.bru")
    );
    assert_eq!(read(&root, "Posts/get-post.bru"), "déjà là");
    // Le `seq` de la source est renuméroté (list-posts : 2 → 1).
    assert!(read(&root, "list-posts.bru").contains("seq: 1"));

    // Renumérotation en échec ou impossible : le déplacement est fait, le
    // statut le dit.
    let outcome = files::execute(
        &root,
        &FileOp::Move {
            path: "list-posts.bru".into(),
            target_dir: "NoMeta".into(),
            expected: vec![],
            renumber: renumber("source périmée"),
        },
        || Ok(Trash::at(data.join("Trash"))),
        "2026-10-14T00:00:00",
    )
    .expect("déplacement fait");
    assert!(root.join("NoMeta/list-posts.bru").exists());
    assert!(
        outcome
            .message
            .starts_with("Déplacé vers NoMeta : list-posts.bru ; renumérotation des seq en échec"),
        "{}",
        outcome.message
    );
    let outcome = files::execute(
        &root,
        &FileOp::Move {
            path: "NoMeta/list-posts.bru".into(),
            target_dir: "".into(),
            expected: vec![],
            renumber: Err("raison".into()),
        },
        || Ok(Trash::at(data.join("Trash"))),
        "2026-10-14T00:00:00",
    )
    .expect("déplacement fait");
    assert_eq!(
        outcome.message,
        "Déplacé vers la racine de la collection : list-posts.bru ; seq non renumérotés : raison"
    );
    assert!(
        !data.join("Trash").exists(),
        "rien n'est mis à la corbeille"
    );
}

#[test]
fn create_environment_creates_exact_content_and_refuses_collision_and_invalid_name() {
    let (root, _) = workspace("create-env");
    let outcome = files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "production".into(),
        },
        || unreachable!(),
        "",
    )
    .expect("création");
    assert_eq!(
        outcome.select,
        Some(PathBuf::from("environments/production.bru"))
    );
    assert_eq!(outcome.message, "Environnement créé : production.bru");

    // Exactement 9 octets : vars {\n}\n
    let content = fs::read(root.join("environments/production.bru")).expect("lecture");
    assert_eq!(content, b"vars {\n}\n");

    // Collision insensible à la casse et sans espaces
    let err = files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "PRODUCTION".into(),
        },
        || unreachable!(),
        "",
    )
    .expect_err("collision");
    assert!(matches!(err, FileOpError::EnvironmentAlreadyExists));

    let err2 = files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "Production".into(),
        },
        || unreachable!(),
        "",
    )
    .expect_err("collision");
    assert!(matches!(err2, FileOpError::EnvironmentAlreadyExists));

    // Nom invalide
    let err_name = files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "invalid/name".into(),
        },
        || unreachable!(),
        "",
    )
    .expect_err("nom invalide");
    assert!(matches!(err_name, FileOpError::Name(_)));
}

#[test]
fn rename_environment_updates_extends_cascade_and_preserves_other_files() {
    let (root, _) = workspace("rename-env");
    // Créer base.bru
    files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "base".into(),
        },
        || unreachable!(),
        "",
    )
    .expect("créer base");
    let base_content = "vars {\n}\n";

    // Créer un enfant avec extends: base
    let child_content =
        "extends: base\n\nvars {\n  url: http://localhost:3000\n}\n\ncolor: #ff0000\n";
    fs::write(root.join("environments/child.bru"), child_content).expect("write child");

    // Créer un enfant avec extends: BASE (casse différente : ne doit pas être touché)
    let different_case_content = "extends: BASE\n\nvars {\n}\n";
    fs::write(
        root.join("environments/diff-case.bru"),
        different_case_content,
    )
    .expect("write diff");

    // Créer un enfant avec extends list (ne doit pas être touché)
    let list_content = "extends [\n  base\n]\n\nvars {\n}\n";
    fs::write(root.join("environments/list.bru"), list_content).expect("write list");

    // Créer un fichier malformé pour vérifier la résilience
    fs::write(
        root.join("environments/broken.bru"),
        "ceci n'est pas du bru valide {{{",
    )
    .expect("write broken");

    // Renommer base -> root-env
    let outcome = files::execute(
        &root,
        &FileOp::RenameEnvironment {
            old_name: "base".into(),
            new_name: "root-env".into(),
            expected: base_content.into(),
            siblings: vec![
                ("child".into(), child_content.into()),
                ("diff-case".into(), different_case_content.into()),
                ("list".into(), list_content.into()),
            ],
        },
        || unreachable!(),
        "",
    )
    .expect("renommer base");
    assert_eq!(
        outcome.select,
        Some(PathBuf::from("environments/root-env.bru"))
    );
    assert_eq!(outcome.message, "Renommé : root-env.bru");
    assert!(!root.join("environments/base.bru").exists());
    assert!(root.join("environments/root-env.bru").exists());

    // Vérifier la réécriture chirurgicale de child.bru : seul extends: root-env a changé
    let expected_child =
        "extends: root-env\n\nvars {\n  url: http://localhost:3000\n}\n\ncolor: #ff0000\n";
    assert_eq!(
        fs::read_to_string(root.join("environments/child.bru")).expect("read"),
        expected_child
    );

    // Les autres fichiers n'ont pas bougé
    assert_eq!(
        fs::read_to_string(root.join("environments/diff-case.bru")).expect("read"),
        different_case_content
    );
    assert_eq!(
        fs::read_to_string(root.join("environments/list.bru")).expect("read"),
        list_content
    );
    assert_eq!(
        fs::read_to_string(root.join("environments/broken.bru")).expect("read"),
        "ceci n'est pas du bru valide {{{"
    );

    // Collision au renommage
    let err = files::execute(
        &root,
        &FileOp::RenameEnvironment {
            old_name: "root-env".into(),
            new_name: "child".into(),
            expected: base_content.into(),
            siblings: vec![],
        },
        || unreachable!(),
        "",
    )
    .expect_err("collision");
    assert!(matches!(err, FileOpError::EnvironmentAlreadyExists));
}

#[test]
fn clone_and_trash_environment_behave_faithfully() {
    let (root, data) = workspace("clone-trash-env");
    let source_content = "extends: parent\n\nvars {\n  token: secret123\n}\n\nvars:secret [\n  token\n]\n\ncolor: #123456\n";
    fs::create_dir_all(root.join("environments")).expect("create envs dir");
    fs::write(root.join("environments/source.bru"), source_content).expect("write source");

    // Clone à l'octet près
    let outcome = files::execute(
        &root,
        &FileOp::CloneEnvironment {
            source_name: "source".into(),
            target_name: "source clone".into(),
            expected: source_content.into(),
        },
        || unreachable!(),
        "",
    )
    .expect("clonage");
    assert_eq!(
        outcome.select,
        Some(PathBuf::from("environments/source clone.bru"))
    );
    assert_eq!(outcome.message, "Clone créé : source clone.bru");
    assert_eq!(
        fs::read_to_string(root.join("environments/source clone.bru")).expect("read"),
        source_content
    );

    // Collision au clonage
    let err = files::execute(
        &root,
        &FileOp::CloneEnvironment {
            source_name: "source".into(),
            target_name: "source".into(),
            expected: source_content.into(),
        },
        || unreachable!(),
        "",
    )
    .expect_err("collision");
    assert!(matches!(err, FileOpError::EnvironmentAlreadyExists));

    // Trash de source clone
    let outcome = files::execute(
        &root,
        &FileOp::TrashEnvironment {
            name: "source clone".into(),
            expected: Some(source_content.into()),
        },
        || Ok(Trash::at(data.join("Trash"))),
        "2026-10-14T00:00:00",
    )
    .expect("corbeille");
    assert_eq!(outcome.select, None);
    assert_eq!(outcome.message, "Mis à la corbeille : source clone.bru");
    assert!(!root.join("environments/source clone.bru").exists());
    assert!(root.join("environments/source.bru").exists());
}

#[test]
fn environment_operations_refuse_a_file_changed_on_disk() {
    let (root, data) = workspace("env-stale");
    fs::create_dir_all(root.join("environments")).expect("dossier");
    let loaded = "vars {\n  a: 1\n}\n";
    let path = root.join("environments/dev.bru");
    fs::write(&path, "vars {\n  a: 2\n}\n").expect("modifié depuis le chargement");

    for op in [
        FileOp::RenameEnvironment {
            old_name: "dev".into(),
            new_name: "prod".into(),
            expected: loaded.into(),
            siblings: vec![],
        },
        FileOp::CloneEnvironment {
            source_name: "dev".into(),
            target_name: "dev copie".into(),
            expected: loaded.into(),
        },
        FileOp::TrashEnvironment {
            name: "dev".into(),
            expected: Some(loaded.into()),
        },
    ] {
        let err = files::execute(
            &root,
            &op,
            || Ok(Trash::at(data.join("Trash"))),
            "2026-10-14T00:00:00",
        )
        .expect_err("fichier périmé");
        assert!(matches!(err, FileOpError::Stale { .. }), "{op:?}");
    }
    assert_eq!(
        fs::read_to_string(&path).expect("lecture"),
        "vars {\n  a: 2\n}\n"
    );
    assert!(!root.join("environments/prod.bru").exists());
    assert!(!root.join("environments/dev copie.bru").exists());
    assert!(!data.join("Trash").exists());
}

#[test]
fn rename_environment_reports_siblings_that_could_not_be_updated() {
    let (root, _) = workspace("env-cascade-errors");
    fs::create_dir_all(root.join("environments")).expect("dossier");
    let base = "vars {\n}\n";
    fs::write(root.join("environments/base.bru"), base).expect("base");
    let fresh = "extends: base\n\nvars {\n}\n";
    fs::write(root.join("environments/fresh.bru"), fresh).expect("fresh");
    let stale_loaded = "extends: base\n\nvars {\n}\n";
    let stale_disk = "extends: base\n\nvars {\n  x: 1\n}\n";
    fs::write(root.join("environments/stale.bru"), stale_disk).expect("stale");
    fs::write(root.join("environments/newcomer.bru"), fresh).expect("apparu");

    let outcome = files::execute(
        &root,
        &FileOp::RenameEnvironment {
            old_name: "base".into(),
            new_name: "root".into(),
            expected: base.into(),
            siblings: vec![
                ("fresh".into(), fresh.into()),
                ("stale".into(), stale_loaded.into()),
            ],
        },
        || unreachable!(),
        "",
    )
    .expect("le renommage reste fait");

    assert!(root.join("environments/root.bru").exists());
    assert!(!root.join("environments/base.bru").exists());
    assert_eq!(
        fs::read_to_string(root.join("environments/fresh.bru")).expect("lecture"),
        "extends: root\n\nvars {\n}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("environments/stale.bru")).expect("lecture"),
        stale_disk
    );
    assert_eq!(
        fs::read_to_string(root.join("environments/newcomer.bru")).expect("lecture"),
        fresh
    );
    assert!(
        outcome
            .message
            .starts_with("Renommé ; extends non mis à jour dans : ")
    );
    assert!(outcome.message.contains("stale.bru (modifié sur le disque"));
    assert!(outcome.message.contains("newcomer.bru (fichier apparu"));
    assert!(!outcome.message.contains("fresh.bru"));
}

#[test]
fn environment_names_compare_like_bruno_with_unicode_lowercase() {
    let (root, _) = workspace("env-unicode");
    files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "Été".into()
        },
        || unreachable!(),
        "",
    )
    .expect("création");
    for name in ["été", " été", "ÉTÉ"] {
        let err = files::execute(
            &root,
            &FileOp::CreateEnvironment { name: name.into() },
            || unreachable!(),
            "",
        );
        // Les noms avec espace de bord sont invalides ; les autres sont pris.
        assert!(err.is_err(), "{name:?} doit être refusé");
    }
    let err = files::execute(
        &root,
        &FileOp::CreateEnvironment {
            name: "été".into()
        },
        || unreachable!(),
        "",
    )
    .expect_err("pris");
    assert!(matches!(err, FileOpError::EnvironmentAlreadyExists));
    assert!(!root.join("environments/été.bru").exists());
}

#[test]
fn clone_environment_never_suffixes_a_taken_name() {
    let (root, _) = workspace("env-clone-no-suffix");
    fs::create_dir_all(root.join("environments")).expect("dossier");
    let source = "vars {\n}\n";
    fs::write(root.join("environments/dev.bru"), source).expect("dev");
    fs::write(
        root.join("environments/dev copie.bru"),
        "vars {\n  k: v\n}\n",
    )
    .expect("copie");
    let err = files::execute(
        &root,
        &FileOp::CloneEnvironment {
            source_name: "dev".into(),
            target_name: "dev copie".into(),
            expected: source.into(),
        },
        || unreachable!(),
        "",
    )
    .expect_err("nom pris");
    assert!(matches!(err, FileOpError::EnvironmentAlreadyExists));
    assert!(!root.join("environments/dev copie 2.bru").exists());
    assert!(!root.join("environments/dev copie 1.bru").exists());
    assert_eq!(
        fs::read_to_string(root.join("environments/dev copie.bru")).expect("lecture"),
        "vars {\n  k: v\n}\n"
    );
}
