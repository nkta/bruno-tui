//! Écriture chirurgicale de `bruno.json` (`docs/reglages-collection.md`).
//!
//! Seules trois clés de premier niveau peuvent être écrites : `proxy`,
//! `clientCertificates` et `presets`. Le fichier n'est pas réécrit : seule
//! la valeur de la clé modifiée est remplacée (ou la clé est ajoutée à la
//! fin, comme Bruno), le reste est recopié à l'octet près. La valeur
//! insérée est celle que Bruno écrirait (`JSON.stringify(v, null, 2)`),
//! décalée au niveau d'indentation de la clé.
//!
//! Aucun message d'erreur ne cite une valeur du fichier : un mot de passe
//! de proxy ou une passphrase peut s'y trouver.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::error::WriteError;
use super::{FileStamp, write_atomic};

/// Nom du fichier de configuration, à la racine de la collection.
pub const CONFIG_FILE: &str = "bruno.json";

/// Clés de premier niveau que le TUI a le droit d'écrire.
pub const WRITABLE_KEYS: [&str; 3] = ["proxy", "clientCertificates", "presets"];

/// Refus d'une écriture de `bruno.json`. Ne porte jamais une valeur lue.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("JSON invalide")]
    InvalidJson,
    #[error("la racine n'est pas un objet JSON")]
    NotAnObject,
    #[error("clé `{0}` en double au premier niveau (ambiguïté)")]
    DuplicateKey(String),
    #[error("clé `{0}` non autorisée en écriture")]
    ForbiddenKey(String),
    #[error("indentation par tabulations non prise en charge")]
    Tabs,
    #[error("fins de ligne mixtes non prises en charge")]
    MixedLineEndings,
    #[error("le fichier est un lien symbolique")]
    Symlink,
    #[error("le fichier produit ne se relit pas")]
    Unreadable,
    #[error("chaîne non représentable")]
    UnsupportedString,
}

/// Valeur JSON ordonnée : l'ordre des clés est celui du fichier, ce que
/// `serde_json::Value` ne garde pas sans dépendance supplémentaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    /// Nombre, en texte, tel qu'il est écrit dans le fichier.
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Lit un document JSON complet.
    pub fn parse(text: &str) -> Result<Json, ConfigError> {
        let mut parser = Parser::new(text);
        parser.skip_ws();
        let value = parser.value(0)?;
        parser.skip_ws();
        if parser.pos != text.len() {
            return Err(ConfigError::InvalidJson);
        }
        Ok(value)
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Json> {
        match self {
            Json::Obj(members) => members.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Remplace la valeur de `key` à sa place, ou l'ajoute à la fin
    /// (`{...objet, ...changement}` de JavaScript). Sans effet hors objet.
    pub fn set(&mut self, key: &str, value: Json) {
        let Json::Obj(members) = self else {
            return;
        };
        match members.iter_mut().find(|(k, _)| k == key) {
            Some((_, slot)) => *slot = value,
            None => members.push((key.to_owned(), value)),
        }
    }

    /// Objet enfant `key`, créé (ou remplacé s'il n'est pas un objet) à la
    /// place ou à la fin. Hors objet, rend `self`.
    pub fn object_entry(&mut self, key: &str) -> &mut Json {
        let Json::Obj(members) = self else {
            return self;
        };
        let index = match members.iter().position(|(k, _)| k == key) {
            Some(i) => i,
            None => {
                members.push((key.to_owned(), Json::Obj(Vec::new())));
                members.len() - 1
            }
        };
        let slot = &mut members[index].1;
        if !matches!(slot, Json::Obj(_)) {
            *slot = Json::Obj(Vec::new());
        }
        slot
    }

    /// Retire `key` (`delete objet.clé`).
    pub fn remove(&mut self, key: &str) {
        if let Json::Obj(members) = self {
            members.retain(|(k, _)| k != key);
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Vrai pour une valeur « truthy » de JavaScript (`valeur || défaut`).
    fn truthy(&self) -> bool {
        match self {
            Json::Null | Json::Bool(false) => false,
            Json::Str(text) => !text.is_empty(),
            Json::Num(raw) => raw.parse::<f64>() != Ok(0.0),
            _ => true,
        }
    }

    /// `JSON.stringify(valeur, null, 2)`.
    pub fn to_pretty(&self) -> String {
        let mut out = String::new();
        self.write_pretty(&mut out, 0);
        out
    }

    fn write_pretty(&self, out: &mut String, level: usize) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Num(raw) => out.push_str(raw),
            Json::Str(text) => write_string(out, text),
            Json::Arr(items) if items.is_empty() => out.push_str("[]"),
            Json::Arr(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    push_indent(out, level + 1);
                    item.write_pretty(out, level + 1);
                }
                out.push('\n');
                push_indent(out, level);
                out.push(']');
            }
            Json::Obj(members) if members.is_empty() => out.push_str("{}"),
            Json::Obj(members) => {
                out.push('{');
                for (index, (key, value)) in members.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    out.push('\n');
                    push_indent(out, level + 1);
                    write_string(out, key);
                    out.push_str(": ");
                    value.write_pretty(out, level + 1);
                }
                out.push('\n');
                push_indent(out, level);
                out.push('}');
            }
        }
    }
}

fn push_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// Chaîne JSON comme `JSON.stringify` : seuls `"`, `\` et les caractères
/// de contrôle sont échappés.
fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Profondeur maximale acceptée : un fichier plus imbriqué est refusé.
const MAX_DEPTH: usize = 64;

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ConfigError> {
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(ConfigError::InvalidJson)
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), ConfigError> {
        if self.text[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(())
        } else {
            Err(ConfigError::InvalidJson)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, ConfigError> {
        if depth > MAX_DEPTH {
            return Err(ConfigError::InvalidJson);
        }
        match self.peek().ok_or(ConfigError::InvalidJson)? {
            b'{' => {
                self.pos += 1;
                let mut members = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b'}') {
                    self.pos += 1;
                    return Ok(Json::Obj(members));
                }
                loop {
                    self.skip_ws();
                    let key = self.string()?;
                    self.skip_ws();
                    self.expect(b':')?;
                    self.skip_ws();
                    members.push((key, self.value(depth + 1)?));
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(Json::Obj(members));
                        }
                        _ => return Err(ConfigError::InvalidJson),
                    }
                }
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_ws();
                if self.peek() == Some(b']') {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                loop {
                    self.skip_ws();
                    items.push(self.value(depth + 1)?);
                    self.skip_ws();
                    match self.peek() {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Json::Arr(items));
                        }
                        _ => return Err(ConfigError::InvalidJson),
                    }
                }
            }
            b'"' => Ok(Json::Str(self.string()?)),
            b't' => self.literal("true").map(|()| Json::Bool(true)),
            b'f' => self.literal("false").map(|()| Json::Bool(false)),
            b'n' => self.literal("null").map(|()| Json::Null),
            b'-' | b'0'..=b'9' => self.number(),
            _ => Err(ConfigError::InvalidJson),
        }
    }

    fn number(&mut self) -> Result<Json, ConfigError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let digits = |parser: &mut Self| {
            let from = parser.pos;
            while matches!(parser.peek(), Some(b'0'..=b'9')) {
                parser.pos += 1;
            }
            parser.pos > from
        };
        if self.peek() == Some(b'0') {
            self.pos += 1;
        } else if !digits(self) {
            return Err(ConfigError::InvalidJson);
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if !digits(self) {
                return Err(ConfigError::InvalidJson);
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !digits(self) {
                return Err(ConfigError::InvalidJson);
            }
        }
        Ok(Json::Num(self.text[start..self.pos].to_owned()))
    }

    fn hex4(&mut self) -> Result<u32, ConfigError> {
        let digits = self
            .text
            .get(self.pos..self.pos + 4)
            .ok_or(ConfigError::InvalidJson)?;
        let value = u32::from_str_radix(digits, 16).map_err(|_| ConfigError::InvalidJson)?;
        self.pos += 4;
        Ok(value)
    }

    fn string(&mut self) -> Result<String, ConfigError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let rest = &self.text[self.pos..];
            let c = rest.chars().next().ok_or(ConfigError::InvalidJson)?;
            self.pos += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let escape = self.peek().ok_or(ConfigError::InvalidJson)?;
                    self.pos += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&first) {
                                if !self.text[self.pos..].starts_with("\\u") {
                                    return Err(ConfigError::UnsupportedString);
                                }
                                self.pos += 2;
                                let second = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&second) {
                                    return Err(ConfigError::UnsupportedString);
                                }
                                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                            } else {
                                first
                            };
                            out.push(char::from_u32(code).ok_or(ConfigError::UnsupportedString)?);
                        }
                        _ => return Err(ConfigError::InvalidJson),
                    }
                }
                c if (c as u32) < 0x20 => return Err(ConfigError::InvalidJson),
                c => out.push(c),
            }
        }
    }
}

/// Membre de premier niveau, avec la position de sa valeur dans le texte.
struct Member {
    key: String,
    key_start: usize,
    value_start: usize,
    value_end: usize,
}

/// Racine du document : membres de premier niveau et bornes des accolades.
struct Root {
    open: usize,
    close: usize,
    members: Vec<Member>,
}

fn scan_root(text: &str) -> Result<Root, ConfigError> {
    let mut parser = Parser::new(text);
    parser.skip_ws();
    if parser.peek() != Some(b'{') {
        // Un document JSON valide mais pas objet : refus distinct.
        return match Json::parse(text) {
            Ok(_) => Err(ConfigError::NotAnObject),
            Err(error) => Err(error),
        };
    }
    let open = parser.pos;
    parser.pos += 1;
    let mut members = Vec::new();
    parser.skip_ws();
    if parser.peek() == Some(b'}') {
        let close = parser.pos;
        parser.pos += 1;
        parser.skip_ws();
        return if parser.pos == text.len() {
            Ok(Root {
                open,
                close,
                members,
            })
        } else {
            Err(ConfigError::InvalidJson)
        };
    }
    let close = loop {
        parser.skip_ws();
        let key_start = parser.pos;
        let key = parser.string()?;
        parser.skip_ws();
        parser.expect(b':')?;
        parser.skip_ws();
        let value_start = parser.pos;
        parser.value(1)?;
        members.push(Member {
            key,
            key_start,
            value_start,
            value_end: parser.pos,
        });
        parser.skip_ws();
        match parser.peek() {
            Some(b',') => parser.pos += 1,
            Some(b'}') => {
                let close = parser.pos;
                parser.pos += 1;
                break close;
            }
            _ => return Err(ConfigError::InvalidJson),
        }
    };
    parser.skip_ws();
    if parser.pos != text.len() {
        return Err(ConfigError::InvalidJson);
    }
    Ok(Root {
        open,
        close,
        members,
    })
}

/// Indentation (espaces) de la ligne qui porte `position`, si `position`
/// est le premier caractère non blanc de sa ligne.
fn line_indent(text: &str, position: usize) -> Option<&str> {
    let line_start = text[..position].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..position];
    before.chars().all(|c| c == ' ').then_some(before)
}

/// Refus des fichiers que la spec ne sait pas encore traiter.
fn check_layout(text: &str) -> Result<(), ConfigError> {
    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count();
    if crlf != 0 && crlf != lf {
        return Err(ConfigError::MixedLineEndings);
    }
    if text.contains('\r') && text.matches('\r').count() != crlf {
        return Err(ConfigError::MixedLineEndings);
    }
    for line in text.split('\n') {
        let leading: &str = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
        if leading.contains('\t') {
            return Err(ConfigError::Tabs);
        }
    }
    Ok(())
}

/// Valeur telle que Bruno l'écrirait, décalée de `indent` à partir de la
/// deuxième ligne et avec la fin de ligne `eol`.
fn render_value(value: &Json, indent: &str, eol: &str) -> String {
    value.to_pretty().replace('\n', &format!("{eol}{indent}"))
}

/// Applique `changes` (clé de premier niveau, nouvelle valeur) à `text`.
///
/// Les nettoyages de Bruno avant écriture (`proxy.disabled: false` et
/// `proxy.config.auth.disabled: false` retirés) sont appliqués ici.
pub fn apply(text: &str, changes: &[(&str, Json)]) -> Result<String, ConfigError> {
    check_layout(text)?;
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = text.to_owned();
    for (key, value) in changes {
        if !WRITABLE_KEYS.contains(key) {
            return Err(ConfigError::ForbiddenKey((*key).to_owned()));
        }
        let mut value = value.clone();
        if *key == "proxy" {
            clean_proxy(&mut value);
        }
        let root = scan_root(&out)?;
        let mut found = root.members.iter().filter(|m| m.key == *key);
        let existing = found.next();
        if found.next().is_some() {
            return Err(ConfigError::DuplicateKey((*key).to_owned()));
        }
        match existing {
            Some(member) => {
                let indent = line_indent(&out, member.key_start).unwrap_or("");
                let rendered = render_value(&value, indent, eol);
                out.replace_range(member.value_start..member.value_end, &rendered);
            }
            None => {
                let indent = root
                    .members
                    .first()
                    .and_then(|m| line_indent(&out, m.key_start))
                    .unwrap_or("  ")
                    .to_owned();
                let mut member = String::new();
                write_string(&mut member, key);
                member.push_str(": ");
                member.push_str(&render_value(&value, &indent, eol));
                match root.members.last() {
                    Some(last) => {
                        let insertion = format!(",{eol}{indent}{member}");
                        out.insert_str(last.value_end, &insertion);
                    }
                    None => {
                        let inner = format!("{eol}{indent}{member}{eol}");
                        out.replace_range(root.open + 1..root.close, &inner);
                    }
                }
            }
        }
    }
    // Garde-fou : le résultat doit se relire.
    Json::parse(&out).map_err(|_| ConfigError::Unreadable)?;
    Ok(out)
}

/// `transformBrunoConfigBeforeSave`, étapes 3 et 4 (`docs/reglages-collection.md`
/// § 1.2) : `disabled: false` n'est jamais écrit.
pub fn clean_proxy(proxy: &mut Json) {
    if proxy.get("disabled").and_then(Json::as_bool) == Some(false) {
        proxy.remove("disabled");
    }
    if let Some(config) = proxy.get_mut("config")
        && let Some(auth) = config.get_mut("auth")
        && auth.get("disabled").and_then(Json::as_bool) == Some(false)
    {
        auth.remove("disabled");
    }
}

/// `true` si `proxy` est à l'ancien format (clé `enabled`).
pub fn is_legacy_proxy(proxy: &Json) -> bool {
    proxy.get("enabled").is_some()
}

/// Conversion de l'ancien format de proxy (`transformProxyConfig` de
/// `@usebruno/requests`). Un proxy au nouveau format est rendu tel quel.
pub fn convert_legacy_proxy(proxy: &Json) -> Json {
    if !is_legacy_proxy(proxy) {
        return proxy.clone();
    }
    let or = |value: Option<&Json>, default: Json| match value {
        Some(v) if v.truthy() => v.clone(),
        _ => default,
    };
    let auth = proxy.get("auth");
    let enabled = proxy.get("enabled");
    let mut inherit = true;
    let mut disabled = None;
    match enabled {
        Some(Json::Bool(true)) => {
            disabled = Some(false);
            inherit = false;
        }
        Some(Json::Bool(false)) => {
            disabled = Some(true);
            inherit = false;
        }
        Some(Json::Str(text)) if text == "global" => {
            disabled = Some(false);
            inherit = true;
        }
        _ => {}
    }
    let mut new_auth = Json::Obj(vec![
        (
            "username".into(),
            or(
                auth.and_then(|a| a.get("username")),
                Json::Str(String::new()),
            ),
        ),
        (
            "password".into(),
            or(
                auth.and_then(|a| a.get("password")),
                Json::Str(String::new()),
            ),
        ),
    ]);
    if auth.and_then(|a| a.get("enabled")).and_then(Json::as_bool) == Some(false) {
        new_auth.set("disabled", Json::Bool(true));
    }
    let mut out = Json::Obj(vec![
        ("inherit".into(), Json::Bool(inherit)),
        (
            "config".into(),
            Json::Obj(vec![
                (
                    "protocol".into(),
                    or(proxy.get("protocol"), Json::Str("http".into())),
                ),
                (
                    "hostname".into(),
                    or(proxy.get("hostname"), Json::Str(String::new())),
                ),
                ("port".into(), or(proxy.get("port"), Json::Null)),
                ("auth".into(), new_auth),
                (
                    "bypassProxy".into(),
                    or(proxy.get("bypassProxy"), Json::Str(String::new())),
                ),
            ]),
        ),
    ]);
    if disabled == Some(true) {
        out.set("disabled", Json::Bool(true));
    }
    out
}

/// Chemin de `bruno.json` pour la collection de racine `root`.
pub fn config_path(root: &Path) -> PathBuf {
    root.join(CONFIG_FILE)
}

fn io_error(path: &Path, source: io::Error) -> WriteError {
    WriteError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Refuse un `bruno.json` qui est un lien symbolique : l'écriture doit
/// rester confinée à `<racine>/bruno.json`.
fn check_not_symlink(path: &Path) -> Result<(), WriteError> {
    let meta = fs::symlink_metadata(path).map_err(|source| io_error(path, source))?;
    if meta.file_type().is_symlink() {
        return Err(ConfigError::Symlink.into());
    }
    Ok(())
}

/// Lit `bruno.json` au moment d'ouvrir le popup : texte et instantané de
/// fraîcheur. Refuse un lien symbolique, un JSON invalide et une racine
/// qui n'est pas un objet.
pub fn read_for_edit(root: &Path) -> Result<(String, FileStamp), WriteError> {
    let path = config_path(root);
    check_not_symlink(&path)?;
    let stamp = FileStamp::capture(&path).map_err(|source| io_error(&path, source))?;
    let bytes = fs::read(&path).map_err(|source| io_error(&path, source))?;
    let text = String::from_utf8(bytes).map_err(|_| WriteError::from(ConfigError::InvalidJson))?;
    scan_root(&text)?;
    check_layout(&text)?;
    Ok((text, stamp))
}

/// Écrit `changes` dans `<root>/bruno.json` si `stamp` correspond encore
/// au fichier. Écriture atomique ; le seul fichier touché est `bruno.json`.
pub fn write_bruno_config(
    root: &Path,
    stamp: &FileStamp,
    changes: &[(&str, Json)],
) -> Result<FileStamp, WriteError> {
    let path = config_path(root);
    check_not_symlink(&path)?;
    let current = FileStamp::capture(&path).map_err(|source| io_error(&path, source))?;
    if current != *stamp {
        return Err(WriteError::Stale { path });
    }
    let bytes = fs::read(&path).map_err(|source| io_error(&path, source))?;
    let text = String::from_utf8(bytes).map_err(|_| WriteError::from(ConfigError::InvalidJson))?;
    let updated = apply(&text, changes)?;
    write_atomic(&path, stamp, updated.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Json {
        Json::Str(text.to_owned())
    }

    fn obj(members: Vec<(&str, Json)>) -> Json {
        Json::Obj(
            members
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect(),
        )
    }

    #[test]
    fn pretty_matches_json_stringify() {
        let value = obj(vec![
            ("a", Json::Arr(vec![])),
            ("b", obj(vec![])),
            (
                "c",
                Json::Arr(vec![s("x\"\\\n\t\u{1}é"), Json::Num("8080".into())]),
            ),
            ("d", Json::Null),
        ]);
        assert_eq!(
            value.to_pretty(),
            "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    \"x\\\"\\\\\\n\\t\\u0001é\",\n    8080\n  ],\n  \"d\": null\n}"
        );
    }

    #[test]
    fn parse_round_trips_order_and_escapes() {
        let text = r#"{"b":1,"a":"é😀\/","c":[true,null,-1.5e3]}"#;
        let value = Json::parse(text).expect("valide");
        assert_eq!(
            value,
            obj(vec![
                ("b", Json::Num("1".into())),
                ("a", s("é😀/")),
                (
                    "c",
                    Json::Arr(vec![
                        Json::Bool(true),
                        Json::Null,
                        Json::Num("-1.5e3".into())
                    ])
                ),
            ])
        );
    }

    #[test]
    fn parse_rejects_invalid_documents() {
        for bad in [
            "",
            "{",
            "{\"a\":}",
            "{\"a\":1,}",
            "[1,]",
            "{\"a\":01}",
            "tru",
        ] {
            assert!(Json::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn adds_missing_key_at_the_end() {
        let before = "{\n  \"version\": \"1\",\n  \"ignore\": [\"node_modules\", \".git\"]\n}";
        let value = obj(vec![("requestType", s("http")), ("requestUrl", s(""))]);
        let after = apply(before, &[("presets", value)]).expect("ok");
        assert_eq!(
            after,
            "{\n  \"version\": \"1\",\n  \"ignore\": [\"node_modules\", \".git\"],\n  \"presets\": {\n    \"requestType\": \"http\",\n    \"requestUrl\": \"\"\n  }\n}"
        );
    }

    #[test]
    fn replaces_only_the_value_of_an_existing_key() {
        let before = "{\n  \"proxy\": {\"inherit\": true},\n  \"ignore\": [\"a\", \"b\"],\n  \"name\": \"x\"\n}\n";
        let after = apply(
            before,
            &[("proxy", obj(vec![("inherit", Json::Bool(false))]))],
        )
        .expect("ok");
        assert_eq!(
            after,
            "{\n  \"proxy\": {\n    \"inherit\": false\n  },\n  \"ignore\": [\"a\", \"b\"],\n  \"name\": \"x\"\n}\n"
        );
    }

    #[test]
    fn four_space_indentation_and_crlf_are_kept() {
        let before = "{\r\n    \"name\": \"x\"\r\n}";
        let after = apply(
            before,
            &[("presets", obj(vec![("requestType", s("http"))]))],
        )
        .expect("ok");
        assert_eq!(
            after,
            "{\r\n    \"name\": \"x\",\r\n    \"presets\": {\r\n      \"requestType\": \"http\"\r\n    }\r\n}"
        );
    }

    #[test]
    fn empty_object_gets_the_key() {
        let after = apply("{}", &[("presets", obj(vec![]))]).expect("ok");
        assert_eq!(after, "{\n  \"presets\": {}\n}");
    }

    #[test]
    fn refusals() {
        let p = || obj(vec![]);
        assert_eq!(
            apply("[1]", &[("proxy", p())]),
            Err(ConfigError::NotAnObject)
        );
        assert_eq!(apply("{", &[("proxy", p())]), Err(ConfigError::InvalidJson));
        assert_eq!(
            apply("{\"proxy\":1,\n\"proxy\":2}", &[("proxy", p())]),
            Err(ConfigError::DuplicateKey("proxy".into()))
        );
        assert_eq!(
            apply("{\n\t\"a\": 1\n}", &[("proxy", p())]),
            Err(ConfigError::Tabs)
        );
        assert_eq!(
            apply("{\r\n\"a\": 1\n}", &[("proxy", p())]),
            Err(ConfigError::MixedLineEndings)
        );
        assert_eq!(
            apply("{}", &[("name", p())]),
            Err(ConfigError::ForbiddenKey("name".into()))
        );
    }

    #[test]
    fn duplicate_of_another_key_is_not_a_problem() {
        let before = "{\n  \"a\": 1,\n  \"a\": 2\n}";
        assert!(apply(before, &[("proxy", obj(vec![]))]).is_ok());
    }

    #[test]
    fn disabled_false_is_never_written() {
        let proxy = obj(vec![
            ("disabled", Json::Bool(false)),
            ("inherit", Json::Bool(false)),
            (
                "config",
                obj(vec![(
                    "auth",
                    obj(vec![("username", s("")), ("disabled", Json::Bool(false))]),
                )]),
            ),
        ]);
        let after = apply("{}", &[("proxy", proxy)]).expect("ok");
        assert!(!after.contains("disabled"));
    }

    #[test]
    fn legacy_proxy_conversion() {
        let legacy = Json::parse(
            r#"{"enabled":true,"protocol":"https","hostname":"h","port":8080,
                "auth":{"enabled":false,"username":"u","password":"{{p}}"},"bypassProxy":"x"}"#,
        )
        .expect("valide");
        let converted = convert_legacy_proxy(&legacy);
        assert_eq!(
            converted.to_pretty(),
            "{\n  \"inherit\": false,\n  \"config\": {\n    \"protocol\": \"https\",\n    \"hostname\": \"h\",\n    \"port\": 8080,\n    \"auth\": {\n      \"username\": \"u\",\n      \"password\": \"{{p}}\",\n      \"disabled\": true\n    },\n    \"bypassProxy\": \"x\"\n  }\n}"
        );
        let off = convert_legacy_proxy(&obj(vec![("enabled", Json::Bool(false))]));
        assert_eq!(off.get("disabled"), Some(&Json::Bool(true)));
        assert_eq!(off.get("inherit"), Some(&Json::Bool(false)));
        let global = convert_legacy_proxy(&obj(vec![("enabled", s("global"))]));
        assert_eq!(global.get("disabled"), None);
        assert_eq!(global.get("inherit"), Some(&Json::Bool(true)));
        assert_eq!(
            global.get("config").and_then(|c| c.get("port")),
            Some(&Json::Null)
        );
    }

    #[test]
    fn errors_never_quote_values() {
        let text = format!("{}", ConfigError::DuplicateKey("proxy".into()));
        assert!(!text.contains("secret"));
    }
}
