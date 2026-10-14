//! Diagnostics agrégés de la collection.
//!
//! Parcourt l'arbre à la demande pour extraire les nœuds en erreur
//! (fichiers malformés et dossiers aux métadonnées invalides), puis ajoute
//! les avis non bloquants du chargement (champs YAML ignorés).

use std::path::Path;

use crate::collection::opencollection::FOLDER_FILE;
use crate::collection::{Collection, TreeNode};

/// Une entrée du panneau de diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticEntry<'a> {
    pub path: &'a Path,
    pub reason: String,
    /// Adresse du nœud concerné dans l'arbre ; vide si le fichier n'y
    /// figure pas (collection, environnement).
    pub address: Vec<usize>,
    /// Vrai pour un avis non bloquant, faux pour une erreur.
    pub notice: bool,
}

/// Parcourt l'arbre et collecte tous les nœuds en erreur, dans l'ordre
/// de l'arbre (même ordre que l'affichage, donc déterministe).
pub fn diagnostics(collection: &Collection) -> Vec<DiagnosticEntry<'_>> {
    let mut entries = Vec::new();
    walk(&collection.tree, &mut Vec::new(), &mut entries);
    for notice in &collection.notices {
        // `folder.yml` désigne son dossier.
        let target = match notice.path.parent() {
            Some(parent) if notice.path.file_name().is_some_and(|n| n == FOLDER_FILE) => parent,
            _ => notice.path.as_path(),
        };
        entries.push(DiagnosticEntry {
            path: notice.path.as_path(),
            reason: notice.message.clone(),
            address: address_of(&collection.tree, target, &mut Vec::new()).unwrap_or_default(),
            notice: true,
        });
    }
    entries
}

/// Adresse du nœud de chemin `path`, s'il est dans l'arbre.
fn address_of(nodes: &[TreeNode], path: &Path, prefix: &mut Vec<usize>) -> Option<Vec<usize>> {
    for (index, node) in nodes.iter().enumerate() {
        prefix.push(index);
        if node.path() == path {
            return Some(prefix.clone());
        }
        if let TreeNode::Folder(folder) = node
            && path.starts_with(&folder.path)
            && let Some(found) = address_of(&folder.children, path, prefix)
        {
            return Some(found);
        }
        prefix.pop();
    }
    None
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
                        notice: false,
                    });
                }
                walk(&folder.children, prefix, out);
            }
            TreeNode::Error(error_node) => {
                out.push(DiagnosticEntry {
                    path: error_node.path.as_path(),
                    reason: error_node.error.to_string(),
                    address: prefix.clone(),
                    notice: false,
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
            format: Default::default(),
            notices: Vec::new(),
            tree: vec![],
            environments: vec![],
            presets: None,
        };
        let entries = diagnostics(&collection);
        assert!(entries.is_empty());
    }

    #[test]
    fn notices_follow_errors_and_point_to_their_node() {
        use crate::collection::{LoadNotice, OpenCollectionLoader};
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/collections/oc-probe");
        let mut collection = OpenCollectionLoader.load(&root).expect("chargement");
        assert!(diagnostics(&collection).is_empty());
        collection.notices = vec![
            LoadNotice {
                path: "data/nested/folder.yml".into(),
                message: "clé `x` inconnue, ignorée".into(),
            },
            LoadNotice {
                path: "pages/index.yml".into(),
                message: "clé `y` inconnue, ignorée".into(),
            },
            LoadNotice {
                path: "environments/local.yml".into(),
                message: "secrets externes".into(),
            },
        ];
        let entries = diagnostics(&collection);
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().all(|entry| entry.notice));
        let nested = crate::app::model::tree_node_at(&collection.tree, &entries[0].address)
            .expect("dossier");
        assert_eq!(nested.path(), Path::new("data/nested"));
        let index = crate::app::model::tree_node_at(&collection.tree, &entries[1].address)
            .expect("requête");
        assert_eq!(index.path(), Path::new("pages/index.yml"));
        assert!(entries[2].address.is_empty());
    }
}
