//! Surveillance des fichiers de la collection pour le rechargement automatique.
//!
//! Surveille la racine de la collection et regroupe les événements par un
//! délai de débounce (~300 ms). Les fichiers ignorés (`.git`, `node_modules`,
//! fichiers temporaires d'éditeurs) sont filtrés avant tout traitement.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::mpsc;

use super::event::{AppEvent, EVENT_BUFFER};

/// Délai de regroupement des rafales d'événements du système de fichiers (~300 ms).
pub const DEBOUNCE_DURATION: Duration = Duration::from_millis(300);

/// Vérifie si un chemin doit être totalement ignoré.
///
/// Ignore les répertoires de gestion de version (`.git/`), de dépendances
/// (`node_modules/`), ainsi que les fichiers temporaires et sauvegardes
/// d'éditeurs de texte (Vim, Emacs, Nano, Gedit, etc.).
pub fn is_ignored_path(path: &Path) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if name == ".git" || name == "node_modules" {
            return true;
        }
    }

    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
        // Fichiers de sauvegarde ou temporaires d'éditeurs
        if file_name.ends_with('~')
            || file_name.ends_with(".tmp")
            || file_name.ends_with(".bak")
            || file_name.ends_with(".crswap")
            || file_name.starts_with(".#")
            || (file_name.starts_with('#') && file_name.ends_with('#'))
            || (file_name.starts_with('.')
                && (file_name.ends_with(".swp")
                    || file_name.ends_with(".swo")
                    || file_name.ends_with(".swx")))
            || file_name.contains(".tmp-")
            || file_name == "4913"
        {
            return true;
        }
    }

    false
}

/// Vérifie si une modification de ce chemin justifie un rechargement de collection.
///
/// Déclenche pour :
/// - Les fichiers `.bru`
/// - Les fichiers `bruno.json`, `collection.bru`, `folder.bru`
/// - N'importe quel fichier sous le dossier `environments/`
pub fn is_watched_path(path: &Path) -> bool {
    if is_ignored_path(path) {
        return false;
    }

    // N'importe quel fichier dans le dossier environments/
    let in_environments = path.components().any(|c| c.as_os_str() == "environments");
    if in_environments {
        return true;
    }

    if let Some(file_name) = path.file_name().and_then(|n| n.to_str())
        && matches!(file_name, "bruno.json" | "collection.bru" | "folder.bru")
    {
        return true;
    }

    if path.extension().is_some_and(|ext| ext == "bru") {
        return true;
    }

    false
}

/// Vérifie si au moins un des chemins modifiés justifie un rechargement.
pub fn should_reload(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| is_watched_path(path))
}

/// Regroupe les événements arrivant sur `input_rx` pendant `duration`.
///
/// Émet un `AppEvent::WatchReload` contenant l'ensemble des chemins uniques modifiés,
/// relatifs à la racine de la collection,
/// une fois le délai écoulé sans nouvel événement.
pub async fn debounce_events(
    mut input_rx: mpsc::Receiver<PathBuf>,
    output_tx: mpsc::Sender<AppEvent>,
    duration: Duration,
) {
    let mut pending: Vec<PathBuf> = Vec::new();
    loop {
        tokio::select! {
            maybe_path = input_rx.recv() => {
                match maybe_path {
                    Some(path) => {
                        if !pending.contains(&path) {
                            pending.push(path);
                        }
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(duration), if !pending.is_empty() => {
                let paths = std::mem::take(&mut pending);
                if output_tx.send(AppEvent::WatchReload(paths)).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Poignée de surveillance active de la collection.
pub struct CollectionWatcher {
    _watcher: notify::RecommendedWatcher,
}

/// Démarre la surveillance récursive de `root` avec regroupement des événements.
pub fn start_watcher(
    root: PathBuf,
    output_tx: mpsc::Sender<AppEvent>,
) -> notify::Result<CollectionWatcher> {
    use notify::Watcher;

    let (raw_tx, raw_rx) = mpsc::channel(EVENT_BUFFER);

    tokio::spawn(debounce_events(raw_rx, output_tx, DEBOUNCE_DURATION));

    let watched_root = root.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            for path in event.paths {
                // Chemins relatifs à la racine : les filtres ne doivent pas
                // réagir aux dossiers parents de la collection (un ancêtre
                // nommé `environments` ou `node_modules`, par exemple).
                let Ok(relative) = path.strip_prefix(&watched_root) else {
                    continue;
                };
                if is_watched_path(relative) {
                    let _ = raw_tx.blocking_send(relative.to_path_buf());
                }
            }
        }
    })?;

    watcher.watch(&root, notify::RecursiveMode::Recursive)?;

    Ok(CollectionWatcher { _watcher: watcher })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignored_paths_filter_correctly() {
        assert!(is_ignored_path(Path::new(".git/HEAD")));
        assert!(is_ignored_path(Path::new("sub/.git/config")));
        assert!(is_ignored_path(Path::new("node_modules/package/index.js")));
        assert!(is_ignored_path(Path::new("sub/node_modules/test.bru")));
        assert!(is_ignored_path(Path::new("request.bru~")));
        assert!(is_ignored_path(Path::new("request.bru.tmp")));
        assert!(is_ignored_path(Path::new("request.bru.bak")));
        assert!(is_ignored_path(Path::new("sub/request.bru.crswap")));
        assert!(is_ignored_path(Path::new(".#request.bru")));
        assert!(is_ignored_path(Path::new("#request.bru#")));
        assert!(is_ignored_path(Path::new(".request.bru.swp")));
        assert!(is_ignored_path(Path::new(".request.bru.swo")));
        assert!(is_ignored_path(Path::new(".request.bru.swx")));
        assert!(is_ignored_path(Path::new("4913")));
        assert!(is_ignored_path(Path::new("sub/4913")));

        assert!(!is_ignored_path(Path::new("request.bru")));
        assert!(!is_ignored_path(Path::new("bruno.json")));
        assert!(!is_ignored_path(Path::new("collection.bru")));
        assert!(!is_ignored_path(Path::new("folder.bru")));
        assert!(!is_ignored_path(Path::new("environments/local.bru")));
    }

    #[test]
    fn watched_paths_filter_correctly() {
        // Déclencheurs valides
        assert!(is_watched_path(Path::new("bruno.json")));
        assert!(is_watched_path(Path::new("sub/bruno.json")));
        assert!(is_watched_path(Path::new("collection.bru")));
        assert!(is_watched_path(Path::new("sub/collection.bru")));
        assert!(is_watched_path(Path::new("folder.bru")));
        assert!(is_watched_path(Path::new("sub/folder.bru")));
        assert!(is_watched_path(Path::new("test.bru")));
        assert!(is_watched_path(Path::new("sub/deep/request.bru")));
        assert!(is_watched_path(Path::new("environments/local.bru")));
        assert!(is_watched_path(Path::new("environments/custom.env")));
        assert!(is_watched_path(Path::new("sub/environments/vars.json")));

        // Non déclencheurs
        assert!(!is_watched_path(Path::new("README.md")));
        assert!(!is_watched_path(Path::new("package.json")));
        assert!(!is_watched_path(Path::new(".gitignore")));
        assert!(!is_watched_path(Path::new("notes.txt")));

        // Fichiers ignorés même s'ils ont l'extension .bru
        assert!(!is_watched_path(Path::new(".git/test.bru")));
        assert!(!is_watched_path(Path::new("node_modules/test.bru")));
        assert!(!is_watched_path(Path::new("test.bru~")));
        assert!(!is_watched_path(Path::new(".test.bru.swp")));
    }

    #[test]
    fn should_reload_checks_any_watched_path() {
        assert!(should_reload(&[
            PathBuf::from("README.md"),
            PathBuf::from("request.bru")
        ]));
        assert!(!should_reload(&[
            PathBuf::from("README.md"),
            PathBuf::from("package.json"),
            PathBuf::from(".git/config")
        ]));
        assert!(!should_reload(&[]));
    }

    #[tokio::test]
    async fn debounce_groups_burst_events() {
        let (input_tx, input_rx) = mpsc::channel(16);
        let (output_tx, mut output_rx) = mpsc::channel(16);

        tokio::spawn(debounce_events(
            input_rx,
            output_tx,
            Duration::from_millis(30),
        ));

        // Envoi d'événements rapprochés
        input_tx
            .send(PathBuf::from("first.bru"))
            .await
            .expect("send");
        tokio::time::sleep(Duration::from_millis(10)).await;

        input_tx
            .send(PathBuf::from("second.bru"))
            .await
            .expect("send");
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Doublon
        input_tx
            .send(PathBuf::from("first.bru"))
            .await
            .expect("send");

        // Rien ne doit être émis immédiatement (< 30 ms depuis le dernier)
        assert!(output_rx.try_recv().is_err());

        // On attend l'expiration du debounce (30 ms sans nouvel événement)
        tokio::time::sleep(Duration::from_millis(45)).await;

        // Un seul événement regroupé doit être reçu
        let received = output_rx.recv().await.expect("reçu");
        match received {
            AppEvent::WatchReload(paths) => {
                assert_eq!(
                    paths,
                    vec![PathBuf::from("first.bru"), PathBuf::from("second.bru")]
                );
            }
            other => panic!("attendu WatchReload, obtenu {other:?}"),
        }

        // Rien d'autre en attente
        assert!(output_rx.try_recv().is_err());
    }
}
