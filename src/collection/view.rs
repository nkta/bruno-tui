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
    /// Interprète un nom de mode (`basic`, `bearer`, ...), identique en
    /// bru-lang et en OpenCollection.
    pub(crate) fn parse(value: &str) -> Self {
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

    /// Nom de mode en chaîne de caractères (`basic`, `bearer`, etc.).
    pub fn as_str(&self) -> &str {
        match self {
            Self::NoAuth => "none",
            Self::Inherit => "inherit",
            Self::Basic => "basic",
            Self::Bearer => "bearer",
            Self::Digest => "digest",
            Self::Ntlm => "ntlm",
            Self::OAuth2 => "oauth2",
            Self::AwsV4 => "awsv4",
            Self::Wsse => "wsse",
            Self::ApiKey => "apikey",
            Self::Other(other) => other.as_str(),
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

/// Données des onglets additionnels (Vars, Docs, Settings), regroupées
/// dans une indirection sur le tas pour préserver la taille de [`RequestView`]
/// et éviter le gonflement des variantes d'énumérations (`TreeNode`, `AppEvent`, `Message`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExtraTabs {
    pub vars_pre_request: Vec<KeyValue>,
    pub vars_post_response: Vec<KeyValue>,
    pub script_pre_request: Option<String>,
    pub script_post_response: Option<String>,
    pub tests: Option<String>,
    pub docs: Option<String>,
    pub settings: Vec<KeyValue>,
    pub assertions: Vec<KeyValue>,
    /// Champs propres à une requête gRPC ; `None` pour les autres types.
    pub grpc: Option<GrpcView>,
    pub auth_params: Vec<KeyValue>,
}

/// Champs propres à une requête gRPC (bloc `grpc` et scripts
/// `script:grpc:*`), tels qu'écrits dans le fichier.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrpcView {
    /// Méthode RPC complète (`/package.Service/Method`).
    pub method: Option<String>,
    /// `unary`, `client-streaming`, `server-streaming` ou `bidi-streaming`.
    pub method_type: Option<String>,
    pub proto_path: Option<String>,
    pub before_call_start: Option<String>,
    pub before_message_send: Option<String>,
    pub after_message_receive: Option<String>,
    pub after_call_end: Option<String>,
}

impl GrpcView {
    /// Les quatre scripts gRPC avec leur libellé, dans l'ordre du cycle
    /// de vie d'un appel.
    pub fn scripts(&self) -> [(&'static str, Option<&str>); 4] {
        [
            ("before-call-start", self.before_call_start.as_deref()),
            ("before-message-send", self.before_message_send.as_deref()),
            (
                "after-message-receive",
                self.after_message_receive.as_deref(),
            ),
            ("after-call-end", self.after_call_end.as_deref()),
        ]
    }

    /// Vrai si au moins un script gRPC est déclaré.
    pub fn has_scripts(&self) -> bool {
        self.scripts()
            .iter()
            .any(|(_, code)| code.is_some_and(|c| !c.trim().is_empty()))
    }
}

/// Un message d'une requête gRPC ou WebSocket (bloc `body:grpc` ou
/// `body:ws`, élément de `message` en YAML).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MessageView {
    pub name: String,
    /// Type de message WebSocket (`text`, `json`, `xml`, `binary`).
    pub kind: String,
    pub selected: bool,
    pub content: String,
}

/// Corps affiché d'une requête gRPC ou WebSocket : le contenu seul pour un
/// message unique anonyme, sinon chaque message précédé d'un en-tête
/// `▸ nom (type, sélectionné)`. `None` sans message.
pub fn messages_body(messages: &[MessageView], kind: &str) -> Option<BodyView> {
    let text = match messages {
        [] => return None,
        [only] if only.name.is_empty() && only.kind.is_empty() && !only.selected => {
            only.content.clone()
        }
        _ => messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                let name = if message.name.is_empty() {
                    format!("message {}", index + 1)
                } else {
                    message.name.clone()
                };
                let mut tags = Vec::new();
                if !message.kind.is_empty() {
                    tags.push(message.kind.as_str());
                }
                if message.selected {
                    tags.push("sélectionné");
                }
                let tags = if tags.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", tags.join(", "))
                };
                format!("▸ {name}{tags}\n{}", message.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    };
    Some(BodyView {
        kind: BodyKind::Other(kind.to_owned()),
        content: BodyContent::Text(text),
    })
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
    pub extra: Box<ExtraTabs>,
}

impl RequestView {
    pub fn vars_pre_request(&self) -> &[KeyValue] {
        &self.extra.vars_pre_request
    }

    pub fn vars_post_response(&self) -> &[KeyValue] {
        &self.extra.vars_post_response
    }

    pub fn script_pre_request(&self) -> Option<&str> {
        self.extra.script_pre_request.as_deref()
    }

    pub fn script_post_response(&self) -> Option<&str> {
        self.extra.script_post_response.as_deref()
    }

    pub fn tests(&self) -> Option<&str> {
        self.extra.tests.as_deref()
    }

    pub fn docs(&self) -> Option<&str> {
        self.extra.docs.as_deref()
    }

    pub fn settings(&self) -> &[KeyValue] {
        &self.extra.settings
    }

    pub fn assertions(&self) -> &[KeyValue] {
        &self.extra.assertions
    }

    pub fn auth_params(&self) -> &[KeyValue] {
        &self.extra.auth_params
    }

    /// Champs gRPC ; `None` hors requête gRPC.
    pub fn grpc(&self) -> Option<&GrpcView> {
        self.extra.grpc.as_ref()
    }

    /// Vrai pour une requête gRPC (`type: grpc`).
    pub fn is_grpc(&self) -> bool {
        self.kind.as_deref() == Some("grpc")
    }

    /// Vrai pour une requête WebSocket (`type: ws` en `.bru`,
    /// `type: websocket` en YAML).
    pub fn is_ws(&self) -> bool {
        matches!(self.kind.as_deref(), Some("ws" | "websocket"))
    }

    /// Libellé du type pour une requête que `bru run` n'exécute pas et
    /// que le writer ne modifie pas (`gRPC`, `WebSocket`) ; `None` sinon.
    pub fn non_http_label(&self) -> Option<&'static str> {
        if self.is_grpc() {
            Some("gRPC")
        } else if self.is_ws() {
            Some("WebSocket")
        } else {
            None
        }
    }

    /// Vrai pour une requête HTTP ou GraphQL : la seule sorte que
    /// `bru run` 4.2.1 exécute et que le writer sait modifier.
    pub fn is_http(&self) -> bool {
        !self.is_grpc() && !self.is_ws()
    }

    /// Construit la vue ; échoue si aucun bloc de méthode HTTP n'existe.
    /// Le type `grpc` ou `ws` se lit dans `meta`, comme le fait Bruno ;
    /// leur bloc principal (`grpc`, `ws`) remplace alors le bloc de méthode.
    pub fn from_ast(file: &BruFile) -> Result<Self, ParseError> {
        match file.entry("meta", "type") {
            Some("grpc") => return Ok(Self::grpc_from_ast(file)),
            Some("ws") => return Ok(Self::ws_from_ast(file)),
            _ => {}
        }
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

        let auth = method_entry("auth").map(AuthMode::parse).or_else(|| {
            file.blocks()
                .find(|b| b.name.starts_with("auth:"))
                .map(|b| AuthMode::parse(&b.name["auth:".len()..]))
        });

        let auth_params = auth_params_of(file, auth.as_ref());

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
            auth,
            has_pre_request_script: file.has_block("script:pre-request"),
            has_post_response_script: file.has_block("script:post-response"),
            has_tests: file.has_block("tests"),
            has_assert: file.has_block("assert"),
            extra: Box::new(ExtraTabs {
                vars_pre_request: key_values(file, "vars:pre-request"),
                vars_post_response: key_values(file, "vars:post-response"),
                script_pre_request: file.text("script:pre-request"),
                script_post_response: file.text("script:post-response"),
                tests: file.text("tests"),
                docs: file.text("docs"),
                settings: key_values(file, "settings"),
                assertions: key_values(file, "assert"),
                grpc: None,
                auth_params,
            }),
        })
    }

    /// Vue d'une requête gRPC : bloc `grpc`, métadonnées `metadata`,
    /// messages `body:grpc` et scripts `script:grpc:*`.
    fn grpc_from_ast(file: &BruFile) -> Self {
        let entry = |key: &str| file.entry("grpc", key).map(str::to_owned);
        let grpc = GrpcView {
            method: entry("method"),
            method_type: entry("methodType"),
            proto_path: entry("protoPath"),
            before_call_start: file.text("script:grpc:before-call-start"),
            before_message_send: file.text("script:grpc:before-message-send"),
            after_message_receive: file.text("script:grpc:after-message-receive"),
            after_call_end: file.text("script:grpc:after-call-end"),
        };
        let mut view = Self::common_from_ast(file, "GRPC", "grpc");
        view.headers = key_values(file, "metadata");
        view.body = messages_body(&messages(file, "body:grpc"), "grpc");
        view.has_pre_request_script |=
            grpc.before_call_start.is_some() || grpc.before_message_send.is_some();
        view.has_post_response_script |=
            grpc.after_message_receive.is_some() || grpc.after_call_end.is_some();
        view.extra.grpc = Some(grpc);
        view
    }

    /// Vue d'une requête WebSocket : bloc `ws`, en-têtes `headers` et
    /// messages `body:ws`.
    fn ws_from_ast(file: &BruFile) -> Self {
        let mut view = Self::common_from_ast(file, "WS", "ws");
        view.body = messages_body(&messages(file, "body:ws"), "ws");
        view
    }

    /// Champs communs à gRPC et WebSocket ; `block` est le bloc principal
    /// qui porte `url` et `auth`.
    fn common_from_ast(file: &BruFile, label: &str, block: &str) -> Self {
        let auth = file.entry(block, "auth").map(AuthMode::parse);
        Self {
            name: meta_name(file),
            kind: file.entry("meta", "type").map(str::to_owned),
            seq: meta_seq(file),
            method: label.to_owned(),
            url: file.entry(block, "url").unwrap_or_default().to_owned(),
            headers: key_values(file, "headers"),
            query_params: key_values(file, "params:query"),
            path_params: key_values(file, "params:path"),
            body: None,
            auth: auth.clone(),
            has_pre_request_script: file.has_block("script:pre-request"),
            has_post_response_script: file.has_block("script:post-response"),
            has_tests: file.has_block("tests"),
            has_assert: file.has_block("assert"),
            extra: Box::new(ExtraTabs {
                vars_pre_request: key_values(file, "vars:pre-request"),
                vars_post_response: key_values(file, "vars:post-response"),
                script_pre_request: file.text("script:pre-request"),
                script_post_response: file.text("script:post-response"),
                tests: file.text("tests"),
                docs: file.text("docs"),
                settings: key_values(file, "settings"),
                assertions: key_values(file, "assert"),
                grpc: None,
                auth_params: auth_params_of(file, auth.as_ref()),
            }),
        }
    }

    /// Construit une vue pour un fichier sans méthode HTTP (`folder.bru`, `collection.bru`).
    pub fn from_file_ast(file: &BruFile) -> Self {
        let meta = FileMeta::from_ast(file.clone());
        Self::from_file_meta(&meta)
    }

    /// Construit une vue de requête minimale à partir d'un `FileMeta`.
    pub fn from_file_meta(meta: &FileMeta) -> Self {
        Self {
            name: meta.name.clone(),
            kind: None,
            seq: meta.seq,
            method: String::new(),
            url: String::new(),
            headers: meta.headers().to_vec(),
            query_params: Vec::new(),
            path_params: Vec::new(),
            body: None,
            auth: meta.auth().cloned(),
            has_pre_request_script: meta.has_pre_request_script,
            has_post_response_script: meta.has_post_response_script,
            has_tests: meta.has_tests,
            has_assert: !meta.assertions().is_empty(),
            extra: Box::new(ExtraTabs {
                vars_pre_request: meta.vars_pre_request().to_vec(),
                vars_post_response: meta.vars_post_response().to_vec(),
                script_pre_request: meta.script_pre_request().map(|s| s.to_owned()),
                script_post_response: meta.script_post_response().map(|s| s.to_owned()),
                tests: meta.tests().map(|s| s.to_owned()),
                docs: meta.docs().map(|s| s.to_owned()),
                settings: meta.settings().to_vec(),
                assertions: meta.assertions().to_vec(),
                grpc: None,
                auth_params: meta.auth_params().to_vec(),
            }),
        }
    }

    /// Vrai si cette vue représente un fichier de dossier ou collection (sans méthode HTTP).
    pub fn is_file_meta(&self) -> bool {
        self.method.is_empty()
    }
}

/// Contenu détaillé des onglets de `folder.bru` ou `collection.bru`,
/// regroupé dans une indirection sur le tas pour préserver la taille de
/// [`FileMeta`] et éviter le gonflement des variantes d'énumérations.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileMetaContent {
    pub headers: Vec<KeyValue>,
    pub auth: Option<AuthMode>,
    pub auth_params: Vec<KeyValue>,
    pub vars_pre_request: Vec<KeyValue>,
    pub vars_post_response: Vec<KeyValue>,
    pub script_pre_request: Option<String>,
    pub script_post_response: Option<String>,
    pub tests: Option<String>,
    pub docs: Option<String>,
    pub settings: Vec<KeyValue>,
    pub assertions: Vec<KeyValue>,
}

/// Méta-données et contenu de `collection.bru` ou `folder.bru`.
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
    /// Présence d'un bloc `vars:pre-request`, `vars:post-response` ou `vars`.
    pub has_vars: bool,
    /// AST source ; absent pour un format de collection autre que `.bru`.
    pub ast: Option<BruFile>,
    pub content: Box<FileMetaContent>,
}

impl FileMeta {
    pub fn headers(&self) -> &[KeyValue] {
        &self.content.headers
    }

    pub fn auth(&self) -> Option<&AuthMode> {
        self.content.auth.as_ref()
    }

    pub fn auth_params(&self) -> &[KeyValue] {
        &self.content.auth_params
    }

    pub fn vars_pre_request(&self) -> &[KeyValue] {
        &self.content.vars_pre_request
    }

    pub fn vars_post_response(&self) -> &[KeyValue] {
        &self.content.vars_post_response
    }

    pub fn settings(&self) -> &[KeyValue] {
        &self.content.settings
    }

    pub fn assertions(&self) -> &[KeyValue] {
        &self.content.assertions
    }

    pub fn script_pre_request(&self) -> Option<&str> {
        self.content.script_pre_request.as_deref()
    }

    pub fn script_post_response(&self) -> Option<&str> {
        self.content.script_post_response.as_deref()
    }

    pub fn tests(&self) -> Option<&str> {
        self.content.tests.as_deref()
    }

    pub fn docs(&self) -> Option<&str> {
        self.content.docs.as_deref()
    }

    pub fn from_ast(file: BruFile) -> Self {
        let headers = key_values(&file, "headers");

        let auth_entry = file.entry("auth", "mode").map(AuthMode::parse);
        let auth = auth_entry.or_else(|| {
            file.blocks()
                .find(|b| b.name.starts_with("auth:"))
                .map(|b| AuthMode::parse(&b.name[5..]))
        });

        let auth_params = match &auth {
            Some(AuthMode::Basic) => key_values(&file, "auth:basic"),
            Some(AuthMode::Bearer) => key_values(&file, "auth:bearer"),
            Some(AuthMode::Digest) => key_values(&file, "auth:digest"),
            Some(AuthMode::Ntlm) => key_values(&file, "auth:ntlm"),
            Some(AuthMode::OAuth2) => key_values(&file, "auth:oauth2"),
            Some(AuthMode::AwsV4) => key_values(&file, "auth:awsv4"),
            Some(AuthMode::Wsse) => key_values(&file, "auth:wsse"),
            Some(AuthMode::ApiKey) => key_values(&file, "auth:apikey"),
            Some(AuthMode::Other(name)) => key_values(&file, &format!("auth:{name}")),
            _ => Vec::new(),
        };

        let mut vars_pre_request = key_values(&file, "vars:pre-request");
        if vars_pre_request.is_empty() {
            vars_pre_request = key_values(&file, "vars");
        }
        let vars_post_response = key_values(&file, "vars:post-response");
        let script_pre_request = file.text("script:pre-request");
        let script_post_response = file.text("script:post-response");
        let tests = file.text("tests");
        let docs = file.text("docs");
        let settings = key_values(&file, "settings");
        let assertions = key_values(&file, "assert");

        let has_auth = auth
            .as_ref()
            .is_some_and(|a| !matches!(a, AuthMode::NoAuth | AuthMode::Inherit))
            || !auth_params.is_empty()
            || file
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
            has_vars: file.has_block("vars:pre-request")
                || file.has_block("vars:post-response")
                || file.has_block("vars"),
            ast: Some(file),
            content: Box::new(FileMetaContent {
                headers,
                auth,
                auth_params,
                vars_pre_request,
                vars_post_response,
                script_pre_request,
                script_post_response,
                tests,
                docs,
                settings,
                assertions,
            }),
        }
    }
}

/// Environnement(s) parent(s) dont hérite cet environnement (`extends`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvironmentExtends {
    /// Héritage simple : `extends: <nom>`.
    Single(String),
    /// Héritage multiple : `extends [ <nom1>, <nom2> ]`.
    List(Vec<String>),
}

impl EnvironmentExtends {
    /// Représentation textuelle de l'héritage pour l'affichage (ex. `base` ou `base, parent`).
    pub fn display_name(&self) -> String {
        match self {
            Self::Single(name) => name.clone(),
            Self::List(names) => names.join(", "),
        }
    }
}

/// Nom d'environnement valide selon Bruno (`validatedEnvironmentName` de
/// `@usebruno/lang`) : non vide, 255 caractères au plus, ni caractère
/// réservé ni nom de périphérique, sans espace ou `-` en tête ni `.` ou
/// espace en fin. Un nom invalide est ignoré par Bruno ; on l'ignore aussi.
pub(crate) fn valid_environment_name(name: &str) -> Option<String> {
    let name = name.trim();
    let reserved_char = |c: char| "<>:\"/\\|?*".contains(c) || c.is_control();
    let upper = name.to_ascii_uppercase();
    let device = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || ["COM", "LPT"].iter().any(|prefix| {
            upper
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.len() == 1 && rest.chars().all(|c| c.is_ascii_digit()))
        });
    let valid = !name.is_empty()
        && name.chars().count() <= 255
        && !device
        && !name.chars().any(reserved_char)
        && !name.starts_with(|c: char| c.is_whitespace() || c == '-')
        && !name.ends_with(|c: char| c.is_whitespace() || c == '.');
    valid.then(|| name.to_owned())
}

/// Interprète le contenu d'un bloc `extends [ ... ]` comme la grammaire de
/// Bruno : entrées séparées par des virgules, soit entre guillemets (avec
/// échappements `\"`, `\\`, `\n`, `\r`, `\t`), soit nues et rognées. Les
/// entrées vides sont ignorées. Une entrée invalide invalide toute la liste.
pub(crate) fn parse_extends_list(content: &str) -> Option<Vec<String>> {
    let mut items: Vec<String> = Vec::new();
    let mut chars = content.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut item = String::new();
        if chars.next_if_eq(&'"').is_some() {
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' => match chars.next() {
                        Some('n') => item.push('\n'),
                        Some('r') => item.push('\r'),
                        Some('t') => item.push('\t'),
                        Some(c @ ('"' | '\\')) => item.push(c),
                        Some(other) => {
                            item.push('\\');
                            item.push(other);
                        }
                        None => item.push('\\'),
                    },
                    other => item.push(other),
                }
            }
            // Texte résiduel jusqu'à la prochaine virgule : ignoré.
            while chars.next_if(|c| *c != ',').is_some() {}
        } else {
            while let Some(c) = chars.next_if(|c| *c != ',') {
                item.push(c);
            }
            item = item.trim().to_owned();
        }
        if !item.is_empty() {
            items.push(item);
        }
        if chars.next().is_none() {
            break;
        }
    }
    if items.is_empty() {
        return None;
    }
    items.iter().map(|n| valid_environment_name(n)).collect()
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
    /// Environnement(s) parent(s) (`extends: base` ou `extends [ ... ]`), non résolu.
    pub extends: Option<EnvironmentExtends>,
    /// Valeur brute déclarée de la couleur (`color: <valeur>`).
    pub color: Option<String>,
    pub ast: Option<BruFile>,
}

impl Environment {
    pub fn from_ast(name: String, path: PathBuf, file: BruFile) -> Self {
        let secret_names = match file.block("vars:secret").map(|b| &b.body) {
            Some(BlockBody::List(items)) => items.clone(),
            _ => Vec::new(),
        };
        let extends = match file.declaration("extends") {
            Some(parent) => valid_environment_name(parent).map(EnvironmentExtends::Single),
            None => match file.block("extends").map(|b| &b.body) {
                Some(BlockBody::Unknown { content }) => {
                    parse_extends_list(file.slice(content)).map(EnvironmentExtends::List)
                }
                _ => None,
            },
        };
        let color = file
            .declaration("color")
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty());
        Self {
            name,
            path,
            variables: key_values(&file, "vars"),
            secret_names,
            extends,
            color,
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

/// Paramètres du bloc `auth:<mode>` du mode déclaré, sinon du premier
/// bloc `auth:*` du fichier.
fn auth_params_of(file: &BruFile, auth: Option<&AuthMode>) -> Vec<KeyValue> {
    let declared = auth
        .map(|mode| key_values(file, &format!("auth:{}", mode.as_str())))
        .unwrap_or_default();
    if !declared.is_empty() {
        return declared;
    }
    file.blocks()
        .find(|b| b.name.starts_with("auth:"))
        .map(|b| key_values(file, &b.name))
        .unwrap_or_default()
}

/// Messages des blocs `block` (`body:grpc` ou `body:ws`), un par bloc,
/// dans l'ordre du fichier (`bodygrpc` / `bodyws` de bru-lang).
fn messages(file: &BruFile, block: &str) -> Vec<MessageView> {
    file.blocks()
        .filter(|b| b.name == block)
        .filter_map(|b| match &b.body {
            BlockBody::Dictionary(entries) => {
                let value = |key: &str| {
                    entries
                        .iter()
                        .find(|entry| entry.key == key)
                        .map(|entry| entry.value.clone())
                        .unwrap_or_default()
                };
                Some(MessageView {
                    name: value("name"),
                    kind: value("type"),
                    selected: value("selected") == "true",
                    content: value("content"),
                })
            }
            _ => None,
        })
        .collect()
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
        assert_eq!(view.assertions().len(), 1);
        assert_eq!(view.assertions()[0].key, "res.status");
        assert_eq!(view.assertions()[0].value, "eq 200");
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
        assert_eq!(meta.auth(), Some(&AuthMode::Bearer));
        assert_eq!(meta.auth_params().len(), 1);
        assert_eq!(meta.auth_params()[0].key, "token");
        assert_eq!(meta.auth_params()[0].value, "{{tok}}");
        assert!(!meta.has_headers);
        assert!(!meta.has_tests);
        assert!(meta.ast.is_some());
    }

    #[test]
    fn folder_meta_full_content_parsed() {
        let source = r#"meta {
  name: MonDossier
  seq: 2
}

headers {
  X-Custom: 123
  ~X-Disabled: true
}

auth {
  mode: basic
}

auth:basic {
  username: user
  password: pwd
}

vars:pre-request {
  a: 1
}

vars:post-response {
  b: 2
}

script:pre-request {
  console.log("pre");
}

script:post-response {
  console.log("post");
}

tests {
  test("ok", () => {});
}

docs {
  Documentation du dossier.
}
"#;
        let file = parse(source);
        let meta = FileMeta::from_ast(file);
        assert_eq!(meta.name.as_deref(), Some("MonDossier"));
        assert_eq!(meta.seq, Some(2));
        assert_eq!(meta.headers().len(), 2);
        assert_eq!(meta.headers()[0].key, "X-Custom");
        assert!(meta.headers()[0].enabled);
        assert_eq!(meta.headers()[1].key, "X-Disabled");
        assert!(!meta.headers()[1].enabled);

        assert_eq!(meta.auth(), Some(&AuthMode::Basic));
        assert_eq!(meta.auth_params().len(), 2);
        assert_eq!(meta.auth_params()[0].key, "username");
        assert_eq!(meta.auth_params()[0].value, "user");
        assert_eq!(meta.auth_params()[1].key, "password");
        assert_eq!(meta.auth_params()[1].value, "pwd");

        assert_eq!(meta.vars_pre_request().len(), 1);
        assert_eq!(meta.vars_pre_request()[0].key, "a");
        assert_eq!(meta.vars_post_response().len(), 1);
        assert_eq!(meta.vars_post_response()[0].key, "b");

        assert_eq!(meta.script_pre_request(), Some("console.log(\"pre\");"));
        assert_eq!(meta.script_post_response(), Some("console.log(\"post\");"));
        assert_eq!(meta.tests(), Some("test(\"ok\", () => {});"));
        assert_eq!(meta.docs(), Some("Documentation du dossier."));

        assert!(meta.has_headers);
        assert!(meta.has_auth);
        assert!(meta.has_pre_request_script);
        assert!(meta.has_post_response_script);
        assert!(meta.has_tests);
        assert!(meta.has_vars);
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

    #[test]
    fn vars_docs_and_settings_views() {
        let source = "get {\n  url: http://x\n}\n\nvars:pre-request {\n  a: 1\n  ~b: 2\n}\n\nvars:post-response {\n  token: abc\n  @order: 42\n  ~@tx: 99\n}\n\nsettings {\n  encodeUrl: true\n  timeout: 5000\n}\n\ndocs {\n  This is documentation.\n  Line 2.\n}\n";
        let file = parse(source);
        let view = RequestView::from_ast(&file).expect("vue");
        assert_eq!(
            view.vars_pre_request(),
            &[
                KeyValue {
                    key: "a".into(),
                    value: "1".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "b".into(),
                    value: "2".into(),
                    enabled: false,
                },
            ]
        );
        assert_eq!(
            view.vars_post_response(),
            &[
                KeyValue {
                    key: "token".into(),
                    value: "abc".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "@order".into(),
                    value: "42".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "@tx".into(),
                    value: "99".into(),
                    enabled: false,
                },
            ]
        );
        assert_eq!(
            view.settings(),
            &[
                KeyValue {
                    key: "encodeUrl".into(),
                    value: "true".into(),
                    enabled: true,
                },
                KeyValue {
                    key: "timeout".into(),
                    value: "5000".into(),
                    enabled: true,
                },
            ]
        );
        assert_eq!(view.docs(), Some("This is documentation.\nLine 2."));
    }

    #[test]
    fn environment_with_single_extends_and_color() {
        let source = "extends: base\nvars {\n  host: staging.example.com\n}\ncolor: #ffaa00\n";
        let file = BruFile::parse_environment(source.to_owned()).expect("valide");
        let env = Environment::from_ast("staging".into(), "environments/staging.bru".into(), file);
        assert_eq!(env.name, "staging");
        assert_eq!(
            env.extends,
            Some(EnvironmentExtends::Single("base".to_owned()))
        );
        assert_eq!(
            env.extends.as_ref().map(|e| e.display_name()),
            Some("base".to_owned())
        );
        assert_eq!(env.color, Some("#ffaa00".to_owned()));
        assert_eq!(env.variables.len(), 1);
        assert_eq!(env.variables[0].key, "host");
    }

    #[test]
    fn environment_with_list_extends_and_quoted_entries() {
        let source = "extends [\n  base,\n  \"custom parent\"\n]\nvars {\n}\ncolor: #00ff00\n";
        let file = BruFile::parse_environment(source.to_owned()).expect("valide");
        let env = Environment::from_ast("multi".into(), "environments/multi.bru".into(), file);
        assert_eq!(
            env.extends,
            Some(EnvironmentExtends::List(vec![
                "base".to_owned(),
                "custom parent".to_owned()
            ]))
        );
        assert_eq!(
            env.extends.as_ref().map(|e| e.display_name()),
            Some("base, custom parent".to_owned())
        );
        assert_eq!(env.color, Some("#00ff00".to_owned()));
    }

    #[test]
    fn extends_list_follows_bruno_grammar() {
        // Entrée entre guillemets contenant une virgule.
        assert_eq!(
            parse_extends_list("  base,\n  \"a, b\"\n"),
            Some(vec!["base".into(), "a, b".into()])
        );
        // L'échappement `\"` est décodé, mais `"` est un caractère interdit
        // dans un nom d'environnement (`invalidNameCharacters`) : liste rejetée.
        assert_eq!(parse_extends_list("base, \"q\\\"x\""), None);
        // Entrée nue avec espace interne, entrées vides ignorées.
        assert_eq!(
            parse_extends_list("custom parent,, other"),
            Some(vec!["custom parent".into(), "other".into()])
        );
        assert_eq!(parse_extends_list("\n"), None);
        // Un nom invalide (séparateur de chemin) invalide toute la liste.
        assert_eq!(parse_extends_list("base, ../x"), None);
    }

    #[test]
    fn invalid_extends_name_and_empty_color_are_ignored() {
        let file =
            BruFile::parse_environment("extends: bad/name\ncolor:\n".to_owned()).expect("valide");
        let env = Environment::from_ast("e".into(), "environments/e.bru".into(), file);
        assert_eq!(env.extends, None);
        assert_eq!(env.color, None);
        assert_eq!(valid_environment_name("CON"), None);
        assert_eq!(valid_environment_name("-x"), None);
        assert_eq!(valid_environment_name("x."), None);
        assert_eq!(valid_environment_name(" base "), Some("base".into()));
    }

    #[test]
    fn grpc_request_view_from_ast() {
        let source = r#"meta {
  name: say hello
  type: grpc
  seq: 1
}

grpc {
  url: grpc://localhost:50051
  method: /hello.Greeter/SayHello
  body: grpc
  protoPath: ./hello.proto
  auth: none
  methodType: unary
}

metadata {
  x-custom-meta: test-value
}

body:grpc {
  name: message 1
  content: '''
    {
      "name": "world"
    }
  '''
}

script:grpc:before-call-start {
  bru.setVar("started", true);
}

tests {
  test("ok", function() {});
}

docs {
  gRPC docs
}
"#;
        let file = parse(source);
        let view = RequestView::from_ast(&file).expect("vue gRPC");
        assert_eq!(view.name.as_deref(), Some("say hello"));
        assert_eq!(view.kind.as_deref(), Some("grpc"));
        assert_eq!(view.method, "GRPC");
        assert!(view.is_grpc());
        assert!(!view.is_ws());
        assert!(!view.is_http());
        assert_eq!(view.url, "grpc://localhost:50051");
        assert_eq!(view.auth, Some(AuthMode::NoAuth));
        let grpc = view.grpc().expect("champs gRPC");
        assert_eq!(grpc.method.as_deref(), Some("/hello.Greeter/SayHello"));
        assert_eq!(grpc.method_type.as_deref(), Some("unary"));
        assert_eq!(grpc.proto_path.as_deref(), Some("./hello.proto"));
        assert_eq!(view.headers.len(), 1);
        assert_eq!(view.headers[0].key, "x-custom-meta");
        assert_eq!(view.headers[0].value, "test-value");
        assert_eq!(
            view.body,
            Some(BodyView {
                kind: BodyKind::Other("grpc".to_owned()),
                content: BodyContent::Text("▸ message 1\n{\n  \"name\": \"world\"\n}".to_owned()),
            })
        );
        assert!(view.has_pre_request_script);
        assert!(!view.has_post_response_script);
        assert_eq!(
            grpc.before_call_start.as_deref(),
            Some("bru.setVar(\"started\", true);")
        );
        assert!(grpc.has_scripts());
        assert!(view.has_tests);
        assert_eq!(view.docs(), Some("gRPC docs"));
    }

    #[test]
    fn ws_request_view_from_ast() {
        let source = r#"meta {
  name: echo websocket
  type: ws
  seq: 2
}

ws {
  url: ws://localhost:8080/ws
  body: ws
  auth: none
}

headers {
  Sec-WebSocket-Protocol: chat
}

body:ws {
  name: hello message
  type: json
  selected: true
  content: '''
    {
      "type": "greet"
    }
  '''
}

script:pre-request {
  bru.setVar("connecting", true);
}
"#;
        let file = parse(source);
        let view = RequestView::from_ast(&file).expect("vue WS");
        assert_eq!(view.name.as_deref(), Some("echo websocket"));
        assert_eq!(view.kind.as_deref(), Some("ws"));
        assert_eq!(view.method, "WS");
        assert!(!view.is_grpc());
        assert!(view.is_ws());
        assert!(!view.is_http());
        assert_eq!(view.grpc(), None);
        assert_eq!(view.url, "ws://localhost:8080/ws");
        assert_eq!(view.headers.len(), 1);
        assert_eq!(view.headers[0].key, "Sec-WebSocket-Protocol");
        assert_eq!(view.headers[0].value, "chat");
        assert_eq!(
            view.body,
            Some(BodyView {
                kind: BodyKind::Other("ws".to_owned()),
                content: BodyContent::Text(
                    "▸ hello message (json, sélectionné)\n{\n  \"type\": \"greet\"\n}".to_owned()
                ),
            })
        );
        assert!(view.has_pre_request_script);
        assert_eq!(
            view.script_pre_request(),
            Some("bru.setVar(\"connecting\", true);")
        );
    }

    #[test]
    fn grpc_type_comes_from_meta_and_metadata_is_not_headers() {
        // Bruno lit le type dans `meta` : un bloc `grpc` dans une requête
        // HTTP ne change rien, et `headers` n'alimente pas une requête gRPC.
        let http = parse("meta {\n  type: http\n}\n\nget {\n  url: x\n}\n\ngrpc {\n  url: y\n}\n");
        let view = RequestView::from_ast(&http).expect("http");
        assert!(view.is_http());
        assert_eq!(view.url, "x");

        let grpc = parse("meta {\n  type: grpc\n}\n\nheaders {\n  a: 1\n}\n");
        let view = RequestView::from_ast(&grpc).expect("grpc sans bloc");
        assert!(view.is_grpc());
        assert_eq!(view.method, "GRPC");
        assert_eq!(view.url, "");
        assert!(view.headers.is_empty());
        assert_eq!(view.body, None);
        assert!(!view.grpc().expect("grpc").has_scripts());
    }

    #[test]
    fn messages_body_layouts() {
        assert_eq!(messages_body(&[], "ws"), None);
        let anonymous = MessageView {
            content: "hello".into(),
            ..MessageView::default()
        };
        assert_eq!(
            messages_body(std::slice::from_ref(&anonymous), "ws").map(|b| b.content),
            Some(BodyContent::Text("hello".into()))
        );
        let named = MessageView {
            name: "bye".into(),
            kind: "text".into(),
            content: "bye".into(),
            ..MessageView::default()
        };
        assert_eq!(
            messages_body(&[anonymous, named], "ws").map(|b| b.content),
            Some(BodyContent::Text(
                "▸ message 1\nhello\n\n▸ bye (text)\nbye".into()
            ))
        );
    }
}
