//! Renumérotation des `seq` (`docs/edition-phase3.md` § 4.5 et § 5) :
//! comparaison avec la table produite par le vrai code de Bruno desktop
//! 4.2.1 (`scripts/bruno-sort-items.js`, `sortItems`,
//! `getItemsToResequence`, `localeCompare`).

use std::cmp::Ordering;
use std::fs;
use std::path::Path;

use bruno_tui::writer::sequence::{SeqItem, items_to_resequence, locale_compare, sort_items};
use serde_json::Value;

fn table() -> Value {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sequence/bruno-sort-items.json");
    serde_json::from_str(&fs::read_to_string(path).expect("table")).expect("JSON")
}

#[test]
fn sort_items_and_resequence_match_bruno() {
    let table = table();
    let cases = table["sortCases"].as_array().expect("cas");
    assert!(cases.len() >= 10);
    for case in cases {
        let items: Vec<SeqItem> = case["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|item| SeqItem {
                name: item["name"].as_str().expect("name").to_owned(),
                // `-1` n'est pas représentable : invalide pour Bruno comme
                // un `seq` absent.
                seq: item["seq"].as_i64().and_then(|seq| u32::try_from(seq).ok()),
            })
            .collect();
        let order: Vec<usize> = case["order"]
            .as_array()
            .expect("order")
            .iter()
            .map(|index| usize::try_from(index.as_u64().expect("indice")).expect("indice"))
            .collect();
        let changes: Vec<(usize, u32)> = case["changes"]
            .as_array()
            .expect("changes")
            .iter()
            .map(|pair| {
                (
                    usize::try_from(pair[0].as_u64().expect("indice")).expect("indice"),
                    u32::try_from(pair[1].as_u64().expect("seq")).expect("seq"),
                )
            })
            .collect();
        assert_eq!(sort_items(&items), Ok(order), "{items:?}");
        assert_eq!(items_to_resequence(&items), Ok(changes), "{items:?}");
    }
}

#[test]
fn locale_compare_matches_node_on_the_reproduced_case() {
    let table = table();
    let mut compared = 0;
    for pair in table["locale"].as_array().expect("locale") {
        let (a, b) = (pair[0].as_str().expect("a"), pair[1].as_str().expect("b"));
        let expected = match pair[2].as_i64().expect("signe") {
            -1 => Ordering::Less,
            0 => Ordering::Equal,
            _ => Ordering::Greater,
        };
        match locale_compare(a, b) {
            Some(ordering) => {
                assert_eq!(ordering, expected, "{a:?} {b:?}");
                compared += 1;
            }
            // Hors du cas reproduit : seuls les noms non ASCII.
            None => assert!(!a.is_ascii() || !b.is_ascii(), "{a:?} {b:?}"),
        }
    }
    assert!(compared > 1000);
}
