//! Description d'une exécution demandée et construction des arguments `bru`.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

/// Chemin passé à `--reporter-json` : le fd 3 de l'enfant est l'extrémité
/// écriture d'un pipe anonyme, le rapport ne touche jamais le disque.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) const REPORT_FD: i32 = 3;
const REPORT_PATH: &str = "/dev/fd/3";

/// Valeur sensible masquée dans `Debug` et `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Accès explicite à la valeur ; ne jamais la formater dans un log.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Ajoute un caractère, pour une saisie qui ne passe jamais en clair.
    pub fn push(&mut self, c: char) {
        self.0.push(c);
    }

    /// Retire le dernier caractère, s'il y en a un.
    pub fn pop(&mut self) {
        self.0.pop();
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Nombre de caractères, pour un rendu masqué pendant la saisie.
    pub fn char_count(&self) -> usize {
        self.0.chars().count()
    }
}

impl Default for SecretString {
    fn default() -> Self {
        Self::new(String::new())
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Exécution demandée par l'appelant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    /// Racine de la collection (répertoire contenant `bruno.json`),
    /// utilisée comme répertoire de travail de `bru`.
    pub collection_root: PathBuf,
    /// Requêtes ou dossiers relatifs à la racine ; vide = collection entière.
    pub targets: Vec<PathBuf>,
    pub recursive: bool,
    /// Nom de l'environnement (`--env`).
    pub env: Option<String>,
    /// Surcharges de variables (`--env-var nom=valeur`).
    pub env_vars: Vec<(String, SecretString)>,
}

impl RunRequest {
    /// Exécution de toute la collection, en récursif, sans environnement.
    pub fn collection(collection_root: impl Into<PathBuf>) -> Self {
        Self {
            collection_root: collection_root.into(),
            targets: Vec::new(),
            recursive: true,
            env: None,
            env_vars: Vec::new(),
        }
    }

    /// Arguments passés au programme `bru`, sous-commande comprise.
    ///
    /// Contient les valeurs secrètes en clair : ne jamais les formater.
    pub(crate) fn bru_args(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["run".into()];
        args.extend(self.targets.iter().map(|target| target.into()));
        if self.recursive {
            args.push("-r".into());
        }
        if let Some(env) = &self.env {
            args.push("--env".into());
            args.push(env.into());
        }
        for (name, value) in &self.env_vars {
            args.push("--env-var".into());
            args.push(format!("{name}={}", value.expose()).into());
        }
        args.push("--reporter-json".into());
        args.push(REPORT_PATH.into());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(request: &RunRequest) -> Vec<String> {
        request
            .bru_args()
            .into_iter()
            .map(|arg| arg.into_string().expect("argument UTF-8"))
            .collect()
    }

    #[test]
    fn whole_collection() {
        let request = RunRequest::collection("/c");
        assert_eq!(
            args(&request),
            ["run", "-r", "--reporter-json", "/dev/fd/3"]
        );
    }

    #[test]
    fn recursive_folder_with_env() {
        let request = RunRequest {
            targets: vec!["users".into()],
            env: Some("local".into()),
            ..RunRequest::collection("/c")
        };
        assert_eq!(
            args(&request),
            [
                "run",
                "users",
                "-r",
                "--env",
                "local",
                "--reporter-json",
                "/dev/fd/3"
            ]
        );
    }

    #[test]
    fn several_targets_and_env_vars() {
        let request = RunRequest {
            targets: vec!["a.bru".into(), "folder".into()],
            recursive: false,
            env_vars: vec![
                ("token".into(), SecretString::new("s3cr3t")),
                ("host".into(), SecretString::new("x=y")),
            ],
            ..RunRequest::collection("/c")
        };
        assert_eq!(
            args(&request),
            [
                "run",
                "a.bru",
                "folder",
                "--env-var",
                "token=s3cr3t",
                "--env-var",
                "host=x=y",
                "--reporter-json",
                "/dev/fd/3"
            ]
        );
    }

    #[test]
    fn secret_buffer_editing() {
        let mut secret = SecretString::default();
        assert!(secret.is_empty());
        secret.push('a');
        secret.push('é');
        assert_eq!(secret.char_count(), 2);
        secret.pop();
        assert_eq!(secret.expose(), "a");
        secret.pop();
        secret.pop();
        assert!(secret.is_empty());
        assert!(!format!("{secret:?}").contains('a'));
    }

    #[test]
    fn secrets_are_masked_in_debug_and_display() {
        let secret = SecretString::new("s3cr3t");
        let request = RunRequest {
            env_vars: vec![("token".into(), secret.clone())],
            ..RunRequest::collection("/c")
        };
        assert!(!format!("{request:?}").contains("s3cr3t"));
        assert!(!format!("{secret}").contains("s3cr3t"));
        assert!(!format!("{secret:?}").contains("s3cr3t"));
    }
}
