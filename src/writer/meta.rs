//! Réécriture chirurgicale du bloc `meta` : seules les lignes `name:` et
//! `seq:` changent, le reste du fichier est réémis à l'octet près.
//!
//! Utilisé par les opérations de fichiers (`docs/edition-phase3.md`) :
//! renommer, cloner, réordonner. Une clé absente est insérée à la place où
//! `jsonToBru` l'écrirait (`name`, `type`, `seq`), avec l'indentation et la
//! fin de ligne du bloc.

use super::error::EditError;
use crate::collection::{BlockBody, BruFile, NodeKind};

/// Ordre des clés de `meta` dans `jsonToBru` / `jsonToCollectionBru`.
const META_ORDER: [&str; 3] = ["name", "type", "seq"];

/// Nouvelles valeurs de `meta` ; `None` laisse la clé intacte.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaUpdate {
    pub name: Option<String>,
    pub seq: Option<u32>,
}

/// Que faire quand le fichier n'a pas de bloc `meta`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingMeta {
    /// Requête : refus (le type de la requête n'est pas connu ici).
    Refuse,
    /// `folder.bru` : bloc créé en tête, comme `jsonToCollectionBru`.
    CreateAtTop,
}

/// Applique `update` au source `source` d'un fichier `.bru`.
pub fn rewrite_meta(
    source: &str,
    update: &MetaUpdate,
    missing: MissingMeta,
) -> Result<String, EditError> {
    if let Some(name) = &update.name
        && (name.contains(['\n', '\r']) || name.trim() != name || name.is_empty())
    {
        return Err(EditError::InvalidMetaName);
    }
    let file = BruFile::parse(source.to_owned()).map_err(|_| EditError::Unreadable)?;
    let eol = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };

    let Some((block_span, entries)) = file.nodes().iter().find_map(|node| match &node.kind {
        NodeKind::Block(block) if block.name == "meta" => match &block.body {
            BlockBody::Dictionary(entries) => Some((node.span.clone(), entries)),
            _ => None,
        },
        _ => None,
    }) else {
        return match missing {
            MissingMeta::Refuse => Err(EditError::MissingField { field: "meta" }),
            MissingMeta::CreateAtTop => {
                let mut block = format!("meta {{{eol}");
                if let Some(name) = &update.name {
                    block.push_str(&format!("  name: {name}{eol}"));
                }
                if let Some(seq) = update.seq {
                    block.push_str(&format!("  seq: {seq}{eol}"));
                }
                block.push_str(&format!("}}{eol}{eol}"));
                Ok(format!("{block}{source}"))
            }
        };
    };

    let indent = entries
        .first()
        .map(|entry| {
            let line = file.slice(&entry.span);
            line[..line.len() - line.trim_start().len()].to_owned()
        })
        .unwrap_or_else(|| "  ".to_owned());

    // Remplacements (plage, texte), triés par position.
    let mut replacements: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for (key, value) in [
        ("name", update.name.clone()),
        ("seq", update.seq.map(|seq| seq.to_string())),
    ] {
        let Some(value) = value else { continue };
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.key == key && !entry.disabled)
        {
            let line = file.slice(&entry.span);
            if line.trim_start().starts_with('~') || line.contains("'''") {
                return Err(EditError::MissingField { field: "meta" });
            }
            let (body, line_eol) = split_eol(line);
            let Some(colon) = body.find(':') else {
                return Err(EditError::MissingField { field: "meta" });
            };
            let text = format!("{}: {value}{line_eol}", &body[..colon]);
            replacements.push((entry.span.clone(), text));
            continue;
        }
        // Clé absente : insérée après la dernière clé qui la précède dans
        // l'ordre de Bruno, sinon en tête du bloc.
        let rank = META_ORDER.iter().position(|k| *k == key).unwrap_or(0);
        let anchor = entries
            .iter()
            .filter(|entry| {
                META_ORDER
                    .iter()
                    .position(|k| *k == entry.key)
                    .is_some_and(|r| r < rank)
            })
            .map(|entry| entry.span.end)
            .max();
        let at = match anchor {
            Some(end) => end,
            None => {
                // Juste après la ligne d'ouverture `meta {`.
                let opening = file.slice(&block_span);
                let line_end = opening.find('\n').map_or(opening.len(), |i| i + 1);
                block_span.start + line_end
            }
        };
        replacements.push((at..at, format!("{indent}{key}: {value}{eol}")));
    }
    replacements.sort_by_key(|(range, _)| (range.start, range.end));

    let mut out = String::with_capacity(source.len() + 32);
    let mut cursor = 0;
    for (range, text) in replacements {
        out.push_str(&source[cursor..range.start]);
        out.push_str(&text);
        cursor = range.end;
    }
    out.push_str(&source[cursor..]);
    Ok(out)
}

/// Sépare une ligne de sa fin de ligne.
fn split_eol(line: &str) -> (&str, &str) {
    if let Some(body) = line.strip_suffix("\r\n") {
        (body, "\r\n")
    } else if let Some(body) = line.strip_suffix('\n') {
        (body, "\n")
    } else {
        (line, "")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(value: &str) -> MetaUpdate {
        MetaUpdate {
            name: Some(value.into()),
            seq: None,
        }
    }

    fn seq(value: u32) -> MetaUpdate {
        MetaUpdate {
            name: None,
            seq: Some(value),
        }
    }

    const REQUEST: &str =
        "meta {\n  name: Lire\n  type: http\n  seq: 2\n}\n\nget {\n  url: http://a\n}\n";

    #[test]
    fn only_the_name_line_changes() {
        let out = rewrite_meta(REQUEST, &name("Lire un article"), MissingMeta::Refuse)
            .expect("réécriture");
        assert_eq!(
            out,
            "meta {\n  name: Lire un article\n  type: http\n  seq: 2\n}\n\nget {\n  url: http://a\n}\n"
        );
    }

    #[test]
    fn only_the_seq_line_changes() {
        let out = rewrite_meta(REQUEST, &seq(7), MissingMeta::Refuse).expect("réécriture");
        assert_eq!(out, REQUEST.replace("seq: 2", "seq: 7"));
    }

    #[test]
    fn crlf_and_odd_spacing_are_kept() {
        let source = "meta {\r\n    name:Lire\r\n    seq:   2\r\n}\r\n";
        let out = rewrite_meta(
            source,
            &MetaUpdate {
                name: Some("B".into()),
                seq: Some(3),
            },
            MissingMeta::Refuse,
        )
        .expect("réécriture");
        assert_eq!(out, "meta {\r\n    name: B\r\n    seq: 3\r\n}\r\n");
    }

    #[test]
    fn missing_seq_is_inserted_after_type() {
        let source = "meta {\n  name: A\n  type: http\n}\n\nget {\n  url: x\n}\n";
        let out = rewrite_meta(source, &seq(4), MissingMeta::Refuse).expect("réécriture");
        assert_eq!(
            out,
            "meta {\n  name: A\n  type: http\n  seq: 4\n}\n\nget {\n  url: x\n}\n"
        );
    }

    #[test]
    fn missing_name_is_inserted_first() {
        let source = "meta {\n  seq: 1\n}\n";
        let out = rewrite_meta(source, &name("Posts"), MissingMeta::Refuse).expect("réécriture");
        assert_eq!(out, "meta {\n  name: Posts\n  seq: 1\n}\n");
    }

    #[test]
    fn folder_without_meta_gets_one_at_the_top() {
        let source = "auth {\n  mode: inherit\n}\n";
        let out = rewrite_meta(source, &name("Posts"), MissingMeta::CreateAtTop).expect("création");
        assert_eq!(
            out,
            "meta {\n  name: Posts\n}\n\nauth {\n  mode: inherit\n}\n"
        );
        assert_eq!(
            rewrite_meta(source, &name("Posts"), MissingMeta::Refuse),
            Err(EditError::MissingField { field: "meta" })
        );
    }

    #[test]
    fn invalid_names_are_refused() {
        for bad in ["", "a\nb", " a", "a\r"] {
            assert_eq!(
                rewrite_meta(REQUEST, &name(bad), MissingMeta::Refuse),
                Err(EditError::InvalidMetaName),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn rewritten_file_reparses_with_the_new_values() {
        let out = rewrite_meta(
            REQUEST,
            &MetaUpdate {
                name: Some("Nouveau: nom".into()),
                seq: Some(12),
            },
            MissingMeta::Refuse,
        )
        .expect("réécriture");
        let file = BruFile::parse(out).expect("relecture");
        assert_eq!(file.entry("meta", "name"), Some("Nouveau: nom"));
        assert_eq!(file.entry("meta", "seq"), Some("12"));
    }
}
