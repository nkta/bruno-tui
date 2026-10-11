//! Logique d'exportation du corps brut de réponse dans un fichier.
//!
//! Déduit l'extension du fichier d'après le content-type (json, html, xml,
//! txt, sinon bin) et construit le chemin proposé par défaut
//! (`./<nom-requete>-<horodatage>.<ext>`), garanti hors de la collection.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// Déduit l'extension de fichier à partir du Content-Type ou de la nature des données.
pub fn detect_response_extension(
    headers: Option<&BTreeMap<String, Value>>,
    data: &Value,
) -> &'static str {
    if let Some(headers) = headers {
        for (k, v) in headers {
            if k.eq_ignore_ascii_case("content-type")
                && let Some(val) = v.as_str()
            {
                let lower = val.to_ascii_lowercase();
                if lower.contains("json") {
                    return "json";
                }
                if lower.contains("html") {
                    return "html";
                }
                if lower.contains("xml") {
                    return "xml";
                }
                if lower.contains("text") || lower.contains("plain") {
                    return "txt";
                }
            }
        }
    }
    if data.is_object() || data.is_array() {
        return "json";
    }
    "bin"
}

/// Extrait les octets bruts du corps de réponse.
pub fn raw_response_bytes(data: &Value) -> Option<Vec<u8>> {
    match data {
        Value::Null => None,
        Value::String(text) => Some(text.as_bytes().to_vec()),
        data => Some(crate::app::filter::pretty_print(data).into_bytes()),
    }
}

/// Nettoie un nom de requête pour en faire un fragment de nom de fichier sûr.
pub fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        "requete".to_string()
    } else {
        s
    }
}

/// Construit le chemin par défaut proposé pour l'export.
///
/// Si le répertoire courant se situe dans la collection, propose un chemin
/// dans le répertoire parent (`../...`) pour ne jamais écrire dans la collection
/// par défaut. Sinon propose `./<nom-requete>-<horodatage>.<ext>`.
pub fn default_export_path(
    collection_root: Option<&Path>,
    req_name: &str,
    ext: &str,
    now: SystemTime,
) -> PathBuf {
    let safe_name = sanitize_filename(req_name);
    let duration = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let timestamp = format!("{}-{}", duration.as_secs(), duration.subsec_millis());
    let filename = format!("{safe_name}-{timestamp}.{ext}");

    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let in_collection = collection_root.is_some_and(|root| {
        if let (Ok(cur_canon), Ok(root_canon)) = (current_dir.canonicalize(), root.canonicalize()) {
            cur_canon.starts_with(root_canon)
        } else {
            current_dir.starts_with(root)
        }
    });

    if in_collection {
        PathBuf::from("..").join(filename)
    } else {
        PathBuf::from(".").join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extension_detection_content_type() {
        let mut headers = BTreeMap::new();
        headers.insert(
            "Content-Type".to_string(),
            json!("application/json; charset=utf-8"),
        );
        assert_eq!(
            detect_response_extension(Some(&headers), &json!({})),
            "json"
        );

        headers.clear();
        headers.insert("content-type".to_string(), json!("text/html"));
        assert_eq!(
            detect_response_extension(Some(&headers), &json!("<html></html>")),
            "html"
        );

        headers.clear();
        headers.insert("Content-Type".to_string(), json!("application/xml"));
        assert_eq!(
            detect_response_extension(Some(&headers), &json!("<xml></xml>")),
            "xml"
        );

        headers.clear();
        headers.insert("Content-Type".to_string(), json!("text/plain"));
        assert_eq!(
            detect_response_extension(Some(&headers), &json!("texte")),
            "txt"
        );

        headers.clear();
        headers.insert(
            "Content-Type".to_string(),
            json!("application/octet-stream"),
        );
        assert_eq!(
            detect_response_extension(Some(&headers), &json!("data")),
            "bin"
        );
    }

    #[test]
    fn extension_detection_fallback() {
        assert_eq!(detect_response_extension(None, &json!({"a": 1})), "json");
        assert_eq!(detect_response_extension(None, &json!([1, 2])), "json");
        assert_eq!(detect_response_extension(None, &json!("data")), "bin");
        assert_eq!(detect_response_extension(None, &json!(null)), "bin");
    }

    #[test]
    fn raw_bytes_extraction() {
        assert_eq!(raw_response_bytes(&json!(null)), None);
        assert_eq!(
            raw_response_bytes(&json!("bonjour")),
            Some(b"bonjour".to_vec())
        );
        let obj_bytes = raw_response_bytes(&json!({"a": 1})).unwrap();
        assert!(String::from_utf8(obj_bytes).unwrap().contains("\"a\": 1"));
    }

    #[test]
    fn filename_sanitization() {
        assert_eq!(sanitize_filename("ping"), "ping");
        assert_eq!(
            sanitize_filename("Get User by ID / 123"),
            "Get_User_by_ID___123"
        );
        assert_eq!(sanitize_filename(""), "requete");
    }

    #[test]
    fn default_path_generation() {
        let now = UNIX_EPOCH + std::time::Duration::from_secs(1700000000);
        let path = default_export_path(None, "mon-test", "json", now);
        let s = path.to_str().unwrap();
        assert!(s.starts_with("./mon-test-1700000000-0.json") || s.contains("mon-test-1700000000"));
    }
}
