//! Analyse des arguments de la ligne de commande.
//!
//! Analyseur écrit à la main : un argument positionnel et deux options ne
//! justifient pas une dépendance. Les chemins restent des `OsString` pour
//! accepter les noms non UTF-8.

use std::ffi::OsString;
use std::path::PathBuf;

use thiserror::Error;

/// Texte d'aide affiché par `--help` et en cas d'erreur d'usage.
pub const USAGE: &str = "\
Usage : bruno-tui [CHEMIN]

Explore une collection Bruno dans le terminal.

Arguments :
  CHEMIN          collection, ou fichier ou dossier qu'elle contient
                  (défaut : répertoire courant)

Options :
  -h, --help      affiche cette aide
  -V, --version   affiche la version
";

/// Nom et version du binaire.
pub const VERSION: &str = concat!(env!("CARGO_PKG_NAME"), " ", env!("CARGO_PKG_VERSION"));

/// Action demandée par la ligne de commande.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Ouvre l'interface sur ce chemin.
    Run(PathBuf),
    Help,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UsageError {
    #[error("option inconnue : {0}")]
    UnknownOption(String),
    #[error("un seul chemin est accepté")]
    TooManyArguments,
}

/// Analyse les arguments, nom du programme exclu.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut path: Option<PathBuf> = None;
    for arg in args {
        match arg.to_str() {
            Some("-h" | "--help") => return Ok(Command::Help),
            Some("-V" | "--version") => return Ok(Command::Version),
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
    Ok(Command::Run(path.unwrap_or_else(|| PathBuf::from("."))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn default_is_current_directory() {
        assert_eq!(parse_strs(&[]), Ok(Command::Run(PathBuf::from("."))));
    }

    #[test]
    fn explicit_path() {
        assert_eq!(
            parse_strs(&["tests/fixtures"]),
            Ok(Command::Run(PathBuf::from("tests/fixtures")))
        );
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["-V"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["x", "--help"]), Ok(Command::Help));
        assert!(VERSION.starts_with("bruno-tui "));
    }

    #[test]
    fn usage_errors() {
        assert_eq!(
            parse_strs(&["--inconnu"]),
            Err(UsageError::UnknownOption("--inconnu".into()))
        );
        assert_eq!(parse_strs(&["a", "b"]), Err(UsageError::TooManyArguments));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_accepted() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![b'c', 0xff, b'l']);
        assert_eq!(parse([raw.clone()]), Ok(Command::Run(PathBuf::from(raw))));
    }
}
