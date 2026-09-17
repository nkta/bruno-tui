//! Lecture d'un fichier `.env` selon les règles de `dotenv.parse`.
//!
//! `bru` 2.13.2 lit le `.env` de la collection avec `dotenv.parse` 16.6.1,
//! sans expansion de variables. Ce module reproduit son expression :
//!
//! ```text
//! /(?:^|^)\s*(?:export\s+)?([\w.-]+)(?:\s*=\s*?|:\s+?)(\s*'(?:\\'|[^'])*'
//!  |\s*"(?:\\"|[^"])*"|\s*`(?:\\`|[^`])*`|[^#\r\n]+)?\s*(?:#.*)?(?:$|$)/mg
//! ```
//!
//! par un moteur à retour arrière dédié qui respecte l'ordre des
//! alternatives, la gourmandise des quantificateurs, et les classes `\s`,
//! `.`, `^` et `$` au sens JavaScript (mode multiligne), puis son
//! post-traitement (`trim`, guillemets, `\n`/`\r` entre `"`). Aucune
//! valeur n'est jamais formatée : elles sortent en [`SecretString`].

use std::collections::{HashMap, HashSet};

use crate::runner::SecretString;

/// Analyse le contenu d'un `.env` ; la dernière occurrence d'une clé gagne.
pub fn parse(source: &str) -> HashMap<String, SecretString> {
    let normalized = source.replace("\r\n", "\n").replace('\r', "\n");
    let chars: Vec<char> = normalized.chars().collect();
    let mut entries = HashMap::new();
    let mut position = 0;
    while position < chars.len() {
        if !is_line_start(&chars, position) {
            position += 1;
            continue;
        }
        match match_line(&chars, position) {
            Some(found) => {
                let key: String = chars[found.key.0..found.key.1].iter().collect();
                let raw: String = chars[found.value.0..found.value.1].iter().collect();
                entries.insert(key, SecretString::new(post_process(&raw)));
                // Une correspondance n'est jamais vide : la clé ne l'est jamais.
                position = found.end.max(position + 1);
            }
            None => position += 1,
        }
    }
    entries
}

/// Terminateur de ligne au sens JavaScript (`.` ne les reconnaît pas, `^`
/// et `$` s'y ancrent en mode multiligne).
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// Classe `\s` de JavaScript.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0B}'
            | '\u{0C}'
            | '\r'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    ) || ('\u{2000}'..='\u{200A}').contains(&c)
}

/// Classe `[\w.-]` de JavaScript (sans drapeau `u`).
fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')
}

fn is_line_start(chars: &[char], position: usize) -> bool {
    position == 0 || is_line_terminator(chars[position - 1])
}

fn is_line_end(chars: &[char], position: usize) -> bool {
    position == chars.len() || is_line_terminator(chars[position])
}

fn skip_spaces(chars: &[char], mut position: usize) -> usize {
    while position < chars.len() && is_js_space(chars[position]) {
        position += 1;
    }
    position
}

struct LineMatch {
    key: (usize, usize),
    value: (usize, usize),
    end: usize,
}

/// Tente l'expression à partir d'un début de ligne.
fn match_line(chars: &[char], start: usize) -> Option<LineMatch> {
    // `\s*` : seule la longueur maximale peut être suivie d'une clé ou de
    // `export`, qui ne commencent pas par un espace.
    let after_space = skip_spaces(chars, start);
    for with_export in [true, false] {
        let mut key_start = after_space;
        if with_export {
            let word = ['e', 'x', 'p', 'o', 'r', 't'];
            let end = after_space + word.len();
            if end >= chars.len() || chars[after_space..end] != word || !is_js_space(chars[end]) {
                continue;
            }
            key_start = skip_spaces(chars, end);
        }
        let mut key_end = key_start;
        while key_end < chars.len() && is_key_char(chars[key_end]) {
            key_end += 1;
        }
        if key_end == key_start {
            continue;
        }
        let key = (key_start, key_end);

        // `\s*=\s*?` : `\s*` maximal (suivi de `=`), `\s*?` paresseux.
        let equal = skip_spaces(chars, key_end);
        if equal < chars.len() && chars[equal] == '=' {
            let mut value_start = equal + 1;
            loop {
                if let Some((value, end)) = match_value(chars, value_start) {
                    return Some(LineMatch { key, value, end });
                }
                if value_start < chars.len() && is_js_space(chars[value_start]) {
                    value_start += 1;
                } else {
                    break;
                }
            }
        }
        // `:\s+?` : au moins un espace, paresseux.
        if key_end < chars.len() && chars[key_end] == ':' {
            let mut value_start = key_end + 1;
            while value_start < chars.len() && is_js_space(chars[value_start]) {
                value_start += 1;
                if let Some((value, end)) = match_value(chars, value_start) {
                    return Some(LineMatch { key, value, end });
                }
            }
        }
    }
    None
}

/// Groupe de valeur optionnel puis `\s*(?:#.*)?$` ; rend la plage
/// capturée et la fin de la correspondance.
fn match_value(chars: &[char], start: usize) -> Option<((usize, usize), usize)> {
    for quote in ['\'', '"', '`'] {
        let open = skip_spaces(chars, start);
        if open < chars.len() && chars[open] == quote {
            let mut result = None;
            closing_quotes(chars, open + 1, quote, &mut |close| {
                result = match_tail(chars, close + 1).map(|end| ((start, close + 1), end));
                result.is_some()
            });
            if result.is_some() {
                return result;
            }
        }
    }
    let mut unquoted_end = start;
    while unquoted_end < chars.len() && !matches!(chars[unquoted_end], '#' | '\r' | '\n') {
        unquoted_end += 1;
    }
    for end in (start + 1..=unquoted_end).rev() {
        if let Some(tail) = match_tail(chars, end) {
            return Some(((start, end), tail));
        }
    }
    match_tail(chars, start).map(|end| ((start, start), end))
}

/// Énumère, dans l'ordre du retour arrière de `(?:\\q|[^q])*q`, les
/// positions de guillemet fermant, jusqu'à ce que `accept` en retienne
/// une. Les positions déjà explorées sans succès ne sont pas revisitées :
/// elles produiraient la même suite de candidats.
fn closing_quotes(
    chars: &[char],
    start: usize,
    quote: char,
    accept: &mut dyn FnMut(usize) -> bool,
) -> bool {
    enum Step {
        Enter(usize),
        Candidate(usize),
    }
    let mut visited = HashSet::new();
    let mut stack = vec![Step::Enter(start)];
    while let Some(step) = stack.pop() {
        match step {
            Step::Enter(position) => {
                if !visited.insert(position) {
                    continue;
                }
                // Empilés à l'envers : `\\q` d'abord, puis `[^q]`, puis
                // l'arrêt de la répétition sur un guillemet.
                if position < chars.len() && chars[position] == quote {
                    stack.push(Step::Candidate(position));
                }
                if position < chars.len() && chars[position] != quote {
                    stack.push(Step::Enter(position + 1));
                }
                if position + 1 < chars.len()
                    && chars[position] == '\\'
                    && chars[position + 1] == quote
                {
                    stack.push(Step::Enter(position + 2));
                }
            }
            Step::Candidate(position) => {
                if accept(position) {
                    return true;
                }
            }
        }
    }
    false
}

/// `\s*(?:#.*)?(?:$|$)` à partir de `start`.
fn match_tail(chars: &[char], start: usize) -> Option<usize> {
    let spaces_end = skip_spaces(chars, start);
    for position in (start..=spaces_end).rev() {
        if position < chars.len() && chars[position] == '#' {
            let mut end = position + 1;
            while end < chars.len() && !is_line_terminator(chars[end]) {
                end += 1;
            }
            return Some(end);
        }
        if is_line_end(chars, position) {
            return Some(position);
        }
    }
    None
}

/// `trim`, retrait des guillemets englobants, puis `\n`/`\r` si la valeur
/// commençait par `"`.
fn post_process(raw: &str) -> String {
    let trimmed = raw.trim_matches(is_js_space);
    let double_quoted = trimmed.starts_with('"');
    let mut value = strip_quotes(trimmed);
    if double_quoted {
        value = value.replace("\\n", "\n").replace("\\r", "\r");
    }
    value
}

/// `value.replace(/^(['"`])([\s\S]*)\1$/mg, '$2')`.
fn strip_quotes(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut position = 0;
    while position < chars.len() {
        let quote = chars[position];
        if is_line_start(&chars, position) && matches!(quote, '\'' | '"' | '`') {
            let close = (position + 1..chars.len())
                .rev()
                .find(|&end| chars[end] == quote && is_line_end(&chars, end + 1));
            if let Some(close) = close {
                out.extend(&chars[position + 1..close]);
                position = close + 1;
                continue;
            }
        }
        out.push(quote);
        position += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(source: &str) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = parse(source)
            .into_iter()
            .map(|(k, v)| (k, v.expose().to_owned()))
            .collect();
        entries.sort();
        entries
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = expected
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn quotes_and_comments() {
        assert_eq!(
            parsed("A=\"x y\"\nB='z'\n# commentaire\nC=v # note\n"),
            pairs(&[("A", "x y"), ("B", "z"), ("C", "v")])
        );
    }

    #[test]
    fn last_occurrence_wins_and_crlf() {
        assert_eq!(parsed("A=1\r\nA=2\r\n"), pairs(&[("A", "2")]));
    }

    #[test]
    fn debug_of_values_is_masked() {
        let entries = parse("TOKEN=s3cr3t\n");
        assert!(!format!("{entries:?}").contains("s3cr3t"));
    }
}
