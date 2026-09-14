//! Exécution de requêtes et campagnes Bruno par délégation à `bru run`.
//!
//! Le module lance `bru` en processus asynchrone, récupère le rapport JSON
//! par un pipe hérité (jamais sur disque) et le restitue sous forme typée.

pub mod error;
pub mod process;
pub mod report;
pub mod request;

pub use error::{ReportErrorKind, RunError, RunOutcome};
pub use process::{BruRunner, RunEvent, RunHandle, RunId};
pub use report::Report;
pub use request::{RunRequest, SecretString};
