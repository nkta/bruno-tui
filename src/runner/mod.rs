//! Exécution de requêtes et campagnes Bruno par délégation à `bru run`.
//!
//! Le module lance `bru` en processus asynchrone, récupère le rapport JSON
//! par un pipe hérité (jamais sur disque) et le restitue sous forme typée.

pub mod curl;
pub mod error;
pub mod options;
pub mod process;
pub mod report;
pub mod request;

pub use error::{ReportErrorKind, RunError, RunOutcome};
pub use options::{
    RunnerOptions, RunnerOptionsConfig, RunnerOptionsError, default_reports_dir,
    generate_report_path, parse_delay, parse_existing_file_path, parse_tags,
};
pub use process::{BruRunner, RunEvent, RunHandle, RunId};
pub use report::Report;
pub use request::{RunRequest, SecretString};
