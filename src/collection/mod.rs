//! Chargement en lecture seule d'une collection Bruno.
//!
//! Le module découvre la racine d'une collection (`bruno.json`), parse
//! chaque fichier `.bru` en un AST fidèle à l'octet, et construit un arbre
//! ordonné par `seq` doublé d'une vue typée pour l'affichage. Il ne résout
//! aucune variable, n'applique aucun héritage d'auth et n'écrit jamais sur
//! le disque.

pub mod ast;
pub mod config;
pub mod error;
mod lexer;
pub mod loader;
pub mod tree;
pub mod view;

pub use ast::{Block, BlockBody, BruFile, Entry, Node, NodeKind};
pub use config::BrunoConfig;
pub use error::{LoadError, ParseError};
pub use loader::{BruLoader, CollectionLoader};
pub use tree::{Collection, ErrorNode, FolderNode, RequestNode, TreeNode};
pub use view::{
    AuthMode, BodyContent, BodyKind, BodyView, Environment, FileMeta, KeyValue, RequestView,
};
