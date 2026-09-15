//! Vues typées dérivées de l'AST, destinées à l'affichage.
//!
//! Les valeurs sont exposées telles qu'écrites dans le fichier : aucun
//! motif `{{variable}}` n'est résolu et le mode d'auth n'est jamais hérité
//! d'un dossier ou de la collection.

use std::path::PathBuf;

use super::ast::{BlockBody, BruFile, Entry, METHODS};
use super::error::ParseError;

/// Paire clé/valeur avec son état.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    /// Faux pour une clé préfixée de `~`.
    pub enabled: bool,
}

impl From<&Entry> for KeyValue {
    fn from(entry: &Entry) -> Self {
        Self {
            key: entry.key.clone(),
            value: entry.value.clone(),
            enabled: !entry.disabled,
        }
    }
}

/// Mode d'auth déclaré, non résolu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMode {
    /// `none`
    NoAuth,
    /// `inherit` : le mode effectif dépend du dossier ou de la collection,
    /// ce que la vue ne résout pas.
    Inherit,
    Basic,
    Bearer,
    Digest,
    Ntlm,
    OAuth2,
    AwsV4,
    Wsse,
    ApiKey,
    Other(String),
}

impl AuthMode {
    fn parse(value: &str) -> Self {
        match value {
            "none" => Self::NoAuth,
            "inherit" => Self::Inherit,
            "basic" => Self::Basic,
            "bearer" => Self::Bearer,
            "digest" => Self::Digest,
            "ntlm" => Self::Ntlm,
            "oauth2" => Self::OAuth2,
            "awsv4" => Self::AwsV4,
            "wsse" => Self::Wsse,
            "apikey" => Self::ApiKey,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Type de corps déclaré par la clé `body` du bloc de méthode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyKind {
    Json,
    Text,
    Xml,
    Sparql,
    Graphql,
    FormUrlEncoded,
    MultipartForm,
    File,
    Other(String),
}

impl BodyKind {
    fn parse(value: &str) -> Self {
        match value {
            "json" => Self::Json,
            "text" => Self::Text,
            "xml" => Self::Xml,
            "sparql" => Self::Sparql,
            "graphql" => Self::Graphql,
            "formUrlEncoded" => Self::FormUrlEncoded,
            "multipartForm" => Self::MultipartForm,
            "file" => Self::File,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Nom du bloc portant le contenu de ce type de corps.
    fn block_name(&self) -> String {
        match self {
            Self::Json => "body:json".to_owned(),
            Self::Text => "body:text".to_owned(),
            Self::Xml => "body:xml".to_owned(),
            Self::Sparql => "body:sparql".to_owned(),
            Self::Graphql => "body:graphql".to_owned(),
            Self::FormUrlEncoded => "body:form-urlencoded".to_owned(),
            Self::MultipartForm => "body:multipart-form".to_owned(),
            Self::File => "body:file".to_owned(),
            Self::Other(name) => format!("body:{name}"),
        }
    }
}

/// Contenu du bloc de corps correspondant au type déclaré.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyContent {
    /// Bloc texte, désindenté.
    Text(String),
    /// Bloc dictionnaire (formulaires, fichier).
    Entries(Vec<KeyValue>),
    /// Aucun bloc de corps de ce type dans le fichier.
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyView {
    pub kind: BodyKind,
    pub content: BodyContent,
}

/// Vue d'une requête pour l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestView {
    pub name: Option<String>,
    /// Valeur `type` de `meta` (`http`, `graphql`, ...).
    pub kind: Option<String>,
    pub seq: Option<u32>,
    /// Nom du bloc de méthode, en majuscules.
    pub method: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query_params: Vec<KeyValue>,
    pub path_params: Vec<KeyValue>,
    /// `None` quand `body` vaut `none` ou est absent.
    pub body: Option<BodyView>,
    /// `None` quand la clé `auth` est absente du bloc de méthode.
    pub auth: Option<AuthMode>,
    pub has_pre_request_script: bool,
    pub has_post_response_script: bool,
    pub has_tests: bool,
    pub has_assert: bool,
    pub assertions: Vec<KeyValue>,
}

impl RequestView {
    /// Construit la vue ; échoue si aucun bloc de méthode HTTP n'existe.
    pub fn from_ast(file: &BruFile) -> Result<Self, ParseError> {
        let method = file
            .blocks()
            .find(|block| METHODS.contains(&block.name.as_str()))
            .ok_or(ParseError::MissingMethod)?;
        let method_entry = |key: &str| match &method.body {
            BlockBody::Dictionary(entries) => entries
                .iter()
                .find(|entry| entry.key == key)
                .map(|entry| entry.value.as_str()),
            _ => None,
        };

        let body = method_entry("body")
            .filter(|value| !value.is_empty() && *value != "none")
            .map(|value| {
                let kind = BodyKind::parse(value);
                let block = kind.block_name();
                let content = match file.block(&block).map(|b| &b.body) {
                    Some(BlockBody::Text { .. }) => {
                        BodyContent::Text(file.text(&block).unwrap_or_default())
                    }
                    Some(BlockBody::Dictionary(entries)) => {
                        BodyContent::Entries(entries.iter().map(KeyValue::from).collect())
                    }
                    Some(BlockBody::Unknown { content }) => {
                        BodyContent::Text(super::ast::dedent(file.slice(content)))
                    }
                    Some(BlockBody::List(_)) | None => BodyContent::Missing,
                };
                BodyView { kind, content }
            });

        Ok(Self {
            name: meta_name(file),
            kind: file.entry("meta", "type").map(str::to_owned),
            seq: meta_seq(file),
            method: method.name.to_ascii_uppercase(),
            url: method_entry("url").unwrap_or_default().to_owned(),
            headers: key_values(file, "headers"),
            query_params: key_values(file, "params:query"),
            path_params: key_values(file, "params:path"),
            body,
            auth: method_entry("auth").map(AuthMode::parse),
            has_pre_request_script: file.has_block("script:pre-request"),
            has_post_response_script: file.has_block("script:post-response"),
            has_tests: file.has_block("tests"),
            has_assert: file.has_block("assert"),
            assertions: key_values(file, "assert"),
        })
    }
}

/// Méta-données de `collection.bru` ou `folder.bru`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub name: Option<String>,
    pub seq: Option<u32>,
    pub has_headers: bool,
    /// Présence d'un bloc `auth` ou `auth:*`, sans résolution.
    pub has_auth: bool,
    pub has_pre_request_script: bool,
    pub has_post_response_script: bool,
    pub has_tests: bool,
    /// Présence d'un bloc `vars:pre-request` ou `vars:post-response`.
    pub has_vars: bool,
    /// AST source ; absent pour un format de collection autre que `.bru`.
    pub ast: Option<BruFile>,
}

impl FileMeta {
    pub fn from_ast(file: BruFile) -> Self {
        let has_auth = file
            .blocks()
            .any(|b| b.name == "auth" || b.name.starts_with("auth:"));
        Self {
            name: meta_name(&file),
            seq: meta_seq(&file),
            has_headers: file.has_block("headers"),
            has_auth,
            has_pre_request_script: file.has_block("script:pre-request"),
            has_post_response_script: file.has_block("script:post-response"),
            has_tests: file.has_block("tests"),
            has_vars: file.has_block("vars:pre-request") || file.has_block("vars:post-response"),
            ast: Some(file),
        }
    }
}

/// Environnement déclaré dans `environments/*.bru`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Environment {
    /// Nom de fichier sans extension.
    pub name: String,
    /// Chemin relatif à la racine de la collection.
    pub path: PathBuf,
    pub variables: Vec<KeyValue>,
    /// Noms des variables secrètes ; leurs valeurs ne sont jamais dans le
    /// fichier.
    pub secret_names: Vec<String>,
    pub ast: Option<BruFile>,
}

impl Environment {
    pub fn from_ast(name: String, path: PathBuf, file: BruFile) -> Self {
        let secret_names = match file.block("vars:secret").map(|b| &b.body) {
            Some(BlockBody::List(items)) => items.clone(),
            _ => Vec::new(),
        };
        Self {
            name,
            path,
            variables: key_values(&file, "vars"),
            secret_names,
            ast: Some(file),
        }
    }
}

fn key_values(file: &BruFile, block: &str) -> Vec<KeyValue> {
    file.dictionary(block)
        .map(|entries| entries.iter().map(KeyValue::from).collect())
        .unwrap_or_default()
}

fn meta_name(file: &BruFile) -> Option<String> {
    file.entry("meta", "name").map(str::to_owned)
}

fn meta_seq(file: &BruFile) -> Option<u32> {
    file.entry("meta", "seq").and_then(|seq| seq.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> BruFile {
        BruFile::parse(source.to_owned()).expect("source valide")
    }

    #[test]
    fn simple_get_keeps_variables_unresolved() {
        let file = parse(
            "meta {\n  name: ping\n  type: http\n  seq: 1\n}\n\nget {\n  url: https://{{host}}/ping\n  body: none\n  auth: none\n}\n",
        );
        let view = RequestView::from_ast(&file).expect("vue");
        assert_eq!(view.name.as_deref(), Some("ping"));
        assert_eq!(view.kind.as_deref(), Some("http"));
        assert_eq!(view.seq, Some(1));
        assert_eq!(view.method, "GET");
        assert_eq!(view.url, "https://{{host}}/ping");
        assert_eq!(view.body, None);
        assert_eq!(view.auth, Some(AuthMode::NoAuth));
        assert!(!view.has_pre_request_script);
        assert!(!view.has_post_response_script);
        assert!(!view.has_tests);
        assert!(!view.has_assert);
        assert!(view.headers.is_empty());
    }

    #[test]
    fn post_json_body() {
        let file = parse(
            "post {\n  url: http://x\n  body: json\n  auth: none\n}\n\nbody:json {\n  {\n    \"a\": 1\n  }\n}\n",
        );
        let view = RequestView::from_ast(&file).expect("vue");
        assert_eq!(view.method, "POST");
        assert_eq!(
            view.body,
            Some(BodyView {
                kind: BodyKind::Json,
                content: BodyContent::Text("{\n  \"a\": 1\n}".to_owned()),
            })
        );
    }

    #[test]
    fn form_body_and_missing_body_block() {
        let file = parse(
            "put {\n  url: http://x\n  body: formUrlEncoded\n}\n\nbody:form-urlencoded {\n  a: 1\n  ~b: 2\n}\n",
        );
        let view = RequestView::from_ast(&file).expect("vue");
        let body = view.body.expect("corps");
        assert_eq!(body.kind, BodyKind::FormUrlEncoded);
        assert_eq!(
            body.content,
            BodyContent::Entries(vec![
                KeyValue {
                    key: "a".into(),
                    value: "1".into(),
                    enabled: true
                },
                KeyValue {
                    key: "b".into(),
                    value: "2".into(),
                    enabled: false
                },
            ])
        );

        let file = parse("post {\n  url: http://x\n  body: xml\n}\n");
        let body = RequestView::from_ast(&file)
            .expect("vue")
            .body
            .expect("corps");
        assert_eq!(body.kind, BodyKind::Xml);
        assert_eq!(body.content, BodyContent::Missing);
    }

    #[test]
    fn inherited_auth_is_exposed_as_declared() {
        let file =
            parse("get {\n  url: http://x\n  auth: inherit\n}\n\nauth:bearer {\n  token: t\n}\n");
        let view = RequestView::from_ast(&file).expect("vue");
        assert_eq!(view.auth, Some(AuthMode::Inherit));
    }

    #[test]
    fn scripts_tests_and_assertions_presence() {
        let file = parse(
            "get {\n  url: http://x\n}\n\nassert {\n  res.status: eq 200\n}\n\nscript:pre-request {\n  a();\n}\n\nscript:post-response {\n  b();\n}\n\ntests {\n  c();\n}\n",
        );
        let view = RequestView::from_ast(&file).expect("vue");
        assert!(view.has_pre_request_script);
        assert!(view.has_post_response_script);
        assert!(view.has_tests);
        assert!(view.has_assert);
        assert_eq!(view.assertions.len(), 1);
        assert_eq!(view.assertions[0].key, "res.status");
        assert_eq!(view.assertions[0].value, "eq 200");
    }

    #[test]
    fn missing_method() {
        let file = parse("meta {\n  name: a\n}\n\nheaders {\n  A: b\n}\n");
        let error = RequestView::from_ast(&file).expect_err("sans méthode");
        assert!(matches!(error, ParseError::MissingMethod));
    }

    #[test]
    fn non_numeric_seq_is_none() {
        let file = parse("meta {\n  seq: abc\n}\n\nget {\n  url: x\n}\n");
        assert_eq!(RequestView::from_ast(&file).expect("vue").seq, None);
    }

    #[test]
    fn folder_meta_with_bearer_auth() {
        let file =
            parse("meta {\n  name: Groupe\n  seq: 3\n}\n\nauth:bearer {\n  token: {{tok}}\n}\n");
        let meta = FileMeta::from_ast(file);
        assert_eq!(meta.name.as_deref(), Some("Groupe"));
        assert_eq!(meta.seq, Some(3));
        assert!(meta.has_auth);
        assert!(!meta.has_headers);
        assert!(!meta.has_tests);
        assert!(meta.ast.is_some());
    }

    #[test]
    fn environment_with_secret_names() {
        let file =
            parse("vars {\n  host: localhost\n  ~debug: true\n}\n\nvars:secret [\n  token\n]\n");
        let env = Environment::from_ast("local".into(), "environments/local.bru".into(), file);
        assert_eq!(env.name, "local");
        assert_eq!(
            env.variables,
            vec![
                KeyValue {
                    key: "host".into(),
                    value: "localhost".into(),
                    enabled: true
                },
                KeyValue {
                    key: "debug".into(),
                    value: "true".into(),
                    enabled: false
                },
            ]
        );
        assert_eq!(env.secret_names, ["token"]);
    }
}
