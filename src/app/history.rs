//! Persistance de l'historique des exécutions dans le répertoire d'état XDG.
//!
//! Stocke l'historique dans `$XDG_STATE_HOME/bruno-tui/history.json`
//! (repli `~/.local/state/bruno-tui/history.json`).
//! Les entrées sont associées par clé au chemin absolu de la racine de la collection.
//! Le contenu est STRICTEMENT limité aux métadonnées (horodatage, cible relative,
//! récursif, verdict et compteurs, durée) : jamais de secret, d'en-tête, d'URL
//! résolue, de console ni de corps de requête/réponse.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::model::{HISTORY_LIMIT, HistoryEntry, HistoryOutcome};

/// Entrée d'historique telle que sérialisée sur disque.
/// STRICTEMENT restreinte aux métadonnées essentielles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredHistoryEntry {
    pub started_at_epoch_ms: u64,
    pub target: PathBuf,
    pub recursive: bool,
    pub outcome: StoredHistoryOutcome,
}

/// Issue d'une exécution sérialisée.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "verdict", rename_all = "lowercase")]
pub enum StoredHistoryOutcome {
    Completed {
        total: u64,
        failed: u64,
        duration_secs: f64,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

impl From<&HistoryEntry> for StoredHistoryEntry {
    fn from(entry: &HistoryEntry) -> Self {
        let started_at_epoch_ms = entry
            .started_at
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let outcome = match &entry.outcome {
            HistoryOutcome::Completed {
                total,
                failed,
                duration_secs,
            } => StoredHistoryOutcome::Completed {
                total: *total,
                failed: *failed,
                duration_secs: *duration_secs,
            },
            HistoryOutcome::Failed(err) => StoredHistoryOutcome::Failed { error: err.clone() },
            HistoryOutcome::Cancelled => StoredHistoryOutcome::Cancelled,
        };
        Self {
            started_at_epoch_ms,
            target: entry.target.clone(),
            recursive: entry.recursive,
            outcome,
        }
    }
}

impl From<StoredHistoryEntry> for HistoryEntry {
    fn from(stored: StoredHistoryEntry) -> Self {
        let started_at = UNIX_EPOCH + Duration::from_millis(stored.started_at_epoch_ms);
        let outcome = match stored.outcome {
            StoredHistoryOutcome::Completed {
                total,
                failed,
                duration_secs,
            } => HistoryOutcome::Completed {
                total,
                failed,
                duration_secs,
            },
            StoredHistoryOutcome::Failed { error } => HistoryOutcome::Failed(error),
            StoredHistoryOutcome::Cancelled => HistoryOutcome::Cancelled,
        };
        Self {
            started_at,
            target: stored.target,
            recursive: stored.recursive,
            outcome,
        }
    }
}

/// Résout le chemin du fichier d'historique selon les spécifications XDG.
pub fn history_file_path() -> Option<PathBuf> {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME")
        && !state_home.is_empty()
    {
        return Some(
            PathBuf::from(state_home)
                .join("bruno-tui")
                .join("history.json"),
        );
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return Some(
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("bruno-tui")
                .join("history.json"),
        );
    }
    None
}

/// Calcule la clé d'identification d'une collection (chemin absolu de sa racine).
pub fn collection_key(root: &Path) -> String {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// Charge l'historique d'une collection depuis un chemin de fichier spécifié.
/// Tolérant aux corruptions : retourne une liste vide et un diagnostic le cas échéant.
pub fn load_history_from_path(
    file_path: &Path,
    root: &Path,
) -> (Vec<HistoryEntry>, Option<String>) {
    if !file_path.exists() {
        return (Vec::new(), None);
    }
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => {
            return (
                Vec::new(),
                Some(format!("impossible de lire le fichier d'historique : {e}")),
            );
        }
    };
    let map: BTreeMap<String, Vec<StoredHistoryEntry>> = match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            return (
                Vec::new(),
                Some(format!("fichier d'historique corrompu ignoré ({e})")),
            );
        }
    };
    let key = collection_key(root);
    let entries = map
        .get(&key)
        .map(|list| {
            list.iter()
                .take(HISTORY_LIMIT)
                .cloned()
                .map(HistoryEntry::from)
                .collect()
        })
        .unwrap_or_default();
    (entries, None)
}

/// Charge l'historique d'une collection depuis l'emplacement XDG standard.
pub fn load_history(root: &Path) -> (Vec<HistoryEntry>, Option<String>) {
    match history_file_path() {
        Some(path) => load_history_from_path(&path, root),
        None => (Vec::new(), None),
    }
}

/// Sauvegarde l'historique d'une collection vers un chemin de fichier spécifié.
/// Borne la liste à `HISTORY_LIMIT` (200) entrées.
pub fn save_history_to_path(
    file_path: &Path,
    root: &Path,
    entries: &[HistoryEntry],
) -> Result<(), String> {
    let mut map: BTreeMap<String, Vec<StoredHistoryEntry>> = if file_path.exists() {
        std::fs::read_to_string(file_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    let key = collection_key(root);
    let stored_entries: Vec<StoredHistoryEntry> = entries
        .iter()
        .take(HISTORY_LIMIT)
        .map(StoredHistoryEntry::from)
        .collect();

    map.insert(key, stored_entries);

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    std::fs::write(file_path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Sauvegarde l'historique d'une collection vers l'emplacement XDG standard.
pub fn save_history(root: &Path, entries: &[HistoryEntry]) -> Result<(), String> {
    match history_file_path() {
        Some(path) => save_history_to_path(&path, root, entries),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    #[test]
    fn test_conversion_roundtrip() {
        let started_at = UNIX_EPOCH + Duration::from_secs(1700000000);
        let entry = HistoryEntry {
            started_at,
            target: PathBuf::from("dossier/requete.bru"),
            recursive: true,
            outcome: HistoryOutcome::Completed {
                total: 10,
                failed: 1,
                duration_secs: 1.25,
            },
        };
        let stored = StoredHistoryEntry::from(&entry);
        assert_eq!(stored.started_at_epoch_ms, 1700000000000);
        assert_eq!(stored.target, PathBuf::from("dossier/requete.bru"));
        assert!(stored.recursive);
        assert_eq!(
            stored.outcome,
            StoredHistoryOutcome::Completed {
                total: 10,
                failed: 1,
                duration_secs: 1.25,
            }
        );

        let roundtrip = HistoryEntry::from(stored);
        assert_eq!(roundtrip, entry);
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("bruno-tui-hist-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_corrupted_history_ignored_without_panic() {
        let temp_dir = TempDir::new("corrupted");
        let file_path = temp_dir.path().join("corrupted.json");
        std::fs::write(&file_path, b"CECI N'EST PAS DU JSON VALIDE {{{").unwrap();

        let (entries, notice) = load_history_from_path(&file_path, temp_dir.path());
        assert!(entries.is_empty());
        assert!(notice.is_some());
        assert!(notice.unwrap().contains("corrompu"));
    }

    #[test]
    fn test_save_and_load_multiple_collections_bounded_to_limit() {
        let temp_dir = TempDir::new("bounded");
        let file_path = temp_dir.path().join("history.json");
        let col1 = temp_dir.path().join("col1");
        let col2 = temp_dir.path().join("col2");
        std::fs::create_dir(&col1).unwrap();
        std::fs::create_dir(&col2).unwrap();

        // Plus de 200 entrées pour tester le bornage
        let mut entries = Vec::new();
        for i in 0..250 {
            entries.push(HistoryEntry {
                started_at: UNIX_EPOCH + Duration::from_secs(i),
                target: PathBuf::from(format!("req_{i}.bru")),
                recursive: false,
                outcome: HistoryOutcome::Completed {
                    total: 1,
                    failed: 0,
                    duration_secs: 0.1,
                },
            });
        }

        save_history_to_path(&file_path, &col1, &entries).unwrap();

        let (loaded_col1, notice1) = load_history_from_path(&file_path, &col1);
        assert!(notice1.is_none());
        assert_eq!(loaded_col1.len(), HISTORY_LIMIT);
        assert_eq!(loaded_col1[0].target, PathBuf::from("req_0.bru"));

        // Sauvegarder pour col2 ne doit pas écraser col1
        let col2_entries = vec![HistoryEntry {
            started_at: SystemTime::now(),
            target: PathBuf::from("other.bru"),
            recursive: true,
            outcome: HistoryOutcome::Cancelled,
        }];
        save_history_to_path(&file_path, &col2, &col2_entries).unwrap();

        let (loaded_col1_after, _) = load_history_from_path(&file_path, &col1);
        assert_eq!(loaded_col1_after.len(), HISTORY_LIMIT);

        let (loaded_col2, notice2) = load_history_from_path(&file_path, &col2);
        assert!(notice2.is_none());
        assert_eq!(loaded_col2.len(), 1);
        assert_eq!(loaded_col2[0].outcome, HistoryOutcome::Cancelled);
    }

    #[test]
    fn test_history_contains_strictly_no_sensitive_data() {
        let temp_dir = TempDir::new("audit");
        let file_path = temp_dir.path().join("history_audit.json");
        let col = temp_dir.path().join("audit_col");
        std::fs::create_dir(&col).unwrap();

        let entries = vec![
            HistoryEntry {
                started_at: UNIX_EPOCH + Duration::from_secs(1700000000),
                target: PathBuf::from("users/get.bru"),
                recursive: false,
                outcome: HistoryOutcome::Completed {
                    total: 1,
                    failed: 0,
                    duration_secs: 0.25,
                },
            },
            HistoryEntry {
                started_at: UNIX_EPOCH + Duration::from_secs(1700000005),
                target: PathBuf::from("auth/login.bru"),
                recursive: false,
                outcome: HistoryOutcome::Failed("serveur injoignable".to_owned()),
            },
            HistoryEntry {
                started_at: UNIX_EPOCH + Duration::from_secs(1700000010),
                target: PathBuf::from("all"),
                recursive: true,
                outcome: HistoryOutcome::Cancelled,
            },
        ];

        save_history_to_path(&file_path, &col, &entries).unwrap();

        let json_text = std::fs::read_to_string(&file_path).unwrap();
        let json_val: serde_json::Value = serde_json::from_str(&json_text).unwrap();

        // Mots-clés sensibles interdits
        let forbidden_keywords = [
            "body",
            "headers",
            "header",
            "url",
            "secret",
            "variable",
            "variables",
            "console",
            "stdout",
            "stderr",
            "token",
            "password",
            "auth",
            "data",
            "cookie",
            "bearer",
        ];

        // Clés JSON explicitement autorisées
        let allowed_keys = [
            "started_at_epoch_ms",
            "target",
            "recursive",
            "outcome",
            "verdict",
            "total",
            "failed",
            "duration_secs",
            "error",
        ];

        fn check_json_node(
            val: &serde_json::Value,
            forbidden: &[&str],
            allowed: &[&str],
            is_root_map: bool,
        ) {
            match val {
                serde_json::Value::Object(map) => {
                    for (k, v) in map {
                        if !is_root_map {
                            assert!(
                                allowed.contains(&k.as_str()),
                                "Clé JSON inattendue trouvée dans l'historique : {k}"
                            );
                            for f in forbidden {
                                assert!(
                                    !k.to_ascii_lowercase().contains(f),
                                    "Clé interdite {f} détectée dans {k}"
                                );
                            }
                        }
                        check_json_node(v, forbidden, allowed, false);
                    }
                }
                serde_json::Value::Array(list) => {
                    for item in list {
                        check_json_node(item, forbidden, allowed, false);
                    }
                }
                serde_json::Value::String(s) => {
                    let lower = s.to_ascii_lowercase();
                    // S'assurer que le contenu des valeurs ne fuit pas de tokens ou mots de passe
                    assert!(
                        !lower.contains("bearer") && !lower.contains("password"),
                        "Chaîne suspecte dans l'historique : {s}"
                    );
                }
                _ => {}
            }
        }

        check_json_node(&json_val, &forbidden_keywords, &allowed_keys, true);
    }
}
