//! AST fidèle d'un fichier `.bru`.
//!
//! Chaque nœud est une tranche du source d'origine ; les tranches sont
//! contiguës et couvrent tout le fichier, si bien que `raw()` reproduit le
//! fichier à l'octet près sans aucune logique de sérialisation. La forme
//! interprétée (dictionnaire, texte, liste) est dérivée du nom du bloc et
//! ne fait jamais autorité sur la tranche brute.

use std::ops::Range;

use super::error::ParseError;
use super::lexer::{self, RawNode};

pub use super::lexer::Delimiter;

/// Noms des blocs de méthode HTTP, tels qu'écrits dans un `.bru`.
pub const METHODS: [&str; 9] = [
    "get", "post", "put", "delete", "patch", "options", "head", "connect", "trace",
];

/// Forme d'interprétation d'un bloc connu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockForm {
    /// `clé: valeur` par ligne.
    Dictionary,
    /// Contenu libre, désindenté de deux espaces à la lecture.
    Text,
    /// Éléments séparés par virgules ou sauts de ligne.
    List,
}

/// Forme d'un bloc d'après son nom ; `None` pour un bloc inconnu.
pub fn known_form(name: &str) -> Option<BlockForm> {
    if METHODS.contains(&name) || name.starts_with("auth:") {
        return Some(BlockForm::Dictionary);
    }
    match name {
        "meta"
        | "params:query"
        | "params:path"
        | "headers"
        | "settings"
        | "assert"
        | "vars"
        | "vars:pre-request"
        | "vars:post-response"
        | "auth"
        | "body:form-urlencoded"
        | "body:multipart-form"
        | "body:file" => Some(BlockForm::Dictionary),
        "body"
        | "body:json"
        | "body:text"
        | "body:xml"
        | "body:sparql"
        | "body:graphql"
        | "body:graphql:vars"
        | "script:pre-request"
        | "script:post-response"
        | "tests"
        | "docs" => Some(BlockForm::Text),
        "vars:secret" => Some(BlockForm::List),
        _ => None,
    }
}

/// Entrée d'un bloc dictionnaire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    /// Valeur telle qu'écrite ; une valeur `'''` multi-ligne est rassemblée
    /// sans ses délimiteurs, lignes intérieures désindentées de quatre
    /// espaces au plus.
    pub value: String,
    /// Clé préfixée de `~`.
    pub disabled: bool,
    /// Ligne(s) brute(s) de l'entrée, fins de ligne comprises.
    pub span: Range<usize>,
}

/// Contenu interprété d'un bloc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockBody {
    Dictionary(Vec<Entry>),
    /// Tranche brute des lignes entre ouverture et fermeture.
    Text {
        content: Range<usize>,
    },
    List(Vec<String>),
    /// Bloc dont le nom n'est pas reconnu : contenu brut seulement.
    Unknown {
        content: Range<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub delimiter: Delimiter,
    pub body: BlockBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    /// Lignes vides entre deux blocs.
    Blank,
    Block(Block),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Tranche brute dans le source.
    pub span: Range<usize>,
    pub kind: NodeKind,
}

/// Fichier `.bru` parsé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BruFile {
    source: String,
    nodes: Vec<Node>,
}

impl BruFile {
    /// Parse un source complet. Ne rejette que les défauts de structure
    /// (bloc non fermé, texte hors bloc, ouverture ou fermeture invalide).
    pub fn parse(source: String) -> Result<Self, ParseError> {
        let raw = lexer::lex(&source)?;
        let nodes = raw
            .into_iter()
            .map(|node| match node {
                RawNode::Blank(span) => Node {
                    span,
                    kind: NodeKind::Blank,
                },
                RawNode::Block(block) => {
                    let body = match known_form(&block.name) {
                        Some(BlockForm::Dictionary) => {
                            BlockBody::Dictionary(parse_dictionary(&source, block.content))
                        }
                        Some(BlockForm::Text) => BlockBody::Text {
                            content: block.content,
                        },
                        Some(BlockForm::List) => {
                            BlockBody::List(parse_list(&source[block.content]))
                        }
                        None => BlockBody::Unknown {
                            content: block.content,
                        },
                    };
                    Node {
                        span: block.span,
                        kind: NodeKind::Block(Block {
                            name: block.name,
                            delimiter: block.delimiter,
                            body,
                        }),
                    }
                }
            })
            .collect();
        Ok(Self { source, nodes })
    }

    /// Source d'origine, identique à l'octet au fichier lu.
    pub fn raw(&self) -> &str {
        &self.source
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Texte brut d'une tranche.
    pub fn slice(&self, range: &Range<usize>) -> &str {
        &self.source[range.clone()]
    }

    /// Blocs dans l'ordre du fichier.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.nodes.iter().filter_map(|node| match &node.kind {
            NodeKind::Block(block) => Some(block),
            NodeKind::Blank => None,
        })
    }

    /// Premier bloc portant ce nom.
    pub fn block(&self, name: &str) -> Option<&Block> {
        self.blocks().find(|block| block.name == name)
    }

    pub fn has_block(&self, name: &str) -> bool {
        self.block(name).is_some()
    }

    /// Entrées du premier bloc dictionnaire portant ce nom.
    pub fn dictionary(&self, name: &str) -> Option<&[Entry]> {
        match &self.block(name)?.body {
            BlockBody::Dictionary(entries) => Some(entries),
            _ => None,
        }
    }

    /// Valeur d'une entrée d'un bloc dictionnaire, entrées désactivées
    /// comprises.
    pub fn entry(&self, block: &str, key: &str) -> Option<&str> {
        self.dictionary(block)?
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value.as_str())
    }

    /// Contenu désindenté du premier bloc texte portant ce nom.
    pub fn text(&self, name: &str) -> Option<String> {
        match &self.block(name)?.body {
            BlockBody::Text { content } => Some(dedent(self.slice(content))),
            _ => None,
        }
    }

    /// Vrai si les tranches des nœuds sont contiguës et couvrent tout le
    /// source : l'invariant qui garantit le round-trip.
    pub fn spans_are_contiguous(&self) -> bool {
        let mut expected = 0;
        for node in &self.nodes {
            if node.span.start != expected {
                return false;
            }
            expected = node.span.end;
        }
        expected == self.source.len()
    }
}

/// Désindente un contenu de bloc texte : retire la fin de ligne finale et
/// au plus deux espaces en tête de chaque ligne, comme Bruno.
pub fn dedent(raw: &str) -> String {
    let raw = raw
        .strip_suffix("\r\n")
        .or_else(|| raw.strip_suffix('\n'))
        .unwrap_or(raw);
    raw.split('\n')
        .map(|line| dedent_by(line.strip_suffix('\r').unwrap_or(line), 2))
        .collect::<Vec<_>>()
        .join("\n")
}

fn dedent_by(line: &str, max: usize) -> String {
    let spaces = line.chars().take(max).take_while(|c| *c == ' ').count();
    line[spaces..].to_owned()
}

fn parse_dictionary(source: &str, content: Range<usize>) -> Vec<Entry> {
    let base = content.start;
    let text = &source[content];
    let mut entries = Vec::new();
    let mut lines = lexer::lines(text);
    while let Some(line) = lines.next() {
        let trimmed = line.text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (disabled, rest) = match trimmed.strip_prefix('~') {
            Some(rest) => (true, rest),
            None => (false, trimmed),
        };
        let (key, value) = match rest.find(':') {
            Some(i) => (rest[..i].trim(), rest[i + 1..].trim()),
            None => (rest, ""),
        };
        let mut end = line.end;
        let value = match value.strip_prefix("'''") {
            None => value.to_owned(),
            Some(after) => match after.strip_suffix("'''") {
                Some(inner) => inner.to_owned(),
                None => {
                    let mut parts: Vec<String> = Vec::new();
                    if !after.trim().is_empty() {
                        parts.push(after.trim().to_owned());
                    }
                    for inner in lines.by_ref() {
                        end = inner.end;
                        if let Some(before) = inner.text.trim_end().strip_suffix("'''") {
                            if !before.trim().is_empty() {
                                parts.push(dedent_by(before, 4));
                            }
                            break;
                        }
                        parts.push(dedent_by(inner.text, 4));
                    }
                    parts.join("\n")
                }
            },
        };
        entries.push(Entry {
            key: key.to_owned(),
            value,
            disabled,
            span: base + line.start..base + end,
        });
    }
    entries
}

fn parse_list(text: &str) -> Vec<String> {
    text.split([',', '\n', '\r'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> BruFile {
        BruFile::parse(source.to_owned()).expect("source valide")
    }

    #[test]
    fn disabled_entry() {
        let file = parse("headers {\n  Accept: application/json\n  ~X-Debug: 1\n}\n");
        let entries = file.dictionary("headers").expect("dictionnaire");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "Accept");
        assert_eq!(entries[0].value, "application/json");
        assert!(!entries[0].disabled);
        assert_eq!(entries[1].key, "X-Debug");
        assert_eq!(entries[1].value, "1");
        assert!(entries[1].disabled);
        assert_eq!(file.slice(&entries[1].span), "  ~X-Debug: 1\n");
    }

    #[test]
    fn value_with_colon_and_url() {
        let file = parse("get {\n  url: https://{{host}}/a?b=c:d\n}\n");
        assert_eq!(file.entry("get", "url"), Some("https://{{host}}/a?b=c:d"));
    }

    #[test]
    fn multiline_value_over_three_lines() {
        let source = "params:query {\n  q: '''\n    line one\n    line two\n    line three\n  '''\n  page: 1\n}\n";
        let file = parse(source);
        let entries = file.dictionary("params:query").expect("dictionnaire");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].key, "q");
        assert_eq!(entries[0].value, "line one\nline two\nline three");
        assert_eq!(
            file.slice(&entries[0].span),
            "  q: '''\n    line one\n    line two\n    line three\n  '''\n"
        );
        assert_eq!(entries[1].key, "page");
        assert_eq!(entries[1].value, "1");
    }

    #[test]
    fn single_line_triple_quoted_value() {
        let file = parse("vars {\n  a: '''x'''\n}\n");
        assert_eq!(file.entry("vars", "a"), Some("x"));
    }

    #[test]
    fn line_without_colon_is_kept() {
        let file = parse("meta {\n  weird line\n  name: a\n}\n");
        let entries = file.dictionary("meta").expect("dictionnaire");
        assert_eq!(entries[0].key, "weird line");
        assert_eq!(entries[0].value, "");
        assert_eq!(entries[1].key, "name");
    }

    #[test]
    fn dedented_json_body_with_nested_braces() {
        let source = "body:json {\n  {\n    \"a\": {\n      \"b\": [\n        { \"c\": 1 }\n      ]\n    },\n    \"s\": \"}\"\n  }\n}\n";
        let file = parse(source);
        let json = file.text("body:json").expect("bloc texte");
        assert_eq!(
            json,
            "{\n  \"a\": {\n    \"b\": [\n      { \"c\": 1 }\n    ]\n  },\n  \"s\": \"}\"\n}"
        );
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON valide");
        assert_eq!(parsed["s"], "}");
        assert_eq!(parsed["a"]["b"][0]["c"], 1);
    }

    #[test]
    fn dedent_handles_crlf_and_short_indent() {
        assert_eq!(dedent("  a\r\n b\r\nc\r\n"), "a\nb\nc");
        assert_eq!(dedent(""), "");
        assert_eq!(dedent("    four\n"), "  four");
    }

    #[test]
    fn unknown_block_keeps_name_and_raw_content() {
        let source = "meta {\n  name: a\n}\n\nfuture:thing {\n  mode: x\n  nested {\n    deeper: yes\n  }\n}\n";
        let file = parse(source);
        let names: Vec<&str> = file.blocks().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["meta", "future:thing"]);
        let block = file.block("future:thing").expect("bloc inconnu");
        match &block.body {
            BlockBody::Unknown { content } => {
                assert_eq!(
                    file.slice(content),
                    "  mode: x\n  nested {\n    deeper: yes\n  }\n"
                );
            }
            other => panic!("attendu Unknown, obtenu {other:?}"),
        }
        assert!(file.spans_are_contiguous());
        assert_eq!(file.raw(), source);
    }

    #[test]
    fn list_block() {
        let file = parse("vars:secret [\n  token,\n  other\n  third\n]\n");
        match &file.block("vars:secret").expect("bloc").body {
            BlockBody::List(items) => assert_eq!(items, &["token", "other", "third"]),
            other => panic!("attendu List, obtenu {other:?}"),
        }
    }

    #[test]
    fn coverage_invariant_on_lexer_cases() {
        for source in [
            "meta {   \n  name: a\n}  \n",
            "body:json {\n  {\n    \"s\": \"}\"\n  }\n}\n",
            "meta {\n  name: a\n}",
            "meta {\r\n  name: a\r\n}\r\n\r\nget {\r\n  url: x\r\n}\r\n",
            "\n\nmeta {\n}\n\n\n",
            "",
        ] {
            let file = parse(source);
            assert!(file.spans_are_contiguous(), "{source:?}");
            assert_eq!(file.raw(), source);
        }
    }

    #[test]
    fn known_forms() {
        assert_eq!(known_form("get"), Some(BlockForm::Dictionary));
        assert_eq!(known_form("auth:oauth2"), Some(BlockForm::Dictionary));
        assert_eq!(known_form("body:json"), Some(BlockForm::Text));
        assert_eq!(known_form("vars:secret"), Some(BlockForm::List));
        assert_eq!(known_form("grpc"), None);
    }
}
