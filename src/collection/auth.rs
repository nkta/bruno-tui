//! Masquage des paramètres d'authentification sensibles pour l'affichage.
//!
//! Les valeurs de paramètres sensibles (mots de passe, tokens, secrets, clés)
//! sont masquées par un placeholder fixe (`********`) lorsqu'elles sont écrites
//! en clair (littérales). Les références à des variables (`{{variable}}`)
//! restent visibles telles quelles, car elles ne contiennent pas de secret en clair
//! dans le fichier affiché.

use super::view::KeyValue;

/// Placeholder appliqué sur les valeurs sensibles en clair.
pub const MASKED_SECRET: &str = "********";

/// Vrai si `value` est exactement UNE référence de variable `{{nom}}`.
///
/// Stricte : `{{a}}secret{{b}}` ou `Bearer {{tok}}` ne sont pas des
/// références seules et restent donc masqués s'ils sont sensibles.
pub fn is_variable_reference(value: &str) -> bool {
    let trimmed = value.trim();
    let Some(inner) = trimmed
        .strip_prefix("{{")
        .and_then(|rest| rest.strip_suffix("}}"))
    else {
        return false;
    };
    !inner.trim().is_empty() && !inner.contains(['{', '}', '\n', '\r'])
}

/// Vrai si le nom d'un paramètre d'auth désigne une valeur sensible.
pub fn is_sensitive_auth_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    // Séparateurs retirés : `client_secret`, `client-secret`, `clientSecret`.
    let norm: String = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    norm.contains("password")
        || norm.contains("passwd")
        || norm.contains("motdepasse")
        || norm == "pass"
        || norm.contains("token")
        || norm.contains("secret")
        || norm.contains("apikey")
        || norm.contains("privatekey")
        || norm.contains("authkey")
        // `key` seul est le nom de l'en-tête d'`auth:apikey`, pas un secret ;
        // `accessKeyId`, `clientKey`... en sont.
        || (norm.contains("key") && norm != "key")
        || norm.starts_with("cle")
        || norm.ends_with("cle")
        || norm.contains("clé")
        || norm.contains("clef")
        // Valeur de la clé dans `auth:apikey`.
        || norm == "value"
}

/// Masque une valeur de paramètre d'authentification si sa clé est sensible et
/// que la valeur n'est pas une référence de variable `{{...}}`.
pub fn mask_auth_value(key: &str, value: &str) -> String {
    if value.trim().is_empty() {
        return value.to_owned();
    }
    if is_sensitive_auth_key(key) && !is_variable_reference(value) {
        MASKED_SECRET.to_owned()
    } else {
        value.to_owned()
    }
}

/// Masque les valeurs sensibles d'une liste de paramètres d'authentification.
pub fn mask_auth_params(params: &[KeyValue]) -> Vec<KeyValue> {
    params
        .iter()
        .map(|param| KeyValue {
            key: param.key.clone(),
            value: mask_auth_value(&param.key, &param.value),
            enabled: param.enabled,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_references_are_detected() {
        assert!(is_variable_reference("{{tok}}"));
        assert!(is_variable_reference("{{ token }}"));
        assert!(is_variable_reference("  {{secret_var}}  "));
        assert!(!is_variable_reference("{{}}"));
        assert!(!is_variable_reference("{{a}}secret{{b}}"));

        assert!(!is_variable_reference("s3cr3t"));
        assert!(!is_variable_reference("Bearer {{tok}}"));
        assert!(!is_variable_reference("{{tok"));
        assert!(!is_variable_reference("tok}}"));
        assert!(!is_variable_reference(""));
    }

    #[test]
    fn sensitive_keys_are_recognized() {
        assert!(is_sensitive_auth_key("password"));
        assert!(is_sensitive_auth_key("Password"));
        assert!(is_sensitive_auth_key("user_password"));
        assert!(is_sensitive_auth_key("token"));
        assert!(is_sensitive_auth_key("accessToken"));
        assert!(is_sensitive_auth_key("secret"));
        assert!(is_sensitive_auth_key("clientSecret"));
        assert!(is_sensitive_auth_key("secretAccessKey"));
        assert!(is_sensitive_auth_key("apiKey"));
        assert!(is_sensitive_auth_key("api_key"));
        assert!(is_sensitive_auth_key("private_key"));
        assert!(is_sensitive_auth_key("value")); // valeur dans auth:apikey

        // Non sensible
        assert!(!is_sensitive_auth_key("username"));
        assert!(!is_sensitive_auth_key("key")); // nom d'en-tête dans auth:apikey (ex: X-API-Key)
        assert!(!is_sensitive_auth_key("placement"));
        assert!(!is_sensitive_auth_key("grant_type"));
        assert!(!is_sensitive_auth_key("callbackUrl"));
    }

    #[test]
    fn masking_replaces_literals_and_keeps_variables() {
        assert_eq!(
            mask_auth_value("password", "mon_mot_de_passe"),
            MASKED_SECRET
        );
        assert_eq!(mask_auth_value("password", "{{env_pass}}"), "{{env_pass}}");
        assert_eq!(mask_auth_value("token", "ghp_1234567890"), MASKED_SECRET);
        assert_eq!(mask_auth_value("token", "{{token}}"), "{{token}}");
        assert_eq!(mask_auth_value("username", "admin"), "admin");
        assert_eq!(mask_auth_value("key", "X-API-Key"), "X-API-Key");
        assert_eq!(mask_auth_value("value", "s3cr3t_api_key"), MASKED_SECRET);
        assert_eq!(mask_auth_value("value", "{{api_key}}"), "{{api_key}}");
        assert_eq!(mask_auth_value("token", ""), "");
    }

    #[test]
    fn mask_auth_params_list() {
        let params = vec![
            KeyValue {
                key: "username".into(),
                value: "alice".into(),
                enabled: true,
            },
            KeyValue {
                key: "password".into(),
                value: "supersecret".into(),
                enabled: true,
            },
            KeyValue {
                key: "token".into(),
                value: "{{my_tok}}".into(),
                enabled: false,
            },
        ];

        let masked = mask_auth_params(&params);
        assert_eq!(masked[0].value, "alice");
        assert_eq!(masked[1].value, MASKED_SECRET);
        assert_eq!(masked[2].value, "{{my_tok}}");
        assert!(!masked[2].enabled);
    }
}
