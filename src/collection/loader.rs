//! Abstraction de chargement d'une collection, indépendante du format.

use std::path::Path;

use super::config::{find_root, read_config};
use super::error::LoadError;
use super::tree::{Collection, Ignore, load_settings, load_tree};

/// Charge une collection depuis un chemin.
///
/// Le chargement est synchrone et fait des lectures disque : l'appelant
/// l'exécute hors de la boucle d'événements (`tokio::task::spawn_blocking`).
pub trait CollectionLoader: Send + Sync {
    /// `path` peut désigner la racine ou n'importe quel fichier ou dossier
    /// de la collection.
    fn load(&self, path: &Path) -> Result<Collection, LoadError>;
}

/// Loader des collections Bruno au format `.bru`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BruLoader;

impl CollectionLoader for BruLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError> {
        let root = find_root(path)?;
        let config = read_config(&root)?;
        let (tree, environments) = load_tree(&root, &Ignore::new(&config.ignore))?;
        let name = config.name.unwrap_or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        Ok(Collection {
            settings: load_settings(&root),
            root,
            name,
            tree,
            environments,
        })
    }
}
