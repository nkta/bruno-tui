//! Construction et échappement de commandes `curl` à partir des requêtes résolues.
//!
//! Les valeurs proviennent exclusivement du rapport émis par `bru run` (pas
//! d'interpolation locale). L'échappement shell utilise des quotes simples
//! (`'...'`) conformes POSIX : tout caractère y est littéral, et le quote
//! simple est échappé par `'\''`.

use serde_json::Value;

use super::report::RequestInfo;

/// Échappe une chaîne pour le shell POSIX en l'entourant de quotes simples.
///
/// Dans les quotes simples `'...'`, tous les caractères sont interprétés
/// littéralement (espaces, sauts de ligne, variables `$`, caractères spéciaux).
/// Le seul caractère à échapper est le quote simple `'`, qui est remplacé
/// par `'\''`.
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Détermine si un nom d'en-tête HTTP contient des données sensibles ou d'authentification.
///
/// La comparaison est insensible à la casse.
pub fn is_sensitive_header(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "x-api-key"
            | "api-key"
            | "apikey"
            | "x-auth-token"
            | "x-access-token"
            | "token"
            | "secret"
    ) || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("api-key")
        || lower.contains("apikey")
        || lower.contains("auth")
        || lower.contains("cookie")
}

/// Détermine si la requête résolue contient des en-têtes sensibles.
pub fn has_sensitive_headers(request: &RequestInfo) -> bool {
    request
        .headers
        .as_ref()
        .is_some_and(|headers| headers.keys().any(|k| is_sensitive_header(k)))
}

/// Masque la valeur d'un en-tête sensible pour éviter toute fuite dans les logs
/// ou partages non sécurisés.
pub fn mask_header_value(_name: &str, value: &str) -> String {
    let trimmed = value.trim();
    for prefix in &["Bearer ", "bearer ", "Basic ", "basic "] {
        if trimmed
            .strip_prefix(prefix)
            .is_some_and(|rest| !rest.is_empty())
        {
            return format!("{prefix}********");
        }
    }
    "********".to_string()
}

/// Construit une commande `curl` complète à partir de la requête résolue.
///
/// Si `masked` est vrai, la valeur des en-têtes d'authentification et secrets
/// est remplacée par un placeholder (`********`).
///
/// Renvoie `None` si la requête n'a pas d'URL (requête non résolue ou en erreur
/// avant envoi).
pub fn build_curl(request: &RequestInfo, masked: bool) -> Option<String> {
    let url = request.url.as_ref()?;
    let method = request.method.as_deref().unwrap_or("GET");

    let mut parts = vec!["curl".to_string()];

    // Méthode HTTP
    parts.push("-X".to_string());
    parts.push(method.to_string());

    // URL échappée
    parts.push(shell_quote(url));

    // En-têtes résolus
    if let Some(headers) = &request.headers {
        for (key, val) in headers {
            let val_str = match val {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => other.to_string(),
            };
            let formatted_val = if masked && is_sensitive_header(key) {
                mask_header_value(key, &val_str)
            } else {
                val_str
            };
            parts.push("-H".to_string());
            parts.push(shell_quote(&format!("{key}: {formatted_val}")));
        }
    }

    // Corps de requête
    if let Some(data) = &request.data
        && !data.is_null()
    {
        let body_str = match data {
            Value::String(s) => s.clone(),
            other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
        };
        if !body_str.is_empty() {
            parts.push("-d".to_string());
            parts.push(shell_quote(&body_str));
        }
    }

    Some(parts.join(" "))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn shell_quote_handles_simple_and_spaces() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("hello world"), "'hello world'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn shell_quote_handles_quotes_and_newlines() {
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote("'"), "''\\'''");
        assert_eq!(shell_quote("line1\nline2"), "'line1\nline2'");
        assert_eq!(
            shell_quote("it's a\n'test' with spaces"),
            "'it'\\''s a\n'\\''test'\\'' with spaces'"
        );
    }

    #[test]
    fn shell_quote_handles_shell_special_characters() {
        assert_eq!(
            shell_quote("$HOME and `whoami` and \"double\" and \\slash and *;!"),
            "'$HOME and `whoami` and \"double\" and \\slash and *;!'"
        );
    }

    #[test]
    fn sensitive_headers_detection() {
        assert!(is_sensitive_header("Authorization"));
        assert!(is_sensitive_header("authorization"));
        assert!(is_sensitive_header("Cookie"));
        assert!(is_sensitive_header("Set-Cookie"));
        assert!(is_sensitive_header("X-Api-Key"));
        assert!(is_sensitive_header("api-key"));
        assert!(is_sensitive_header("apikey"));
        assert!(is_sensitive_header("x-auth-token"));
        assert!(is_sensitive_header("X-Custom-Token"));
        assert!(is_sensitive_header("my-secret-header"));

        assert!(!is_sensitive_header("Content-Type"));
        assert!(!is_sensitive_header("Accept"));
        assert!(!is_sensitive_header("User-Agent"));
        assert!(!is_sensitive_header("Host"));
    }

    #[test]
    fn build_curl_without_url_returns_none() {
        let req = RequestInfo {
            method: Some("GET".into()),
            url: None,
            headers: None,
            data: None,
        };
        assert_eq!(build_curl(&req, true), None);
    }

    #[test]
    fn build_curl_masked_replaces_secrets() {
        let mut headers = BTreeMap::new();
        headers.insert("Content-Type".into(), json!("application/json"));
        headers.insert("Authorization".into(), json!("Bearer s3cr3t-t0k3n"));
        headers.insert("X-Api-Key".into(), json!("secret-key-123"));
        headers.insert("Cookie".into(), json!("sessionid=abc456"));

        let req = RequestInfo {
            method: Some("POST".into()),
            url: Some("https://api.example.com/data".into()),
            headers: Some(headers),
            data: Some(json!("{\"msg\": \"hello world\"}")),
        };

        let cmd = build_curl(&req, true).expect("commande construite");
        assert!(cmd.contains("curl -X POST 'https://api.example.com/data'"));
        assert!(cmd.contains("-H 'Content-Type: application/json'"));
        assert!(cmd.contains("-H 'Authorization: Bearer ********'"));
        assert!(cmd.contains("-H 'X-Api-Key: ********'"));
        assert!(cmd.contains("-H 'Cookie: ********'"));
        assert!(!cmd.contains("s3cr3t-t0k3n"));
        assert!(!cmd.contains("secret-key-123"));
        assert!(!cmd.contains("sessionid=abc456"));
        assert!(cmd.contains("-d '{\"msg\": \"hello world\"}'"));
    }

    #[test]
    fn build_curl_unmasked_preserves_secrets() {
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".into(), json!("Bearer s3cr3t-t0k3n"));
        headers.insert("X-Api-Key".into(), json!("secret-key-123"));

        let req = RequestInfo {
            method: Some("GET".into()),
            url: Some("https://api.example.com/secure".into()),
            headers: Some(headers),
            data: None,
        };

        let cmd = build_curl(&req, false).expect("commande construite");
        assert!(cmd.contains("-H 'Authorization: Bearer s3cr3t-t0k3n'"));
        assert!(cmd.contains("-H 'X-Api-Key: secret-key-123'"));
    }

    #[test]
    fn build_curl_escapes_edge_cases() {
        let mut headers = BTreeMap::new();
        headers.insert("X-Quote".into(), json!("valeur d'essai"));

        let req = RequestInfo {
            method: Some("POST".into()),
            url: Some("https://example.com/path'with'quotes?a=1&b=2".into()),
            headers: Some(headers),
            data: Some(json!("ligne 1\nligne d'après\n$VAR")),
        };

        let cmd = build_curl(&req, true).expect("commande construite");
        assert!(cmd.contains("'https://example.com/path'\\''with'\\''quotes?a=1&b=2'"));
        assert!(cmd.contains("-H 'X-Quote: valeur d'\\''essai'"));
        assert!(cmd.contains("-d 'ligne 1\nligne d'\\''après\n$VAR'"));
    }
}
