//! Lecture de `bruno.json` et découverte de la racine d'une collection.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::error::LoadError;
use super::opencollection::COLLECTION_FILE as OPENCOLLECTION_FILE;
use super::tree::CollectionFormat;

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
    /// Bloc `presets`, lu sans rien exiger : un type inattendu ne doit pas
    /// empêcher de charger la collection.
    #[serde(default)]
    pub presets: serde_json::Value,
}

/// Préréglages de nouvelle requête de `bruno.json` (`presets.requestType`
/// et `presets.requestUrl`), que le TUI applique au popup `a`
/// (`docs/reglages-collection.md`, D6). Jamais résolus : `{{variable}}`
/// reste une chaîne.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Presets {
    pub request_type: Option<String>,
    pub request_url: Option<String>,
}

impl BrunoConfig {
    /// Préréglages lisibles ; toute valeur qui n'est pas une chaîne est ignorée.
    pub fn request_presets(&self) -> Presets {
        let text = |key: &str| {
            self.presets
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        Presets {
            request_type: text("requestType"),
            request_url: text("requestUrl"),
        }
    }
}

/// Remonte depuis `path` (fichier ou répertoire) jusqu'au premier
/// répertoire contenant `bruno.json`.
pub fn find_root(path: &Path) -> Result<PathBuf, LoadError> {
    find_ancestor(path, |dir| dir.join(CONFIG_FILE).is_file())
}

/// Remonte depuis `path` jusqu'au premier répertoire contenant
/// `opencollection.yml`.
pub fn find_opencollection_root(path: &Path) -> Result<PathBuf, LoadError> {
    find_ancestor(path, |dir| dir.join(OPENCOLLECTION_FILE).is_file())
}

/// Remonte depuis `path` jusqu'au premier répertoire qui marque une
/// collection, et en détecte le format comme Bruno 4
/// (`getCollectionFormat`) : `opencollection.yml` prime sur `bruno.json`.
pub fn detect_format(path: &Path) -> Result<(PathBuf, CollectionFormat), LoadError> {
    let root = find_ancestor(path, |dir| {
        dir.join(OPENCOLLECTION_FILE).is_file() || dir.join(CONFIG_FILE).is_file()
    })?;
    let format = if root.join(OPENCOLLECTION_FILE).is_file() {
        CollectionFormat::OpenCollection
    } else {
        CollectionFormat::Bru
    };
    Ok((root, format))
}

fn find_ancestor(path: &Path, is_root: impl Fn(&Path) -> bool) -> Result<PathBuf, LoadError> {
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
        .find(|dir| is_root(dir))
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
    fn presets_are_read_leniently() {
        let config: BrunoConfig = serde_json::from_str(
            r#"{"presets":{"requestType":"graphql","requestUrl":"{{base}}"}}"#,
        )
        .expect("config valide");
        assert_eq!(
            config.request_presets(),
            Presets {
                request_type: Some("graphql".into()),
                request_url: Some("{{base}}".into()),
            }
        );
        let odd: BrunoConfig =
            serde_json::from_str(r#"{"presets":{"requestType":3,"requestUrl":null}}"#)
                .expect("config valide");
        assert_eq!(odd.request_presets(), Presets::default());
        let wrong: BrunoConfig = serde_json::from_str(r#"{"presets":"x"}"#).expect("valide");
        assert_eq!(wrong.request_presets(), Presets::default());
    }

    #[test]
    fn missing_fields_default() {
        let config: BrunoConfig = serde_json::from_str("{}").expect("config valide");
        assert_eq!(config, BrunoConfig::default());
    }
}
