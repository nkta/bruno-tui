//! Interface plein écran de bruno-tui, architecture Elm-like.
//!
//! Toutes les entrées arrivent sous forme d'`AppEvent` sur un unique canal.
//! La boucle les traduit en `Message`, applique `update` au `Model`, puis
//! redessine avec `view`. Le chargement de la collection s'exécute hors de
//! la boucle, qui ne bloque jamais.

pub mod cli;
pub mod clipboard;
pub mod clock;
pub mod diagnostics;
pub mod editor;
pub mod event;
pub mod export;
pub mod file_popup;
pub mod filter;
pub mod global_search;
pub mod history;
pub mod message;
pub mod model;
pub mod mouse;
pub mod search;
pub mod settings;
pub mod text_input;
pub mod tree_filter;
pub mod update;
pub mod view;
pub mod watch;
pub mod workspace;

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::Terminal;
use ratatui::backend::Backend;
use tokio::sync::mpsc;

use crate::collection::CollectionLoader;
use crate::runner::BruRunner;
use crate::secrets::{self, SecretMapping};
use crate::writer::{self, RequestWriter};
use clipboard::Clipboard;
use event::{AppEvent, EVENT_BUFFER};
use message::{Message, to_message};
use model::{Exit, Model};
use mouse::MouseSetup;
use update::{Command, update};

/// Fait tourner l'interface jusqu'à la demande de sortie.
///
/// Le chargement est lancé par `spawn_blocking` ; `events` reçoit aussi
/// les événements du terminal, publiés par l'appelant sur `sender`.
/// `bru_program` est le programme délégué pour `Command::StartRun` : `bru`
/// en usage réel, le chemin d'un faux `bru` dans les tests. `clipboard` est
/// de même injectable, pour les mêmes raisons (une fausse implémentation
/// en test, `SystemClipboard` en usage réel). `writer` est également
/// injectable (`BruWriter` en usage réel). `secrets` porte les
/// déclarations `--secret` de la ligne de commande. `mouse` porte le moyen
/// de piloter la capture souris et l'état appliqué au démarrage
/// (`mouse-support`), injectable comme le presse-papiers. `suspended` est
/// l'indicateur partagé permettant de suspendre la lecture du terminal
/// pendant qu'un processus externe possède le terminal.
#[allow(clippy::too_many_arguments)]
pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    loader: Arc<dyn CollectionLoader>,
    launch: impl Into<workspace::Launch>,
    sender: mpsc::Sender<AppEvent>,
    mut events: mpsc::Receiver<AppEvent>,
    bru_program: OsString,
    clipboard: Arc<dyn Clipboard>,
    writer: Arc<dyn RequestWriter>,
    secrets: Vec<SecretMapping>,
    mouse: MouseSetup,
    suspended: Arc<AtomicBool>,
    watch: bool,
    history: bool,
    editor: Option<String>,
) -> Result<Exit, B::Error> {
    let size = terminal.size()?;
    let (source, members) = match launch.into() {
        workspace::Launch::Single(source) => (source, None),
        workspace::Launch::Workspace(ws) => {
            let first = ws
                .members
                .first()
                .map(|member| member.root.clone())
                .unwrap_or_default();
            (first, Some(ws))
        }
    };
    let mut model = Model::new(source.clone(), (size.width, size.height));
    model.secrets.mappings = secrets;
    model.mouse.capture = mouse.enabled;
    model.history_enabled = history;
    model.editor = editor;
    if let Some(reason) = mouse.startup_error {
        model.last_status = Some(model::StatusMessage::MouseCaptureError(reason));
    }
    // Racines des membres du workspace, dans l'ordre ; vide hors workspace.
    let member_roots: Vec<PathBuf> = members
        .as_ref()
        .map(|ws| ws.members.iter().map(|m| m.root.clone()).collect())
        .unwrap_or_default();
    if let Some(ws) = members {
        // `--workspace-path` par défaut : le dossier de `workspace.yml`.
        model.runner_options.workspace_path = ws.path.clone();
        workspace::install(&mut model, ws);
    }

    let mut _watcher = if watch && member_roots.is_empty() {
        crate::collection::config::find_root(&source)
            .ok()
            .and_then(|root| watch::start_watcher(root, sender.clone()).ok())
    } else {
        None
    };
    // Un watcher par collection du workspace ; un chemin invalide n'en a pas.
    let _member_watchers: Vec<watch::CollectionWatcher> = if watch {
        member_roots
            .iter()
            .enumerate()
            .filter_map(|(member, root)| {
                let root = std::fs::canonicalize(root).ok()?;
                watch::start_member_watcher(member, root, sender.clone()).ok()
            })
            .collect()
    } else {
        Vec::new()
    };

    let (run_tx, mut run_rx) = mpsc::channel(EVENT_BUFFER);
    let runner = BruRunner::with_program(bru_program, run_tx);
    {
        let sender = sender.clone();
        tokio::spawn(async move {
            while let Some(event) = run_rx.recv().await {
                if sender.send(AppEvent::Run(event)).await.is_err() {
                    break;
                }
            }
        });
    }

    if member_roots.is_empty() {
        let sender = sender.clone();
        let init_loader = Arc::clone(&loader);
        tokio::task::spawn_blocking(move || {
            let result = init_loader.load(&source);
            // Boucle terminée : plus personne n'attend la collection.
            let _ = sender.blocking_send(AppEvent::CollectionLoaded(result));
        });
    }
    for member in 0..member_roots.len() {
        spawn_member_load(&loader, &member_roots, member, &sender);
    }

    terminal.draw(|frame| view::view(&model, frame))?;
    loop {
        let Some(event) = events.recv().await else {
            return Ok(Exit::TerminalError(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "flux d'événements fermé",
            )));
        };
        if let AppEvent::CollectionLoaded(Ok(ref collection)) = event
            && _watcher.is_none()
            && watch
            && member_roots.is_empty()
        {
            _watcher = watch::start_watcher(collection.root.clone(), sender.clone()).ok();
        }
        // Un événement écarté (survol souris, focus du terminal) ne change
        // rien au modèle : pas de redessin.
        let Some(message) = to_message(event, model.text_capture()) else {
            continue;
        };
        {
            let mut pending_commands = vec![update(&mut model, message)];
            while let Some(command) = pending_commands.pop() {
                match command {
                    Command::None => {}
                    Command::Batch(commands) => {
                        pending_commands.extend(commands.into_iter().rev());
                    }
                    Command::StartRun {
                        request,
                        target,
                        is_campaign,
                    } => {
                        let recursive = request.recursive;
                        let handle = runner.start(*request);
                        update(
                            &mut model,
                            Message::RunStarted {
                                id: handle.id(),
                                target,
                                recursive,
                                is_campaign,
                                handle,
                            },
                        );
                    }
                    Command::CopyToClipboard { token, text } => {
                        let clipboard = Arc::clone(&clipboard);
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let result = clipboard.set_text(text);
                            let _ =
                                sender.blocking_send(AppEvent::ClipboardResult { token, result });
                        });
                    }
                    Command::SaveEdit {
                        path,
                        ast,
                        stamp,
                        edits,
                    } => {
                        let root = model
                            .loaded()
                            .map(|c| c.root.clone())
                            .unwrap_or_else(|| PathBuf::from("."));
                        let full_path = root.join(&path);
                        let writer = Arc::clone(&writer);
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let result = writer
                                .write_request(&full_path, &ast, &stamp, &edits)
                                .and_then(|new_stamp| {
                                    let bytes = std::fs::read(&full_path).map_err(|source| {
                                        writer::WriteError::Io {
                                            path: full_path.clone(),
                                            source,
                                        }
                                    })?;
                                    let text = String::from_utf8(bytes).map_err(|source| {
                                        writer::WriteError::Io {
                                            path: full_path.clone(),
                                            source: io::Error::new(
                                                io::ErrorKind::InvalidData,
                                                source,
                                            ),
                                        }
                                    })?;
                                    let ast = crate::collection::BruFile::parse(text).map_err(
                                        |source| writer::WriteError::Io {
                                            path: full_path.clone(),
                                            source: io::Error::new(
                                                io::ErrorKind::InvalidData,
                                                source,
                                            ),
                                        },
                                    )?;
                                    let view = match crate::collection::RequestView::from_ast(&ast)
                                    {
                                        Ok(v) => v,
                                        Err(crate::collection::ParseError::MissingMethod) => {
                                            crate::collection::FileMeta::from_ast(ast.clone())
                                                .to_request_view()
                                        }
                                        Err(source) => {
                                            return Err(writer::WriteError::Io {
                                                path: full_path.clone(),
                                                source: io::Error::new(
                                                    io::ErrorKind::InvalidData,
                                                    source,
                                                ),
                                            });
                                        }
                                    };
                                    Ok(model::SavedEdit {
                                        stamp: new_stamp,
                                        ast,
                                        view,
                                    })
                                });
                            let _ =
                                sender.blocking_send(AppEvent::EditSaved { root, path, result });
                        });
                    }
                    Command::SaveEnvironment {
                        path,
                        ast,
                        stamp,
                        edits,
                    } => {
                        let root = model
                            .loaded()
                            .map(|c| c.root.clone())
                            .unwrap_or_else(|| PathBuf::from("."));
                        let full_path = root.join(&path);
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let result =
                                writer::write_environment(&full_path, &ast, &stamp, &edits)
                                    .and_then(|new_stamp| {
                                        let bytes =
                                            std::fs::read(&full_path).map_err(|source| {
                                                writer::WriteError::Io {
                                                    path: full_path.clone(),
                                                    source,
                                                }
                                            })?;
                                        let text = String::from_utf8(bytes).map_err(|source| {
                                            writer::WriteError::Io {
                                                path: full_path.clone(),
                                                source: io::Error::new(
                                                    io::ErrorKind::InvalidData,
                                                    source,
                                                ),
                                            }
                                        })?;
                                        // Fichier d'environnement : accepte `extends`/`color`.
                                        let ast =
                                            crate::collection::BruFile::parse_environment(text)
                                                .map_err(|source| writer::WriteError::Io {
                                                    path: full_path.clone(),
                                                    source: io::Error::new(
                                                        io::ErrorKind::InvalidData,
                                                        source,
                                                    ),
                                                })?;
                                        Ok(model::SavedEnvironment {
                                            stamp: new_stamp,
                                            ast,
                                        })
                                    });
                            let _ = sender.blocking_send(AppEvent::EnvironmentSaved {
                                root,
                                path,
                                result,
                            });
                        });
                    }
                    Command::SaveSettings {
                        root,
                        stamp,
                        changes,
                        added_paths,
                    } => {
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let result =
                                writer::bruno_config::write_bruno_config(&root, &stamp, &changes)
                                    .map(|stamp| {
                                        // Un chemin de certificat introuvable n'est
                                        // qu'un avertissement : `bru` l'affichera.
                                        let warnings = added_paths
                                            .iter()
                                            .filter(|path| !root.join(path).exists())
                                            .map(|path| format!("fichier introuvable : {path}"))
                                            .collect();
                                        model::SavedSettings { stamp, warnings }
                                    });
                            let _ = sender.blocking_send(AppEvent::SettingsSaved { root, result });
                        });
                    }
                    Command::SetMouseCapture(enabled) => {
                        let result = mouse
                            .capture
                            .set(enabled)
                            .map_err(|error| error.to_string());
                        update(&mut model, Message::MouseCaptureChanged { enabled, result });
                    }
                    Command::ResolveSecrets { root, lookups } => {
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let resolved =
                                secrets::resolve(&root, &lookups, &|key| std::env::var_os(key));
                            let _ =
                                sender.blocking_send(AppEvent::SecretsResolved { root, resolved });
                        });
                    }
                    Command::OpenInEditor { program, text } => {
                        suspended.store(true, Ordering::SeqCst);
                        let mouse_was_captured = model.mouse.capture;
                        if mouse_was_captured {
                            let _ = mouse.capture.set(false);
                        }
                        let _ = ratatui::try_restore();

                        let result = tokio::task::spawn_blocking(move || {
                            editor::spawn_external_editor(&program, &text)
                        })
                        .await;

                        let _ = ratatui::try_init();
                        let _ = terminal.clear();
                        if mouse_was_captured {
                            let _ = mouse.capture.set(true);
                        }
                        suspended.store(false, Ordering::SeqCst);

                        match result {
                            Ok(Err(error)) => {
                                model.last_status =
                                    Some(model::StatusMessage::EditorError(error.to_string()));
                            }
                            Err(join_error) => {
                                model.last_status =
                                    Some(model::StatusMessage::EditorError(join_error.to_string()));
                            }
                            Ok(Ok(())) => {}
                        }
                    }
                    Command::EditTextInEditor {
                        program,
                        text,
                        extension,
                        field,
                    } => {
                        suspended.store(true, Ordering::SeqCst);
                        let mouse_was_captured = model.mouse.capture;
                        if mouse_was_captured {
                            let _ = mouse.capture.set(false);
                        }
                        let _ = ratatui::try_restore();

                        let result = tokio::task::spawn_blocking(move || {
                            editor::edit_external_text(&program, &text, &extension)
                        })
                        .await;

                        let _ = ratatui::try_init();
                        let _ = terminal.clear();
                        if mouse_was_captured {
                            let _ = mouse.capture.set(true);
                        }
                        suspended.store(false, Ordering::SeqCst);

                        let event_result = match result {
                            Ok(res) => res.map_err(|e| e.to_string()),
                            Err(join_error) => Err(join_error.to_string()),
                        };
                        let _ = sender
                            .send(AppEvent::TextEditorClosed {
                                field,
                                result: event_result,
                            })
                            .await;
                    }
                    Command::ReloadMember(member) => {
                        spawn_member_load(&loader, &member_roots, member, &sender);
                    }
                    Command::ReloadCollection if !member_roots.is_empty() => {
                        if let Some(member) = workspace::active_member(&model) {
                            spawn_member_load(&loader, &member_roots, member, &sender);
                        }
                    }
                    Command::ReloadCollection => {
                        let loader = Arc::clone(&loader);
                        let source = model.source.clone();
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let result = loader.load(&source);
                            let _ = sender.blocking_send(AppEvent::CollectionLoaded(result));
                        });
                    }
                    Command::ExportResponseBody { path, bytes } => {
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let result = std::fs::write(&path, &bytes).map_err(|e| e.to_string());
                            let _ = sender
                                .blocking_send(AppEvent::ResponseBodyExported { path, result });
                        });
                    }
                    Command::LoadHistory { root } => {
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let (entries, notice) = history::load_history(&root);
                            let _ = sender.blocking_send(AppEvent::HistoryLoaded {
                                root,
                                entries,
                                notice,
                            });
                        });
                    }
                    Command::SaveHistory { root, history } => {
                        tokio::task::spawn_blocking(move || {
                            let _ = history::save_history(&root, &history);
                        });
                    }
                    Command::FileOp { root, op } => {
                        let sender = sender.clone();
                        tokio::task::spawn_blocking(move || {
                            let date = clock::trash_date(std::time::SystemTime::now());
                            let result = writer::files::execute(
                                &root,
                                &op,
                                writer::trash::Trash::from_env,
                                &date,
                            );
                            let _ = sender.blocking_send(AppEvent::FileOpDone { root, result });
                        });
                    }
                }
            }
        }
        if let Some(exit) = model.exit.take() {
            return Ok(exit);
        }
        terminal.draw(|frame| view::view(&model, frame))?;
    }
}

/// Charge hors de la boucle la collection d'un membre du workspace.
fn spawn_member_load(
    loader: &Arc<dyn CollectionLoader>,
    roots: &[PathBuf],
    member: usize,
    sender: &mpsc::Sender<AppEvent>,
) {
    let Some(root) = roots.get(member).cloned() else {
        return;
    };
    let loader = Arc::clone(loader);
    let sender = sender.clone();
    tokio::task::spawn_blocking(move || {
        let result = crate::collection::workspace::load_member(&*loader, &root);
        let _ = sender.blocking_send(AppEvent::MemberLoaded { member, result });
    });
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Outils communs aux tests unitaires du module.

    use std::path::{Path, PathBuf};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::message::Message;
    use super::model::Model;
    use super::update::{refresh_rows, scroll_tree_into_view, update};
    use super::view::{tree::display_name, view};
    use crate::collection::{BruLoader, CollectionLoader};

    pub fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases")
    }

    /// Modèle avec `parser-cases` chargée.
    pub fn loaded_model(size: (u16, u16)) -> Model {
        let mut model = Model::new(fixture(), size);
        update(
            &mut model,
            Message::CollectionLoaded(BruLoader.load(&fixture())),
        );
        model
    }

    /// Modèle avec `runner-probe` chargée et le rapport `mixed.json` appliqué.
    pub fn runner_probe_model() -> Model {
        let probe_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/runner-probe");
        let mut model = Model::new(probe_path.clone(), (100, 30));
        update(
            &mut model,
            Message::CollectionLoaded(crate::collection::BruLoader.load(&probe_path)),
        );
        let mixed_report: crate::runner::report::Report =
            serde_json::from_str(include_str!("../../tests/fixtures/reports/mixed.json"))
                .expect("mixed.json doit se désérialiser");
        for result in mixed_report.0.into_iter().flat_map(|it| it.results) {
            let key = std::path::PathBuf::from(&result.test.filename);
            model
                .run
                .outcomes
                .insert(key, super::model::RequestOutcome::new(result, Some(0)));
        }
        model
    }

    /// Modèle en mode workspace sur `runner-probe` et `multi-env-probe`,
    /// toutes deux chargées et repliées, la première active.
    pub fn workspace_model() -> Model {
        use crate::collection::workspace::from_paths;
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections");
        let a = fixtures.join("runner-probe");
        let b = fixtures.join("multi-env-probe");
        let mut model = Model::new(a.clone(), (100, 30));
        super::workspace::install(&mut model, from_paths(&[a.clone(), b.clone()]));
        model.collection =
            super::model::CollectionState::Loaded(BruLoader.load(&a).expect("runner-probe"));
        if let Some(state) = model.workspace.as_mut() {
            state.slots[1].collection =
                super::model::CollectionState::Loaded(BruLoader.load(&b).expect("multi-env-probe"));
        }
        refresh_rows(&mut model);
        model
    }

    /// Déplie les dossiers parents et sélectionne le nœud de ce chemin.
    pub fn select(model: &mut Model, path: &str) {
        let target = Path::new(path);
        super::workspace::open_active(model);
        for ancestor in target.ancestors().skip(1) {
            if !ancestor.as_os_str().is_empty() {
                model.tree.expanded.insert(ancestor.to_path_buf());
            }
        }
        refresh_rows(model);
        let index = model
            .active_row_position(|node| node.path() == target)
            .unwrap_or_else(|| panic!("`{path}` introuvable"));
        model.tree.selected = index;
        model.detail_scroll = 0;
        scroll_tree_into_view(model);
    }

    pub fn selected_name(model: &Model) -> String {
        model.selected_node().map(display_name).unwrap_or_default()
    }

    /// Rend le modèle et retourne les lignes de l'écran.
    pub fn render(model: &Model, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal.draw(|frame| view(model, frame)).expect("rendu");
        screen_lines(terminal.backend())
    }

    pub fn screen_lines(backend: &TestBackend) -> Vec<String> {
        let buffer = backend.buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }
}
