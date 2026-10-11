//! Lecture des fichiers d'une collection OpenCollection (YAML).
//!
//! Traduction fidèle, pour l'affichage seulement, des fonctions `parse*` du
//! format `yml` de Bruno (`@usebruno/filestore`, `src/formats/yml/`). Les
//! valeurs restent telles qu'écrites : aucun `{{variable}}` n'est résolu et
//! l'auth n'est jamais héritée. Les champs que le TUI n'affiche pas sont
//! ignorés ; ceux qu'il ne sait pas représenter donnent un avis (notice)
//! qui cite une clé, jamais une valeur.

use std::path::PathBuf;

use serde_norway::{Mapping, Value};

use super::error::{LoadError, ParseError};
use super::view::{
    AuthMode, BodyContent, BodyKind, BodyView, Environment, EnvironmentExtends, ExtraTabs,
    FileMeta, KeyValue, RequestView, valid_environment_name,
};

/// Fichier racine d'une collection OpenCollection.
pub const COLLECTION_FILE: &str = "opencollection.yml";
/// Métadonnées d'un dossier.
pub const FOLDER_FILE: &str = "folder.yml";
/// Extension des requêtes et des environnements.
pub const EXTENSION: &str = "yml";

/// Longueur maximale d'une clé citée dans un avis.
const MAX_QUOTED_KEY: usize = 40;

/// Clés racine d'une requête HTTP (`HttpRequest` d'OpenCollection).
const HTTP_ITEM_KEYS: [&str; 7] = [
    "info", "http", "runtime", "settings", "examples", "docs", "app",
];
/// Clés racine d'une requête GraphQL.
const GRAPHQL_ITEM_KEYS: [&str; 6] = ["info", "graphql", "runtime", "settings", "examples", "docs"];
/// Clés du bloc `http` / `graphql`.
const DETAILS_KEYS: [&str; 7] = [
    "method", "url", "headers", "params", "body", "auth", "settings",
];
/// Clés du bloc `runtime`.
const RUNTIME_KEYS: [&str; 5] = ["variables", "scripts", "assertions", "actions", "auth"];
/// Clés racine de `folder.yml`.
const FOLDER_KEYS: [&str; 4] = ["info", "request", "docs", "settings"];
/// Clés racine de `opencollection.yml`.
const COLLECTION_KEYS: [&str; 8] = [
    "opencollection",
    "info",
    "config",
    "request",
    "docs",
    "bundled",
    "extensions",
    "items",
];
/// Clés d'un bloc de valeurs par défaut (`request` d'un dossier ou de la
/// collection).
const DEFAULTS_KEYS: [&str; 5] = ["headers", "auth", "variables", "actions", "scripts"];
/// Clés racine d'un environnement.
const ENVIRONMENT_KEYS: [&str; 6] = [
    "name",
    "variables",
    "color",
    "extends",
    "externalSecrets",
    "description",
];
/// Types de requête reconnus par Bruno mais non affichés par le TUI.
const UNSUPPORTED_ITEM_TYPES: [&str; 5] = ["grpc", "websocket", "script", "app", "folder"];

/// Résultat d'une lecture : la valeur et les avis sur ce qui a été ignoré.
#[derive(Debug)]
pub struct Parsed<T> {
    pub value: T,
    pub notices: Vec<String>,
}

/// Configuration utile de `opencollection.yml`.
#[derive(Debug)]
pub struct CollectionFile {
    pub name: Option<String>,
    /// `extensions.bruno.ignore`.
    pub ignore: Vec<String>,
    pub meta: FileMeta,
}

/// Collecte des avis pendant la lecture d'un fichier.
#[derive(Debug, Default)]
struct Notices(Vec<String>);

impl Notices {
    fn push(&mut self, message: String) {
        if !self.0.contains(&message) {
            self.0.push(message);
        }
    }

    /// Signale les clés de `map` absentes de `known`, préfixées de `scope`.
    fn unknown_keys(&mut self, map: &Mapping, known: &[&str], scope: &str) {
        for key in map.keys() {
            let name = key_label(key);
            if !known.contains(&name.as_str()) {
                self.push(format!("clé `{scope}{name}` inconnue, ignorée"));
            }
        }
    }
}

/// Nom affichable d'une clé, tronqué : jamais de valeur citée.
fn key_label(key: &Value) -> String {
    let name = match key {
        Value::String(name) => name.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        _ => "?".to_owned(),
    };
    if name.chars().count() > MAX_QUOTED_KEY {
        let truncated: String = name.chars().take(MAX_QUOTED_KEY).collect();
        format!("{truncated}…")
    } else {
        name
    }
}

/// Parse un document YAML ; seule la position d'une erreur est conservée,
/// le message de `serde_norway` pouvant citer le contenu.
fn parse_document(source: &str) -> Result<Mapping, ParseError> {
    let value: Value = serde_norway::from_str(source).map_err(|error| {
        let location = error.location();
        ParseError::InvalidYaml {
            line: location.as_ref().map(|l| l.line()),
            column: location.as_ref().map(|l| l.column()),
        }
    })?;
    match untag(value) {
        Value::Mapping(map) => Ok(map),
        _ => Err(ParseError::NotAMapping),
    }
}

fn untag(value: Value) -> Value {
    match value {
        Value::Tagged(tagged) => untag(tagged.value),
        other => other,
    }
}

fn untagged(value: &Value) -> &Value {
    match value {
        Value::Tagged(tagged) => untagged(&tagged.value),
        other => other,
    }
}

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(key).map(untagged).filter(|value| !value.is_null())
}

fn mapping<'a>(map: &'a Mapping, key: &str) -> Option<&'a Mapping> {
    get(map, key).and_then(Value::as_mapping)
}

fn sequence<'a>(map: &'a Mapping, key: &str) -> impl Iterator<Item = &'a Mapping> {
    get(map, key)
        .and_then(Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|item| untagged(item).as_mapping())
}

/// Équivalent de `ensureString` : `null`/absent donne `None`, un scalaire
/// est converti en texte, un objet est sérialisé en JSON comme
/// `JSON.stringify`.
fn ensure_string(value: Option<&Value>) -> Option<String> {
    match value.map(untagged)? {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        other => serde_json::to_string(other).ok(),
    }
}

fn string_of(map: &Mapping, key: &str) -> Option<String> {
    ensure_string(map.get(key))
}

/// `disabled: true` désactive l'entrée ; toute autre valeur la laisse active.
fn enabled(map: &Mapping) -> bool {
    get(map, "disabled").and_then(Value::as_bool) != Some(true)
}

/// `seq` numérique entier et positif, sinon absent.
fn seq_of(info: Option<&Mapping>) -> Option<u32> {
    let value = info.and_then(|info| get(info, "seq"))?;
    value
        .as_u64()
        .and_then(|seq| u32::try_from(seq).ok())
        .or_else(|| {
            value
                .as_f64()
                .filter(|seq| seq.fract() == 0.0 && *seq >= 0.0 && *seq <= f64::from(u32::MAX))
                .map(|seq| seq as u32)
        })
}

/// Description : chaîne, ou objet `{ content }`.
fn content_of(value: Option<&Value>) -> Option<String> {
    match value.map(untagged)? {
        Value::String(text) => Some(text.clone()),
        Value::Mapping(map) => string_of(map, "content"),
        _ => None,
    }
}

/// En-têtes (`toBrunoHttpHeaders`).
fn headers(map: &Mapping) -> Vec<KeyValue> {
    sequence(map, "headers")
        .map(|header| KeyValue {
            key: string_of(header, "name").unwrap_or_default(),
            value: string_of(header, "value").unwrap_or_default(),
            enabled: enabled(header),
        })
        .collect()
}

/// Valeur d'une variable : texte, ou valeur typée `{ type, data }`.
fn variable_value(value: Option<&Value>) -> String {
    match value.map(untagged) {
        Some(Value::Mapping(typed)) if typed.contains_key("type") && typed.contains_key("data") => {
            string_of(typed, "data").unwrap_or_default()
        }
        other => ensure_string(other).unwrap_or_default(),
    }
}

/// Variables pré-requête (`toBrunoVariables`).
fn variables(map: &Mapping) -> Vec<KeyValue> {
    sequence(map, "variables")
        .map(|variable| KeyValue {
            key: string_of(variable, "name").unwrap_or_default(),
            value: variable_value(variable.get("value")),
            enabled: enabled(variable),
        })
        .collect()
}

/// Variables post-réponse, tirées des actions `set-variable` de phase
/// `after-response` (`toBrunoPostResponseVariables`). Les autres actions
/// sont ignorées avec un avis.
fn post_response_variables(map: &Mapping, notices: &mut Notices, scope: &str) -> Vec<KeyValue> {
    let mut vars = Vec::new();
    for action in sequence(map, "actions") {
        let kind = get(action, "type").and_then(Value::as_str);
        let phase = get(action, "phase").and_then(Value::as_str);
        if kind == Some("set-variable") && phase == Some("after-response") {
            let name = mapping(action, "variable").and_then(|v| string_of(v, "name"));
            let expression = mapping(action, "selector").and_then(|s| string_of(s, "expression"));
            vars.push(KeyValue {
                key: name.unwrap_or_default(),
                value: expression.unwrap_or_default(),
                enabled: enabled(action),
            });
        } else {
            notices.push(format!(
                "action `{scope}actions` non prise en charge (seules les `set-variable` après réponse sont affichées)"
            ));
        }
    }
    vars
}

/// Présence et code des scripts (`toBrunoScripts`) : code non vide par type.
#[derive(Debug, Default)]
struct Scripts {
    pre_request: Option<String>,
    post_response: Option<String>,
    tests: Option<String>,
}

fn scripts(map: &Mapping, notices: &mut Notices, scope: &str) -> Scripts {
    let mut found = Scripts::default();
    for script in sequence(map, "scripts") {
        let code = string_of(script, "code").filter(|code| !code.is_empty());
        let Some(code) = code else {
            continue;
        };
        match get(script, "type").and_then(Value::as_str) {
            Some("before-request") => match &mut found.pre_request {
                Some(existing) => {
                    existing.push('\n');
                    existing.push_str(&code);
                }
                None => found.pre_request = Some(code),
            },
            Some("after-response") => match &mut found.post_response {
                Some(existing) => {
                    existing.push('\n');
                    existing.push_str(&code);
                }
                None => found.post_response = Some(code),
            },
            Some("tests") => match &mut found.tests {
                Some(existing) => {
                    existing.push('\n');
                    existing.push_str(&code);
                }
                None => found.tests = Some(code),
            },
            _ => notices.push(format!("type de script `{scope}scripts` non affiché")),
        }
    }
    found
}

/// Mode d'auth déclaré (`toBrunoAuth`) : `inherit`, objet `{ type }`, ou
/// absent (`None`, « non déclarée » comme pour un `.bru` sans clé `auth`).
fn auth(map: &Mapping, notices: &mut Notices, scope: &str) -> Option<AuthMode> {
    match get(map, "auth")? {
        Value::String(mode) if mode == "inherit" => Some(AuthMode::Inherit),
        Value::Mapping(auth) => match get(auth, "type").and_then(Value::as_str) {
            Some(kind) => {
                let mode = AuthMode::parse(kind);
                if let AuthMode::Other(_) = mode {
                    notices.push(format!("type d'auth `{scope}auth` non reconnu"));
                }
                Some(mode)
            }
            None => {
                notices.push(format!("auth `{scope}auth` sans type, ignorée"));
                None
            }
        },
        _ => {
            notices.push(format!("auth `{scope}auth` invalide, ignorée"));
            None
        }
    }
}

/// Corps de requête HTTP (`toBrunoBody`) ; `None` sans corps.
fn body(details: &Mapping, notices: &mut Notices) -> Option<BodyView> {
    let body = mapping(details, "body")?;
    let kind = get(body, "type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let text = || BodyContent::Text(string_of(body, "data").unwrap_or_default());
    let view = match kind {
        "json" => BodyView {
            kind: BodyKind::Json,
            content: text(),
        },
        "text" => BodyView {
            kind: BodyKind::Text,
            content: text(),
        },
        "xml" => BodyView {
            kind: BodyKind::Xml,
            content: text(),
        },
        "sparql" => BodyView {
            kind: BodyKind::Sparql,
            content: text(),
        },
        "form-urlencoded" => BodyView {
            kind: BodyKind::FormUrlEncoded,
            content: BodyContent::Entries(
                sequence(body, "data")
                    .map(|entry| KeyValue {
                        key: string_of(entry, "name").unwrap_or_default(),
                        value: string_of(entry, "value").unwrap_or_default(),
                        enabled: enabled(entry),
                    })
                    .collect(),
            ),
        },
        "multipart-form" => BodyView {
            kind: BodyKind::MultipartForm,
            content: BodyContent::Entries(sequence(body, "data").map(multipart_entry).collect()),
        },
        "file" => BodyView {
            kind: BodyKind::File,
            content: BodyContent::Entries(sequence(body, "data").map(file_entry).collect()),
        },
        _ => {
            // Bruno ramène un type inconnu au mode `none`.
            notices.push("type de corps `http.body` non reconnu, ignoré".to_owned());
            return None;
        }
    };
    Some(view)
}

/// Entrée multipart ; un fichier est présenté comme en bru-lang,
/// `@file(a|b)`.
fn multipart_entry(entry: &Mapping) -> KeyValue {
    let value = if get(entry, "type").and_then(Value::as_str) == Some("file") {
        let paths: Vec<String> = match get(entry, "value") {
            Some(Value::Sequence(items)) => items
                .iter()
                .filter_map(|item| ensure_string(Some(item)))
                .collect(),
            other => ensure_string(other).into_iter().collect(),
        };
        format!("@file({})", paths.join("|"))
    } else {
        string_of(entry, "value").unwrap_or_default()
    };
    KeyValue {
        key: string_of(entry, "name").unwrap_or_default(),
        value,
        enabled: enabled(entry),
    }
}

/// Variante d'un corps fichier, présentée comme en bru-lang :
/// `file: @file(chemin) @contentType(type)`, désactivée si non choisie.
fn file_entry(entry: &Mapping) -> KeyValue {
    let path = string_of(entry, "filePath").unwrap_or_default();
    let content_type = string_of(entry, "contentType").unwrap_or_default();
    KeyValue {
        key: "file".to_owned(),
        value: format!("@file({path}) @contentType({content_type})"),
        enabled: get(entry, "selected").and_then(Value::as_bool) == Some(true),
    }
}

/// Réglages scalaires (`settings`) ; les autres sont ignorés avec un avis.
fn settings(item: &Mapping, notices: &mut Notices) -> Vec<KeyValue> {
    let Some(settings) = mapping(item, "settings") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for (key, value) in settings {
        let name = key_label(key);
        match untagged(value) {
            Value::Mapping(_) | Value::Sequence(_) => {
                notices.push(format!("réglage `settings.{name}` non affiché"));
            }
            scalar => {
                if let Some(value) = ensure_string(Some(scalar)) {
                    entries.push(KeyValue {
                        key: name,
                        value,
                        enabled: true,
                    });
                }
            }
        }
    }
    entries
}

/// Assertions (`toBrunoAssertions`) : valeur reconstituée en
/// `opérateur valeur`.
fn assertions(runtime: &Mapping) -> Vec<KeyValue> {
    sequence(runtime, "assertions")
        .map(|assertion| {
            let operator = string_of(assertion, "operator").unwrap_or_default();
            let value = match string_of(assertion, "value") {
                Some(value) => format!("{operator} {value}"),
                None => operator,
            };
            KeyValue {
                key: string_of(assertion, "expression").unwrap_or_default(),
                value,
                enabled: enabled(assertion),
            }
        })
        .collect()
}

/// Lit une requête (`parseItem`). Seuls les types `http` et `graphql` sont
/// affichables ; les autres donnent une erreur propre au fichier.
pub fn parse_item(source: &str) -> Result<Parsed<RequestView>, ParseError> {
    let item = parse_document(source)?;
    let info = mapping(&item, "info");
    let kind = info
        .and_then(|info| get(info, "type"))
        .or_else(|| get(&item, "type"))
        .and_then(Value::as_str)
        .ok_or(ParseError::MissingItemType)?;
    let (details_key, default_method, known): (&str, &str, &[&str]) = match kind {
        "http" => ("http", "GET", &HTTP_ITEM_KEYS),
        "graphql" => ("graphql", "POST", &GRAPHQL_ITEM_KEYS),
        other => {
            return Err(ParseError::UnsupportedItemType {
                kind: UNSUPPORTED_ITEM_TYPES
                    .iter()
                    .find(|known| **known == other)
                    .map_or_else(|| "inconnu".to_owned(), |known| (*known).to_owned()),
            });
        }
    };

    let mut notices = Notices::default();
    notices.unknown_keys(&item, known, "");
    let empty = Mapping::new();
    let details = mapping(&item, details_key).unwrap_or(&empty);
    notices.unknown_keys(details, &DETAILS_KEYS, &format!("{details_key}."));
    let runtime = mapping(&item, "runtime").unwrap_or(&empty);
    notices.unknown_keys(runtime, &RUNTIME_KEYS, "runtime.");

    // Rétrocompatibilité 3.0.0-rc1 : auth sous `runtime`
    // (`ensureAuthV3Rc1BackwardsCompatibility`).
    let auth_source = if details.contains_key("auth") || !runtime.contains_key("auth") {
        details
    } else {
        runtime
    };
    let auth = auth(auth_source, &mut notices, &format!("{details_key}."));

    let mut query_params = Vec::new();
    let mut path_params = Vec::new();
    for param in sequence(details, "params") {
        let entry = KeyValue {
            key: string_of(param, "name").unwrap_or_default(),
            value: string_of(param, "value").unwrap_or_default(),
            enabled: enabled(param),
        };
        match get(param, "type").and_then(Value::as_str) {
            Some("path") => path_params.push(entry),
            Some("query") => query_params.push(entry),
            _ => notices.push(format!(
                "paramètre `{details_key}.params` sans type `query` ni `path`, ignoré"
            )),
        }
    }

    let body = if kind == "graphql" {
        mapping(details, "body").map(|body| BodyView {
            kind: BodyKind::Graphql,
            content: BodyContent::Text(string_of(body, "query").unwrap_or_default()),
        })
    } else {
        body(details, &mut notices)
    };

    let found = scripts(runtime, &mut notices, "runtime.");
    let assertions = assertions(runtime);
    let view = RequestView {
        name: info.and_then(|info| string_of(info, "name")),
        kind: Some(kind.to_owned()),
        seq: seq_of(info),
        method: string_of(details, "method")
            .filter(|method| !method.is_empty())
            .unwrap_or_else(|| default_method.to_owned())
            .to_ascii_uppercase(),
        url: string_of(details, "url").unwrap_or_default(),
        headers: headers(details),
        query_params,
        path_params,
        body,
        auth,
        has_pre_request_script: found.pre_request.is_some(),
        has_post_response_script: found.post_response.is_some(),
        has_tests: found.tests.is_some(),
        has_assert: !assertions.is_empty(),
        extra: Box::new(ExtraTabs {
            vars_pre_request: variables(runtime),
            vars_post_response: post_response_variables(runtime, &mut notices, "runtime."),
            script_pre_request: found.pre_request,
            script_post_response: found.post_response,
            tests: found.tests,
            docs: content_of(item.get("docs")),
            settings: settings(&item, &mut notices),
            assertions,
        }),
    };
    Ok(Parsed {
        value: view,
        notices: notices.0,
    })
}

/// Méta-données d'un bloc de valeurs par défaut (`request` de `folder.yml`
/// ou de `opencollection.yml`).
fn defaults_meta(
    name: Option<String>,
    seq: Option<u32>,
    request: Option<&Mapping>,
    notices: &mut Notices,
) -> FileMeta {
    let empty = Mapping::new();
    let request = request.unwrap_or(&empty);
    notices.unknown_keys(request, &DEFAULTS_KEYS, "request.");
    let found = scripts(request, notices, "request.");
    let has_auth = !matches!(
        auth(request, notices, "request."),
        None | Some(AuthMode::Inherit)
    );
    let has_vars = !variables(request).is_empty()
        || !post_response_variables(request, notices, "request.").is_empty();
    FileMeta {
        name,
        seq,
        has_headers: !headers(request).is_empty(),
        has_auth,
        has_pre_request_script: found.pre_request.is_some(),
        has_post_response_script: found.post_response.is_some(),
        has_tests: found.tests.is_some(),
        has_vars,
        ast: None,
    }
}

/// Lit `folder.yml` (`parseFolder`).
pub fn parse_folder(source: &str) -> Result<Parsed<FileMeta>, ParseError> {
    let folder = parse_document(source)?;
    let mut notices = Notices::default();
    notices.unknown_keys(&folder, &FOLDER_KEYS, "");
    let info = mapping(&folder, "info");
    let name = info.and_then(|info| string_of(info, "name"));
    let meta = defaults_meta(
        name,
        seq_of(info),
        mapping(&folder, "request"),
        &mut notices,
    );
    Ok(Parsed {
        value: meta,
        notices: notices.0,
    })
}

/// Lit `opencollection.yml` (`parseCollection`). Une erreur de syntaxe est
/// fatale : sans ce fichier valide, la collection n'est pas chargée.
pub fn parse_collection(path: PathBuf, source: &str) -> Result<Parsed<CollectionFile>, LoadError> {
    let collection = parse_document(source).map_err(|error| match error {
        ParseError::InvalidYaml { line, column } => LoadError::InvalidOpenCollection {
            path: path.clone(),
            line: line.unwrap_or(0),
            column: column.unwrap_or(0),
        },
        _ => LoadError::InvalidOpenCollection {
            path: path.clone(),
            line: 0,
            column: 0,
        },
    })?;
    let mut notices = Notices::default();
    notices.unknown_keys(&collection, &COLLECTION_KEYS, "");
    let name = mapping(&collection, "info")
        .and_then(|info| string_of(info, "name"))
        .filter(|name| !name.is_empty());
    let ignore = mapping(&collection, "extensions")
        .and_then(|extensions| mapping(extensions, "bruno"))
        .and_then(|bruno| get(bruno, "ignore"))
        .and_then(Value::as_sequence)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| untagged(entry).as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let meta = defaults_meta(
        name.clone(),
        None,
        mapping(&collection, "request"),
        &mut notices,
    );
    Ok(Parsed {
        value: CollectionFile { name, ignore, meta },
        notices: notices.0,
    })
}

/// Lit un environnement (`parseEnvironment`). `name` est le nom de fichier
/// sans extension : c'est lui que `bru run --env` attend. Les valeurs des
/// variables secrètes ne sont jamais dans le fichier ; seuls leurs noms
/// sont retenus.
pub fn parse_environment(
    name: String,
    path: PathBuf,
    source: &str,
) -> Result<Parsed<Environment>, ParseError> {
    let environment = parse_document(source)?;
    let mut notices = Notices::default();
    notices.unknown_keys(&environment, &ENVIRONMENT_KEYS, "");
    if environment.contains_key("externalSecrets") {
        notices.push("secrets externes (`externalSecrets`) non pris en charge".to_owned());
    }

    let mut variables = Vec::new();
    let mut secret_names = Vec::new();
    for variable in sequence(&environment, "variables") {
        let key = string_of(variable, "name").unwrap_or_default();
        if get(variable, "secret").and_then(Value::as_bool) == Some(true) {
            secret_names.push(key);
        } else {
            variables.push(KeyValue {
                key,
                value: variable_value(variable.get("value")),
                enabled: enabled(variable),
            });
        }
    }

    // `validatedEnvironmentExtendsFrom` : un nom, ou une liste de noms
    // tous valides.
    let extends = match get(&environment, "extends") {
        Some(Value::String(parent)) => {
            valid_environment_name(parent).map(EnvironmentExtends::Single)
        }
        Some(Value::Sequence(parents)) => parents
            .iter()
            .map(|parent| untagged(parent).as_str().and_then(valid_environment_name))
            .collect::<Option<Vec<_>>>()
            .filter(|parents| !parents.is_empty())
            .map(EnvironmentExtends::List),
        _ => None,
    };
    let color = string_of(&environment, "color")
        .map(|color| color.trim().to_owned())
        .filter(|color| !color.is_empty());

    Ok(Parsed {
        value: Environment {
            name,
            path,
            variables,
            secret_names,
            extends,
            color,
            ast: None,
        },
        notices: notices.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(source: &str) -> Parsed<RequestView> {
        parse_item(source).expect("requête valide")
    }

    #[test]
    fn minimal_http_request_defaults() {
        let parsed = item("info:\n  type: http\n");
        let view = parsed.value;
        assert_eq!(view.method, "GET");
        assert_eq!(view.url, "");
        assert_eq!(view.name, None);
        assert_eq!(view.seq, None);
        assert_eq!(view.kind.as_deref(), Some("http"));
        assert_eq!(view.auth, None);
        assert_eq!(view.body, None);
        assert!(parsed.notices.is_empty(), "{:?}", parsed.notices);
    }

    #[test]
    fn scalar_values_are_converted_like_ensure_string() {
        let view = item(
            "info:\n  type: http\n  name: 42\n  seq: 2\nhttp:\n  method: post\n  url: x\n  headers:\n    - name: A\n      value: 1\n    - name: B\n      value: off\n    - name: C\n      value: true\n",
        )
        .value;
        assert_eq!(view.name.as_deref(), Some("42"));
        assert_eq!(view.seq, Some(2));
        assert_eq!(view.method, "POST");
        let values: Vec<_> = view.headers.iter().map(|h| h.value.as_str()).collect();
        // YAML 1.2 : `off` reste une chaîne, comme pour la bibliothèque
        // `yaml` de Bruno.
        assert_eq!(values, ["1", "off", "true"]);
    }

    #[test]
    fn unsupported_types_are_errors_without_quoting_the_file() {
        let error = parse_item("info:\n  type: grpc\n").expect_err("grpc");
        assert!(matches!(error, ParseError::UnsupportedItemType { ref kind } if kind == "grpc"));
        let error = parse_item("info:\n  type: s3cr3t\n").expect_err("inconnu");
        assert!(!error.to_string().contains("s3cr3t"), "{error}");
        assert!(matches!(
            parse_item("http:\n  url: x\n"),
            Err(ParseError::MissingItemType)
        ));
        assert!(matches!(parse_item(""), Err(ParseError::NotAMapping)));
        assert!(matches!(parse_item("- a\n"), Err(ParseError::NotAMapping)));
    }

    #[test]
    fn invalid_yaml_keeps_only_the_position() {
        let error = parse_item("info:\n  type: http\n  name: \"Bearer s3cr3t\n").expect_err("yaml");
        assert!(matches!(
            error,
            ParseError::InvalidYaml { line: Some(_), .. }
        ));
        assert!(!error.to_string().contains("s3cr3t"), "{error}");
        assert!(!format!("{error:?}").contains("s3cr3t"));
    }

    #[test]
    fn unknown_fields_give_notices_citing_keys_only() {
        let parsed = item(
            "info:\n  type: http\nhttp:\n  url: x\n  body:\n    type: hologram\n    data: s3cr3t\n  extra: s3cr3t\nruntime:\n  scripts:\n    - type: grpc:before-call-start\n      code: x\nfuture: 1\n",
        );
        assert_eq!(parsed.value.body, None);
        let all = parsed.notices.join("\n");
        assert!(all.contains("`future`"), "{all}");
        assert!(all.contains("`http.extra`"), "{all}");
        assert!(all.contains("corps"), "{all}");
        assert!(all.contains("script"), "{all}");
        assert!(!all.contains("s3cr3t"), "{all}");
    }

    #[test]
    fn graphql_request_shows_query() {
        let view = item(
            "info:\n  type: graphql\n  name: q\ngraphql:\n  url: http://x/graphql\n  body:\n    query: \"{ me { id } }\"\n    variables: \"{}\"\n",
        )
        .value;
        assert_eq!(view.method, "POST");
        assert_eq!(view.kind.as_deref(), Some("graphql"));
        assert_eq!(
            view.body,
            Some(BodyView {
                kind: BodyKind::Graphql,
                content: BodyContent::Text("{ me { id } }".to_owned()),
            })
        );
    }

    #[test]
    fn form_multipart_and_file_bodies() {
        let form = item(
            "info:\n  type: http\nhttp:\n  body:\n    type: form-urlencoded\n    data:\n      - name: a\n        value: '1'\n      - name: b\n        value: '2'\n        disabled: true\n",
        )
        .value;
        assert_eq!(
            form.body.expect("corps").content,
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

        let multipart = item(
            "info:\n  type: http\nhttp:\n  body:\n    type: multipart-form\n    data:\n      - name: doc\n        type: file\n        value:\n          - a.txt\n          - b.txt\n      - name: t\n        type: text\n        value: v\n",
        )
        .value
        .body
        .expect("corps");
        assert_eq!(multipart.kind, BodyKind::MultipartForm);
        let BodyContent::Entries(entries) = multipart.content else {
            panic!("entrées attendues");
        };
        assert_eq!(entries[0].value, "@file(a.txt|b.txt)");
        assert_eq!(entries[1].value, "v");

        let file = item(
            "info:\n  type: http\nhttp:\n  body:\n    type: file\n    data:\n      - filePath: a.bin\n        contentType: application/octet-stream\n        selected: true\n",
        )
        .value
        .body
        .expect("corps");
        assert_eq!(
            file.content,
            BodyContent::Entries(vec![KeyValue {
                key: "file".into(),
                value: "@file(a.bin) @contentType(application/octet-stream)".into(),
                enabled: true,
            }])
        );
    }

    #[test]
    fn auth_modes() {
        let mode = |auth: &str| {
            item(&format!("info:\n  type: http\nhttp:\n  auth: {auth}\n"))
                .value
                .auth
        };
        assert_eq!(mode("inherit"), Some(AuthMode::Inherit));
        assert_eq!(mode("{ type: basic }"), Some(AuthMode::Basic));
        assert_eq!(mode("{ type: awsv4 }"), Some(AuthMode::AwsV4));
        assert_eq!(
            mode("{ type: oauth1 }"),
            Some(AuthMode::Other("oauth1".into()))
        );
        // `none` n'est pas une forme valide : Bruno l'ignore aussi.
        assert_eq!(mode("none"), None);
        // Rétrocompatibilité rc1 : auth sous `runtime`.
        let view = item("info:\n  type: http\nruntime:\n  auth:\n    type: bearer\n").value;
        assert_eq!(view.auth, Some(AuthMode::Bearer));
    }

    #[test]
    fn typed_and_object_variable_values() {
        let view = item(
            "info:\n  type: http\nruntime:\n  variables:\n    - name: n\n      value:\n        type: number\n        data: '3'\n    - name: o\n      value:\n        a: 1\n",
        )
        .value;
        assert_eq!(view.vars_pre_request()[0].value, "3");
        assert_eq!(view.vars_pre_request()[1].value, "{\"a\":1}");
    }

    #[test]
    fn folder_meta() {
        let parsed = parse_folder(
            "info:\n  name: f\n  type: folder\n  seq: 4\nrequest:\n  auth: inherit\n  headers:\n    - name: A\n      value: b\n",
        )
        .expect("dossier");
        let meta = parsed.value;
        assert_eq!(meta.name.as_deref(), Some("f"));
        assert_eq!(meta.seq, Some(4));
        assert!(meta.has_headers);
        assert!(!meta.has_auth);
        assert!(meta.ast.is_none());
        // `seq` non numérique : absent, comme dans Bruno.
        let meta = parse_folder("info:\n  seq: abc\n").expect("dossier").value;
        assert_eq!(meta.seq, None);
    }

    #[test]
    fn collection_file_and_invalid_collection() {
        let parsed = parse_collection(
            "opencollection.yml".into(),
            "opencollection: 1.0.0\ninfo:\n  name: c\nextensions:\n  bruno:\n    ignore:\n      - tmp\n",
        )
        .expect("collection");
        assert_eq!(parsed.value.name.as_deref(), Some("c"));
        assert_eq!(parsed.value.ignore, ["tmp"]);

        let error = parse_collection("opencollection.yml".into(), "info: [\n").expect_err("yaml");
        assert!(matches!(error, LoadError::InvalidOpenCollection { .. }));
    }

    #[test]
    fn environment_extends_list_and_invalid_names() {
        let env = parse_environment(
            "e".into(),
            "environments/e.yml".into(),
            "name: e\nextends:\n  - base\n  - other\n",
        )
        .expect("env")
        .value;
        assert_eq!(
            env.extends,
            Some(EnvironmentExtends::List(vec![
                "base".into(),
                "other".into()
            ]))
        );
        let env = parse_environment(
            "e".into(),
            "environments/e.yml".into(),
            "name: e\nextends: bad/name\ncolor: ''\n",
        )
        .expect("env")
        .value;
        assert_eq!(env.extends, None);
        assert_eq!(env.color, None);
    }
}
