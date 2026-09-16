//! Écriture d'une modification de champ vers un fichier `.bru` de requête
//! déjà chargé.
//!
//! Ne réécrit que les tranches de source correspondant aux champs édités ;
//! le reste du fichier — blocs non touchés, entrées voisines, blocs
//! inconnus, ordre, fins de ligne — est réémis à l'octet près depuis les
//! tranches déjà conservées par l'AST de `bru-parser`. Écriture atomique
//! (fichier temporaire dans le même répertoire, puis renommage), permissions
//! Unix d'origine préservées, refus explicite si le fichier a changé sur
//! disque depuis son chargement. Ce module ne journalise et ne réimplémente
//! rien de `bru-parser` : il consomme son AST tel quel et ne le modifie pas.

mod edit;
mod error;
mod format;

pub use edit::FieldEdit;
pub use error::{EditError, WriteError};

use std::fs;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use crate::collection::BruFile;

/// Instantané de fraîcheur d'un fichier (taille et date de modification),
/// capturé au chargement et comparé avant chaque écriture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStamp {
    len: u64,
    modified: SystemTime,
}

impl FileStamp {
    /// Capture la taille et la date de modification actuelles de `path`.
    pub fn capture(path: &Path) -> io::Result<Self> {
        let meta = fs::metadata(path)?;
        Ok(Self {
            len: meta.len(),
            modified: meta.modified()?,
        })
    }
}

/// Écriture d'une modification de requête, indépendante du format source.
pub trait RequestWriter: Send + Sync {
    /// Écrit `edits` dans le fichier `.bru` situé à `path`, dont l'AST
    /// `ast` a été chargé avec l'instantané `stamp`. Retourne le nouvel
    /// instantané en cas de succès.
    fn write_request(
        &self,
        path: &Path,
        ast: &BruFile,
        stamp: &FileStamp,
        edits: &[FieldEdit],
    ) -> Result<FileStamp, WriteError>;
}

/// Écrivain des fichiers `.bru`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BruWriter;

/// Distingue les fichiers temporaires de deux écritures rapprochées du même
/// processus sur le même fichier.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

impl RequestWriter for BruWriter {
    fn write_request(
        &self,
        path: &Path,
        ast: &BruFile,
        stamp: &FileStamp,
        edits: &[FieldEdit],
    ) -> Result<FileStamp, WriteError> {
        let io_err = |source: io::Error| WriteError::Io {
            path: path.to_path_buf(),
            source,
        };

        let current = FileStamp::capture(path).map_err(io_err)?;
        if current != *stamp {
            return Err(WriteError::Stale {
                path: path.to_path_buf(),
            });
        }

        let bytes = format::serialize(ast, edits)?;
        let permissions = fs::metadata(path).map_err(io_err)?.permissions();

        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temp_path = dir.join(format!(".{file_name}.tmp-{}-{counter}", std::process::id()));

        let result = fs::write(&temp_path, &bytes)
            .and_then(|()| fs::set_permissions(&temp_path, permissions))
            .and_then(|()| fs::rename(&temp_path, path));
        if let Err(source) = result {
            let _ = fs::remove_file(&temp_path);
            return Err(io_err(source));
        }

        FileStamp::capture(path).map_err(io_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workdir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("bruno-tui-writer-mod-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("répertoire de test");
        dir
    }

    #[test]
    fn stamp_is_stable_and_changes_after_write() {
        let dir = workdir("stamp");
        let path = dir.join("a.bru");
        fs::write(&path, "get {\n  url: http://a\n}\n").expect("écriture initiale");

        let first = FileStamp::capture(&path).expect("instantané");
        let second = FileStamp::capture(&path).expect("instantané");
        assert_eq!(first, second);

        // Une nouvelle écriture change au moins la taille.
        fs::write(&path, "get {\n  url: http://a-much-longer-url\n}\n").expect("réécriture");
        let third = FileStamp::capture(&path).expect("instantané");
        assert_ne!(first, third);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_request_updates_url_and_returns_fresh_stamp() {
        let dir = workdir("write");
        let path = dir.join("a.bru");
        fs::write(&path, "get {\n  url: http://a\n}\n").expect("écriture initiale");

        let ast = BruFile::parse(fs::read_to_string(&path).expect("lecture")).expect("AST");
        let stamp = FileStamp::capture(&path).expect("instantané");

        let new_stamp = BruWriter
            .write_request(&path, &ast, &stamp, &[FieldEdit::Url("http://b".into())])
            .expect("écriture");

        let rewritten = fs::read_to_string(&path).expect("relecture");
        assert_eq!(rewritten, "get {\n  url: http://b\n}\n");
        assert_eq!(new_stamp, FileStamp::capture(&path).expect("instantané"));

        let entries: Vec<_> = fs::read_dir(&dir)
            .expect("répertoire")
            .map(|entry| entry.expect("entrée").file_name())
            .collect();
        assert_eq!(entries, [std::ffi::OsString::from("a.bru")]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_request_refuses_a_stale_stamp() {
        let dir = workdir("stale");
        let path = dir.join("a.bru");
        fs::write(&path, "get {\n  url: http://a\n}\n").expect("écriture initiale");

        let ast = BruFile::parse(fs::read_to_string(&path).expect("lecture")).expect("AST");
        let stamp = FileStamp::capture(&path).expect("instantané");

        // Modification par un tiers après la capture de l'instantané.
        fs::write(&path, "get {\n  url: http://third-party\n}\n").expect("tiers");

        let error = BruWriter
            .write_request(&path, &ast, &stamp, &[FieldEdit::Url("http://b".into())])
            .expect_err("refus attendu");
        assert!(matches!(error, WriteError::Stale { .. }));
        assert_eq!(
            fs::read_to_string(&path).expect("relecture"),
            "get {\n  url: http://third-party\n}\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
