//! Affichage et masquage des paramètres d'authentification des requêtes.
//!
//! Les valeurs sont affichées telles qu'écrites dans les fichiers :
//! aucune variable n'est résolue. Les valeurs littérales sensibles
//! (mots de passe, tokens, secrets, clés) sont masquées, tandis que
//! les références à des variables de la forme `{{variable}}` restent visibles.

use ratatui::text::{Line, Span};

use super::detail::DISABLED_ENTRY_VALUE;
use super::theme;
use crate::collection::KeyValue;

/// Détermine si une chaîne est strictement une référence de variable unique
/// de la forme `{{variable}}`.
pub fn is_variable_reference(value: &str) -> bool {
    let trimmed = value.trim();
    if !trimmed.starts_with("{{") || !trimmed.ends_with("}}") {
        return false;
    }
    if trimmed.len() < 5 {
        // Au moins "{{x}}" (5 caractères)
        return false;
    }
    let inner = &trimmed[2..trimmed.len() - 2];
    let inner_trimmed = inner.trim();
    if inner_trimmed.is_empty() {
        return false;
    }
    // Pas d'accolades dans le nom de variable (ex. {{{var}}}, {{a}}{{b}}, etc.)
    if inner.contains('{') || inner.contains('}') {
        return false;
    }
    // Pas de saut de ligne dans le nom de variable
    if inner.contains('\n') || inner.contains('\r') {
        return false;
    }
    true
}

/// Détermine si un nom de clé de paramètre d'auth est considéré sensible.
pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    // Normalisation en retirant les séparateurs usuels (_, -, espaces)
    let norm: String = lower
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == 'é' || *c == 'è')
        .collect();

    // Mots de passe
    if norm.contains("password") || norm.contains("passwd") || norm == "pass" {
        return true;
    }
    // Tokens
    if norm.contains("token") {
        return true;
    }
    // Secrets
    if norm.contains("secret") {
        return true;
    }
    // Clés / API keys / Clé privée
    if norm.contains("apikey")
        || norm.contains("privatekey")
        || norm.contains("authkey")
        || norm == "cle"
        || norm.starts_with("cle")
        || norm.ends_with("cle")
        || lower.contains("clé")
        || lower.contains("clef")
    {
        return true;
    }
    // Valeur de clé API dans auth:apikey ("value")
    if lower == "value" {
        return true;
    }

    false
}

/// Masque une valeur de paramètre d'authentification si la clé est sensible.
///
/// Si la valeur est de la forme `{{variable}}`, elle reste affichée en clair.
/// Si la valeur est littérale et que la clé est sensible, elle est masquée
/// par `********`.
pub fn mask_auth_value(key: &str, value: &str) -> String {
    if !is_sensitive_key(key) {
        return value.to_string();
    }
    if value.is_empty() {
        return String::new();
    }
    if is_variable_reference(value) {
        return value.to_string();
    }
    "********".to_string()
}

/// Rend les lignes de paramètres d'authentification avec masquage.
pub fn render_auth_entries(lines: &mut Vec<Line<'static>>, params: &[KeyValue]) {
    if params.is_empty() {
        lines.push(Line::raw("  aucun"));
        return;
    }
    for entry in params {
        let key = Span::styled(format!("  {}: ", entry.key), theme::LABEL);
        let display_val = mask_auth_value(&entry.key, &entry.value);
        if entry.enabled {
            lines.push(Line::from(vec![key, Span::raw(display_val)]));
        } else {
            lines.push(Line::from(vec![
                key,
                Span::styled(format!("{display_val} (désactivé)"), DISABLED_ENTRY_VALUE),
            ]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_references_detection() {
        // Cas valides
        assert!(is_variable_reference("{{token}}"));
        assert!(is_variable_reference("{{ token }}"));
        assert!(is_variable_reference("  {{token}}  "));
        assert!(is_variable_reference("{{my_secret_var}}"));
        assert!(is_variable_reference("{{process.env.API_KEY}}"));
        assert!(is_variable_reference("{{$guid}}"));
        assert!(is_variable_reference("{{ user.credentials.token }}"));

        // Cas invalides / tordus
        assert!(!is_variable_reference(""));
        assert!(!is_variable_reference("   "));
        assert!(!is_variable_reference("{{}}"));
        assert!(!is_variable_reference("{{ }}"));
        assert!(!is_variable_reference("{{   }}"));
        assert!(!is_variable_reference("{{"));
        assert!(!is_variable_reference("}}"));
        assert!(!is_variable_reference("{{token"));
        assert!(!is_variable_reference("token}}"));
        assert!(!is_variable_reference("{token}"));
        assert!(!is_variable_reference("{{{token}}}"));
        assert!(!is_variable_reference("{{a}}{{b}}"));
        assert!(!is_variable_reference("Bearer {{token}}"));
        assert!(!is_variable_reference("{{token}}-suffix"));
        assert!(!is_variable_reference("prefix-{{token}}"));
        assert!(!is_variable_reference("{{token\nextra}}"));
        assert!(!is_variable_reference("literal_secret"));
    }

    #[test]
    fn sensitive_keys_detection() {
        // Mots de passe
        assert!(is_sensitive_key("password"));
        assert!(is_sensitive_key("Password"));
        assert!(is_sensitive_key("PASSWORD"));
        assert!(is_sensitive_key("user_password"));
        assert!(is_sensitive_key("clientPassword"));
        assert!(is_sensitive_key("passwd"));
        assert!(is_sensitive_key("admin_passwd"));
        assert!(is_sensitive_key("pass"));

        // Tokens
        assert!(is_sensitive_key("token"));
        assert!(is_sensitive_key("access_token"));
        assert!(is_sensitive_key("refresh_token"));
        assert!(is_sensitive_key("id_token"));
        assert!(is_sensitive_key("bearer_token"));
        assert!(is_sensitive_key("session_token"));
        assert!(is_sensitive_key("token_secret"));
        assert!(is_sensitive_key("TOKEN"));

        // Secrets
        assert!(is_sensitive_key("secret"));
        assert!(is_sensitive_key("client_secret"));
        assert!(is_sensitive_key("consumer_secret"));
        assert!(is_sensitive_key("shared_secret"));
        assert!(is_sensitive_key("secret_access_key"));
        assert!(is_sensitive_key("SECRET"));

        // Clés
        assert!(is_sensitive_key("clé"));
        assert!(is_sensitive_key("cle"));
        assert!(is_sensitive_key("cle_api"));
        assert!(is_sensitive_key("clé_secrète"));
        assert!(is_sensitive_key("clef"));
        assert!(is_sensitive_key("apikey"));
        assert!(is_sensitive_key("api_key"));
        assert!(is_sensitive_key("private_key"));
        assert!(is_sensitive_key("privatekey"));

        // Valeur d'API key
        assert!(is_sensitive_key("value"));
        assert!(is_sensitive_key("VALUE"));

        // Non sensibles
        assert!(!is_sensitive_key("username"));
        assert!(!is_sensitive_key("user"));
        assert!(!is_sensitive_key("client_id"));
        assert!(!is_sensitive_key("clientId"));
        assert!(!is_sensitive_key("grant_type"));
        assert!(!is_sensitive_key("scope"));
        assert!(!is_sensitive_key("state"));
        assert!(!is_sensitive_key("redirect_uri"));
        assert!(!is_sensitive_key("key")); // nom du header dans apikey
        assert!(!is_sensitive_key("placement"));
        assert!(!is_sensitive_key("service"));
        assert!(!is_sensitive_key("region"));
    }

    #[test]
    fn masking_behaviour() {
        // Sensible littéral -> masqué
        assert_eq!(mask_auth_value("password", "supersecret"), "********");
        assert_eq!(mask_auth_value("Password", "pass123"), "********");
        assert_eq!(mask_auth_value("token", "eyJh..."), "********");
        assert_eq!(mask_auth_value("client_secret", "cs_abc"), "********");
        assert_eq!(mask_auth_value("value", "apikey123"), "********");
        assert_eq!(mask_auth_value("clé", "secrete"), "********");
        assert_eq!(mask_auth_value("cle", "secrete"), "********");

        // Sensible avec variable -> visible
        assert_eq!(mask_auth_value("password", "{{pwd}}"), "{{pwd}}");
        assert_eq!(mask_auth_value("token", "{{token}}"), "{{token}}");
        assert_eq!(mask_auth_value("token", " {{ token }} "), " {{ token }} ");
        assert_eq!(
            mask_auth_value("client_secret", "{{process.env.SECRET}}"),
            "{{process.env.SECRET}}"
        );
        assert_eq!(mask_auth_value("value", "{{apiKey}}"), "{{apiKey}}");

        // Cas mixtes ou mal formés avec sensible -> masqués
        assert_eq!(mask_auth_value("token", "Bearer {{token}}"), "********");
        assert_eq!(mask_auth_value("token", "{{token}}-suffix"), "********");
        assert_eq!(mask_auth_value("token", "{{}}"), "********");
        assert_eq!(mask_auth_value("password", "{{pass"), "********");

        // Non sensible -> inchangé
        assert_eq!(mask_auth_value("username", "admin"), "admin");
        assert_eq!(mask_auth_value("client_id", "my_app"), "my_app");
        assert_eq!(mask_auth_value("key", "X-API-KEY"), "X-API-KEY");
        assert_eq!(mask_auth_value("placement", "header"), "header");
        assert_eq!(
            mask_auth_value("grant_type", "password"),
            "password" // la valeur est "password" mais la clé "grant_type" n'est pas sensible
        );

        // Chaîne vide
        assert_eq!(mask_auth_value("password", ""), "");
    }
}
