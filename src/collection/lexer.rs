//! Découpage d'un fichier `.bru` en blocs bruts, sans interprétation.
//!
//! Le format est orienté lignes. Un bloc s'ouvre par `nom {` ou `nom [` et
//! se ferme par une ligne dont le premier caractère est `}` ou `]`. Comme
//! dans la grammaire officielle de Bruno, c'est cette accolade en première
//! colonne qui ferme le bloc : les `}` indentés d'un corps JSON ou d'un
//! script restent du contenu. Rien n'est normalisé : les fins de ligne
//! (`\n` ou `\r\n`) et les espaces sont conservés dans les tranches.

use std::ops::Range;

use super::error::ParseError;

/// Caractère d'ouverture d'un bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    /// `{` … `}`
    Brace,
    /// `[` … `]`
    Bracket,
}

impl Delimiter {
    fn closing(self) -> char {
        match self {
            Self::Brace => '}',
            Self::Bracket => ']',
        }
    }
}

/// Bloc repéré dans le source, avant interprétation de son contenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawBlock {
    pub name: String,
    pub delimiter: Delimiter,
    /// De la ligne d'ouverture à la ligne de fermeture incluses.
    pub span: Range<usize>,
    /// Lignes strictement comprises entre ouverture et fermeture.
    pub content: Range<usize>,
    /// Numéro (base 1) de la ligne d'ouverture.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RawNode {
    /// Suite de lignes vides (ou d'espaces) entre deux blocs.
    Blank(Range<usize>),
    Block(RawBlock),
}

/// Une ligne du source avec sa position.
pub(crate) struct Line<'a> {
    /// Début de la ligne dans le source.
    pub start: usize,
    /// Fin de la ligne, fin de ligne comprise.
    pub end: usize,
    /// Texte de la ligne sans `\n` ni `\r\n`.
    pub text: &'a str,
    /// Numéro base 1.
    pub number: usize,
}

/// Itère les lignes de `source`, positions relatives à `source`.
pub(crate) fn lines(source: &str) -> impl Iterator<Item = Line<'_>> {
    let mut pos = 0;
    let mut number = 0;
    std::iter::from_fn(move || {
        if pos >= source.len() {
            return None;
        }
        number += 1;
        let rest = &source[pos..];
        let (text_len, end) = match rest.find('\n') {
            Some(i) => (i, pos + i + 1),
            None => (rest.len(), source.len()),
        };
        let raw_text = &rest[..text_len];
        let text = raw_text.strip_suffix('\r').unwrap_or(raw_text);
        let line = Line {
            start: pos,
            end,
            text,
            number,
        };
        pos = end;
        Some(line)
    })
}

/// Découpe `source` en nœuds bruts contigus couvrant tout le source.
pub(crate) fn lex(source: &str) -> Result<Vec<RawNode>, ParseError> {
    let mut nodes = Vec::new();
    let mut open: Option<RawBlock> = None;
    let mut blank_start: Option<usize> = None;

    for line in lines(source) {
        if let Some(block) = open.as_mut() {
            if !line.text.starts_with(block.delimiter.closing()) {
                continue;
            }
            if !line.text[1..].trim().is_empty() {
                return Err(ParseError::BadClosing { line: line.number });
            }
            block.content.end = line.start;
            block.span.end = line.end;
            if let Some(block) = open.take() {
                nodes.push(RawNode::Block(block));
            }
            continue;
        }

        let trimmed = line.text.trim_end();
        if trimmed.is_empty() {
            blank_start.get_or_insert(line.start);
            continue;
        }
        if let Some(start) = blank_start.take() {
            nodes.push(RawNode::Blank(start..line.start));
        }
        let delimiter = match trimmed.chars().last() {
            Some('{') => Delimiter::Brace,
            Some('[') => Delimiter::Bracket,
            _ => return Err(ParseError::TextOutsideBlock { line: line.number }),
        };
        let name = trimmed[..trimmed.len() - 1].trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(ParseError::BadHeader { line: line.number });
        }
        open = Some(RawBlock {
            name: name.to_owned(),
            delimiter,
            span: line.start..line.end,
            content: line.end..line.end,
            line: line.number,
        });
    }

    if let Some(block) = open {
        return Err(ParseError::UnclosedBlock {
            name: block.name,
            line: block.line,
        });
    }
    if let Some(start) = blank_start {
        nodes.push(RawNode::Blank(start..source.len()));
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(nodes: &[RawNode]) -> Vec<Range<usize>> {
        nodes
            .iter()
            .map(|n| match n {
                RawNode::Blank(r) => r.clone(),
                RawNode::Block(b) => b.span.clone(),
            })
            .collect()
    }

    fn assert_covers(source: &str, nodes: &[RawNode]) {
        let mut expected = 0;
        for span in spans(nodes) {
            assert_eq!(span.start, expected, "tranches non contiguës");
            expected = span.end;
        }
        assert_eq!(expected, source.len(), "tranches ne couvrant pas la fin");
    }

    fn block(nodes: &[RawNode], index: usize) -> &RawBlock {
        match &nodes[index] {
            RawNode::Block(b) => b,
            RawNode::Blank(_) => panic!("nœud {index} : attendu un bloc"),
        }
    }

    #[test]
    fn header_with_trailing_spaces_and_closing_with_spaces() {
        let source = "meta {   \n  name: a\n}  \n";
        let nodes = lex(source).expect("valide");
        assert_eq!(nodes.len(), 1);
        let b = block(&nodes, 0);
        assert_eq!(b.name, "meta");
        assert_eq!(b.delimiter, Delimiter::Brace);
        assert_eq!(&source[b.content.clone()], "  name: a\n");
        assert_covers(source, &nodes);
    }

    #[test]
    fn indented_brace_stays_in_content() {
        let source = "body:json {\n  {\n    \"s\": \"}\"\n  }\n}\n";
        let nodes = lex(source).expect("valide");
        assert_eq!(nodes.len(), 1);
        let b = block(&nodes, 0);
        assert_eq!(&source[b.content.clone()], "  {\n    \"s\": \"}\"\n  }\n");
        assert_covers(source, &nodes);
    }

    #[test]
    fn no_trailing_newline() {
        let source = "meta {\n  name: a\n}";
        let nodes = lex(source).expect("valide");
        assert_eq!(nodes.len(), 1);
        assert_eq!(block(&nodes, 0).span, 0..source.len());
        assert_covers(source, &nodes);
    }

    #[test]
    fn crlf_line_endings() {
        let source = "meta {\r\n  name: a\r\n}\r\n\r\nget {\r\n  url: x\r\n}\r\n";
        let nodes = lex(source).expect("valide");
        assert_eq!(nodes.len(), 3);
        assert_eq!(block(&nodes, 0).name, "meta");
        assert!(matches!(nodes[1], RawNode::Blank(_)));
        assert_eq!(block(&nodes, 2).name, "get");
        assert_eq!(&source[block(&nodes, 0).content.clone()], "  name: a\r\n");
        assert_covers(source, &nodes);
    }

    #[test]
    fn blank_lines_are_merged_and_kept_at_edges() {
        let source = "\n\nmeta {\n}\n\n\n";
        let nodes = lex(source).expect("valide");
        assert_eq!(spans(&nodes), vec![0..2, 2..11, 11..13]);
        assert_covers(source, &nodes);
    }

    #[test]
    fn bracket_block() {
        let source = "vars:secret [\n  token\n]\n";
        let nodes = lex(source).expect("valide");
        let b = block(&nodes, 0);
        assert_eq!(b.delimiter, Delimiter::Bracket);
        assert_eq!(&source[b.content.clone()], "  token\n");
    }

    #[test]
    fn text_outside_block() {
        let err = lex("meta {\n}\nstray line\n").expect_err("invalide");
        assert!(matches!(err, ParseError::TextOutsideBlock { line: 3 }));
    }

    #[test]
    fn bad_header() {
        let err = lex("my block {\n}\n").expect_err("invalide");
        assert!(matches!(err, ParseError::BadHeader { line: 1 }));
        let err = lex("{\n}\n").expect_err("invalide");
        assert!(matches!(err, ParseError::BadHeader { line: 1 }));
    }

    #[test]
    fn bad_closing() {
        let err = lex("meta {\n} trailing\n").expect_err("invalide");
        assert!(matches!(err, ParseError::BadClosing { line: 2 }));
    }

    #[test]
    fn unclosed_block_reports_opening_line() {
        let err = lex("meta {\n  name: a\n}\n\nheaders {\n  A: b\n").expect_err("invalide");
        assert!(matches!(
            err,
            ParseError::UnclosedBlock { ref name, line: 5 } if name == "headers"
        ));
    }

    #[test]
    fn empty_source() {
        let nodes = lex("").expect("valide");
        assert!(nodes.is_empty());
    }
}
