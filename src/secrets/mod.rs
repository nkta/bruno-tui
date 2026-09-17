//! Variables secrètes transmises à `bru` par `--env-var`.
//!
//! Le module déclare les noms (option `--secret`, `vars:secret`), calcule
//! leurs clés candidates et résout leurs valeurs depuis le `.env` de la
//! collection puis l'environnement du processus. Il ne lit que ces deux
//! sources, n'écrit jamais sur disque et ne formate jamais une valeur.

pub mod dotenv;

use std::collections::HashMap;
use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::Path;

use crate::runner::SecretString;

/// Nom du fichier lu à la racine de la collection, comme le fait `bru`.
pub const DOTENV_FILE: &str = ".env";

/// Déclaration issue de `--secret NOM[=CLÉ]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretMapping {
    pub name: String,
    /// Clé explicite ; `None` = clés candidates automatiques.
    pub key: Option<String>,
}

impl SecretMapping {
    /// Analyse `NOM` ou `NOM=CLÉ` ; `None` si la forme est invalide.
    pub fn parse(spec: &str) -> Option<Self> {
        let (name, key) = match spec.split_once('=') {
            Some((name, key)) => (name, Some(key)),
            None => (spec, None),
        };
        if !is_valid_name(name) || key.is_some_and(|key| !is_valid_name(key)) {
            return None;
        }
        Some(Self {
            name: name.to_owned(),
            key: key.map(str::to_owned),
        })
    }

    /// Recherche à effectuer pour cette déclaration.
    pub fn lookup(&self) -> SecretLookup {
        match &self.key {
            Some(key) => SecretLookup {
                name: self.name.clone(),
                keys: vec![key.clone()],
            },
            None => SecretLookup::automatic(&self.name),
        }
    }
}

/// Nom de variable ou de clé acceptable : non vide, sans `=` ni espace
/// blanc (forme `nom=valeur` attendue par `bru`).
pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty() && !name.contains('=') && !name.chars().any(char::is_whitespace)
}

/// Forme `UPPER_SNAKE_CASE` d'un nom (`oktaClientSecret` →
/// `OKTA_CLIENT_SECRET`).
pub fn upper_snake(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &c) in chars.iter().enumerate() {
        if c == '-' || c == '.' || c.is_whitespace() {
            out.push('_');
            continue;
        }
        if c.is_uppercase() && i > 0 {
            let previous = chars[i - 1];
            let next_is_lower = chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if previous.is_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_uppercase() && next_is_lower)
            {
                out.push('_');
            }
        }
        out.extend(c.to_uppercase());
    }
    out
}

/// Nom à résoudre et ses clés candidates, dans l'ordre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretLookup {
    pub name: String,
    pub keys: Vec<String>,
}

impl SecretLookup {
    /// Clés automatiques : le nom exact puis sa forme `UPPER_SNAKE_CASE`.
    pub fn automatic(name: &str) -> Self {
        let mut keys = vec![name.to_owned()];
        let upper = upper_snake(name);
        if upper != name {
            keys.push(upper);
        }
        Self {
            name: name.to_owned(),
            keys,
        }
    }
}

/// Raison fixe d'un refus ; ne contient jamais de valeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidReason {
    /// La valeur contient un saut de ligne, refusé par `--env-var`.
    Multiline,
    /// La variable du shell n'est pas de l'UTF-8.
    NotUtf8,
    /// Le `.env` existe mais n'a pas pu être lu.
    UnreadableDotEnv,
}

impl fmt::Display for InvalidReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Multiline => "valeur multiligne",
            Self::NotUtf8 => "valeur non UTF-8",
            Self::UnreadableDotEnv => ".env illisible",
        })
    }
}

/// Provenance de la valeur d'une variable secrète.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretSource {
    /// Saisie dans le panneau pendant la session.
    Typed,
    DotEnv {
        key: String,
    },
    Shell {
        key: String,
    },
    /// Aucune valeur ; `keys` liste les clés cherchées (vide pour un nom
    /// ajouté depuis le panneau).
    Missing {
        keys: Vec<String>,
    },
    /// Valeur trouvée mais refusée ; `key` est vide si le refus ne porte
    /// sur aucune clé précise (`.env` illisible).
    Invalid {
        key: String,
        reason: InvalidReason,
    },
}

/// Résultat de la résolution d'un nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub name: String,
    pub source: SecretSource,
    pub value: Option<SecretString>,
}

/// Résout chaque recherche : clés candidates dans `<root>/.env`, puis dans
/// l'environnement fourni par `lookup_env`. La source prime sur l'ordre
/// des clés, comme `bru` fait primer `.env` sur `process.env`.
pub fn resolve(
    root: &Path,
    lookups: &[SecretLookup],
    lookup_env: &dyn Fn(&str) -> Option<OsString>,
) -> Vec<Resolved> {
    let dotenv = match std::fs::read(root.join(DOTENV_FILE)) {
        // Même décodage que `readFileSync(path, 'utf8')`.
        Ok(bytes) => Ok(dotenv::parse(&String::from_utf8_lossy(&bytes))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(_) => Err(()),
    };
    lookups
        .iter()
        .map(|lookup| resolve_one(lookup, dotenv.as_ref(), lookup_env))
        .collect()
}

fn resolve_one(
    lookup: &SecretLookup,
    dotenv: Result<&HashMap<String, SecretString>, &()>,
    lookup_env: &dyn Fn(&str) -> Option<OsString>,
) -> Resolved {
    let resolved = |source, value| Resolved {
        name: lookup.name.clone(),
        source,
        value,
    };
    let Ok(dotenv) = dotenv else {
        return resolved(
            SecretSource::Invalid {
                key: String::new(),
                reason: InvalidReason::UnreadableDotEnv,
            },
            None,
        );
    };
    for key in &lookup.keys {
        if let Some(value) = dotenv.get(key) {
            return checked(lookup, key, value.clone(), |key| SecretSource::DotEnv {
                key,
            });
        }
    }
    for key in &lookup.keys {
        if let Some(raw) = lookup_env(key) {
            return match raw.into_string() {
                Ok(value) => checked(lookup, key, SecretString::new(value), |key| {
                    SecretSource::Shell { key }
                }),
                Err(_) => resolved(
                    SecretSource::Invalid {
                        key: key.clone(),
                        reason: InvalidReason::NotUtf8,
                    },
                    None,
                ),
            };
        }
    }
    resolved(
        SecretSource::Missing {
            keys: lookup.keys.clone(),
        },
        None,
    )
}

/// Refuse une valeur que `bru` ne pourrait pas relire en `nom=valeur`.
fn checked(
    lookup: &SecretLookup,
    key: &str,
    value: SecretString,
    source: impl FnOnce(String) -> SecretSource,
) -> Resolved {
    let (source, value) = if value.expose().contains(dotenv::is_line_terminator) {
        (
            SecretSource::Invalid {
                key: key.to_owned(),
                reason: InvalidReason::Multiline,
            },
            None,
        )
    } else {
        (source(key.to_owned()), Some(value))
    };
    Resolved {
        name: lookup.name.clone(),
        source,
        value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn name_validation() {
        assert!(is_valid_name("token"));
        assert!(is_valid_name("OKTA_CLIENT_SECRET"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name("a b"));
        assert!(!is_valid_name("=x"));
        assert!(!is_valid_name("a=b"));
    }

    #[test]
    fn mapping_parse() {
        assert_eq!(
            SecretMapping::parse("token"),
            Some(SecretMapping {
                name: "token".into(),
                key: None
            })
        );
        assert_eq!(
            SecretMapping::parse("okta=OKTA_SECRET"),
            Some(SecretMapping {
                name: "okta".into(),
                key: Some("OKTA_SECRET".into())
            })
        );
        for invalid in ["", "=X", "X=", "a=b=c", "a b", "a=b c"] {
            assert_eq!(SecretMapping::parse(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn upper_snake_forms() {
        assert_eq!(upper_snake("oktaClientSecret"), "OKTA_CLIENT_SECRET");
        assert_eq!(upper_snake("apiURLKey"), "API_URL_KEY");
        assert_eq!(upper_snake("oauth2Token"), "OAUTH2_TOKEN");
        assert_eq!(upper_snake("x-api-key"), "X_API_KEY");
        assert_eq!(upper_snake("client_secret"), "CLIENT_SECRET");
        assert_eq!(upper_snake("a.b c"), "A_B_C");
        assert_eq!(upper_snake("TOKEN"), "TOKEN");
        assert_eq!(upper_snake("URL"), "URL");
    }

    #[test]
    fn lookup_keys() {
        assert_eq!(
            SecretLookup::automatic("token").keys,
            vec!["token".to_owned(), "TOKEN".to_owned()]
        );
        assert_eq!(SecretLookup::automatic("TOKEN").keys, vec!["TOKEN"]);
        let explicit = SecretMapping::parse("okta=OKTA_SECRET").map(|m| m.lookup());
        assert_eq!(explicit.map(|l| l.keys), Some(vec!["OKTA_SECRET".into()]));
    }

    /// Répertoire temporaire propre au test, supprimé à la destruction.
    struct TempRoot(PathBuf);

    impl TempRoot {
        fn new(label: &str, dotenv: Option<&str>) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("bruno-tui-secrets-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("répertoire temporaire");
            if let Some(content) = dotenv {
                std::fs::write(dir.join(DOTENV_FILE), content).expect("écriture .env");
            }
            Self(dir)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn env_table(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let table: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), OsString::from(*v)))
            .collect();
        move |key| table.get(key).cloned()
    }

    fn one(root: &TempRoot, lookup: SecretLookup, env: &[(&str, &str)]) -> Resolved {
        let lookup_env = env_table(env);
        resolve(&root.0, &[lookup], &lookup_env).remove(0)
    }

    fn value(resolved: &Resolved) -> Option<&str> {
        resolved.value.as_ref().map(SecretString::expose)
    }

    #[test]
    fn automatic_upper_snake_from_dotenv() {
        let root = TempRoot::new("auto", Some("OKTA_CLIENT_SECRET=abc\n"));
        let r = one(&root, SecretLookup::automatic("oktaClientSecret"), &[]);
        assert_eq!(
            r.source,
            SecretSource::DotEnv {
                key: "OKTA_CLIENT_SECRET".into()
            }
        );
        assert_eq!(value(&r), Some("abc"));
    }

    #[test]
    fn exact_name_before_upper_snake() {
        let root = TempRoot::new("exact", Some("TOKEN=b\ntoken=a\n"));
        let r = one(&root, SecretLookup::automatic("token"), &[]);
        assert_eq!(
            r.source,
            SecretSource::DotEnv {
                key: "token".into()
            }
        );
        assert_eq!(value(&r), Some("a"));
    }

    #[test]
    fn explicit_key_is_exclusive() {
        let root = TempRoot::new(
            "explicit",
            Some("OKTA_SECRET=abc\nOKTA_CLIENT_SECRET=zzz\n"),
        );
        let lookup = SecretLookup {
            name: "oktaClientSecret".into(),
            keys: vec!["OKTA_SECRET".into()],
        };
        let r = one(&root, lookup.clone(), &[]);
        assert_eq!(value(&r), Some("abc"));
        let empty = TempRoot::new("explicit-empty", Some("OKTA_CLIENT_SECRET=zzz\n"));
        let r = one(&empty, lookup, &[]);
        assert_eq!(
            r.source,
            SecretSource::Missing {
                keys: vec!["OKTA_SECRET".into()]
            }
        );
    }

    #[test]
    fn shell_fallback_and_dotenv_priority() {
        let root = TempRoot::new("shell", None);
        let r = one(&root, SecretLookup::automatic("token"), &[("TOKEN", "xyz")]);
        assert_eq!(
            r.source,
            SecretSource::Shell {
                key: "TOKEN".into()
            }
        );
        assert_eq!(value(&r), Some("xyz"));

        let root = TempRoot::new("priority", Some("TOKEN=a\n"));
        let r = one(&root, SecretLookup::automatic("token"), &[("token", "b")]);
        assert_eq!(
            r.source,
            SecretSource::DotEnv {
                key: "TOKEN".into()
            }
        );
        assert_eq!(value(&r), Some("a"));
    }

    #[test]
    fn missing_lists_searched_keys() {
        let root = TempRoot::new("missing", Some("OTHER=1\n"));
        let r = one(&root, SecretLookup::automatic("token"), &[]);
        assert_eq!(
            r.source,
            SecretSource::Missing {
                keys: vec!["token".into(), "TOKEN".into()]
            }
        );
        assert_eq!(r.value, None);
    }

    #[test]
    fn unreadable_dotenv_leaves_values_unset() {
        let root = TempRoot::new("unreadable", None);
        // Un répertoire à la place du fichier : lecture impossible.
        std::fs::create_dir(root.0.join(DOTENV_FILE)).expect("répertoire .env");
        let r = one(&root, SecretLookup::automatic("token"), &[("TOKEN", "x")]);
        assert_eq!(
            r.source,
            SecretSource::Invalid {
                key: String::new(),
                reason: InvalidReason::UnreadableDotEnv
            }
        );
        assert_eq!(r.value, None);
    }

    #[test]
    fn multiline_and_non_utf8_are_refused() {
        let root = TempRoot::new("multiline", Some("TOKEN=\"a\\nb\"\n"));
        let r = one(&root, SecretLookup::automatic("token"), &[]);
        assert_eq!(
            r.source,
            SecretSource::Invalid {
                key: "TOKEN".into(),
                reason: InvalidReason::Multiline
            }
        );
        assert_eq!(r.value, None);

        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let root = TempRoot::new("utf8", None);
            let lookup_env = |_: &str| Some(OsString::from_vec(vec![0xff]));
            let r = resolve(&root.0, &[SecretLookup::automatic("token")], &lookup_env).remove(0);
            assert_eq!(
                r.source,
                SecretSource::Invalid {
                    key: "token".into(),
                    reason: InvalidReason::NotUtf8
                }
            );
        }
    }

    #[test]
    fn debug_never_shows_values() {
        let root = TempRoot::new("debug", Some("TOKEN=s3cr3t-value\n"));
        let r = one(&root, SecretLookup::automatic("token"), &[]);
        assert_eq!(value(&r), Some("s3cr3t-value"));
        assert!(!format!("{r:?}").contains("s3cr3t-value"));
    }
}
