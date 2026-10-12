//! Gestion du lancement d'un éditeur externe pour la consultation de réponse.
//!
//! Écrit le corps brut dans un fichier temporaire, lance l'éditeur en lui
//! cédant l'entrée et la sortie standard, et garantit la suppression du
//! fichier temporaire dès la fin du processus.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Compteur atomique pour garantir l'unicité des noms de fichiers temporaires.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Gardien RAII garantissant la suppression d'un fichier temporaire à sa
/// sortie de portée, quelle que soit l'issue de l'opération (succès, échec
/// de lancement, panique).
pub struct TempFileGuard(pub PathBuf);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Génère un chemin de fichier temporaire non prévisible dans le répertoire
/// temporaire du système.
pub fn temp_response_path() -> PathBuf {
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("bruno-response-{pid}-{nanos}-{counter}.txt"))
}

/// Découpe une chaîne de commande en binaire et arguments, en préservant
/// les espaces contenus entre guillemets simples ou doubles.
pub fn parse_command_line(cmd: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for c in cmd.chars() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Écrit `text` dans `path` et lance `program` avec ce fichier en argument,
/// stdio hérité. Le fichier est garanti supprimé quoi qu'il arrive.
pub fn spawn_external_editor_with_path(program: &str, text: &str, path: PathBuf) -> io::Result<()> {
    let _guard = TempFileGuard(path.clone());
    std::fs::write(&path, text.as_bytes())?;

    let args = parse_command_line(program);
    let Some(bin) = args.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "programme vide",
        ));
    };

    let mut cmd = std::process::Command::new(bin);
    cmd.args(&args[1..]);
    cmd.arg(&path);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let _status = cmd.status()?;
    Ok(())
}

/// Écrit `text` dans un fichier temporaire non prévisible et lance `program`.
pub fn spawn_external_editor(program: &str, text: &str) -> io::Result<()> {
    spawn_external_editor_with_path(program, text, temp_response_path())
}

/// Retourne le répertoire temporaire privé pour les fichiers d'édition :
/// `$XDG_RUNTIME_DIR` s'il existe et est un répertoire, sinon le répertoire
/// temporaire du système.
pub fn secure_temp_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// Génère un chemin de fichier temporaire non prévisible avec l'extension demandée
/// dans le répertoire temporaire privé (`$XDG_RUNTIME_DIR` ou temp_dir).
pub fn temp_edit_path(extension: &str) -> PathBuf {
    let pid = std::process::id();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let ext = extension.trim_start_matches('.');
    secure_temp_dir().join(format!("bruno-edit-{pid}-{nanos}-{counter}.{ext}"))
}

#[cfg(unix)]
pub fn write_secure_file(path: &std::path::Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

#[cfg(not(unix))]
pub fn write_secure_file(path: &std::path::Path, text: &str) -> io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(text.as_bytes())
}

/// Écrit `text` dans un fichier temporaire privé (droits 0600), lance `program`
/// sur ce fichier en lui cédant stdio, puis relit le contenu modifié à la fermeture.
/// Le fichier temporaire est garanti supprimé quoi qu'il arrive.
pub fn edit_external_text(program: &str, text: &str, extension: &str) -> io::Result<String> {
    edit_external_text_with_path(program, text, temp_edit_path(extension))
}

/// Variante avec chemin explicite, utile pour les tests.
pub fn edit_external_text_with_path(
    program: &str,
    text: &str,
    path: PathBuf,
) -> io::Result<String> {
    let _guard = TempFileGuard(path.clone());
    write_secure_file(&path, text)?;

    let args = parse_command_line(program);
    let Some(bin) = args.first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "programme vide",
        ));
    };

    let mut cmd = std::process::Command::new(bin);
    cmd.args(&args[1..]);
    cmd.arg(&path);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let status = cmd.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "l'éditeur a quitté avec le statut {status}"
        )));
    }

    let bytes = std::fs::read(&path)?;
    let modified = String::from_utf8(bytes).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("contenu non UTF-8 : {e}"),
        )
    })?;
    Ok(modified)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_line_cases() {
        assert_eq!(parse_command_line("nano"), vec!["nano"]);
        assert_eq!(parse_command_line("code -w"), vec!["code", "-w"]);
        assert_eq!(parse_command_line("subl -w -n"), vec!["subl", "-w", "-n"]);
        assert_eq!(
            parse_command_line("vim 'mon fichier' \"autre arg\""),
            vec!["vim", "mon fichier", "autre arg"]
        );
        assert!(parse_command_line("   ").is_empty());
    }

    #[test]
    fn temp_file_deleted_even_when_editor_fails_to_launch() {
        let path = temp_response_path();
        assert!(!path.exists());
        let result = spawn_external_editor_with_path(
            "non_existent_binary_xyz_12345",
            "contenu test",
            path.clone(),
        );
        assert!(result.is_err());
        assert!(
            !path.exists(),
            "Le fichier temporaire doit être supprimé même si le binaire n'existe pas"
        );
    }

    #[test]
    fn secure_edit_file_permissions_and_cleanup() {
        let path = temp_edit_path("js");
        assert!(!path.exists());
        assert!(path.to_string_lossy().ends_with(".js"));

        // Test d'écriture et vérification des droits 0600
        write_secure_file(&path, "const a = 1;").expect("écriture sécurisée");
        let _guard = TempFileGuard(path.clone());
        assert!(path.exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::metadata(&path).expect("metadata");
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "droits 0600 attendus, obtenu {mode:o}");
        }

        drop(_guard);
        assert!(!path.exists(), "le fichier temporaire doit être supprimé");
    }

    #[test]
    fn edit_external_text_successful_round_trip() {
        let path = temp_edit_path("js");
        let script = "sh -c 'echo \"console.log(42);\" > \"$1\"' --";
        let result = edit_external_text_with_path(script, "initial", path.clone())
            .expect("édition externe réussie");
        assert_eq!(result.trim(), "console.log(42);");
        assert!(!path.exists(), "le fichier temporaire doit être nettoyé");
    }

    #[test]
    fn edit_external_text_fails_and_cleans_up() {
        let path = temp_edit_path("md");
        let result = edit_external_text_with_path("non_existent_editor_xyz", "init", path.clone());
        assert!(result.is_err());
        assert!(
            !path.exists(),
            "le fichier temporaire doit être nettoyé même en cas d'erreur"
        );
    }
}
