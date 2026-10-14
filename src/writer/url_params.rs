//! Paramètres qu'extrait Bruno desktop 4.2.1 de l'URL d'une nouvelle
//! requête (action `newHttpRequest` du renderer, relevée dans `app.asar`) :
//! `splitOnFirst(url, "?")` puis `parseQueryParams` pour la requête,
//! `parsePathParams` pour le chemin. Table de référence produite par le vrai
//! code : `tests/fixtures/url-params/bruno-url-params.json`.
//!
//! `parsePathParams` passe l'URL à `new URL(...)` (WHATWG) et lit
//! `pathname` ; si `new URL` échoue, il découpe la chaîne entière sur `/`.
//! Ce module reproduit le cas courant (hôte ASCII, port numérique, chemin
//! encodé comme Node) et l'échec certain (port non numérique, hôte vide ou
//! interdit). Tout cas dont l'issue n'est pas certaine (identifiants, IPv6,
//! `%` ou non-ASCII dans l'hôte, adresse IPv4 abrégée, `\`, segments `.`
//! ou `..`, caractères de contrôle) est refusé : jamais d'autre résultat
//! que celui de Bruno.

use thiserror::Error;

/// Paramètre de requête : `value` vaut `None` sans `=` (`?flag`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryParam {
    pub name: String,
    pub value: Option<String>,
}

/// Paramètres extraits, dans l'ordre de Bruno.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UrlParams {
    pub query: Vec<QueryParam>,
    /// Noms des paramètres de chemin, sans doublon.
    pub path: Vec<String>,
}

/// URL dont Bruno tirerait des paramètres que le TUI ne sait pas
/// reproduire à coup sûr.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "URL non prise en charge pour l'extraction des paramètres ({0}) : crée la requête sans, puis complète l'URL (m)"
)]
pub struct UnsupportedUrl(pub &'static str);

/// Paramètres que Bruno écrit pour une nouvelle requête d'URL `url`.
pub fn bruno_url_params(url: &str) -> Result<UrlParams, UnsupportedUrl> {
    Ok(UrlParams {
        query: query_params(query_part(url)),
        path: path_params(url)?,
    })
}

/// `splitOnFirst(url, "?")[1]` : ce qui suit le premier `?` situé hors de
/// `{{…}}` (masquage `/\{\{.*?\}\}/g`).
fn query_part(url: &str) -> Option<&str> {
    let mut index = 0;
    while index < url.len() {
        let rest = &url[index..];
        if rest.starts_with("{{")
            && let Some(close) = rest[2..].find("}}")
        {
            index += 2 + close + 2;
            continue;
        }
        if rest.starts_with('?') {
            return Some(&url[index + 1..]);
        }
        index += rest.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// `parseQueryParams(text)` sans options : pas de décodage, fragment retiré,
/// nom vide ignoré.
fn query_params(text: Option<&str>) -> Vec<QueryParam> {
    let Some(text) = text.filter(|text| !text.is_empty()) else {
        return Vec::new();
    };
    let text = text.split('#').next().unwrap_or_default();
    text.split('&')
        .filter_map(|part| {
            let (name, value) = match part.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (part, None),
            };
            (!name.is_empty()).then(|| QueryParam {
                name: name.to_owned(),
                value,
            })
        })
        .collect()
}

/// Issue de `new URL(...)` pour l'URL préfixée.
enum Parsed {
    /// `pathname`, encodé comme Node.
    Pathname(String),
    /// `new URL` échoue à coup sûr : Bruno découpe la chaîne entière.
    Fails,
}

/// `parsePathParams(url)`.
fn path_params(url: &str) -> Result<Vec<String>, UnsupportedUrl> {
    if url.is_empty() {
        return Ok(Vec::new());
    }
    let prefixed = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_owned()
    } else {
        format!("http://{url}")
    };
    let segments: Vec<String> = match pathname(&prefixed)? {
        Parsed::Pathname(pathname) => pathname.split('/').map(str::to_owned).collect(),
        Parsed::Fails => prefixed.split('/').map(str::to_owned).collect(),
    };
    let mut names: Vec<String> = Vec::new();
    let add = |name: &str, names: &mut Vec<String>| {
        if !name.is_empty() && !names.iter().any(|known| known == name) {
            names.push(name.to_owned());
        }
    };
    for segment in &segments {
        if let Some(name) = segment.strip_prefix(':') {
            add(name, &mut names);
            continue;
        }
        if !is_function_segment(segment) {
            continue;
        }
        for name in colon_names(segment) {
            let name = name.trim_end_matches(['\'', ')', '"', '`']);
            let name = name.trim_start_matches(['(', '\'', '"', '`']);
            add(name, &mut names);
        }
    }
    Ok(names)
}

/// `/^[A-Za-z0-9_.-]+\([^)]*\)$/`.
fn is_function_segment(segment: &str) -> bool {
    let Some(open) = segment.find('(') else {
        return false;
    };
    let prefix = &segment[..open];
    let Some(inner) = segment[open + 1..].strip_suffix(')') else {
        return false;
    };
    !prefix.is_empty()
        && prefix
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
        && !inner.contains(')')
}

/// Captures de `/[:]([a-zA-Z_]\w*)/g`.
fn colon_names(segment: &str) -> Vec<&str> {
    let bytes = segment.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b':'
            && bytes
                .get(index + 1)
                .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        {
            let start = index + 1;
            let mut end = start;
            while bytes
                .get(end)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
            {
                end += 1;
            }
            names.push(&segment[start..end]);
            index = end;
        } else {
            index += 1;
        }
    }
    names
}

/// `new URL(prefixed).pathname`, pour une URL commençant par `http://` ou
/// `https://`.
fn pathname(prefixed: &str) -> Result<Parsed, UnsupportedUrl> {
    if prefixed.chars().any(|c| c.is_control()) {
        return Err(UnsupportedUrl("caractère de contrôle"));
    }
    if prefixed.contains('\\') {
        return Err(UnsupportedUrl("barre oblique inverse"));
    }
    let after_scheme = if prefixed.starts_with("https://") {
        8
    } else {
        7
    };
    let rest = &prefixed[after_scheme..];
    if rest.starts_with('/') {
        return Err(UnsupportedUrl("hôte absent"));
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.contains(['@', '[', ']', '%']) {
        return Err(UnsupportedUrl("hôte avec identifiants, IPv6 ou `%`"));
    }
    let (host, port) = match authority.rfind(':') {
        Some(colon) => (&authority[..colon], Some(&authority[colon + 1..])),
        None => (authority, None),
    };
    if let Some(port) = port.filter(|port| !port.is_empty())
        && !(port.bytes().all(|b| b.is_ascii_digit())
            && port.parse::<u32>().is_ok_and(|p| p <= 65_535))
    {
        return Ok(Parsed::Fails);
    }
    if host.is_empty() || host.contains([' ', '<', '>', '^', '|', ':']) {
        return Ok(Parsed::Fails);
    }
    if !host
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'~' | b'{' | b'}'))
    {
        return Err(UnsupportedUrl("hôte non ASCII ou caractère inhabituel"));
    }
    if ends_in_number(host) && !is_dotted_ipv4(host) {
        return Err(UnsupportedUrl("adresse IPv4 abrégée"));
    }

    let path = &rest[authority_end..];
    let path = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    if path.is_empty() {
        return Ok(Parsed::Pathname("/".to_owned()));
    }
    let mut encoded = String::with_capacity(path.len());
    for (index, segment) in path.split('/').enumerate() {
        if index > 0 {
            encoded.push('/');
        }
        let lower = segment.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "." | ".." | "%2e" | ".%2e" | "%2e." | "%2e%2e"
        ) {
            return Err(UnsupportedUrl("segment `.` ou `..`"));
        }
        for c in segment.chars() {
            if c.is_ascii() && !matches!(c, ' ' | '"' | '<' | '>' | '`' | '{' | '}') {
                encoded.push(c);
            } else {
                let mut buffer = [0; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    encoded.push_str(&format!("%{byte:02X}"));
                }
            }
        }
    }
    Ok(Parsed::Pathname(encoded))
}

/// L'hôte « finit par un nombre » (WHATWG) : il serait lu comme IPv4.
fn ends_in_number(host: &str) -> bool {
    let mut parts: Vec<&str> = host.split('.').collect();
    if parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    let last = parts.last().copied().unwrap_or_default();
    if !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    last.strip_prefix("0x")
        .or_else(|| last.strip_prefix("0X"))
        .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Adresse IPv4 canonique `a.b.c.d` (décimal, sans zéro de tête, ≤ 255).
fn is_dotted_ipv4(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
                && part.parse::<u16>().is_ok_and(|n| n <= 255)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(url: &str) -> Vec<(String, Option<String>)> {
        bruno_url_params(url)
            .expect("extraction")
            .query
            .into_iter()
            .map(|p| (p.name, p.value))
            .collect()
    }

    #[test]
    fn question_marks_inside_variables_are_skipped() {
        assert_eq!(query_part("{{a?b}}/x?c=1"), Some("c=1"));
        assert_eq!(query_part("{{a?b/x?c"), Some("b/x?c"));
        assert_eq!(query_part("{{a}}/x"), None);
        assert_eq!(
            query("{{u}}/x?{{q?}}=1"),
            [("{{q?}}".into(), Some("1".into()))]
        );
    }

    #[test]
    fn query_rules_follow_bruno() {
        assert_eq!(
            query("h/x?a=1=2&flag&=z&&b=#c=3"),
            [
                ("a".into(), Some("1=2".into())),
                ("flag".into(), None),
                ("b".into(), Some(String::new())),
            ]
        );
    }

    #[test]
    fn path_rules_follow_bruno() {
        let path = |url: &str| bruno_url_params(url).expect("extraction").path;
        assert_eq!(path("{{b}}/a/:id/:id/:x?y=:z"), ["id", "x"]);
        assert_eq!(path("{{b}}/Items(a=:k,b=:l)/:"), ["k", "l"]);
        assert_eq!(path("{{b}}/é/:é"), ["%C3%A9"]);
        assert_eq!(path("{{h}}:{{p}}/:id"), ["id"]);
        assert!(path("{{b?}}/:id").is_empty());
        for refused in [
            "http://u@h/:a",
            "http://h/../:a",
            "http://10.1/:a",
            "http://é.fr/:a",
        ] {
            assert!(bruno_url_params(refused).is_err(), "{refused}");
        }
        assert_eq!(path("http://10.0.0.1/:a"), ["a"]);
    }
}
