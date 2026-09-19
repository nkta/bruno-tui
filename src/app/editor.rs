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
}
