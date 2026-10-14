//! Règles de noms de fichier et de dossier, reprises de Bruno 4.2.1.
//!
//! Source : `@usebruno/common` (`validateName`, `validateNameError`,
//! `sanitizeName`) et `bruno-electron/src/utils/filesystem.js`
//! (`nextSuffixedName`, `truncateBaseForSuffix`, `MAX_DUPLICATE_NAMES`),
//! relevés dans `app.asar` de Bruno desktop 4.2.1. Les messages sont ceux
//! de Bruno, traduits en français.

use std::path::Path;

use thiserror::Error;

/// Longueur maximale d'un nom, en unités UTF-16 comme `String.length` de
/// JavaScript.
pub const MAX_NAME_LENGTH: usize = 255;

/// Nombre maximal de candidats essayés en cas de collision (` 1` … ` 199`).
pub const MAX_DUPLICATE_NAMES: usize = 200;

/// Noms de fichier de requête réservés par Bruno (`folder.bru`,
/// `collection.bru`) : refusés par la boîte « Nouvelle requête ».
pub const RESERVED_REQUEST_FILENAMES: [&str; 2] = ["collection", "folder"];

/// Refus d'un nom, avec le message de Bruno en français.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NameError {
    #[error("Le nom ne peut pas être vide.")]
    Empty,
    #[error("Le nom ne peut pas dépasser 255 caractères.")]
    TooLong,
    #[error("Le nom ne peut pas être un nom de périphérique réservé.")]
    ReservedDevice,
    #[error("Caractère spécial interdit dans le nom : « {0} ».")]
    InvalidCharacter(String),
    #[error("Les noms de fichier « collection » et « folder » sont réservés par Bruno.")]
    ReservedFilename,
    #[error("Trop d'éléments nommés « {0} » (limite 200). Choisis un autre nom.")]
    TooManyDuplicates(String),
}

/// Caractères interdits partout : `< > : " / \ | ? *` et `\x00`-`\x1F`.
fn is_forbidden(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 0x20
}

/// Espace au sens de `\s` en JavaScript.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Nom réservé par Windows : `CON`, `PRN`, `AUX`, `NUL`, `COM0`-`COM9`,
/// `LPT0`-`LPT9`, sans distinction de casse.
fn is_reserved_device(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    let bytes = upper.as_bytes();
    bytes.len() == 4
        && (upper.starts_with("COM") || upper.starts_with("LPT"))
        && bytes[3].is_ascii_digit()
}

/// Longueur en unités UTF-16, comme `String.length`.
fn js_length(name: &str) -> usize {
    name.encode_utf16().count()
}

/// Clé de comparaison des noms d'environnement : `toLowerCase().trim()`
/// de Bruno (minuscules Unicode, puis espaces de bord retirés).
pub fn environment_name_key(name: &str) -> String {
    name.to_lowercase().trim().to_owned()
}

/// Deux noms d'environnement désignent le même environnement pour Bruno.
pub fn same_environment_name(a: &str, b: &str) -> bool {
    environment_name_key(a) == environment_name_key(b)
}

/// `validateNameError` : `Ok` si Bruno accepte ce nom de fichier ou de
/// dossier (sans extension), sinon le premier problème, dans l'ordre de
/// Bruno.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    let Some(first) = name.chars().next() else {
        return Err(NameError::Empty);
    };
    if js_length(name) > MAX_NAME_LENGTH {
        return Err(NameError::TooLong);
    }
    if is_reserved_device(name) {
        return Err(NameError::ReservedDevice);
    }
    if is_forbidden(first) || is_js_whitespace(first) || first == '-' {
        return Err(NameError::InvalidCharacter(first.to_string()));
    }
    let chars: Vec<char> = name.chars().collect();
    let last_index = chars.len() - 1;
    for &c in chars.iter().take(last_index).skip(1) {
        if is_forbidden(c) {
            return Err(NameError::InvalidCharacter(c.to_string()));
        }
    }
    let last = chars[last_index];
    if is_forbidden(last) || is_js_whitespace(last) || last == '.' {
        return Err(NameError::InvalidCharacter(last.to_string()));
    }
    Ok(())
}

/// Validation d'un nom de fichier de requête : `validate_name`, plus le
/// refus de `collection` et `folder`.
pub fn validate_request_filename(name: &str) -> Result<(), NameError> {
    validate_name(name)?;
    if RESERVED_REQUEST_FILENAMES.contains(&name) {
        return Err(NameError::ReservedFilename);
    }
    Ok(())
}

/// `sanitizeName` : nom de fichier proposé à partir du nom affiché.
/// Caractères interdits remplacés par `-`, espaces et `-` de tête retirés,
/// points et espaces de fin retirés.
pub fn sanitize_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| if is_forbidden(c) { '-' } else { c })
        .collect();
    replaced
        .trim_start_matches(|c: char| is_js_whitespace(c) || c == '-')
        .trim_end_matches(|c: char| is_js_whitespace(c) || c == '.')
        .to_owned()
}

/// `nextSuffixedName` : `base`, puis `base 1`, `base 2`…, extension
/// (sans point) ajoutée si non vide.
pub fn suffixed_name(base: &str, extension: &str, n: usize) -> String {
    let suffix = if n == 0 {
        String::new()
    } else {
        format!(" {n}")
    };
    if extension.is_empty() {
        format!("{base}{suffix}")
    } else {
        format!("{base}{suffix}.{extension}")
    }
}

/// `truncateBaseForSuffix` : raccourcit `base` pour que le nom le plus long
/// possible (suffixe ` 199` et extension compris) tienne en 255 caractères.
pub fn truncate_base_for_suffix(base: &str, extension: &str) -> String {
    let extension_length = if extension.is_empty() {
        0
    } else {
        js_length(extension) + 1
    };
    let suffix_reserve = (MAX_DUPLICATE_NAMES - 1).to_string().len() + 1;
    let max = MAX_NAME_LENGTH
        .saturating_sub(extension_length + suffix_reserve)
        .max(1);
    let mut length = 0;
    let mut out = String::new();
    for c in base.chars() {
        length += c.len_utf16();
        if length > max {
            break;
        }
        out.push(c);
    }
    out
}

/// Premier nom libre dans `dir` parmi `base`, `base 1`, `base 2`… ; `taken`
/// dit si un nom est déjà pris (en plus de son existence sur disque, que
/// l'appelant vérifie). Erreur au-delà de 200 essais, comme Bruno.
pub fn first_free_name(
    dir: &Path,
    base: &str,
    extension: &str,
    mut taken: impl FnMut(&Path) -> bool,
) -> Result<String, NameError> {
    let safe = truncate_base_for_suffix(base, extension);
    for n in 0..MAX_DUPLICATE_NAMES {
        let candidate = suffixed_name(&safe, extension, n);
        if !taken(&dir.join(&candidate)) {
            return Ok(candidate);
        }
    }
    Err(NameError::TooManyDuplicates(base.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn valid_names_are_accepted() {
        for name in [
            "a",
            "Lire un article",
            "get-post",
            "v1.2 final",
            "été",
            "a b",
        ] {
            assert_eq!(validate_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn empty_and_too_long_names_are_refused() {
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name(&"a".repeat(255)), Ok(()));
        assert_eq!(validate_name(&"a".repeat(256)), Err(NameError::TooLong));
        // Longueur comptée en UTF-16 : un emoji vaut deux unités.
        let emoji = format!("{}{}", "a".repeat(254), '\u{1F600}');
        assert_eq!(validate_name(&emoji), Err(NameError::TooLong));
    }

    #[test]
    fn reserved_device_names_are_refused_in_any_case() {
        for name in ["CON", "prn", "Aux", "nul", "COM0", "com9", "LPT1", "lpt0"] {
            assert_eq!(
                validate_name(name),
                Err(NameError::ReservedDevice),
                "{name}"
            );
        }
        for name in ["CONS", "COM", "COM10", "LPT", "console"] {
            assert_eq!(validate_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn forbidden_characters_are_refused_with_the_first_offender() {
        for (name, bad) in [
            ("a<b", "<"),
            ("a>b", ">"),
            ("a:b", ":"),
            ("a\"b", "\""),
            ("a/b", "/"),
            ("a\\b", "\\"),
            ("a|b", "|"),
            ("a?b", "?"),
            ("a*b", "*"),
            ("a\u{1}b", "\u{1}"),
            ("a\tb", "\t"),
        ] {
            assert_eq!(
                validate_name(name),
                Err(NameError::InvalidCharacter(bad.into())),
                "{name:?}"
            );
        }
        // DEL (0x7F) n'est pas interdit par Bruno.
        assert_eq!(validate_name("a\u{7F}b"), Ok(()));
    }

    #[test]
    fn leading_and_trailing_rules_follow_bruno() {
        assert_eq!(
            validate_name(" a"),
            Err(NameError::InvalidCharacter(" ".into()))
        );
        assert_eq!(
            validate_name("-a"),
            Err(NameError::InvalidCharacter("-".into()))
        );
        assert_eq!(
            validate_name("a "),
            Err(NameError::InvalidCharacter(" ".into()))
        );
        assert_eq!(
            validate_name("a."),
            Err(NameError::InvalidCharacter(".".into()))
        );
        assert_eq!(
            validate_name("\u{A0}a"),
            Err(NameError::InvalidCharacter("\u{A0}".into()))
        );
        // `-` et `.` sont permis ailleurs.
        assert_eq!(validate_name("a-"), Ok(()));
        assert_eq!(validate_name(".a"), Ok(()));
        assert_eq!(
            validate_name("."),
            Err(NameError::InvalidCharacter(".".into()))
        );
    }

    #[test]
    fn request_filenames_collection_and_folder_are_reserved() {
        assert_eq!(
            validate_request_filename("folder"),
            Err(NameError::ReservedFilename)
        );
        assert_eq!(
            validate_request_filename("collection"),
            Err(NameError::ReservedFilename)
        );
        assert_eq!(validate_request_filename("Folder"), Ok(()));
        assert_eq!(validate_name("folder"), Ok(()));
    }

    #[test]
    fn sanitize_follows_bruno() {
        assert_eq!(sanitize_name("Lire un article"), "Lire un article");
        assert_eq!(sanitize_name("a/b:c"), "a-b-c");
        assert_eq!(sanitize_name("  - -titre"), "titre");
        assert_eq!(sanitize_name("/titre"), "titre");
        assert_eq!(sanitize_name("fin. . "), "fin");
        assert_eq!(sanitize_name("a?"), "a-");
        assert_eq!(sanitize_name("x\u{1F}y"), "x-y");
        assert_eq!(sanitize_name("   "), "");
        // Le résultat d'un nom ordinaire est toujours valide.
        assert_eq!(validate_name(&sanitize_name(" -Mon: test. ")), Ok(()));
    }

    #[test]
    fn suffixes_follow_bruno() {
        assert_eq!(suffixed_name("a", "bru", 0), "a.bru");
        assert_eq!(suffixed_name("a", "bru", 1), "a 1.bru");
        assert_eq!(suffixed_name("Posts", "", 2), "Posts 2");
    }

    #[test]
    fn long_bases_are_truncated_for_suffixes() {
        let base = "a".repeat(300);
        // 255 - 4 (".bru") - 4 (" 199") = 247.
        assert_eq!(truncate_base_for_suffix(&base, "bru").len(), 247);
        assert_eq!(truncate_base_for_suffix(&base, "").len(), 251);
        assert_eq!(truncate_base_for_suffix("court", "bru"), "court");
    }

    #[test]
    fn first_free_name_skips_taken_names() {
        let dir = PathBuf::from("/x");
        let taken = ["/x/a.bru", "/x/a 1.bru"];
        let name = first_free_name(&dir, "a", "bru", |p| {
            taken.contains(&p.to_str().unwrap_or_default())
        })
        .expect("nom libre");
        assert_eq!(name, "a 2.bru");
        assert_eq!(
            first_free_name(&dir, "a", "bru", |_| true),
            Err(NameError::TooManyDuplicates("a".into()))
        );
    }
}
