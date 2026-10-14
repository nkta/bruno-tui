//! Numérotation des `seq` comme Bruno desktop 4.2.1 (renderer, relevé dans
//! `app.asar`) : `sortItems` (module 88008, export `zz`), `ef` (`seq` =
//! rang + 1), `getItemsToResequence` (`ky`) après une suppression, et
//! `getReorderedItemsInSourceDirectory` (`gS`) pour un déplacement d'un
//! cran. Table de référence produite par le vrai code :
//! `tests/fixtures/sequence/bruno-sort-items.json`.
//!
//! `sortItems` ne regarde les noms (`localeCompare`) que s'il y a des `seq`
//! absents, invalides ou en double. Le TUI reproduit `localeCompare`
//! (collation ICU racine, identique en `fr`, `en`, `de`) pour les noms
//! faits de lettres et chiffres ASCII, d'espaces, `_`, `-` et `.` ; hors de
//! ce cas, ou si deux noms sont égaux, la renumérotation est refusée plutôt
//! que de donner un autre ordre que Bruno.

use std::cmp::Ordering;

use thiserror::Error;

/// Élément d'un répertoire : nom affiché et `seq` lu (`None` : absent ou
/// illisible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqItem {
    pub name: String,
    pub seq: Option<u32>,
}

/// Ordre de Bruno non reproductible à coup sûr.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("noms non comparables comme Bruno (« {0} »)")]
pub struct Unsortable(pub String);

/// `seq` valide pour Bruno : entier strictement positif.
fn valid(seq: Option<u32>) -> Option<u32> {
    seq.filter(|seq| *seq > 0)
}

/// Poids primaire d'un caractère (ICU racine), `None` hors du cas reproduit.
fn primary(c: char) -> Option<u32> {
    match c {
        ' ' => Some(0),
        '_' => Some(1),
        '-' => Some(2),
        '.' => Some(3),
        '0'..='9' => Some(10 + u32::from(c) - u32::from('0')),
        'a'..='z' => Some(20 + u32::from(c) - u32::from('a')),
        'A'..='Z' => Some(20 + u32::from(c) - u32::from('A')),
        _ => None,
    }
}

/// `a.localeCompare(b)` pour des noms du cas reproduit : ordre primaire
/// (sans casse), puis minuscule avant majuscule à la première différence.
/// `None` si un nom sort de ce cas.
pub fn locale_compare(a: &str, b: &str) -> Option<Ordering> {
    let weights = |text: &str| text.chars().map(primary).collect::<Option<Vec<u32>>>();
    let (wa, wb) = (weights(a)?, weights(b)?);
    let case = |text: &str| {
        text.chars()
            .map(|c| c.is_ascii_uppercase())
            .collect::<Vec<bool>>()
    };
    Some(wa.cmp(&wb).then_with(|| case(a).cmp(&case(b))))
}

/// `sortItems` : indices des éléments dans l'ordre de Bruno.
pub fn sort_items(items: &[SeqItem]) -> Result<Vec<usize>, Unsortable> {
    let mut seen = Vec::new();
    let names_matter = items.iter().any(|item| match valid(item.seq) {
        None => true,
        Some(seq) if seen.contains(&seq) => true,
        Some(seq) => {
            seen.push(seq);
            false
        }
    });
    let mut by_name: Vec<usize> = (0..items.len()).collect();
    if names_matter {
        for item in items {
            if item.name.is_empty() || locale_compare(&item.name, &item.name).is_none() {
                return Err(Unsortable(item.name.clone()));
            }
        }
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                if locale_compare(&a.name, &b.name) == Some(Ordering::Equal) {
                    return Err(Unsortable(a.name.clone()));
                }
            }
        }
        by_name.sort_by(|&a, &b| {
            locale_compare(&items[a].name, &items[b].name).unwrap_or(Ordering::Equal)
        });
    }

    enum Slot {
        One(usize),
        Group(Vec<usize>),
    }
    let mut slots: Vec<Slot> = by_name
        .iter()
        .filter(|&&index| valid(items[index].seq).is_none())
        .map(|&index| Slot::One(index))
        .collect();
    let mut with_seq: Vec<usize> = by_name
        .iter()
        .copied()
        .filter(|&index| valid(items[index].seq).is_some())
        .collect();
    with_seq.sort_by_key(|&index| items[index].seq);
    for index in with_seq {
        let seq = items[index].seq.unwrap_or_default();
        let at = usize::try_from(seq - 1).unwrap_or(usize::MAX);
        let same_seq = |other: usize| items[other].seq == Some(seq);
        // Même `seq` déjà à cette place : regroupés, dans l'ordre des noms.
        match slots.get_mut(at) {
            Some(Slot::Group(group)) if same_seq(group[0]) => group.push(index),
            Some(slot) if matches!(*slot, Slot::One(other) if same_seq(other)) => {
                if let Slot::One(other) = *slot {
                    *slot = Slot::Group(vec![other, index]);
                }
            }
            _ => slots.insert(at.min(slots.len()), Slot::One(index)),
        }
    }
    Ok(slots
        .into_iter()
        .flat_map(|slot| match slot {
            Slot::One(index) => vec![index],
            Slot::Group(group) => group,
        })
        .collect())
}

/// Nouveaux `seq` (rang + 1) de `order`, réduits à ceux qui changent :
/// `(indice, seq)`.
fn changes(items: &[SeqItem], order: &[usize]) -> Vec<(usize, u32)> {
    order
        .iter()
        .enumerate()
        .filter_map(|(rank, &index)| {
            let seq = u32::try_from(rank + 1).unwrap_or(u32::MAX);
            (items[index].seq != Some(seq)).then_some((index, seq))
        })
        .collect()
}

/// `getItemsToResequence` : frères restants après une suppression.
pub fn items_to_resequence(items: &[SeqItem]) -> Result<Vec<(usize, u32)>, Unsortable> {
    let order = sort_items(items)?;
    Ok(changes(items, &order))
}

/// Déplacement d'un cran (`Maj+↑` si `up`) de l'élément `moved`, dans
/// l'ordre de Bruno. `None` s'il est déjà en tête (ou en fin).
pub fn move_one(
    items: &[SeqItem],
    moved: usize,
    up: bool,
) -> Result<Option<Vec<(usize, u32)>>, Unsortable> {
    let mut order = sort_items(items)?;
    let Some(position) = order.iter().position(|&index| index == moved) else {
        return Ok(None);
    };
    let neighbour = if up {
        position.checked_sub(1)
    } else {
        Some(position + 1).filter(|next| *next < order.len())
    };
    let Some(neighbour) = neighbour else {
        return Ok(None);
    };
    order.swap(position, neighbour);
    Ok(Some(changes(items, &order)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(list: &[(&str, Option<u32>)]) -> Vec<SeqItem> {
        list.iter()
            .map(|(name, seq)| SeqItem {
                name: (*name).to_owned(),
                seq: *seq,
            })
            .collect()
    }

    #[test]
    fn distinct_valid_seqs_ignore_names() {
        let list = items(&[("é", Some(3)), ("", Some(1)), ("?", Some(2))]);
        assert_eq!(sort_items(&list), Ok(vec![1, 2, 0]));
        assert_eq!(items_to_resequence(&list), Ok(vec![]));
    }

    #[test]
    fn names_outside_the_reproduced_case_are_refused_when_they_matter() {
        let list = items(&[("Requête", None), ("a", Some(1))]);
        assert_eq!(sort_items(&list), Err(Unsortable("Requête".into())));
        let list = items(&[("a", None), ("a", None)]);
        assert!(sort_items(&list).is_err());
    }

    #[test]
    fn move_one_swaps_neighbours_in_bruno_order() {
        let list = items(&[("A", Some(1)), ("B", Some(2)), ("C", Some(3))]);
        assert_eq!(move_one(&list, 2, true), Ok(Some(vec![(2, 2), (1, 3)])));
        assert_eq!(move_one(&list, 0, true), Ok(None));
        assert_eq!(move_one(&list, 2, false), Ok(None));
        let list = items(&[("b", None), ("a", None), ("c", Some(2))]);
        // Ordre de Bruno : a, c, b.
        assert_eq!(
            move_one(&list, 0, true),
            Ok(Some(vec![(1, 1), (0, 2), (2, 3)]))
        );
    }
}
