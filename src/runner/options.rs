//! Options d'exécution de requêtes et campagnes (`RunnerOptions`).
//!
//! Permet de configurer les filtres par tags, le délai entre requêtes,
//! l'arrêt anticipé (`--bail`), le filtrage sur tests seuls, ainsi que
//! l'export des rapports JUnit et HTML délégués à `bru run`.

use std::path::{Path, PathBuf};

/// Erreurs de validation des options d'exécution.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RunnerOptionsError {
    #[error("le tag '{0}' ne doit pas contenir d'espace")]
    TagContainsWhitespace(String),
    #[error("le délai '{0}' doit être un entier positif (en ms)")]
    InvalidDelay(String),
    #[error("le fichier '{0}' n'existe pas")]
    FileNotFound(String),
    #[error("--ignore-truststore ne peut être utilisé qu'avec --cacert")]
    IgnoreTruststoreWithoutCacert,
}

/// Options effectives transmises à [`crate::runner::RunRequest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerOptions {
    /// Tags à inclure (`--tags t1,t2`).
    pub tags: Vec<String>,
    /// Tags à exclure (`--exclude-tags t1,t2`).
    pub exclude_tags: Vec<String>,
    /// Délai entre requêtes en millisecondes (`--delay <ms>`).
    pub delay_ms: Option<u64>,
    /// Interrompt l'exécution au premier échec (`--bail`).
    pub bail: bool,
    /// Exécute uniquement les requêtes ayant des tests ou assertions (`--tests-only`).
    pub tests_only: bool,
    /// Autorise les connexions TLS non sécurisées (`--insecure`).
    pub insecure: bool,
    /// Chemin absolu du certificat CA (`--cacert <path>`).
    pub cacert: Option<PathBuf>,
    /// Ignore le truststore système pour utiliser uniquement le cacert (`--ignore-truststore`).
    pub ignore_truststore: bool,
    /// Chemin absolu de configuration du certificat client (`--client-cert-config <path>`).
    pub client_cert_config: Option<PathBuf>,
    /// Désactive tous les proxys (`--noproxy`).
    pub noproxy: bool,
    /// Désactive la sauvegarde et l'envoi automatique de cookies (`--disable-cookies`).
    pub disable_cookies: bool,
    /// Chemin du fichier de rapport JUnit (`--reporter-junit <path>`).
    pub reporter_junit: Option<PathBuf>,
    /// Chemin du fichier de rapport HTML (`--reporter-html <path>`).
    pub reporter_html: Option<PathBuf>,
    /// Masque les en-têtes sensibles dans les rapports (`--reporter-skip-all-headers`).
    pub reporter_skip_all_headers: bool,
}

impl Default for RunnerOptions {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            delay_ms: None,
            bail: false,
            tests_only: false,
            insecure: false,
            cacert: None,
            ignore_truststore: false,
            client_cert_config: None,
            noproxy: false,
            disable_cookies: false,
            reporter_junit: None,
            reporter_html: None,
            reporter_skip_all_headers: true,
        }
    }
}

impl RunnerOptions {
    /// Indique si au moins une option influençant l'exécution ou l'export est active.
    pub fn has_active_options(&self) -> bool {
        !self.tags.is_empty()
            || !self.exclude_tags.is_empty()
            || self.delay_ms.is_some()
            || self.bail
            || self.tests_only
            || self.insecure
            || self.cacert.is_some()
            || self.ignore_truststore
            || self.client_cert_config.is_some()
            || self.noproxy
            || self.disable_cookies
            || self.reporter_junit.is_some()
            || self.reporter_html.is_some()
    }
}

/// Configuration des options de campagne mémorisée pour la session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerOptionsConfig {
    pub tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub delay_ms: Option<u64>,
    pub bail: bool,
    pub tests_only: bool,
    pub insecure: bool,
    pub cacert: Option<PathBuf>,
    pub ignore_truststore: bool,
    pub client_cert_config: Option<PathBuf>,
    pub noproxy: bool,
    pub disable_cookies: bool,
    pub export_junit: bool,
    pub export_html: bool,
    pub skip_headers: bool,
}

impl Default for RunnerOptionsConfig {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            delay_ms: None,
            bail: false,
            tests_only: false,
            insecure: false,
            cacert: None,
            ignore_truststore: false,
            client_cert_config: None,
            noproxy: false,
            disable_cookies: false,
            export_junit: false,
            export_html: false,
            skip_headers: true,
        }
    }
}

impl RunnerOptionsConfig {
    /// Indique si au moins une option influençant l'exécution ou l'export est active.
    pub fn has_active_options(&self) -> bool {
        !self.tags.is_empty()
            || !self.exclude_tags.is_empty()
            || self.delay_ms.is_some()
            || self.bail
            || self.tests_only
            || self.insecure
            || self.cacert.is_some()
            || self.ignore_truststore
            || self.client_cert_config.is_some()
            || self.noproxy
            || self.disable_cookies
            || self.export_junit
            || self.export_html
    }

    /// Résumé textuel court des options actives pour l'affichage en ligne d'état ou en-tête.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.tags.is_empty() {
            parts.push(format!("tags: {}", self.tags.join(",")));
        }
        if !self.exclude_tags.is_empty() {
            parts.push(format!("excl: {}", self.exclude_tags.join(",")));
        }
        if let Some(delay) = self.delay_ms {
            parts.push(format!("délai: {delay}ms"));
        }
        if self.bail {
            parts.push("bail".to_string());
        }
        if self.tests_only {
            parts.push("tests-only".to_string());
        }
        if self.insecure {
            parts.push("insecure".to_string());
        }
        if let Some(cacert) = &self.cacert {
            let name = cacert
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("cacert");
            parts.push(format!("cacert: {name}"));
        }
        if self.ignore_truststore {
            parts.push("ignore-truststore".to_string());
        }
        if let Some(cert) = &self.client_cert_config {
            let name = cert
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("client-cert");
            parts.push(format!("client-cert: {name}"));
        }
        if self.noproxy {
            parts.push("noproxy".to_string());
        }
        if self.disable_cookies {
            parts.push("disable-cookies".to_string());
        }
        if self.export_junit {
            parts.push("junit".to_string());
        }
        if self.export_html {
            parts.push("html".to_string());
        }
        if parts.is_empty() {
            "aucune".to_string()
        } else {
            parts.join(" · ")
        }
    }

    /// Construit les [`RunnerOptions`] en générant les chemins de rapport horodatés si nécessaire.
    pub fn build_runner_options(&self, reports_dir: &Path) -> (RunnerOptions, Vec<PathBuf>) {
        let mut reports_paths = Vec::new();
        let reporter_junit = if self.export_junit {
            let _ = std::fs::create_dir_all(reports_dir);
            let path = generate_report_path(reports_dir, "xml");
            reports_paths.push(path.clone());
            Some(path)
        } else {
            None
        };

        let reporter_html = if self.export_html {
            let _ = std::fs::create_dir_all(reports_dir);
            let path = generate_report_path(reports_dir, "html");
            reports_paths.push(path.clone());
            Some(path)
        } else {
            None
        };

        let options = RunnerOptions {
            tags: self.tags.clone(),
            exclude_tags: self.exclude_tags.clone(),
            delay_ms: self.delay_ms,
            bail: self.bail,
            tests_only: self.tests_only,
            insecure: self.insecure,
            cacert: self.cacert.clone(),
            ignore_truststore: self.ignore_truststore,
            client_cert_config: self.client_cert_config.clone(),
            noproxy: self.noproxy,
            disable_cookies: self.disable_cookies,
            reporter_junit,
            reporter_html,
            reporter_skip_all_headers: self.skip_headers,
        };

        (options, reports_paths)
    }
}

/// Analyse une liste de tags séparés par des virgules et valide l'absence d'espaces dans chaque tag.
pub fn parse_tags(input: &str) -> Result<Vec<String>, RunnerOptionsError> {
    let mut tags = Vec::new();
    for part in input.split(',') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().any(char::is_whitespace) {
            return Err(RunnerOptionsError::TagContainsWhitespace(
                trimmed.to_string(),
            ));
        }
        tags.push(trimmed.to_string());
    }
    Ok(tags)
}

/// Analyse une saisie de délai en millisecondes et valide qu'il s'agit d'un entier positif.
pub fn parse_delay(input: &str) -> Result<Option<u64>, RunnerOptionsError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    match trimmed.parse::<u64>() {
        Ok(val) => Ok(Some(val)),
        Err(_) => Err(RunnerOptionsError::InvalidDelay(trimmed.to_string())),
    }
}

/// Répertoire par défaut pour les exports de rapports, hors de la collection.
pub fn default_reports_dir() -> PathBuf {
    if let Ok(state_home) = std::env::var("XDG_STATE_HOME")
        && !state_home.is_empty()
    {
        return PathBuf::from(state_home).join("bruno-tui").join("reports");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("bruno-tui")
            .join("reports");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("reports")
}

/// Génère un nom de fichier horodaté dans `dir` (ex: `report-1710000000-123.xml`).
pub fn generate_report_path(dir: &Path, extension: &str) -> PathBuf {
    let now = std::time::SystemTime::now();
    let duration = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let filename = format!("report-{secs}-{millis:03}.{extension}");
    dir.join(filename)
}

/// Analyse un chemin de fichier, vérifie son existence sur le disque et retourne
/// son chemin absolu. Si la chaîne est vide, retourne `Ok(None)`.
pub fn parse_existing_file_path(input: &str) -> Result<Option<PathBuf>, RunnerOptionsError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let path = Path::new(trimmed);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    if !resolved.exists() {
        return Err(RunnerOptionsError::FileNotFound(trimmed.to_string()));
    }
    Ok(Some(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tags_valid() {
        assert_eq!(parse_tags("").unwrap(), Vec::<String>::new());
        assert_eq!(parse_tags("   ").unwrap(), Vec::<String>::new());
        assert_eq!(
            parse_tags("smoke, v1, regression").unwrap(),
            vec!["smoke", "v1", "regression"]
        );
        assert_eq!(parse_tags("a,b,c").unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_tags_invalid_with_whitespace() {
        assert!(matches!(
            parse_tags("smoke test, v1"),
            Err(RunnerOptionsError::TagContainsWhitespace(t)) if t == "smoke test"
        ));
        assert!(matches!(
            parse_tags("hello\tworld"),
            Err(RunnerOptionsError::TagContainsWhitespace(_))
        ));
    }

    #[test]
    fn parse_delay_valid() {
        assert_eq!(parse_delay("").unwrap(), None);
        assert_eq!(parse_delay("   ").unwrap(), None);
        assert_eq!(parse_delay("0").unwrap(), Some(0));
        assert_eq!(parse_delay(" 150 ").unwrap(), Some(150));
        assert_eq!(parse_delay("5000").unwrap(), Some(5000));
    }

    #[test]
    fn parse_delay_invalid() {
        assert!(matches!(
            parse_delay("-10"),
            Err(RunnerOptionsError::InvalidDelay(_))
        ));
        assert!(matches!(
            parse_delay("abc"),
            Err(RunnerOptionsError::InvalidDelay(_))
        ));
        assert!(matches!(
            parse_delay("12.5"),
            Err(RunnerOptionsError::InvalidDelay(_))
        ));
    }

    #[test]
    fn parse_existing_file_path_valid_and_invalid() {
        assert_eq!(parse_existing_file_path("").unwrap(), None);
        assert_eq!(parse_existing_file_path("   ").unwrap(), None);

        // Fichier existant (Cargo.toml à la racine)
        let cargo_toml = parse_existing_file_path("Cargo.toml").unwrap().unwrap();
        assert!(cargo_toml.is_absolute());
        assert!(cargo_toml.exists());

        // Fichier inexistant
        assert!(matches!(
            parse_existing_file_path("non_existent_file_xyz.pem"),
            Err(RunnerOptionsError::FileNotFound(f)) if f == "non_existent_file_xyz.pem"
        ));
    }

    #[test]
    fn runner_options_config_summary() {
        let mut config = RunnerOptionsConfig::default();
        assert_eq!(config.summary(), "aucune");
        assert!(!config.has_active_options());

        config.tags = vec!["smoke".into()];
        config.bail = true;
        config.insecure = true;
        config.noproxy = true;
        assert_eq!(config.summary(), "tags: smoke · bail · insecure · noproxy");
        assert!(config.has_active_options());

        config.delay_ms = Some(200);
        config.export_junit = true;
        config.cacert = Some(PathBuf::from("/etc/ssl/ca.pem"));
        config.ignore_truststore = true;
        config.disable_cookies = true;
        assert_eq!(
            config.summary(),
            "tags: smoke · délai: 200ms · bail · insecure · cacert: ca.pem · ignore-truststore · noproxy · disable-cookies · junit"
        );
    }

    #[test]
    fn build_runner_options_creates_report_paths() {
        let temp =
            std::env::temp_dir().join(format!("bruno-tui-test-reports-{}", std::process::id()));
        let config = RunnerOptionsConfig {
            tags: vec!["smoke".into()],
            insecure: true,
            noproxy: true,
            export_junit: true,
            export_html: true,
            skip_headers: true,
            ..RunnerOptionsConfig::default()
        };

        let (opts, paths) = config.build_runner_options(&temp);
        assert_eq!(paths.len(), 2);
        assert!(opts.insecure);
        assert!(opts.noproxy);
        assert!(opts.reporter_junit.is_some());
        assert!(opts.reporter_html.is_some());
        assert!(opts.reporter_skip_all_headers);
        assert!(temp.is_dir());
        let _ = std::fs::remove_dir_all(&temp);
    }
}
