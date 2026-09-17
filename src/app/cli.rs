//! Analyse des arguments de la ligne de commande.
//!
//! Analyseur écrit à la main : un argument positionnel et quatre options ne
//! justifient pas une dépendance. Les chemins restent des `OsString` pour
//! accepter les noms non UTF-8.

use std::ffi::OsString;
use std::path::PathBuf;

use thiserror::Error;

use crate::secrets::SecretMapping;

/// Texte d'aide affiché par `--help` et en cas d'erreur d'usage.
pub const USAGE: &str = "\
Usage : bruno-tui [OPTIONS] [CHEMIN]

Explore une collection Bruno dans le terminal.

Arguments :
  CHEMIN          collection, ou fichier ou dossier qu'elle contient
                  (défaut : répertoire courant)

Options :
  --secret NOM[=CLÉ]
                  variable secrète transmise à bru (--env-var), répétable.
                  Les vars:secret de l'environnement choisi sont cherchées
                  sans cette option ; elle sert aux noms non déclarés ou
                  à imposer une clé. Valeur lue sous CLÉ, ou à défaut sous
                  NOM puis sa forme UPPER_SNAKE_CASE, dans le .env de la
                  collection puis dans le shell ; une saisie dans le
                  panneau S prime. Jamais de valeur sur la ligne de
                  commande : la valeur reste toutefois visible dans les
                  arguments de bru (ps) pendant son exécution.
  --no-mouse      n'active pas la capture souris : la sélection native du
                  terminal reste disponible. En cours de session, M bascule
                  la capture ; la plupart des terminaux gardent aussi la
                  sélection native avec Maj+glisser.
  -h, --help      affiche cette aide
  -V, --version   affiche la version
";

/// Nom et version du binaire.
pub const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

/// Action demandée par la ligne de commande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Ouvre l'interface sur ce chemin.
    Run {
        path: PathBuf,
        /// Déclarations `--secret`, dans l'ordre ; un nom répété garde sa
        /// première déclaration.
        secrets: Vec<SecretMapping>,
        /// Capture souris au démarrage ; `false` avec `--no-mouse`.
        mouse: bool,
    },
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UsageError {
    #[error("option inconnue : {0}")]
    UnknownOption(String),
    #[error("un seul chemin est accepté")]
    TooManyArguments,
    #[error("--secret attend NOM ou NOM=CLÉ")]
    MissingSecretArgument,
    /// Porte l'argument fautif, qui n'est qu'un nom et jamais une valeur.
    #[error("--secret invalide : `{0}` (attendu NOM ou NOM=CLÉ, sans espace)")]
    InvalidSecret(String),
}

/// Analyse les arguments, nom du programme exclu.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut path: Option<PathBuf> = None;
    let mut secrets: Vec<SecretMapping> = Vec::new();
    let mut mouse = true;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
            Some("--no-mouse") => mouse = false,
            Some("--secret") => {
                let spec = args.next().ok_or(UsageError::MissingSecretArgument)?;
                push_secret(&mut secrets, &spec)?;
            }
            Some(option) if option.starts_with("--secret=") => {
                push_secret(&mut secrets, &OsString::from(&option["--secret=".len()..]))?;
            }
            Some(option) if option.starts_with('-') => {
                return Err(UsageError::UnknownOption(option.to_owned()));
            }
            _ => {
                if path.is_some() {
                    return Err(UsageError::TooManyArguments);
                }
                path = Some(PathBuf::from(arg));
            }
        }
    }
    Ok(Command::Run {
        path: path.unwrap_or_else(|| PathBuf::from(".")),
        secrets,
        mouse,
    })
}

fn push_secret(secrets: &mut Vec<SecretMapping>, spec: &OsString) -> Result<(), UsageError> {
    let invalid = || UsageError::InvalidSecret(spec.to_string_lossy().into_owned());
    let mapping = spec
        .to_str()
        .and_then(SecretMapping::parse)
        .ok_or_else(invalid)?;
    if !secrets.iter().any(|known| known.name == mapping.name) {
        secrets.push(mapping);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    fn run(path: &str) -> Command {
        Command::Run {
            path: PathBuf::from(path),
            secrets: Vec::new(),
            mouse: true,
        }
    }

    fn mapping(name: &str, key: Option<&str>) -> SecretMapping {
        SecretMapping {
            name: name.to_owned(),
            key: key.map(str::to_owned),
        }
    }

    #[test]
    fn default_is_current_directory() {
        assert_eq!(parse_strs(&[]), Ok(run(".")));
    }

    #[test]
    fn explicit_path() {
        assert_eq!(parse_strs(&["tests/fixtures"]), Ok(run("tests/fixtures")));
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["-V"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["x", "--help"]), Ok(Command::Help));
        assert!(VERSION.starts_with("bruno-tui "));
        assert!(USAGE.contains("--secret NOM[=CLÉ]"));
    }

    #[test]
    fn usage_errors() {
        assert_eq!(
            parse_strs(&["--inconnu"]),
            Err(UsageError::UnknownOption("--inconnu".into()))
        );
        assert_eq!(parse_strs(&["a", "b"]), Err(UsageError::TooManyArguments));
    }

    #[test]
    fn no_mouse_option() {
        let without_mouse = |path: &str| Command::Run {
            path: PathBuf::from(path),
            secrets: Vec::new(),
            mouse: false,
        };
        assert_eq!(parse_strs(&["./c"]), Ok(run("./c")));
        assert_eq!(parse_strs(&["--no-mouse", "./c"]), Ok(without_mouse("./c")));
        assert_eq!(parse_strs(&["./c", "--no-mouse"]), Ok(without_mouse("./c")));
        assert_eq!(
            parse_strs(&["--no-mouse", "--no-mouse"]),
            Ok(without_mouse("."))
        );
        assert_eq!(
            parse_strs(&["--no-mouse=1"]),
            Err(UsageError::UnknownOption("--no-mouse=1".into()))
        );
        assert!(USAGE.contains("--no-mouse"));
    }

    #[test]
    fn secret_declarations() {
        assert_eq!(
            parse_strs(&[
                "--secret",
                "oktaClientSecret=OKTA_CLIENT_SECRET",
                "--secret",
                "token",
                "./c",
            ]),
            Ok(Command::Run {
                path: PathBuf::from("./c"),
                secrets: vec![
                    mapping("oktaClientSecret", Some("OKTA_CLIENT_SECRET")),
                    mapping("token", None),
                ],
                mouse: true,
            })
        );
        assert_eq!(
            parse_strs(&["./c", "--secret=a", "--secret=b=B", "--secret", "a=X"]),
            Ok(Command::Run {
                path: PathBuf::from("./c"),
                secrets: vec![mapping("a", None), mapping("b", Some("B"))],
                mouse: true,
            })
        );
    }

    #[test]
    fn invalid_secret_declarations() {
        assert_eq!(
            parse_strs(&["--secret"]),
            Err(UsageError::MissingSecretArgument)
        );
        for spec in ["=X", "a=b=c", "a b", "X=", ""] {
            assert_eq!(
                parse_strs(&["--secret", spec]),
                Err(UsageError::InvalidSecret(spec.into())),
                "{spec}"
            );
        }
        assert_eq!(
            parse_strs(&["--secret=a=b=c"]),
            Err(UsageError::InvalidSecret("a=b=c".into()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_accepted() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![b'c', 0xff, b'l']);
        assert_eq!(
            parse([raw.clone()]),
            Ok(Command::Run {
                path: PathBuf::from(raw),
                secrets: Vec::new(),
                mouse: true,
            })
        );
    }
}
