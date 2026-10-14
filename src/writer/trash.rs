//! Corbeille freedesktop.org (« Trash specification » 1.0), sans dépendance.
//!
//! Corbeille de l'utilisateur seulement : `$XDG_DATA_HOME/Trash` (par
//! défaut `~/.local/share/Trash`), avec `files/` et `info/<nom>.trashinfo`.
//! Un nom déjà pris reçoit le suffixe `.2`, `.3`… (comme `g_file_trash`).
//! Un fichier situé sur un autre système de fichiers que la corbeille est
//! refusé : jamais d'effacement définitif, ni en repli ni en cas d'erreur.
//! Le seul fichier jamais supprimé ici est le `.trashinfo` que l'on vient
//! de créer, quand le déplacement échoue.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use thiserror::Error;

/// Nombre maximal de noms essayés dans la corbeille.
const MAX_ATTEMPTS: u32 = 10_000;

/// Erreur de mise à la corbeille. L'élément d'origine est toujours intact.
#[derive(Debug, Error)]
pub enum TrashError {
    #[error("Impossible de mettre à la corbeille : autre disque")]
    OtherDevice,
    #[error("Impossible de mettre à la corbeille : ni XDG_DATA_HOME ni HOME")]
    NoHome,
    #[error("Impossible de mettre à la corbeille : nom libre introuvable")]
    NoFreeName,
    #[error("Impossible de mettre à la corbeille {path} : {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> TrashError + '_ {
    move |source| TrashError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Corbeille de l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trash {
    /// Répertoire `Trash` (parent de `files/` et `info/`).
    dir: PathBuf,
}

impl Trash {
    /// Corbeille située dans `dir` (le répertoire `Trash` lui-même).
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Corbeille de l'utilisateur d'après l'environnement du processus.
    pub fn from_env() -> Result<Self, TrashError> {
        Self::from_env_values(std::env::var_os("XDG_DATA_HOME"), std::env::var_os("HOME"))
    }

    /// `$XDG_DATA_HOME/Trash`, sinon `$HOME/.local/share/Trash`. Une valeur
    /// vide ou relative de `XDG_DATA_HOME` est ignorée, comme le prévoit la
    /// spécification XDG.
    pub fn from_env_values(
        xdg_data_home: Option<OsString>,
        home: Option<OsString>,
    ) -> Result<Self, TrashError> {
        if let Some(xdg) = xdg_data_home.map(PathBuf::from)
            && xdg.is_absolute()
        {
            return Ok(Self::at(xdg.join("Trash")));
        }
        match home.map(PathBuf::from) {
            Some(home) if home.is_absolute() => {
                Ok(Self::at(home.join(".local").join("share").join("Trash")))
            }
            _ => Err(TrashError::NoHome),
        }
    }

    /// Répertoire `Trash`.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Met `path` (fichier, lien ou dossier) à la corbeille. `deletion_date`
    /// est l'heure locale au format `AAAA-MM-JJThh:mm:ss`. Retourne le
    /// chemin de l'élément dans `files/`.
    pub fn put(&self, path: &Path, deletion_date: &str) -> Result<PathBuf, TrashError> {
        let original = absolute_original(path)?;
        let meta = fs::symlink_metadata(&original).map_err(io_error(&original))?;

        let files = self.dir.join("files");
        let info = self.dir.join("info");
        create_private_dir(&self.dir)?;
        create_private_dir(&files)?;
        create_private_dir(&info)?;

        let files_meta = fs::metadata(&files).map_err(io_error(&files))?;
        if files_meta.dev() != meta.dev() {
            return Err(TrashError::OtherDevice);
        }

        let base = original
            .file_name()
            .map(|name| name.to_os_string())
            .ok_or_else(|| TrashError::Io {
                path: original.clone(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "nom de fichier absent"),
            })?;
        let content = format!(
            "[Trash Info]\nPath={}\nDeletionDate={deletion_date}\n",
            encode_path(&original)
        );

        for attempt in 1..=MAX_ATTEMPTS {
            let mut name = base.clone();
            if attempt > 1 {
                name.push(format!(".{attempt}"));
            }
            let target = files.join(&name);
            if fs::symlink_metadata(&target).is_ok() {
                continue;
            }
            let mut info_name = name.clone();
            info_name.push(".trashinfo");
            let info_path = info.join(info_name);
            let mut file = match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&info_path)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(io_error(&info_path)(error)),
            };
            let written = file
                .write_all(content.as_bytes())
                .and_then(|()| file.sync_all());
            drop(file);
            if let Err(error) = written {
                let _ = fs::remove_file(&info_path);
                return Err(io_error(&info_path)(error));
            }
            if let Err(error) = fs::rename(&original, &target) {
                let _ = fs::remove_file(&info_path);
                if error.kind() == io::ErrorKind::CrossesDevices {
                    return Err(TrashError::OtherDevice);
                }
                return Err(io_error(&original)(error));
            }
            return Ok(target);
        }
        Err(TrashError::NoFreeName)
    }
}

/// Chemin absolu de l'élément, parent résolu (liens compris) mais nom final
/// gardé tel quel : un lien symbolique part à la corbeille, pas sa cible.
fn absolute_original(path: &Path) -> Result<PathBuf, TrashError> {
    let name = path.file_name().ok_or_else(|| TrashError::Io {
        path: path.to_path_buf(),
        source: io::Error::new(io::ErrorKind::InvalidInput, "nom de fichier absent"),
    })?;
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let parent = fs::canonicalize(parent).map_err(io_error(parent))?;
    Ok(parent.join(name))
}

/// Crée un répertoire de la corbeille, droits `0700`, s'il manque.
fn create_private_dir(dir: &Path) -> Result<(), TrashError> {
    match fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io_error(dir)),
        Err(error) => Err(io_error(dir)(error)),
    }
}

/// Encodage URL de `Path=` (RFC 2396) : tout octet hors caractères non
/// réservés et `/` devient `%XX`.
fn encode_path(path: &Path) -> String {
    let mut out = String::new();
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workdir(tag: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("tmp-trash-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("répertoire de test");
        dir
    }

    const DATE: &str = "2026-10-14T09:30:00";

    #[test]
    fn env_values_follow_xdg() {
        assert_eq!(
            Trash::from_env_values(Some("/data".into()), Some("/home/u".into())).expect("xdg"),
            Trash::at("/data/Trash")
        );
        assert_eq!(
            Trash::from_env_values(Some("relatif".into()), Some("/home/u".into())).expect("home"),
            Trash::at("/home/u/.local/share/Trash")
        );
        assert_eq!(
            Trash::from_env_values(Some("".into()), Some("/home/u".into())).expect("home"),
            Trash::at("/home/u/.local/share/Trash")
        );
        assert!(matches!(
            Trash::from_env_values(None, None),
            Err(TrashError::NoHome)
        ));
    }

    #[test]
    fn path_is_url_encoded() {
        assert_eq!(
            encode_path(Path::new("/a b/é%/x-y_z.bru")),
            "/a%20b/%C3%A9%25/x-y_z.bru"
        );
    }

    #[test]
    fn file_goes_to_trash_with_a_valid_trashinfo() {
        let dir = workdir("file");
        let data = dir.join("data");
        let trash = Trash::at(data.join("Trash"));
        let collection = dir.join("ma collection");
        fs::create_dir_all(&collection).expect("collection");
        let file = collection.join("get post.bru");
        fs::write(&file, "meta {\n}\n").expect("fichier");

        let trashed = trash.put(&file, DATE).expect("corbeille");
        assert!(!file.exists());
        assert_eq!(trashed, data.join("Trash/files/get post.bru"));
        assert_eq!(fs::read_to_string(&trashed).expect("copie"), "meta {\n}\n");

        let info =
            fs::read_to_string(data.join("Trash/info/get post.bru.trashinfo")).expect("trashinfo");
        let expected_path = encode_path(
            &fs::canonicalize(&collection)
                .expect("abs")
                .join("get post.bru"),
        );
        assert_eq!(
            info,
            format!("[Trash Info]\nPath={expected_path}\nDeletionDate={DATE}\n")
        );
        assert!(info.contains("ma%20collection/get%20post.bru"));

        let mode = fs::metadata(data.join("Trash")).expect("trash").mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn name_collisions_get_numeric_suffixes() {
        let dir = workdir("collision");
        let trash = Trash::at(dir.join("Trash"));
        for round in 1..=3 {
            let file = dir.join("a.bru");
            fs::write(&file, format!("{round}")).expect("fichier");
            trash.put(&file, DATE).expect("corbeille");
        }
        let files = dir.join("Trash/files");
        assert_eq!(fs::read_to_string(files.join("a.bru")).expect("1"), "1");
        assert_eq!(fs::read_to_string(files.join("a.bru.2")).expect("2"), "2");
        assert_eq!(fs::read_to_string(files.join("a.bru.3")).expect("3"), "3");
        assert!(dir.join("Trash/info/a.bru.2.trashinfo").is_file());
        assert!(dir.join("Trash/info/a.bru.3.trashinfo").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_leftover_trashinfo_reserves_its_name() {
        let dir = workdir("leftover");
        let trash = Trash::at(dir.join("Trash"));
        fs::create_dir_all(dir.join("Trash/info")).expect("info");
        fs::write(dir.join("Trash/info/a.bru.trashinfo"), "x").expect("orphelin");
        let file = dir.join("a.bru");
        fs::write(&file, "a").expect("fichier");
        let trashed = trash.put(&file, DATE).expect("corbeille");
        assert_eq!(trashed, dir.join("Trash/files/a.bru.2"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn folder_goes_to_trash_whole() {
        let dir = workdir("folder");
        let trash = Trash::at(dir.join("Trash"));
        let folder = dir.join("Posts");
        fs::create_dir_all(folder.join("sub")).expect("dossier");
        fs::write(folder.join("sub/a.bru"), "a").expect("fichier");
        let trashed = trash.put(&folder, DATE).expect("corbeille");
        assert!(!folder.exists());
        assert_eq!(
            fs::read_to_string(trashed.join("sub/a.bru")).expect("copie"),
            "a"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn another_filesystem_is_refused_and_nothing_is_deleted() {
        let dir = workdir("device");
        let trash = Trash::at(dir.join("Trash"));
        let shm = Path::new("/dev/shm");
        let same_device = fs::metadata(shm)
            .ok()
            .zip(fs::metadata(&dir).ok())
            .is_none_or(|(a, b)| a.dev() == b.dev());
        if same_device {
            // Pas de second système de fichiers disponible sur cette machine.
            let _ = fs::remove_dir_all(&dir);
            return;
        }
        let file = shm.join(format!("bruno-tui-trash-test-{}.bru", std::process::id()));
        fs::write(&file, "garde-moi").expect("fichier");
        let error = trash.put(&file, DATE).expect_err("refus");
        assert!(matches!(error, TrashError::OtherDevice));
        assert_eq!(
            error.to_string(),
            "Impossible de mettre à la corbeille : autre disque"
        );
        assert_eq!(fs::read_to_string(&file).expect("intact"), "garde-moi");
        let leftovers = fs::read_dir(dir.join("Trash/info")).expect("info").count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_file(&file);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_item_is_an_error_without_side_effect() {
        let dir = workdir("missing");
        let trash = Trash::at(dir.join("Trash"));
        assert!(matches!(
            trash.put(&dir.join("absent.bru"), DATE),
            Err(TrashError::Io { .. })
        ));
        assert!(
            !dir.join("Trash/info").exists()
                || fs::read_dir(dir.join("Trash/info")).expect("info").count() == 0
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
