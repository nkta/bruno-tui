//! Diagnostics agrégés de la collection.
//!
//! Parcourt l'arbre à la demande pour extraire les nœuds en erreur
//! (fichiers `.bru` malformés et dossiers aux métadonnées invalides).

use std::path::Path;

use crate::collection::{Collection, TreeNode};

/// Une entrée du panneau de diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEntry<'a> {
    pub path: &'a Path,
    pub reason: String,
    pub address: Vec<usize>,
}

/// Parcourt l'arbre et collecte tous les nœuds en erreur, dans l'ordre
/// de l'arbre (même ordre que l'affichage, donc déterministe).
pub fn diagnostics(collection: &Collection) -> Vec<DiagnosticEntry<'_>> {
    let mut entries = Vec::new();
    walk(&collection.tree, &mut Vec::new(), &mut entries);
    entries
}

fn walk<'a>(nodes: &'a [TreeNode], prefix: &mut Vec<usize>, out: &mut Vec<DiagnosticEntry<'a>>) {
    for (index, node) in nodes.iter().enumerate() {
        prefix.push(index);
        match node {
            TreeNode::Request(_) => {}
            TreeNode::Folder(folder) => {
                if let Some(Err(error)) = &folder.meta {
                    out.push(DiagnosticEntry {
                        path: folder.path.as_path(),
                        reason: error.to_string(),
                        address: prefix.clone(),
                    });
                }
                walk(&folder.children, prefix, out);
            }
            TreeNode::Error(error_node) => {
                out.push(DiagnosticEntry {
                    path: error_node.path.as_path(),
                    reason: error_node.error.to_string(),
                    address: prefix.clone(),
                });
            }
        }
        prefix.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{BruLoader, CollectionLoader};
    use std::path::PathBuf;

    fn fixture_cases() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/parser-cases")
    }

    #[test]
    fn parser_cases_diagnostics_contain_expected_errors_in_tree_order() {
        let collection = BruLoader.load(&fixture_cases()).expect("chargement");
        let entries = diagnostics(&collection);

        assert_eq!(entries.len(), 3);

        // Dans l'ordre de l'arbre : badmeta, broken.bru, no-method.bru
        assert_eq!(entries[0].path, Path::new("badmeta"));
        assert!(
            entries[0].reason.contains("jamais fermé"),
            "Raison inattendue pour badmeta: {}",
            entries[0].reason
        );
        assert_eq!(entries[0].address, vec![4]);

        assert_eq!(entries[1].path, Path::new("broken.bru"));
        assert!(
            entries[1].reason.contains("headers") && entries[1].reason.contains("13"),
            "Raison inattendue pour broken.bru: {}",
            entries[1].reason
        );
        assert_eq!(entries[1].address, vec![5]);

        assert_eq!(entries[2].path, Path::new("no-method.bru"));
        assert!(
            entries[2].reason.contains("aucun bloc de méthode HTTP"),
            "Raison inattendue pour no-method.bru: {}",
            entries[2].reason
        );
        assert_eq!(entries[2].address, vec![8]);
    }

    #[test]
    fn collection_without_errors_gives_empty_diagnostics() {
        // Une collection synthétique sans nœud en erreur
        let collection = Collection {
            root: PathBuf::from("/test"),
            name: "clean".to_string(),
            settings: None,
            tree: vec![],
            environments: vec![],
        };
        let entries = diagnostics(&collection);
        assert!(entries.is_empty());
    }
}
