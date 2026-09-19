//! Diffusion d'un ajout, d'une suppression ou d'une modification
//! d'entrée dans un bloc dictionnaire `.bru` (`headers`, `params:query`,
//! `params:path`, `vars`, ...) vers des tranches de source minimales à
//! réécrire. Générique : ne dépend d'aucune notion propre aux requêtes
//! ni aux environnements — partagé par `draft.rs` (requêtes) et
//! `environment.rs` (`add-environment-entry-management`, design D1).
//!
//! Tout est pur : aucune I/O, aucun effet de bord.

use std::ops::Range;

use super::edit::Replacement;
use super::error::{EditError, EntryProblem};
use super::format;
use crate::collection::{BlockBody, BruFile, Entry, NodeKind};

/// Entrée courante d'un bloc dictionnaire ; `origin` est l'indice de
/// l'`Entry` d'origine dans le bloc, `None` pour une entrée ajoutée.
#[derive(Debug, Clone)]
pub(crate) struct DictEntry {
    pub(crate) origin: Option<usize>,
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) disabled: bool,
}

impl DictEntry {
    pub(crate) fn from_entry(index: usize, entry: &Entry) -> Self {
        Self {
            origin: Some(index),
            key: entry.key.clone(),
            value: entry.value.clone(),
            disabled: entry.disabled,
        }
    }
}

/// État courant d'un bloc dictionnaire, comparé à son état d'origine.
pub(crate) struct DictSection {
    pub(crate) entries: Vec<DictEntry>,
    /// Le bloc existe dans le fichier chargé.
    pub(crate) block_present: bool,
    /// Une modification structurelle ou une synchronisation a touché la
    /// section : un bloc resté vide est alors retiré.
    pub(crate) touched: bool,
}

/// Tranche et entrées du premier bloc dictionnaire portant ce nom.
pub(crate) fn block_node<'f>(ast: &'f BruFile, name: &str) -> Option<(Range<usize>, &'f [Entry])> {
    ast.nodes().iter().find_map(|node| match &node.kind {
        NodeKind::Block(block) if block.name == name => match &block.body {
            BlockBody::Dictionary(entries) => Some((node.span.clone(), entries.as_slice())),
            _ => None,
        },
        _ => None,
    })
}

fn format_added(section: &DictSection, eol: &str) -> String {
    section
        .entries
        .iter()
        .filter(|entry| entry.origin.is_none())
        .map(|entry| format::format_entry(&entry.key, &entry.value, entry.disabled, eol))
        .collect()
}

/// Fin de la ligne commençant à `start` (fin de ligne comprise).
pub(crate) fn line_end(raw: &str, start: usize) -> usize {
    raw[start..].find('\n').map_or(raw.len(), |i| start + i + 1)
}

/// Longueur de la ligne vide qui se termine juste avant `start`, zéro s'il
/// n'y en a pas.
pub(crate) fn preceding_blank_line_len(raw: &str, start: usize) -> usize {
    let before = &raw[..start];
    let eol_len = if before.ends_with("\r\n") {
        2
    } else if before.ends_with('\n') {
        1
    } else {
        return 0;
    };
    let rest = &before[..before.len() - eol_len];
    if rest.is_empty() || rest.ends_with('\n') {
        eol_len
    } else {
        0
    }
}

/// Longueur de la ligne vide qui commence à `start`, zéro s'il n'y en a
/// pas — symétrique de `preceding_blank_line_len`.
pub(crate) fn following_blank_line_len(raw: &str, start: usize) -> usize {
    let after = &raw[start..];
    if after.starts_with("\r\n") {
        2
    } else if after.starts_with('\n') {
        1
    } else {
        0
    }
}

/// Diffuse les entrées courantes de `section` (comparées au bloc
/// d'origine `block`, s'il existe) vers les tranches de source à
/// réécrire pour le bloc `block_name`.
///
/// Si le bloc n'existe pas et que `section` porte au moins une entrée,
/// il est créé à `create_anchor`. `separate_from_previous` gouverne
/// l'insertion d'une ligne vide avant le bloc créé (l'appelant la met à
/// `false` quand un bloc précédent a déjà été créé au même point, pour
/// ne jamais doubler la ligne de séparation).
pub(crate) fn dictionary_replacements(
    raw: &str,
    eol: &str,
    block_name: &str,
    block: Option<(Range<usize>, &[Entry])>,
    section: &DictSection,
    create_anchor: usize,
    separate_from_previous: bool,
) -> Vec<Replacement> {
    let mut replacements = Vec::new();
    match block {
        Some((span, entries)) => {
            if section.touched && section.entries.is_empty() {
                let preceding = preceding_blank_line_len(raw, span.start);
                let start = span.start - preceding;
                // Rien avant le bloc supprimé (il commençait le fichier) :
                // une ligne vide qui le séparait de ce qui suit devient
                // orpheline, à retirer aussi.
                let end = if preceding == 0 && start == 0 {
                    span.end + following_blank_line_len(raw, span.end)
                } else {
                    span.end
                };
                replacements.push(Replacement {
                    span: start..end,
                    bytes: String::new(),
                });
                return replacements;
            }
            for (index, original) in entries.iter().enumerate() {
                let current = section.entries.iter().find(|e| e.origin == Some(index));
                match current {
                    None => replacements.push(Replacement {
                        span: original.span.clone(),
                        bytes: String::new(),
                    }),
                    Some(entry)
                        if entry.key != original.key
                            || entry.value != original.value
                            || entry.disabled != original.disabled =>
                    {
                        replacements.push(Replacement {
                            span: original.span.clone(),
                            bytes: format::format_entry(
                                &entry.key,
                                &entry.value,
                                entry.disabled,
                                eol,
                            ),
                        });
                    }
                    Some(_) => {}
                }
            }
            let added = format_added(section, eol);
            if !added.is_empty() {
                let anchor = entries
                    .last()
                    .map_or_else(|| line_end(raw, span.start), |e| e.span.end);
                replacements.push(Replacement {
                    span: anchor..anchor,
                    bytes: added,
                });
            }
        }
        None if !section.entries.is_empty() => {
            let mut bytes = String::new();
            let anything_before = !raw[..create_anchor].is_empty();
            if separate_from_previous && anything_before && !raw[..create_anchor].ends_with('\n') {
                bytes.push_str(eol);
            }
            // Ligne vide avant le bloc créé, séparant du contenu qui le
            // précède — sans objet en tout début de fichier (rien à
            // séparer).
            if anything_before {
                bytes.push_str(eol);
            }
            bytes.push_str(block_name);
            bytes.push_str(" {");
            bytes.push_str(eol);
            bytes.push_str(&format_added(section, eol));
            bytes.push('}');
            bytes.push_str(eol);
            // Ligne vide après le bloc créé si du contenu suit déjà à ce
            // point (insertion avant un bloc existant plutôt qu'en fin de
            // fichier) sans déjà commencer par sa propre ligne vide —
            // sinon la ligne vide d'origine suffit déjà à séparer.
            if !raw[create_anchor..].is_empty() && following_blank_line_len(raw, create_anchor) == 0
            {
                bytes.push_str(eol);
            }
            replacements.push(Replacement {
                span: create_anchor..create_anchor,
                bytes,
            });
        }
        None => {}
    }
    replacements
}

/// Règles générales de validité d'une clé, communes à toute section
/// dictionnaire : vide, espace/tabulation/saut de ligne, `:`, ou `~`/`"`
/// en tête. Un appelant propre à une section (ex. paramètres de requête)
/// ajoute ses propres règles par-dessus.
pub(crate) fn check_key_general(
    key: &str,
    block: &'static str,
    index: Option<usize>,
) -> Result<(), EditError> {
    let problem = if key.is_empty() {
        Some(EntryProblem::EmptyKey)
    } else if key.contains([' ', '\t', '\n', '\r']) {
        Some(EntryProblem::KeyWhitespace)
    } else if key.contains(':') {
        Some(EntryProblem::KeyColon)
    } else if key.starts_with(['~', '"']) {
        Some(EntryProblem::KeyLeadingMarker)
    } else {
        None
    };
    match problem {
        Some(problem) => Err(EditError::InvalidEntry {
            block,
            index,
            problem,
        }),
        None => Ok(()),
    }
}
