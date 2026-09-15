//! Lecture de `bruno.json` et découverte de la racine d'une collection.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::error::LoadError;

/// Nom du fichier qui marque la racine d'une collection Bruno.
pub const CONFIG_FILE: &str = "bruno.json";

/// Champs de `bruno.json` utiles au chargement ; les autres sont ignorés.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct BrunoConfig {
    #[serde(default)]
    pub name: Option<String>,
    /// Entrées relatives à la racine à ne pas parcourir.
    #[serde(default)]
    pub ignore: Vec<String>,
}

/// Remonte depuis `path` (fichier ou répertoire) jusqu'au premier
/// répertoire contenant `bruno.json`.
pub fn find_root(path: &Path) -> Result<PathBuf, LoadError> {
    let canonical = fs::canonicalize(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let start = if canonical.is_dir() {
        canonical.as_path()
    } else {
        canonical.parent().unwrap_or(canonical.as_path())
    };
    start
        .ancestors()
        .find(|dir| dir.join(CONFIG_FILE).is_file())
        .map(Path::to_path_buf)
        .ok_or_else(|| LoadError::NotACollection {
            path: path.to_path_buf(),
        })
}

/// Lit `bruno.json` à la racine.
pub fn read_config(root: &Path) -> Result<BrunoConfig, LoadError> {
    let path = root.join(CONFIG_FILE);
    let bytes = fs::read(&path).map_err(|source| LoadError::Io {
        path: path.clone(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|error| LoadError::InvalidConfig {
        path,
        line: error.line(),
        column: error.column(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_are_ignored() {
        let config: BrunoConfig = serde_json::from_str(
            r#"{"version":"1","name":"c","ignore":["tmp"],"scripts":{"a":1}}"#,
        )
        .expect("config valide");
        assert_eq!(config.name.as_deref(), Some("c"));
        assert_eq!(config.ignore, ["tmp"]);
    }

    #[test]
    fn missing_fields_default() {
        let config: BrunoConfig = serde_json::from_str("{}").expect("config valide");
        assert_eq!(config, BrunoConfig::default());
    }
}
