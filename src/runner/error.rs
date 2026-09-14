//! Erreurs et issues d'une exécution de `bru run`.

use std::io;

use thiserror::Error;

use super::report::Report;

/// Issue unique délivrée pour chaque exécution.
#[derive(Debug)]
pub enum RunOutcome {
    /// Rapport valide obtenu. Le code de sortie est informatif : des tests en
    /// échec donnent un code non nul sans que l'exécution soit en erreur.
    Completed {
        report: Report,
        exit_code: Option<i32>,
    },
    Failed(RunError),
    Cancelled,
}

/// Impossible d'obtenir un rapport exploitable.
///
/// Aucune variante ne contient le rapport, les arguments ou les valeurs de
/// surcharge ; seule `NoReport` porte la sortie console de `bru`, destinée à
/// l'affichage et jamais aux logs.
#[derive(Debug, Error)]
pub enum RunError {
    #[error("programme `bru` introuvable, vérifier qu'il est installé et dans le PATH")]
    BruNotFound,

    #[error("impossible de lancer `bru` : {0}")]
    Spawn(#[source] io::Error),

    #[error("`bru` s'est terminé sans produire de rapport (code de sortie {})", display_code(*.exit_code))]
    NoReport {
        exit_code: Option<i32>,
        /// Fin de la sortie standard et d'erreur de `bru` (64 Kio au plus).
        output: String,
    },

    #[error("rapport `bru` non conforme au modèle : {kind} (ligne {line}, colonne {column})")]
    InvalidReport {
        kind: ReportErrorKind,
        line: usize,
        column: usize,
    },

    #[error("erreur d'entrée/sortie pendant l'exécution de `bru` : {0}")]
    Io(#[source] io::Error),

    #[error("exécution de `bru` non supportée sur cette plateforme (Unix requis)")]
    UnsupportedPlatform,
}

impl RunError {
    /// Construit `InvalidReport` sans conserver le message serde, qui peut
    /// citer des valeurs du rapport (`invalid type: string "..."`).
    #[cfg_attr(not(unix), allow(dead_code))]
    pub(crate) fn invalid_report(error: &serde_json::Error) -> Self {
        use serde_json::error::Category;
        let kind = match error.classify() {
            Category::Syntax => ReportErrorKind::Syntax,
            Category::Eof => ReportErrorKind::Truncated,
            Category::Data => ReportErrorKind::Structure,
            Category::Io => ReportErrorKind::Io,
        };
        Self::InvalidReport {
            kind,
            line: error.line(),
            column: error.column(),
        }
    }
}

/// Nature d'un rapport non conforme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReportErrorKind {
    #[error("JSON invalide")]
    Syntax,
    #[error("JSON tronqué")]
    Truncated,
    #[error("structure inattendue")]
    Structure,
    #[error("lecture interrompue")]
    Io,
}

fn display_code(code: Option<i32>) -> String {
    code.map_or_else(
        || "inconnu, processus tué par un signal".to_owned(),
        |c| c.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_report_does_not_leak_report_content() {
        let raw = r#"[{"iterationIndex": 0, "results": [], "summary": "Bearer s3cr3t"}]"#;
        let serde_error = serde_json::from_str::<Report>(raw).expect_err("summary invalide");
        // Le message serde brut cite la valeur : c'est ce qu'on doit éviter.
        assert!(serde_error.to_string().contains("s3cr3t"));

        let error = RunError::invalid_report(&serde_error);
        let displayed = error.to_string();
        assert!(!displayed.contains("s3cr3t"), "{displayed}");
        assert!(!format!("{error:?}").contains("s3cr3t"));
        assert!(matches!(
            error,
            RunError::InvalidReport {
                kind: ReportErrorKind::Structure,
                line: 1,
                ..
            }
        ));
    }

    #[test]
    fn invalid_syntax_and_truncation_are_classified() {
        let syntax = serde_json::from_str::<Report>("[nope").expect_err("syntaxe");
        assert!(matches!(
            RunError::invalid_report(&syntax),
            RunError::InvalidReport {
                kind: ReportErrorKind::Syntax,
                ..
            }
        ));
        let eof = serde_json::from_str::<Report>("[{").expect_err("tronqué");
        assert!(matches!(
            RunError::invalid_report(&eof),
            RunError::InvalidReport {
                kind: ReportErrorKind::Truncated,
                ..
            }
        ));
    }

    #[test]
    fn no_report_displays_exit_code() {
        let error = RunError::NoReport {
            exit_code: Some(5),
            output: "Path not found".into(),
        };
        assert!(error.to_string().contains("code de sortie 5"));
    }
}
