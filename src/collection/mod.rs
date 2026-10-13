//! Chargement en lecture seule d'une collection Bruno.
//!
//! Le module découvre la racine d'une collection et en détecte le format
//! comme Bruno 4 : `opencollection.yml` (OpenCollection YAML, lecture
//! seule), sinon `bruno.json` (fichiers `.bru`, parsés en un AST fidèle à
//! l'octet). Il construit un arbre ordonné par `seq` doublé d'une vue
//! typée pour l'affichage, identique pour les deux formats. Il ne résout
//! aucune variable, n'applique aucun héritage d'auth et n'écrit jamais sur
//! le disque.

pub mod ast;
pub mod auth;
pub mod config;
pub mod error;
mod lexer;
pub mod loader;
pub mod opencollection;
pub mod tree;
pub mod view;
pub mod workspace;

pub use ast::{Block, BlockBody, BruFile, Declaration, Entry, Node, NodeKind};
pub use auth::{
    MASKED_SECRET, is_sensitive_auth_key, is_variable_reference, mask_auth_params, mask_auth_value,
};
pub use config::BrunoConfig;
pub use error::{LoadError, ParseError};
pub use loader::{AutoLoader, BruLoader, CollectionLoader, OpenCollectionLoader};
pub use tree::{
    Collection, CollectionFormat, ErrorNode, FolderNode, LoadNotice, RequestNode, TreeNode,
};
pub use view::{
    AuthMode, BodyContent, BodyKind, BodyView, Environment, EnvironmentExtends, ExtraTabs,
    FileMeta, KeyValue, RequestView,
};
