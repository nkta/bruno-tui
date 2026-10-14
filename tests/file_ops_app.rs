//! Opérations de fichiers depuis l'arbre (`docs/edition-phase3.md`) :
//! touches réelles (`to_message`), popups, `Ctrl+S`, puis exécution de
//! `Command::FileOp` comme la boucle (corbeille dans un `XDG_DATA_HOME`
//! temporaire) et rechargement. La sélection doit suivre l'élément.

use std::fs;
use std::path::{Path, PathBuf};

use bruno_tui::app::event::AppEvent;
use bruno_tui::app::message::{Message, to_message};
use bruno_tui::app::model::{EnvSeries, Model, PendingConfirm, StatusMessage};
use bruno_tui::app::update::{Command, refresh_rows, update};
use bruno_tui::app::view;
use bruno_tui::app::workspace::{active_member, install};
use bruno_tui::collection::{BruLoader, CollectionLoader, workspace::from_paths};
use bruno_tui::writer::files::{self, NewRequest, RequestKind, new_request_content};
use bruno_tui::writer::trash::Trash;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/collections/writer-cases/files/collection")
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

struct Ui {
    model: Model,
    root: PathBuf,
    /// Racines des membres, en mode workspace.
    members: Vec<PathBuf>,
    data: PathBuf,
}

impl Ui {
    fn base(tag: &str) -> PathBuf {
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("tmp-file-ops-app-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        base
    }

    fn new(tag: &str) -> Self {
        let base = Self::base(tag);
        let root = base.join("collection");
        copy_dir(&fixture(), &root);
        let mut model = Model::new(root.clone(), (120, 40));
        update(&mut model, Message::CollectionLoaded(BruLoader.load(&root)));
        Self {
            model,
            root,
            members: Vec::new(),
            data: base.join("data"),
        }
    }

    /// Workspace de deux copies de la mini-collection ; la première est
    /// active.
    fn workspace(tag: &str) -> Self {
        let base = Self::base(tag);
        let first = base.join("un");
        let second = base.join("deux");
        copy_dir(&fixture(), &first);
        copy_dir(&fixture(), &second);
        let mut model = Model::new(first.clone(), (120, 40));
        install(&mut model, from_paths(&[first.clone(), second.clone()]));
        for (member, root) in [(1, &second), (0, &first)] {
            update(
                &mut model,
                Message::MemberLoaded {
                    member,
                    result: BruLoader.load(root),
                },
            );
        }
        let mut ui = Self {
            model,
            root: first.clone(),
            members: vec![first, second],
            data: base.join("data"),
        };
        // En-tête de la collection active sélectionné : on le déplie.
        ui.press(KeyCode::Right);
        ui
    }

    /// Exécute une commande comme la boucle : opération de fichier, puis
    /// rechargement.
    fn run(&mut self, command: Command) {
        match command {
            Command::FileOp { root, op } => {
                let trash = Trash::at(self.data.join("Trash"));
                let result = files::execute(&root, &op, || Ok(trash), "2026-10-14T12:00:00");
                let next = update(&mut self.model, Message::FileOpDone { root, result });
                self.run(next);
            }
            Command::ReloadCollection if !self.members.is_empty() => {
                let member = active_member(&self.model).expect("membre actif");
                let result = BruLoader.load(&self.members[member]);
                let next = update(&mut self.model, Message::MemberLoaded { member, result });
                self.run(next);
            }
            Command::ReloadCollection => {
                let result = BruLoader.load(&self.root);
                let next = update(&mut self.model, Message::CollectionLoaded(result));
                self.run(next);
            }
            Command::ReloadMember(member) => {
                let result = BruLoader.load(&self.members[member]);
                let next = update(&mut self.model, Message::MemberLoaded { member, result });
                self.run(next);
            }
            Command::Batch(commands) => commands.into_iter().for_each(|c| self.run(c)),
            _ => {}
        }
    }

    /// Recharge la collection, comme le watcher après une écriture externe.
    fn reload(&mut self) {
        self.run(Command::ReloadCollection);
    }

    fn write(&self, path: &str, text: &str) {
        fs::write(self.root.join(path), text).expect("écriture");
    }

    fn press_with(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let event = AppEvent::Terminal(Event::Key(KeyEvent::new(code, modifiers)));
        if let Some(message) = to_message(event, self.model.text_capture()) {
            let command = update(&mut self.model, message);
            self.run(command);
        }
    }

    fn press(&mut self, code: KeyCode) {
        self.press_with(code, KeyModifiers::NONE);
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.press(KeyCode::Char(c));
        }
    }

    fn save(&mut self) {
        self.press_with(KeyCode::Char('s'), KeyModifiers::CONTROL);
    }

    /// Efface le champ texte sous le curseur.
    fn clear_field(&mut self) {
        self.press(KeyCode::End);
        for _ in 0..80 {
            self.press(KeyCode::Backspace);
        }
    }

    fn select(&mut self, path: &str) {
        let target = Path::new(path);
        for ancestor in target.ancestors().skip(1) {
            if !ancestor.as_os_str().is_empty() {
                self.model.tree.expanded.insert(ancestor.to_path_buf());
            }
        }
        refresh_rows(&mut self.model);
        let index = self
            .model
            .active_row_position(|node| node.path() == target)
            .expect("ligne");
        self.model.tree.selected = index;
        self.model.selected_collection = false;
    }

    fn selected_path(&self) -> Option<PathBuf> {
        if self.model.selected_collection {
            return None;
        }
        self.model
            .selected_node()
            .map(|node| node.path().to_path_buf())
    }

    fn read(&self, path: &str) -> String {
        fs::read_to_string(self.root.join(path)).expect("lecture")
    }

    fn status(&self) -> Option<StatusMessage> {
        self.model.last_status.clone()
    }

    fn screen(&self, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| view::view(&self.model, frame))
            .expect("rendu");
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
}

#[test]
fn new_request_in_a_folder_is_created_and_selected() {
    let mut ui = Ui::new("new-request");
    ui.select("Posts");
    ui.press(KeyCode::Char('a'));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.title(), " Nouvelle requête — dans Articles ");
    ui.type_text("Lire un article");
    ui.press(KeyCode::Tab);
    ui.press(KeyCode::Tab);
    ui.press(KeyCode::Tab);
    ui.press(KeyCode::Right);
    ui.press(KeyCode::Tab);
    ui.type_text("{{base_url}}/posts/:id?full=1");
    // Entrée sur la dernière ligne crée.
    ui.press(KeyCode::Enter);

    assert!(ui.model.file_popup.is_none());
    assert_eq!(
        ui.read("Posts/Lire un article.bru"),
        new_request_content(&NewRequest {
            name: "Lire un article".into(),
            filename: "Lire un article".into(),
            kind: RequestKind::Http,
            method: "post".into(),
            url: "{{base_url}}/posts/:id?full=1".into(),
            seq: 2,
        })
        .expect("contenu")
    );
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Posts/Lire un article.bru"))
    );
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpDone(
            "Requête créée : Lire un article.bru".into()
        ))
    );
}

#[test]
fn new_request_next_to_a_request_uses_its_folder_and_bruno_seq() {
    let mut ui = Ui::new("new-request-root");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('a'));
    assert_eq!(ui.model.file_popup.as_ref().expect("popup").seq, 5);
    ui.type_text("Lire un article");
    ui.save();
    // Collision avec rien : `Lire un article.bru` n'existe pas à la racine.
    assert!(ui.read("Lire un article.bru").contains("  seq: 5\n"));
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('a'));
    ui.type_text("Lire un article");
    ui.save();
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Lire un article 1.bru"))
    );
}

#[test]
fn invalid_names_keep_the_popup_open_with_bruno_messages() {
    let mut ui = Ui::new("invalid");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('a'));
    ui.save();
    let popup = ui.model.file_popup.as_ref().expect("popup ouvert");
    assert_eq!(popup.error.as_deref(), Some("Le nom est obligatoire."));
    ui.type_text("a");
    ui.press(KeyCode::Tab);
    ui.type_text("?");
    ui.save();
    let popup = ui.model.file_popup.as_ref().expect("popup ouvert");
    assert_eq!(
        popup.error.as_deref(),
        Some("Caractère spécial interdit dans le nom : « ? ».")
    );
    let screen = ui.screen(80, 24);
    assert!(screen.contains("Caractère spécial interdit"), "{screen}");
    ui.press(KeyCode::Esc);
    assert!(ui.model.file_popup.is_none());
    assert!(!ui.root.join("a.bru").exists());
}

#[test]
fn new_folder_on_the_collection_header_goes_to_the_root() {
    let mut ui = Ui::new("new-folder");
    ui.press(KeyCode::Char('c'));
    assert!(ui.model.selected_collection);
    ui.press(KeyCode::Char('A'));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.title(), " Nouveau dossier — dans file-ops ");
    ui.type_text("Utilisateurs");
    ui.save();
    assert_eq!(
        ui.read("Utilisateurs/folder.bru"),
        files::new_folder_content("Utilisateurs", 5)
    );
    assert_eq!(ui.selected_path(), Some(PathBuf::from("Utilisateurs")));
}

#[test]
fn rename_request_and_file_then_selection_follows() {
    let mut ui = Ui::new("rename");
    ui.select("get-post.bru");
    ui.press(KeyCode::F(2));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.title(), " Renommer — get-post.bru ");
    assert_eq!(popup.name.text(), "Lire un article");
    assert_eq!(popup.filename.text(), "get-post");
    assert!(popup.rename_file);
    ui.clear_field();
    ui.type_text("Lire un billet");
    ui.save();
    assert!(!ui.root.join("get-post.bru").exists());
    assert_eq!(
        ui.read("Lire un billet.bru"),
        fs::read_to_string(fixture().join("get-post.bru"))
            .expect("fixture")
            .replace("name: Lire un article", "name: Lire un billet")
    );
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Lire un billet.bru"))
    );
}

#[test]
fn rename_with_the_box_unchecked_keeps_the_file_name() {
    let mut ui = Ui::new("rename-name-only");
    ui.select("Posts");
    ui.press(KeyCode::F(2));
    ui.clear_field();
    ui.type_text("Billets");
    ui.press(KeyCode::Down);
    ui.press(KeyCode::Down);
    ui.press(KeyCode::Char(' '));
    assert!(!ui.model.file_popup.as_ref().expect("popup").rename_file);
    let screen = ui.screen(80, 24);
    assert!(screen.contains("[ ] Renommer aussi le dossier"), "{screen}");
    ui.press(KeyCode::Enter);
    assert_eq!(
        ui.read("Posts/folder.bru"),
        fs::read_to_string(fixture().join("Posts/folder.bru"))
            .expect("fixture")
            .replace("name: Articles", "name: Billets")
    );
    assert_eq!(ui.selected_path(), Some(PathBuf::from("Posts")));
}

#[test]
fn clone_request_and_folder() {
    let mut ui = Ui::new("clone");
    ui.select("list-posts.bru");
    ui.press(KeyCode::Char('p'));
    let popup = ui.model.file_popup.as_ref().expect("popup");
    assert_eq!(popup.name.text(), "Lister les articles copie");
    // Requêtes du répertoire : get-post et list-posts.
    assert_eq!(popup.seq, 3);
    ui.save();
    assert_eq!(
        ui.read("Lister les articles copie.bru"),
        fs::read_to_string(fixture().join("list-posts.bru"))
            .expect("fixture")
            .replace(
                "name: Lister les articles",
                "name: Lister les articles copie"
            )
            .replace("seq: 2", "seq: 3")
    );
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Lister les articles copie.bru"))
    );

    ui.select("Posts");
    ui.press(KeyCode::Char('p'));
    // Éléments du répertoire : 4 d'origine et le clone.
    assert_eq!(ui.model.file_popup.as_ref().expect("popup").seq, 6);
    ui.save();
    assert_eq!(
        ui.read("Articles copie/create.bru"),
        fs::read_to_string(fixture().join("Posts/create.bru")).expect("fixture")
    );
    assert_eq!(ui.selected_path(), Some(PathBuf::from("Articles copie")));
}

#[test]
fn shift_arrows_reorder_siblings_and_selection_follows() {
    let mut ui = Ui::new("reorder");
    ui.select("get-post.bru");
    ui.press_with(KeyCode::Down, KeyModifiers::SHIFT);
    assert!(ui.read("get-post.bru").contains("  seq: 2\n"));
    assert!(ui.read("list-posts.bru").contains("  seq: 1\n"));
    assert_eq!(ui.selected_path(), Some(PathBuf::from("get-post.bru")));
    assert_eq!(ui.model.tree.selected, 1);

    // Dossier sans folder.bru : Bruno lui en écrit un, nommé d'après le
    // répertoire.
    ui.select("NoMeta");
    ui.press_with(KeyCode::Up, KeyModifiers::SHIFT);
    assert_eq!(
        ui.read("NoMeta/folder.bru"),
        files::folder_meta_content("NoMeta", Some(3))
    );
    assert!(ui.read("Posts/folder.bru").contains("  seq: 4\n"));
    assert_eq!(ui.selected_path(), Some(PathBuf::from("NoMeta")));

    // Premier élément : rien ne bouge.
    ui.select("list-posts.bru");
    let before = ui.read("list-posts.bru");
    ui.press_with(KeyCode::Up, KeyModifiers::SHIFT);
    assert_eq!(ui.read("list-posts.bru"), before);
}

#[test]
fn trash_asks_for_confirmation_and_counts_requests() {
    let mut ui = Ui::new("trash");
    ui.select("Posts");
    ui.press(KeyCode::Char('d'));
    assert!(matches!(
        ui.model.confirm,
        Some(PendingConfirm::TrashItem { .. })
    ));
    let screen = ui.screen(120, 30);
    assert!(
        screen.contains("Mettre le dossier « Articles » (1 requête) à la corbeille ? (y/n)"),
        "{screen}"
    );
    ui.press(KeyCode::Char('n'));
    assert!(ui.root.join("Posts").exists());

    ui.select("get-post.bru");
    ui.press(KeyCode::Char('d'));
    let screen = ui.screen(120, 30);
    assert!(
        screen.contains("Mettre « Lire un article » à la corbeille ? (y/n)"),
        "{screen}"
    );
    ui.press(KeyCode::Char('y'));
    assert!(!ui.root.join("get-post.bru").exists());
    assert!(ui.data.join("Trash/files/get-post.bru").is_file());
    assert!(ui.data.join("Trash/info/get-post.bru.trashinfo").is_file());
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpDone(
            "Mis à la corbeille : get-post.bru".into()
        ))
    );
}

#[test]
fn collection_header_refuses_all_but_creation() {
    let mut ui = Ui::new("header");
    ui.press(KeyCode::Char('c'));
    for code in [KeyCode::F(2), KeyCode::Char('p'), KeyCode::Char('d')] {
        ui.press(code);
        assert!(ui.model.file_popup.is_none());
        assert!(ui.model.confirm.is_none());
        assert_eq!(
            ui.status(),
            Some(StatusMessage::FileOpRefused(
                "En-tête de la collection : seules a et A sont permises".into()
            )),
            "{code:?}"
        );
    }
    ui.press_with(KeyCode::Down, KeyModifiers::SHIFT);
    assert!(matches!(ui.status(), Some(StatusMessage::FileOpRefused(_))));
}

#[test]
fn a_running_campaign_refuses_file_operations() {
    let mut ui = Ui::new("running");
    ui.select("get-post.bru");
    ui.model.series = Some(EnvSeries::new(PathBuf::new(), vec!["a".into()]));
    for code in [KeyCode::Char('a'), KeyCode::F(2), KeyCode::Char('d')] {
        ui.press(code);
        assert!(ui.model.file_popup.is_none());
        assert!(ui.model.confirm.is_none());
        assert_eq!(
            ui.status(),
            Some(StatusMessage::FileOpRefused(
                "Exécution en cours : opération sur les fichiers refusée".into()
            ))
        );
    }
}

#[test]
fn yaml_collections_stay_read_only() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/oc-probe");
    let mut model = Model::new(root.clone(), (120, 40));
    update(
        &mut model,
        Message::CollectionLoaded(bruno_tui::collection::AutoLoader.load(&root)),
    );
    for code in [
        KeyCode::Char('a'),
        KeyCode::Char('A'),
        KeyCode::F(2),
        KeyCode::Char('d'),
    ] {
        let event = AppEvent::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        let message = to_message(event, model.text_capture()).expect("message");
        let command = update(&mut model, message);
        assert!(matches!(command, Command::None));
        assert!(model.file_popup.is_none());
        assert!(model.confirm.is_none());
        assert_eq!(model.last_status, Some(StatusMessage::ReadOnlyCollection));
    }
}

#[test]
fn popups_render_inside_the_screen_at_80_and_120_columns() {
    for width in [80, 120] {
        let mut ui = Ui::new(&format!("render-{width}"));
        ui.model.size = (width, 30);
        ui.select("Posts");
        ui.press(KeyCode::Char('a'));
        ui.type_text("Lire un article");
        let screen = ui.screen(width, 30);
        for needle in [
            "Nouvelle requête — dans Articles",
            "Nom        Lire un article",
            "Fichier    Lire un article.bru",
            "Type       ‹ http ›",
            "Méthode    ‹ GET ›",
            "URL",
            "Ctrl+S créer  Échap annuler",
        ] {
            assert!(screen.contains(needle), "{width} {needle}\n{screen}");
        }
        ui.press(KeyCode::Esc);
        ui.select("get-post.bru");
        ui.press(KeyCode::F(2));
        let screen = ui.screen(width, 30);
        for needle in [
            "Renommer — get-post.bru",
            "Fichier    get-post.bru",
            "[x] Renommer aussi le fichier",
            "Ctrl+S appliquer",
        ] {
            assert!(screen.contains(needle), "{width} {needle}\n{screen}");
        }
    }
}

#[test]
fn workspace_operations_target_the_active_collection_and_keep_selection() {
    let mut ui = Ui::workspace("workspace");
    assert_eq!(active_member(&ui.model), Some(0));
    ui.select("Posts");
    ui.press(KeyCode::Char('a'));
    ui.type_text("Nouvelle");
    ui.save();
    assert!(ui.members[0].join("Posts/Nouvelle.bru").is_file());
    assert!(!ui.members[1].join("Posts/Nouvelle.bru").exists());
    let row = ui.model.selected_row().expect("ligne").clone();
    assert_eq!(row.member, 0);
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Posts/Nouvelle.bru"))
    );

    ui.press(KeyCode::F(2));
    ui.clear_field();
    ui.type_text("Renommée");
    ui.save();
    assert!(ui.members[0].join("Posts/Renommée.bru").is_file());
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Posts/Renommée.bru"))
    );
}

fn request_text(name: &str, seq: Option<u32>) -> String {
    let seq = seq.map(|seq| format!("  seq: {seq}\n")).unwrap_or_default();
    format!("meta {{\n  name: {name}\n  type: http\n{seq}}}\n\nget {{\n  url: http://x\n}}\n")
}

fn modified(path: &Path) -> std::time::SystemTime {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .expect("date")
}

#[test]
fn trash_renumbers_siblings_like_bruno() {
    let mut ui = Ui::new("trash-renumber");
    let list = ui.read("list-posts.bru");
    let folder = ui.read("Posts/folder.bru");
    let before = modified(&ui.root.join("list-posts.bru"));
    std::thread::sleep(std::time::Duration::from_millis(20));
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('d'));
    ui.press(KeyCode::Char('y'));
    // `sortItems` : NoMeta (sans seq) passe en tête, les autres gardent
    // leur seq ; Bruno lui écrit un folder.bru nommé d'après le répertoire.
    assert_eq!(
        ui.read("NoMeta/folder.bru"),
        files::folder_meta_content("NoMeta", Some(1))
    );
    assert_eq!(ui.read("list-posts.bru"), list);
    assert_eq!(ui.read("Posts/folder.bru"), folder);
    assert_eq!(modified(&ui.root.join("list-posts.bru")), before);
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpDone(
            "Mis à la corbeille : get-post.bru".into()
        ))
    );
}

#[test]
fn trash_closes_gaps_and_duplicates() {
    let mut ui = Ui::new("trash-gaps");
    ui.write("Posts/a.bru", &request_text("A", Some(3)));
    ui.write("Posts/b.bru", &request_text("B", Some(3)));
    ui.write("Posts/c.bru", &request_text("C", Some(7)));
    ui.reload();
    ui.select("Posts/create.bru");
    ui.press(KeyCode::Char('d'));
    ui.press(KeyCode::Char('y'));
    assert!(!ui.root.join("Posts/create.bru").exists());
    assert_eq!(ui.read("Posts/a.bru"), request_text("A", Some(1)));
    assert_eq!(ui.read("Posts/b.bru"), request_text("B", Some(2)));
    assert_eq!(ui.read("Posts/c.bru"), request_text("C", Some(3)));
}

#[test]
fn trash_without_reproducible_order_keeps_seqs_and_says_so() {
    let mut ui = Ui::new("trash-unsortable");
    // « Créer » (non ASCII) et un seq absent : l'ordre de Bruno dépendrait
    // de `localeCompare` hors du cas reproduit.
    ui.write("Posts/x.bru", &request_text("X", None));
    ui.write("Posts/y.bru", &request_text("Y", Some(2)));
    ui.reload();
    ui.select("Posts/y.bru");
    ui.press(KeyCode::Char('d'));
    ui.press(KeyCode::Char('y'));
    assert!(!ui.root.join("Posts/y.bru").exists());
    assert_eq!(ui.read("Posts/x.bru"), request_text("X", None));
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpDone(
            "Mis à la corbeille : y.bru ; seq non renumérotés : noms non comparables comme Bruno (« Créer »)"
                .into()
        ))
    );
}

fn destinations(ui: &Ui) -> Vec<String> {
    ui.model
        .move_popup
        .as_ref()
        .expect("popup")
        .visible()
        .iter()
        .map(|destination| destination.dir.display().to_string())
        .collect()
}

#[test]
fn m_in_the_tree_opens_move_but_stays_the_request_popup_in_the_detail() {
    let mut ui = Ui::new("move-routing");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('m'));
    assert!(ui.model.move_popup.is_some());
    assert!(ui.model.editing.is_none());
    assert_eq!(
        ui.model.move_popup.as_ref().expect("popup").title(),
        " Déplacer — get-post.bru "
    );
    // Le popup est modal : `q` va au filtre, il ne quitte pas.
    ui.press(KeyCode::Char('q'));
    assert!(ui.model.exit.is_none());
    ui.press(KeyCode::Esc);
    assert!(ui.model.move_popup.is_none());

    // Dans le Détail, `m` ouvre toujours le popup Requête.
    ui.model.focus = bruno_tui::app::model::Focus::Detail;
    ui.press(KeyCode::Char('m'));
    assert!(ui.model.move_popup.is_none());
    assert!(ui.model.editing.is_some());
}

#[test]
fn move_request_through_the_popup_follows_the_item_and_renumbers_the_source() {
    let mut ui = Ui::new("move-request");
    let before = ui.read("get-post.bru");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('m'));
    // Racine masquée (c'est son dossier actuel) ; Posts et NoMeta listés.
    assert_eq!(destinations(&ui), ["Posts", "NoMeta"]);
    let screen = ui.screen(100, 30);
    for needle in [
        "Déplacer — get-post.bru",
        "Vers",
        "▸ Articles",
        "  NoMeta",
        "Entrée déplacer  Échap annuler",
    ] {
        assert!(screen.contains(needle), "{needle}\n{screen}");
    }
    ui.type_text("arti");
    assert_eq!(destinations(&ui), ["Posts"]);
    ui.press(KeyCode::Enter);

    assert!(ui.model.move_popup.is_none());
    assert_eq!(ui.read("Posts/get-post.bru"), before);
    assert!(!ui.root.join("get-post.bru").exists());
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Posts/get-post.bru"))
    );
    // Source renumérotée comme après une corbeille (NoMeta passe en tête).
    assert_eq!(
        ui.read("NoMeta/folder.bru"),
        files::folder_meta_content("NoMeta", Some(1))
    );
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpDone(
            "Déplacé vers Posts : get-post.bru".into()
        ))
    );
}

#[test]
fn move_to_the_root_and_name_collision_are_announced() {
    let mut ui = Ui::new("move-collision");
    ui.write("get-post.bru", &ui.read("get-post.bru"));
    ui.select("Posts/create.bru");
    ui.write("Posts/get-post.bru", &request_text("Autre", Some(5)));
    ui.write("Posts/list-posts.bru", &request_text("Encore", Some(6)));
    ui.reload();
    ui.select("Posts/get-post.bru");
    ui.press(KeyCode::Char('m'));
    // Racine en tête : un `get-post.bru` y est déjà.
    assert_eq!(destinations(&ui), ["", "NoMeta"]);
    assert!(
        ui.screen(100, 30).contains("Sera nommé : get-post 1.bru"),
        "{}",
        ui.screen(100, 30)
    );
    // NoMeta : pas de collision, pas de ligne.
    ui.press(KeyCode::Down);
    assert!(!ui.screen(100, 30).contains("Sera nommé"));
    ui.press(KeyCode::Up);
    ui.press(KeyCode::Enter);
    assert_eq!(ui.selected_path(), Some(PathBuf::from("get-post 1.bru")));
    assert_eq!(ui.read("get-post 1.bru"), request_text("Autre", Some(5)));
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpDone(
            "Déplacé vers la racine de la collection : get-post 1.bru".into()
        ))
    );
}

#[test]
fn move_folder_hides_itself_its_descendants_and_its_current_folder() {
    let mut ui = Ui::new("move-folder");
    std::fs::create_dir_all(ui.root.join("Posts/Sub")).expect("sous-dossier");
    ui.write("Posts/Sub/x.bru", &request_text("X", Some(1)));
    ui.reload();
    ui.select("Posts/Sub");
    ui.press(KeyCode::Char('m'));
    // Son dossier actuel (Posts) est masqué, lui-même aussi.
    assert_eq!(destinations(&ui), ["", "NoMeta"]);
    ui.press(KeyCode::Esc);

    ui.select("Posts");
    ui.press(KeyCode::Char('m'));
    // Posts et Posts/Sub masqués ; la racine l'est aussi (dossier actuel).
    assert_eq!(destinations(&ui), ["NoMeta"]);
    ui.press(KeyCode::Enter);
    assert!(ui.root.join("NoMeta/Posts/Sub/x.bru").is_file());
    assert_eq!(
        ui.read("NoMeta/Posts/folder.bru"),
        fs::read_to_string(fixture().join("Posts/folder.bru")).expect("fixture")
    );
    assert_eq!(ui.selected_path(), Some(PathBuf::from("NoMeta/Posts")));
}

#[test]
fn move_refusals_are_announced() {
    // En-tête de la collection.
    let mut ui = Ui::new("move-header");
    ui.model.selected_collection = true;
    ui.press(KeyCode::Char('m'));
    assert!(ui.model.move_popup.is_none());
    assert!(matches!(ui.status(), Some(StatusMessage::FileOpRefused(_))));

    // Exécution en cours.
    let mut ui = Ui::new("move-running");
    ui.select("get-post.bru");
    ui.model.series = Some(EnvSeries::new(PathBuf::new(), vec!["a".into()]));
    ui.press(KeyCode::Char('m'));
    assert!(ui.model.move_popup.is_none());
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpRefused(
            "Exécution en cours : opération sur les fichiers refusée".into()
        ))
    );

    // Aucune destination (collection d'un seul niveau, élément à la racine).
    let mut ui = Ui::new("move-nowhere");
    fs::remove_dir_all(ui.root.join("Posts")).expect("retrait");
    fs::remove_dir_all(ui.root.join("NoMeta")).expect("retrait");
    ui.reload();
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('m'));
    assert!(ui.model.move_popup.is_none());
    assert_eq!(
        ui.status(),
        Some(StatusMessage::FileOpRefused(
            "Aucun autre dossier où déplacer cet élément".into()
        ))
    );
}

#[test]
fn a_file_changed_on_disk_keeps_the_popup_open_with_the_message() {
    let mut ui = Ui::new("move-stale");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('m'));
    ui.write("get-post.bru", "changé par un tiers");
    ui.press(KeyCode::Enter);
    let popup = ui.model.move_popup.as_ref().expect("popup toujours ouvert");
    assert!(!popup.pending);
    assert!(
        popup
            .error
            .as_deref()
            .is_some_and(|error| error.contains("modifié sur disque")),
        "{:?}",
        popup.error
    );
    let screen = ui.screen(100, 30);
    assert!(screen.contains("son chargement"), "{screen}");
    assert_eq!(ui.read("get-post.bru"), "changé par un tiers");
    assert!(!ui.root.join("Posts/get-post.bru").exists());
    ui.press(KeyCode::Esc);
    assert!(ui.model.move_popup.is_none());
}

#[test]
fn an_empty_filter_result_refuses_the_validation() {
    let mut ui = Ui::new("move-nomatch");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('m'));
    ui.type_text("zzz");
    assert!(ui.screen(100, 30).contains("aucun dossier ne correspond"));
    ui.press(KeyCode::Enter);
    let popup = ui.model.move_popup.as_ref().expect("popup");
    assert_eq!(
        popup.error.as_deref(),
        Some("Aucun dossier ne correspond au filtre.")
    );
    assert!(ui.root.join("get-post.bru").exists());
}

#[test]
fn workspace_move_targets_the_active_collection_only() {
    let mut ui = Ui::workspace("move-workspace");
    ui.select("get-post.bru");
    ui.press(KeyCode::Char('m'));
    ui.type_text("arti");
    ui.press(KeyCode::Enter);
    assert!(ui.members[0].join("Posts/get-post.bru").is_file());
    assert!(!ui.members[0].join("get-post.bru").exists());
    // L'autre collection n'a pas bougé.
    assert!(ui.members[1].join("get-post.bru").is_file());
    assert!(!ui.members[1].join("Posts/get-post.bru").exists());
    assert_eq!(
        ui.selected_path(),
        Some(PathBuf::from("Posts/get-post.bru"))
    );
}
