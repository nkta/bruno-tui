//! Popup « Réglages » de la collection (`docs/reglages-collection.md`) :
//! `presets`, `proxy` et `clientCertificates` de `bruno.json`.
//!
//! État et logique purs, sans I/O : le popup tient des formulaires en
//! mémoire et produit, au `Ctrl+S`, la valeur JSON de chaque clé modifiée.
//! `writer::bruno_config` écrit ensuite la seule valeur de chaque clé.
//!
//! Secrets (D3-A) : un mot de passe de proxy ou une passphrase littéral est
//! refusé ; seuls `{{variable}}` et la valeur vide passent. La valeur
//! refusée est oubliée aussitôt et n'apparaît dans aucun message.

use std::path::{Component, Path, PathBuf};

use super::message::SettingsKey;
use super::text_input::TextInput;
use crate::writer::bruno_config::{Json, convert_legacy_proxy, is_legacy_proxy};

/// Types de requête de `presets.requestType` (les quatre valeurs de Bruno).
pub const REQUEST_TYPES: [&str; 4] = ["http", "graphql", "grpc", "ws"];
/// Protocoles de proxy de Bruno.
pub const PROTOCOLS: [&str; 4] = ["http", "https", "socks4", "socks5"];
/// Longueur maximale des textes du proxy, comme Bruno.
pub const MAX_TEXT: usize = 1024;

/// Partie du popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPart {
    Presets,
    Proxy,
    Certs,
}

impl SettingsPart {
    pub const ALL: [SettingsPart; 3] = [
        SettingsPart::Presets,
        SettingsPart::Proxy,
        SettingsPart::Certs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsPart::Presets => "Presets",
            SettingsPart::Proxy => "Proxy",
            SettingsPart::Certs => "Certificats",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    fn previous(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Ligne d'un formulaire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    PresetType,
    PresetUrl,
    PresetEnv,
    ProxyMode,
    ProxyProtocol,
    ProxyHost,
    ProxyPort,
    ProxyAuth,
    ProxyUser,
    ProxyPassword,
    ProxyBypass,
}

impl Row {
    /// Ligne à valeur choisie dans une liste (`←→` ou `Entrée`).
    pub fn is_choice(self) -> bool {
        matches!(
            self,
            Row::PresetType | Row::PresetEnv | Row::ProxyMode | Row::ProxyProtocol
        )
    }

    /// Ligne qui porte un secret.
    pub fn is_secret(self) -> bool {
        self == Row::ProxyPassword
    }

    /// Libellé affiché.
    pub fn label(self) -> &'static str {
        match self {
            Row::PresetType => "Type de requête",
            Row::PresetUrl => "URL par défaut",
            Row::PresetEnv => "Environnement",
            Row::ProxyMode => "Mode",
            Row::ProxyProtocol => "Protocole",
            Row::ProxyHost => "Hôte",
            Row::ProxyPort => "Port",
            Row::ProxyAuth => "Authentification",
            Row::ProxyUser => "Utilisateur",
            Row::ProxyPassword => "Mot de passe",
            Row::ProxyBypass => "Contournement",
        }
    }
}

/// Mode du proxy (§ 1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyMode {
    Inherit,
    Enabled,
    Disabled,
}

impl ProxyMode {
    const ALL: [ProxyMode; 3] = [ProxyMode::Inherit, ProxyMode::Enabled, ProxyMode::Disabled];

    pub fn label(self) -> &'static str {
        match self {
            ProxyMode::Inherit => "inherit",
            ProxyMode::Enabled => "enabled",
            ProxyMode::Disabled => "disabled",
        }
    }
}

/// Valeurs du formulaire « Presets ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresetsForm {
    pub request_type: String,
    pub url: String,
    pub environment: Option<String>,
}

/// Valeurs du formulaire « Proxy ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyForm {
    pub mode: ProxyMode,
    pub protocol: String,
    pub host: String,
    pub port: String,
    pub auth: bool,
    pub user: String,
    pub password: String,
    pub bypass: String,
}

/// Vrai si `text` est entièrement une variable `{{…}}`, comme la règle de
/// Bruno `/^\s*\{\{.*\}\}\s*$/`.
pub fn is_variable_ref(text: &str) -> bool {
    let inner = text.trim();
    inner.len() >= 4
        && inner.starts_with("{{")
        && inner.ends_with("}}")
        && !inner.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// Secret acceptable : vide ou variable.
fn secret_allowed(text: &str) -> bool {
    text.is_empty() || is_variable_ref(text)
}

/// Longueur comme `String.length` de JavaScript (unités UTF-16).
fn js_length(text: &str) -> usize {
    text.encode_utf16().count()
}

fn default_proxy() -> Json {
    Json::parse(
        r#"{"inherit":true,"config":{"protocol":"http","hostname":"","port":"","auth":{"username":"","password":""},"bypassProxy":""}}"#,
    )
    .unwrap_or(Json::Null)
}

fn default_presets() -> Json {
    Json::Obj(vec![
        ("requestType".into(), Json::Str("http".into())),
        ("requestUrl".into(), Json::Str(String::new())),
    ])
}

fn text_of(value: Option<&Json>) -> String {
    match value {
        Some(Json::Str(text)) => text.clone(),
        Some(Json::Num(raw)) => raw.clone(),
        _ => String::new(),
    }
}

fn truthy(value: Option<&Json>) -> bool {
    match value {
        None | Some(Json::Null | Json::Bool(false)) => false,
        Some(Json::Str(text)) => !text.is_empty(),
        Some(Json::Num(raw)) => raw.parse::<f64>() != Ok(0.0),
        Some(_) => true,
    }
}

fn presets_form(value: &Json) -> PresetsForm {
    let request_type = text_of(value.get("requestType"));
    PresetsForm {
        request_type: if request_type.is_empty() {
            "http".into()
        } else {
            request_type
        },
        url: text_of(value.get("requestUrl")),
        environment: match value.get("defaultEnvironment") {
            Some(Json::Str(name)) if !name.is_empty() => Some(name.clone()),
            _ => None,
        },
    }
}

fn proxy_form(value: &Json) -> ProxyForm {
    let config = value.get("config");
    let auth = config.and_then(|c| c.get("auth"));
    let mode = if truthy(value.get("disabled")) {
        ProxyMode::Disabled
    } else if value.get("inherit").and_then(Json::as_bool) == Some(true) {
        ProxyMode::Inherit
    } else {
        ProxyMode::Enabled
    };
    let protocol = text_of(config.and_then(|c| c.get("protocol")));
    ProxyForm {
        mode,
        protocol: if protocol.is_empty() {
            "http".into()
        } else {
            protocol
        },
        host: text_of(config.and_then(|c| c.get("hostname"))),
        port: text_of(config.and_then(|c| c.get("port"))),
        auth: !truthy(auth.and_then(|a| a.get("disabled"))),
        user: text_of(auth.and_then(|a| a.get("username"))),
        password: text_of(auth.and_then(|a| a.get("password"))),
        bypass: text_of(config.and_then(|c| c.get("bypassProxy"))),
    }
}

/// Valeur de `proxy` après les changements du formulaire, comme les
/// gestionnaires du renderer : `{...proxy, ...changement}`, une clé
/// existante garde sa place, une clé nouvelle est ajoutée à la fin.
fn proxy_json(base: &Json, initial: &ProxyForm, now: &ProxyForm) -> Json {
    let mut proxy = if matches!(base, Json::Obj(_)) {
        base.clone()
    } else {
        default_proxy()
    };
    if now.mode != initial.mode {
        let (disabled, inherit) = match now.mode {
            ProxyMode::Inherit => (false, true),
            ProxyMode::Enabled => (false, false),
            ProxyMode::Disabled => (true, false),
        };
        proxy.set("disabled", Json::Bool(disabled));
        proxy.set("inherit", Json::Bool(inherit));
    }
    let text_changes = [
        ("protocol", &now.protocol, &initial.protocol),
        ("hostname", &now.host, &initial.host),
        ("bypassProxy", &now.bypass, &initial.bypass),
    ];
    for (key, value, before) in text_changes {
        if value != before {
            proxy
                .object_entry("config")
                .set(key, Json::Str(value.clone()));
        }
    }
    if now.port != initial.port {
        let port = match now.port.parse::<u32>() {
            Ok(number) => Json::Num(number.to_string()),
            Err(_) => Json::Str(String::new()),
        };
        proxy.object_entry("config").set("port", port);
    }
    let config = proxy.object_entry("config");
    if now.auth != initial.auth {
        config
            .object_entry("auth")
            .set("disabled", Json::Bool(!now.auth));
    }
    if now.user != initial.user {
        config
            .object_entry("auth")
            .set("username", Json::Str(now.user.clone()));
    }
    if now.password != initial.password {
        config
            .object_entry("auth")
            .set("password", Json::Str(now.password.clone()));
    }
    proxy
}

/// Valeur de `presets` après les changements du formulaire.
fn presets_json(base: &Json, initial: &PresetsForm, now: &PresetsForm) -> Json {
    let mut presets = if matches!(base, Json::Obj(_)) {
        base.clone()
    } else {
        default_presets()
    };
    if now.request_type != initial.request_type {
        presets.set("requestType", Json::Str(now.request_type.clone()));
    }
    if now.url != initial.url {
        presets.set("requestUrl", Json::Str(now.url.clone()));
    }
    if now.environment != initial.environment {
        match &now.environment {
            Some(name) => presets.set("defaultEnvironment", Json::Str(name.clone())),
            None => presets.remove("defaultEnvironment"),
        }
    }
    presets
}

/// Un certificat de la liste, lu pour l'affichage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertSummary {
    pub domain: String,
    pub kind: String,
    pub path: String,
    pub disabled: bool,
}

fn cert_summary(entry: &Json) -> CertSummary {
    let kind = text_of(entry.get("type"));
    let path_key = if kind == "pfx" {
        "pfxFilePath"
    } else {
        "certFilePath"
    };
    CertSummary {
        domain: text_of(entry.get("domain")),
        path: text_of(entry.get(path_key)),
        kind,
        disabled: truthy(entry.get("disabled")),
    }
}

/// Type d'un nouveau certificat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertKind {
    Cert,
    Pfx,
}

impl CertKind {
    pub fn label(self) -> &'static str {
        match self {
            CertKind::Cert => "cert",
            CertKind::Pfx => "pfx",
        }
    }
}

/// Ligne du sous-popup « Nouveau certificat ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertField {
    Domain,
    Kind,
    CertFile,
    KeyFile,
    PfxFile,
    Passphrase,
}

impl CertField {
    pub fn label(self) -> &'static str {
        match self {
            CertField::Domain => "Domaine",
            CertField::Kind => "Type",
            CertField::CertFile => "Certificat",
            CertField::KeyFile => "Clé",
            CertField::PfxFile => "Fichier PFX",
            CertField::Passphrase => "Passphrase",
        }
    }
}

/// Sous-popup d'ajout d'un certificat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCert {
    pub kind: CertKind,
    pub cursor: usize,
    pub domain: TextInput,
    pub cert: TextInput,
    pub key: TextInput,
    pub pfx: TextInput,
    pub passphrase: TextInput,
    pub error: Option<String>,
}

impl NewCert {
    fn new() -> Self {
        let empty = || TextInput::new("", false);
        Self {
            kind: CertKind::Cert,
            cursor: 0,
            domain: empty(),
            cert: empty(),
            key: empty(),
            pfx: empty(),
            passphrase: empty(),
            error: None,
        }
    }

    pub fn fields(&self) -> Vec<CertField> {
        match self.kind {
            CertKind::Cert => vec![
                CertField::Domain,
                CertField::Kind,
                CertField::CertFile,
                CertField::KeyFile,
                CertField::Passphrase,
            ],
            CertKind::Pfx => vec![
                CertField::Domain,
                CertField::Kind,
                CertField::PfxFile,
                CertField::Passphrase,
            ],
        }
    }

    pub fn current(&self) -> CertField {
        self.fields()
            .get(self.cursor)
            .copied()
            .unwrap_or(CertField::Domain)
    }

    pub fn input(&self, field: CertField) -> Option<&TextInput> {
        match field {
            CertField::Domain => Some(&self.domain),
            CertField::Kind => None,
            CertField::CertFile => Some(&self.cert),
            CertField::KeyFile => Some(&self.key),
            CertField::PfxFile => Some(&self.pfx),
            CertField::Passphrase => Some(&self.passphrase),
        }
    }

    fn input_mut(&mut self, field: CertField) -> Option<&mut TextInput> {
        match field {
            CertField::Domain => Some(&mut self.domain),
            CertField::Kind => None,
            CertField::CertFile => Some(&mut self.cert),
            CertField::KeyFile => Some(&mut self.key),
            CertField::PfxFile => Some(&mut self.pfx),
            CertField::Passphrase => Some(&mut self.passphrase),
        }
    }

    /// Change le type ; les champs de l'autre type sont vidés, comme Bruno.
    fn toggle_kind(&mut self) {
        self.kind = match self.kind {
            CertKind::Cert => CertKind::Pfx,
            CertKind::Pfx => CertKind::Cert,
        };
        let empty = || TextInput::new("", false);
        self.cert = empty();
        self.key = empty();
        self.pfx = empty();
    }
}

/// Question en attente dans la barre de statut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsConfirm {
    /// Supprimer le certificat d'indice donné.
    DeleteCert(usize),
    /// Abandonner les modifications.
    Discard,
}

/// Ce que `update` doit faire après une touche.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Save,
    Close,
}

/// Saisie de texte en cours sur une ligne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveInput {
    pub row: Row,
    pub input: TextInput,
}

/// Chemin de certificat tel que Bruno l'écrit : un chemin absolu devient
/// relatif à la racine (`path.relative`), un chemin relatif est gardé.
pub fn cert_path(root: &Path, text: &str) -> String {
    let path = Path::new(text);
    if !path.is_absolute() {
        return text.to_owned();
    }
    relative_path(root, path)
}

fn normalized(path: &Path) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop();
            }
            _ => {}
        }
    }
    parts
}

/// `path.relative(from, to)` de Node, sans accès disque.
pub fn relative_path(from: &Path, to: &Path) -> String {
    let from = normalized(from);
    let to = normalized(to);
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".into(); from.len() - common];
    parts.extend(to[common..].iter().cloned());
    parts.join("/")
}

/// Session du popup « Réglages ».
#[derive(Debug, Clone)]
pub struct SettingsSession {
    /// Racine de la collection (pour les chemins de certificats).
    pub root: PathBuf,
    /// Nom affiché de la collection.
    pub name: String,
    /// Instantané de fraîcheur de `bruno.json` à l'ouverture.
    pub stamp: crate::writer::FileStamp,
    pub part: SettingsPart,
    cursors: [usize; 3],
    /// Noms des environnements de la collection.
    pub environments: Vec<String>,
    presets_base: Json,
    presets_initial: PresetsForm,
    pub presets: PresetsForm,
    proxy_base: Json,
    proxy_initial: ProxyForm,
    pub proxy: ProxyForm,
    /// Le proxy du fichier est à l'ancien format : converti à l'affichage.
    pub legacy_proxy: bool,
    certs_initial: Vec<Json>,
    certs: Vec<Json>,
    /// Chemins des certificats ajoutés pendant la session.
    pub added_paths: Vec<String>,
    pub input: Option<ActiveInput>,
    pub add: Option<NewCert>,
    pub confirm: Option<SettingsConfirm>,
    /// Affiche les variables `{{…}}` des secrets (`Ctrl+R`).
    pub reveal: bool,
    /// Refus affiché dans le popup ; ne contient jamais une valeur.
    pub error: Option<String>,
    /// Écriture demandée, résultat pas encore reçu : les touches sont ignorées.
    pub saving: bool,
}

impl SettingsSession {
    /// Ouvre une session sur le texte de `bruno.json`.
    pub fn open(
        root: PathBuf,
        name: String,
        stamp: crate::writer::FileStamp,
        text: &str,
        environments: Vec<String>,
    ) -> Result<Self, crate::writer::bruno_config::ConfigError> {
        let file = Json::parse(text)?;
        if !matches!(file, Json::Obj(_)) {
            return Err(crate::writer::bruno_config::ConfigError::NotAnObject);
        }
        let presets_base = file.get("presets").cloned().unwrap_or(Json::Null);
        let presets_form = presets_form(if matches!(presets_base, Json::Obj(_)) {
            &presets_base
        } else {
            &Json::Null
        });
        let proxy_raw = file.get("proxy").cloned();
        let legacy_proxy = proxy_raw.as_ref().is_some_and(is_legacy_proxy);
        let proxy_base = match &proxy_raw {
            Some(raw) if matches!(raw, Json::Obj(_)) => convert_legacy_proxy(raw),
            _ => default_proxy(),
        };
        let proxy_form = proxy_form(&proxy_base);
        let certs = match file.get("clientCertificates").and_then(|c| c.get("certs")) {
            Some(Json::Arr(items)) => items.clone(),
            _ => Vec::new(),
        };
        Ok(Self {
            root,
            name,
            stamp,
            part: SettingsPart::Presets,
            cursors: [0; 3],
            environments,
            presets_base,
            presets_initial: presets_form.clone(),
            presets: presets_form,
            proxy_base,
            proxy_initial: proxy_form.clone(),
            proxy: proxy_form,
            legacy_proxy,
            certs_initial: certs.clone(),
            certs,
            added_paths: Vec::new(),
            input: None,
            add: None,
            confirm: None,
            reveal: false,
            error: None,
            saving: false,
        })
    }

    pub fn title(&self) -> String {
        format!(" Réglages — {} ", self.name)
    }

    // --- État ---------------------------------------------------------

    pub fn presets_dirty(&self) -> bool {
        self.presets != self.presets_initial
    }

    pub fn proxy_dirty(&self) -> bool {
        self.proxy != self.proxy_initial
    }

    pub fn certs_dirty(&self) -> bool {
        self.certs != self.certs_initial
    }

    /// Au moins une valeur a changé par rapport au fichier.
    pub fn has_unsaved(&self) -> bool {
        self.presets_dirty() || self.proxy_dirty() || self.certs_dirty()
    }

    /// Certificats, lus pour l'affichage.
    pub fn cert_summaries(&self) -> Vec<CertSummary> {
        self.certs.iter().map(cert_summary).collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursors[self.part.index()]
    }

    /// Lignes sélectionnables de la partie courante (les lignes grisées du
    /// proxy n'en font pas partie).
    pub fn selectable_rows(&self) -> Vec<Row> {
        match self.part {
            SettingsPart::Presets => vec![Row::PresetType, Row::PresetUrl, Row::PresetEnv],
            SettingsPart::Proxy => {
                if self.proxy.mode != ProxyMode::Enabled {
                    return vec![Row::ProxyMode];
                }
                let mut rows = vec![
                    Row::ProxyMode,
                    Row::ProxyProtocol,
                    Row::ProxyHost,
                    Row::ProxyPort,
                    Row::ProxyAuth,
                ];
                if self.proxy.auth {
                    rows.push(Row::ProxyUser);
                    rows.push(Row::ProxyPassword);
                }
                rows.push(Row::ProxyBypass);
                rows
            }
            SettingsPart::Certs => Vec::new(),
        }
    }

    fn row_count(&self) -> usize {
        match self.part {
            SettingsPart::Certs => self.certs.len(),
            _ => self.selectable_rows().len(),
        }
    }

    /// Ligne sous le curseur (parties Presets et Proxy).
    pub fn current_row(&self) -> Option<Row> {
        self.selectable_rows().get(self.cursor()).copied()
    }

    fn clamp_cursor(&mut self) {
        let max = self.row_count().saturating_sub(1);
        let cursor = &mut self.cursors[self.part.index()];
        *cursor = (*cursor).min(max);
    }

    /// Valeur affichée d'une ligne de choix ou de texte. Les secrets
    /// ne sont jamais rendus en clair, sauf une variable `{{…}}` révélée.
    pub fn row_value(&self, row: Row) -> String {
        match row {
            Row::PresetType => self.presets.request_type.clone(),
            Row::PresetUrl => self.presets.url.clone(),
            Row::PresetEnv => self
                .presets
                .environment
                .clone()
                .unwrap_or_else(|| "aucun".into()),
            Row::ProxyMode => self.proxy.mode.label().into(),
            Row::ProxyProtocol => self.proxy.protocol.clone(),
            Row::ProxyHost => self.proxy.host.clone(),
            Row::ProxyPort => self.proxy.port.clone(),
            Row::ProxyAuth => String::new(),
            Row::ProxyUser => self.proxy.user.clone(),
            Row::ProxyPassword => self.masked(&self.proxy.password),
            Row::ProxyBypass => self.proxy.bypass.clone(),
        }
    }

    /// Affichage d'un secret : `********` si c'est un littéral, puces pour
    /// une variable (lisible avec `Ctrl+R`), rien s'il est vide.
    pub fn masked(&self, secret: &str) -> String {
        if secret.is_empty() {
            String::new()
        } else if !is_variable_ref(secret) {
            "********".into()
        } else if self.reveal {
            secret.to_owned()
        } else {
            "•".repeat(secret.chars().count())
        }
    }

    /// Valeur affichée pendant la saisie d'un secret.
    pub fn masked_input(&self, text: &str) -> String {
        if self.reveal {
            text.to_owned()
        } else {
            "•".repeat(text.chars().count())
        }
    }

    pub fn toggle_reveal(&mut self) {
        self.reveal = !self.reveal;
    }

    // --- Valeurs à écrire ---------------------------------------------

    /// Valeur JSON de chaque clé modifiée, prête pour `writer::bruno_config`.
    pub fn changes(&self) -> Vec<(&'static str, Json)> {
        let mut out = Vec::new();
        if self.presets_dirty() {
            out.push((
                "presets",
                presets_json(&self.presets_base, &self.presets_initial, &self.presets),
            ));
        }
        if self.proxy_dirty() {
            out.push((
                "proxy",
                proxy_json(&self.proxy_base, &self.proxy_initial, &self.proxy),
            ));
        }
        if self.certs_dirty() {
            out.push((
                "clientCertificates",
                Json::Obj(vec![
                    ("enabled".into(), Json::Bool(true)),
                    ("certs".into(), Json::Arr(self.certs.clone())),
                ]),
            ));
        }
        out
    }

    /// Clés qui seront écrites, pour le message de fin.
    pub fn changed_labels(&self) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if self.presets_dirty() {
            labels.push("presets");
        }
        if self.proxy_dirty() {
            labels.push("proxy");
        }
        if self.certs_dirty() {
            labels.push("certificats");
        }
        labels
    }

    // --- Touches --------------------------------------------------------

    /// `Ctrl+S` : valide la saisie ou le sous-popup en cours, puis demande
    /// l'écriture.
    pub fn save_key(&mut self) -> Action {
        self.error = None;
        if self.confirm.is_some() {
            return Action::None;
        }
        if self.add.is_some() {
            self.submit_add();
            return Action::None;
        }
        if self.input.is_some() && !self.commit_input() {
            return Action::None;
        }
        Action::Save
    }

    pub fn key(&mut self, key: SettingsKey) -> Action {
        self.error = None;
        if let Some(confirm) = self.confirm.clone() {
            return self.confirm_key(&confirm, key);
        }
        if self.add.is_some() {
            self.add_key(key);
            return Action::None;
        }
        if self.input.is_some() {
            self.input_key(key);
            return Action::None;
        }
        self.normal_key(key)
    }

    fn confirm_key(&mut self, confirm: &SettingsConfirm, key: SettingsKey) -> Action {
        match key {
            SettingsKey::Char('y' | 'Y') => {
                self.confirm = None;
                match confirm {
                    SettingsConfirm::Discard => return Action::Close,
                    SettingsConfirm::DeleteCert(index) => {
                        if *index < self.certs.len() {
                            self.certs.remove(*index);
                            self.clamp_cursor();
                        }
                    }
                }
            }
            SettingsKey::Char('n' | 'N') | SettingsKey::Esc => self.confirm = None,
            _ => {}
        }
        Action::None
    }

    fn move_cursor(&mut self, forward: bool) {
        let count = self.row_count();
        if count == 0 {
            return;
        }
        let cursor = &mut self.cursors[self.part.index()];
        *cursor = if forward {
            (*cursor + 1).min(count - 1)
        } else {
            cursor.saturating_sub(1)
        };
    }

    fn normal_key(&mut self, key: SettingsKey) -> Action {
        match key {
            SettingsKey::Up => self.move_cursor(false),
            SettingsKey::Down => self.move_cursor(true),
            SettingsKey::Tab => {
                self.part = self.part.next();
                self.clamp_cursor();
            }
            SettingsKey::BackTab => {
                self.part = self.part.previous();
                self.clamp_cursor();
            }
            SettingsKey::Esc => {
                if self.has_unsaved() {
                    self.confirm = Some(SettingsConfirm::Discard);
                } else {
                    return Action::Close;
                }
            }
            SettingsKey::Left => self.cycle_current(false),
            SettingsKey::Right => self.cycle_current(true),
            SettingsKey::Enter => self.enter_current(),
            SettingsKey::Char(' ') => self.space_current(),
            SettingsKey::Char('a') if self.part == SettingsPart::Certs => {
                self.add = Some(NewCert::new());
            }
            SettingsKey::Char('d') if self.part == SettingsPart::Certs => {
                let index = self.cursor();
                if index < self.certs.len() {
                    self.confirm = Some(SettingsConfirm::DeleteCert(index));
                }
            }
            _ => {}
        }
        Action::None
    }

    fn space_current(&mut self) {
        match self.part {
            SettingsPart::Certs => self.toggle_cert_disabled(),
            _ => {
                if self.current_row() == Some(Row::ProxyAuth) {
                    self.proxy.auth = !self.proxy.auth;
                    self.clamp_cursor();
                }
            }
        }
    }

    fn enter_current(&mut self) {
        let Some(row) = self.current_row() else {
            return;
        };
        match row {
            Row::ProxyAuth => self.space_current(),
            row if row.is_choice() => self.cycle_current(true),
            row => self.begin_input(row),
        }
    }

    /// Certificat sous le curseur : ajoute `disabled: true`, ou retire la clé
    /// (`n.disabled ? delete n.disabled : n.disabled = true`).
    fn toggle_cert_disabled(&mut self) {
        let index = self.cursor();
        let Some(entry) = self.certs.get_mut(index) else {
            return;
        };
        if truthy(entry.get("disabled")) {
            entry.remove("disabled");
        } else {
            entry.set("disabled", Json::Bool(true));
        }
    }

    fn cycle_current(&mut self, forward: bool) {
        let Some(row) = self.current_row() else {
            return;
        };
        fn step(values: &[&str], current: &str, forward: bool) -> usize {
            let count = values.len();
            match values.iter().position(|v| *v == current) {
                Some(i) if forward => (i + 1) % count,
                Some(i) => (i + count - 1) % count,
                None => 0,
            }
        }
        match row {
            Row::PresetType => {
                let i = step(&REQUEST_TYPES, &self.presets.request_type, forward);
                self.presets.request_type = REQUEST_TYPES[i].into();
            }
            Row::PresetEnv => {
                // Index 0 = aucun, puis les environnements.
                let count = self.environments.len() + 1;
                let current = self
                    .presets
                    .environment
                    .as_ref()
                    .and_then(|name| self.environments.iter().position(|env| env == name))
                    .map_or(0, |position| position + 1);
                let next = if forward {
                    (current + 1) % count
                } else {
                    (current + count - 1) % count
                };
                self.presets.environment = match next {
                    0 => None,
                    n => self.environments.get(n - 1).cloned(),
                };
            }
            Row::ProxyMode => {
                let modes = ProxyMode::ALL;
                let i = modes
                    .iter()
                    .position(|m| *m == self.proxy.mode)
                    .unwrap_or(0);
                let next = if forward {
                    (i + 1) % modes.len()
                } else {
                    (i + modes.len() - 1) % modes.len()
                };
                self.proxy.mode = modes[next];
                self.clamp_cursor();
            }
            Row::ProxyProtocol => {
                let i = step(&PROTOCOLS, &self.proxy.protocol, forward);
                self.proxy.protocol = PROTOCOLS[i].into();
            }
            _ => {}
        }
    }

    // --- Saisie d'une ligne -------------------------------------------

    fn begin_input(&mut self, row: Row) {
        let current = match row {
            Row::PresetUrl => self.presets.url.clone(),
            Row::ProxyHost => self.proxy.host.clone(),
            Row::ProxyPort => self.proxy.port.clone(),
            Row::ProxyUser => self.proxy.user.clone(),
            Row::ProxyBypass => self.proxy.bypass.clone(),
            // Un secret littéral n'est jamais chargé dans l'éditeur.
            Row::ProxyPassword if is_variable_ref(&self.proxy.password) => {
                self.proxy.password.clone()
            }
            Row::ProxyPassword => String::new(),
            _ => return,
        };
        self.input = Some(ActiveInput {
            row,
            input: TextInput::new(&current, false),
        });
    }

    fn input_key(&mut self, key: SettingsKey) {
        match key {
            SettingsKey::Enter => {
                self.commit_input();
            }
            SettingsKey::Esc => self.input = None,
            other => {
                if let Some(active) = &mut self.input {
                    edit_text(&mut active.input, other);
                }
            }
        }
    }

    /// Valide la saisie. Retourne `false` si elle est refusée et reste
    /// ouverte ; un secret littéral refusé est oublié et la saisie fermée.
    fn commit_input(&mut self) -> bool {
        let Some(active) = self.input.take() else {
            return true;
        };
        let text = active.input.text().to_owned();
        match self.validate(active.row, &text) {
            Ok(()) => {
                match active.row {
                    Row::PresetUrl => self.presets.url = text,
                    Row::ProxyHost => self.proxy.host = text,
                    Row::ProxyPort => self.proxy.port = text,
                    Row::ProxyUser => self.proxy.user = text,
                    Row::ProxyPassword => self.proxy.password = text,
                    Row::ProxyBypass => self.proxy.bypass = text,
                    _ => {}
                }
                true
            }
            Err(message) => {
                self.error = Some(message);
                if !active.row.is_secret() {
                    self.input = Some(active);
                }
                false
            }
        }
    }

    fn validate(&self, row: Row, text: &str) -> Result<(), String> {
        let too_long =
            |what: &str| Err(format!("{what} doit faire au plus {MAX_TEXT} caractères."));
        match row {
            Row::ProxyHost if js_length(text) > MAX_TEXT => too_long("Le nom d'hôte"),
            Row::ProxyUser if js_length(text) > MAX_TEXT => too_long("Le nom d'utilisateur"),
            Row::ProxyBypass if js_length(text) > MAX_TEXT => too_long("Le contournement"),
            Row::ProxyPassword => {
                if js_length(text) > MAX_TEXT {
                    return too_long("Le mot de passe");
                }
                if secret_allowed(text) {
                    Ok(())
                } else {
                    Err(secret_refused("proxyPassword"))
                }
            }
            Row::ProxyPort => validate_port(text),
            _ => Ok(()),
        }
    }

    // --- Ajout d'un certificat ----------------------------------------

    fn add_key(&mut self, key: SettingsKey) {
        let Some(add) = &mut self.add else {
            return;
        };
        add.error = None;
        let count = add.fields().len();
        match key {
            SettingsKey::Esc => self.add = None,
            SettingsKey::Up | SettingsKey::BackTab => {
                add.cursor = (add.cursor + count - 1) % count;
            }
            SettingsKey::Down | SettingsKey::Tab => add.cursor = (add.cursor + 1) % count,
            SettingsKey::Enter => {
                if add.cursor + 1 >= count {
                    self.submit_add();
                } else {
                    add.cursor += 1;
                }
            }
            other => {
                let field = add.current();
                if field == CertField::Kind {
                    if matches!(
                        other,
                        SettingsKey::Left | SettingsKey::Right | SettingsKey::Char(' ')
                    ) {
                        add.toggle_kind();
                        add.cursor = add.cursor.min(add.fields().len() - 1);
                    }
                } else if let Some(input) = add.input_mut(field) {
                    edit_text(input, other);
                }
            }
        }
    }

    /// Valide le sous-popup et ajoute l'entrée, comme le formulaire de
    /// Bruno (messages de Yup relevés dans le renderer).
    fn submit_add(&mut self) {
        let Some(add) = self.add.take() else {
            return;
        };
        match self.build_cert(&add) {
            Ok((entry, path)) => {
                self.certs.push(entry);
                self.added_paths.push(path);
                let last = self.certs.len() - 1;
                self.cursors[SettingsPart::Certs.index()] = last;
            }
            Err(message) => {
                let mut add = add;
                if message == secret_refused("certPassphrase") {
                    add.passphrase = TextInput::new("", false);
                }
                add.error = Some(message);
                self.add = Some(add);
            }
        }
    }

    fn build_cert(&self, add: &NewCert) -> Result<(Json, String), String> {
        let domain = add.domain.text().trim().to_owned();
        if domain.is_empty() {
            return Err("Domain is required".into());
        }
        let passphrase = add.passphrase.text();
        let required = |input: &TextInput, name: &str| {
            if input.text().is_empty() {
                Err(format!("{name} is a required field"))
            } else {
                Ok(cert_path(&self.root, input.text()))
            }
        };
        let (entry, path) = match add.kind {
            CertKind::Cert => {
                let cert = required(&add.cert, "certFilePath")?;
                let key = required(&add.key, "keyFilePath")?;
                let path = cert.clone();
                (
                    vec![
                        ("domain", Json::Str(domain)),
                        ("type", Json::Str("cert".into())),
                        ("certFilePath", Json::Str(cert)),
                        ("keyFilePath", Json::Str(key)),
                    ],
                    path,
                )
            }
            CertKind::Pfx => {
                let pfx = required(&add.pfx, "pfxFilePath")?;
                let path = pfx.clone();
                (
                    vec![
                        ("domain", Json::Str(domain)),
                        ("type", Json::Str("pfx".into())),
                        ("pfxFilePath", Json::Str(pfx)),
                    ],
                    path,
                )
            }
        };
        if !secret_allowed(passphrase) {
            return Err(secret_refused("certPassphrase"));
        }
        let mut members: Vec<(String, Json)> = entry
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        members.push(("passphrase".into(), Json::Str(passphrase.to_owned())));
        Ok((Json::Obj(members), path))
    }
}

/// Message de refus d'un secret littéral : propose un nom de variable,
/// ne cite jamais la valeur refusée.
pub fn secret_refused(variable: &str) -> String {
    format!(
        "Secret littéral refusé. Utilise une variable, par ex. {{{{{variable}}}}} \
         (à définir dans .env ou avec --secret)."
    )
}

fn validate_port(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    if !text.chars().all(|c| c.is_ascii_digit()) {
        return Err("Le port doit être un nombre valide.".into());
    }
    match text.parse::<u32>() {
        Ok(number) if (1..=65535).contains(&number) => Ok(()),
        _ => Err("Le port doit être compris entre 1 et 65535.".into()),
    }
}

/// Touche d'édition d'un champ texte sur une ligne.
fn edit_text(input: &mut TextInput, key: SettingsKey) {
    match key {
        SettingsKey::Char(c) => input.insert(c),
        SettingsKey::Backspace => input.backspace(),
        SettingsKey::Delete => input.delete(),
        SettingsKey::Left => input.left(),
        SettingsKey::Right => input.right(),
        SettingsKey::Home => input.home(),
        SettingsKey::End => input.end(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::FileStamp;

    fn session(text: &str) -> SettingsSession {
        let dir = std::env::temp_dir();
        let stamp = FileStamp::capture(&dir).expect("instantané");
        SettingsSession::open(
            PathBuf::from("/work/coll"),
            "demo".into(),
            stamp,
            text,
            vec!["local".into(), "staging".into()],
        )
        .expect("session")
    }

    fn type_text(s: &mut SettingsSession, text: &str) {
        for c in text.chars() {
            s.key(SettingsKey::Char(c));
        }
    }

    #[test]
    fn variable_rule_matches_bruno_regex() {
        for ok in ["{{a}}", "  {{a}}  ", "{{process.env.X}}", "{{}}"] {
            assert!(is_variable_ref(ok), "{ok}");
        }
        for bad in ["", "abc", "{{a", "a}}", "{a}", "x{{a}}y", "{{a}}\nx"] {
            assert!(!is_variable_ref(bad), "{bad}");
        }
    }

    #[test]
    fn relative_path_follows_node() {
        let root = Path::new("/work/coll");
        assert_eq!(
            relative_path(root, Path::new("/work/coll/certs/a.crt")),
            "certs/a.crt"
        );
        assert_eq!(
            relative_path(root, Path::new("/work/other/a.crt")),
            "../other/a.crt"
        );
        assert_eq!(
            relative_path(root, Path::new("/etc/ssl/a.crt")),
            "../../etc/ssl/a.crt"
        );
        assert_eq!(relative_path(root, Path::new("/work/coll")), "");
        assert_eq!(cert_path(root, "certs/a.crt"), "certs/a.crt");
        assert_eq!(cert_path(root, "/work/coll/certs/a.crt"), "certs/a.crt");
    }

    #[test]
    fn untouched_session_has_no_changes() {
        let s = session("{\"name\":\"x\"}");
        assert!(!s.has_unsaved());
        assert!(s.changes().is_empty());
    }

    #[test]
    fn proxy_mode_walk_inherit_enabled_disabled() {
        let mut s = session("{\"name\":\"x\"}");
        s.part = SettingsPart::Proxy;
        assert_eq!(s.proxy.mode, ProxyMode::Inherit);
        s.key(SettingsKey::Right);
        assert_eq!(s.proxy.mode, ProxyMode::Enabled);
        assert_eq!(s.selectable_rows().len(), 8);
        s.key(SettingsKey::Right);
        assert_eq!(s.proxy.mode, ProxyMode::Disabled);
        assert_eq!(s.selectable_rows(), vec![Row::ProxyMode]);
        let changes = s.changes();
        assert_eq!(changes[0].0, "proxy");
        let json = changes[0].1.to_pretty();
        assert!(json.contains("\"disabled\": true"));
        assert!(json.contains("\"inherit\": false"));
    }

    #[test]
    fn port_validation() {
        assert!(validate_port("").is_ok());
        assert!(validate_port("1").is_ok());
        assert!(validate_port("65535").is_ok());
        assert!(validate_port("0").is_err());
        assert!(validate_port("65536").is_err());
        assert!(validate_port("12a").is_err());
        assert!(validate_port("{{p}}").is_err());
        assert!(validate_port("-1").is_err());
    }

    #[test]
    fn literal_password_is_refused_and_forgotten() {
        let mut s = session("{\"name\":\"x\"}");
        s.part = SettingsPart::Proxy;
        s.key(SettingsKey::Right); // enabled
        for _ in 0..6 {
            s.key(SettingsKey::Down);
        }
        assert_eq!(s.current_row(), Some(Row::ProxyPassword));
        s.key(SettingsKey::Enter);
        type_text(&mut s, "hunter2");
        s.key(SettingsKey::Enter);
        assert!(s.input.is_none(), "saisie fermée");
        assert_eq!(s.proxy.password, "");
        let message = s.error.clone().expect("refus");
        assert!(!message.contains("hunter2"));
        assert!(message.contains("{{proxyPassword}}"));
        // Une variable passe.
        s.key(SettingsKey::Enter);
        type_text(&mut s, "{{proxyPassword}}");
        s.key(SettingsKey::Enter);
        assert_eq!(s.proxy.password, "{{proxyPassword}}");
        assert!(s.error.is_none());
    }

    #[test]
    fn existing_literal_secret_is_masked_and_not_loaded_in_the_editor() {
        let text = r#"{"proxy":{"inherit":false,"config":{"protocol":"http","hostname":"h","port":1,"auth":{"username":"u","password":"s3cret"},"bypassProxy":""}}}"#;
        let mut s = session(text);
        s.part = SettingsPart::Proxy;
        assert_eq!(s.row_value(Row::ProxyPassword), "********");
        s.reveal = true;
        assert_eq!(s.row_value(Row::ProxyPassword), "********");
        for _ in 0..6 {
            s.key(SettingsKey::Down);
        }
        s.key(SettingsKey::Enter);
        let active = s.input.as_ref().expect("saisie");
        assert_eq!(active.input.text(), "");
        // Modifier un autre champ recopie le secret tel quel.
        s.key(SettingsKey::Esc);
        s.key(SettingsKey::Up);
        s.key(SettingsKey::Up);
        s.key(SettingsKey::Up);
        s.key(SettingsKey::Up);
        assert_eq!(s.current_row(), Some(Row::ProxyHost));
        s.key(SettingsKey::Enter);
        s.key(SettingsKey::Char('x'));
        s.key(SettingsKey::Enter);
        let json = s.changes()[0].1.to_pretty();
        assert!(json.contains("\"password\": \"s3cret\""));
        assert!(json.contains("\"hostname\": \"hx\""));
    }

    #[test]
    fn text_longer_than_1024_is_refused() {
        let mut s = session("{}");
        s.part = SettingsPart::Proxy;
        s.key(SettingsKey::Right);
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Down);
        assert_eq!(s.current_row(), Some(Row::ProxyHost));
        s.key(SettingsKey::Enter);
        for _ in 0..=MAX_TEXT {
            s.key(SettingsKey::Char('a'));
        }
        s.key(SettingsKey::Enter);
        assert!(s.input.is_some(), "saisie gardée");
        assert!(s.error.as_deref().is_some_and(|m| m.contains("1024")));
        s.key(SettingsKey::Backspace);
        s.key(SettingsKey::Enter);
        assert!(s.input.is_none());
        assert_eq!(s.proxy.host.len(), MAX_TEXT);
    }

    #[test]
    fn presets_environment_cycles_through_none_and_environments() {
        let mut s = session("{}");
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Down);
        assert_eq!(s.current_row(), Some(Row::PresetEnv));
        s.key(SettingsKey::Right);
        assert_eq!(s.presets.environment.as_deref(), Some("local"));
        s.key(SettingsKey::Right);
        s.key(SettingsKey::Right);
        assert_eq!(s.presets.environment, None);
        s.key(SettingsKey::Left);
        assert_eq!(s.presets.environment.as_deref(), Some("staging"));
    }

    #[test]
    fn certificate_add_validate_toggle_delete() {
        let mut s = session("{}");
        s.part = SettingsPart::Certs;
        s.key(SettingsKey::Char('a'));
        assert!(s.add.is_some());
        // Domaine vide.
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Enter);
        assert_eq!(
            s.add.as_ref().and_then(|a| a.error.clone()).as_deref(),
            Some("Domain is required")
        );
        // Remplit.
        let add = s.add.as_mut().expect("add");
        add.cursor = 0;
        for c in "api.exemple.fr".chars() {
            s.key(SettingsKey::Char(c));
        }
        s.key(SettingsKey::Enter); // -> Type
        s.key(SettingsKey::Enter); // -> Certificat
        s.key(SettingsKey::Enter); // certificat vide, avance
        assert_eq!(
            s.add.as_ref().map(|a| a.current()),
            Some(CertField::KeyFile)
        );
        s.add.as_mut().expect("add").cursor = 2;
        type_text(&mut s, "/work/coll/certs/c.crt");
        s.key(SettingsKey::Enter);
        type_text(&mut s, "certs/c.key");
        s.key(SettingsKey::Enter); // passphrase
        type_text(&mut s, "plain");
        s.key(SettingsKey::Enter);
        let error = s.add.as_ref().and_then(|a| a.error.clone()).expect("refus");
        assert!(!error.contains("plain"));
        assert_eq!(s.add.as_ref().map(|a| a.passphrase.text()), Some(""));
        type_text(&mut s, "{{certPass}}");
        s.key(SettingsKey::Enter);
        assert!(s.add.is_none());
        let summaries = s.cert_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].path, "certs/c.crt");
        assert_eq!(s.added_paths, vec!["certs/c.crt".to_owned()]);
        // Désactive puis réactive.
        s.key(SettingsKey::Char(' '));
        assert!(s.cert_summaries()[0].disabled);
        s.key(SettingsKey::Char(' '));
        assert!(!s.cert_summaries()[0].disabled);
        // Suppression avec confirmation.
        s.key(SettingsKey::Char('d'));
        assert_eq!(s.confirm, Some(SettingsConfirm::DeleteCert(0)));
        s.key(SettingsKey::Char('n'));
        assert_eq!(s.cert_summaries().len(), 1);
        s.key(SettingsKey::Char('d'));
        s.key(SettingsKey::Char('y'));
        assert!(s.cert_summaries().is_empty());
    }

    #[test]
    fn pfx_type_clears_cert_fields() {
        let mut s = session("{}");
        s.part = SettingsPart::Certs;
        s.key(SettingsKey::Char('a'));
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Down);
        type_text(&mut s, "a.crt");
        s.key(SettingsKey::Up);
        s.key(SettingsKey::Right);
        let add = s.add.as_ref().expect("add");
        assert_eq!(add.kind, CertKind::Pfx);
        assert_eq!(add.cert.text(), "");
        assert_eq!(add.fields().len(), 4);
    }

    #[test]
    fn escape_asks_confirmation_only_when_modified() {
        let mut s = session("{}");
        assert_eq!(s.key(SettingsKey::Esc), Action::Close);
        s.key(SettingsKey::Right);
        assert_eq!(s.key(SettingsKey::Esc), Action::None);
        assert_eq!(s.confirm, Some(SettingsConfirm::Discard));
        assert_eq!(s.key(SettingsKey::Char('n')), Action::None);
        assert!(s.confirm.is_none());
        s.key(SettingsKey::Esc);
        assert_eq!(s.key(SettingsKey::Char('y')), Action::Close);
    }

    #[test]
    fn legacy_proxy_is_shown_converted_and_written_whole() {
        let text = r#"{"proxy":{"enabled":true,"protocol":"https","hostname":"h","port":8080,"auth":{"enabled":false,"username":"u","password":"{{p}}"},"bypassProxy":"x"}}"#;
        let mut s = session(text);
        assert!(s.legacy_proxy);
        assert!(!s.proxy_dirty());
        assert!(
            s.changes().is_empty(),
            "rien n'est converti sans modification"
        );
        assert_eq!(s.proxy.mode, ProxyMode::Enabled);
        assert!(!s.proxy.auth);
        s.part = SettingsPart::Proxy;
        s.key(SettingsKey::Down);
        s.key(SettingsKey::Right);
        let json = s.changes()[0].1.to_pretty();
        assert!(!json.contains("enabled"));
        assert!(json.contains("\"protocol\": \"socks4\""));
        assert!(json.contains("\"disabled\": true"));
    }
}
