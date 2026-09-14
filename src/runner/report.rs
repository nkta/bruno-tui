//! Modèle typé du rapport produit par `bru run --reporter-json`.
//!
//! La structure reflète ce qu'émet réellement `bru` 2.13.2 (voir
//! `tests/fixtures/reports/`). Les champs inconnus sont ignorés pour
//! tolérer les évolutions du reporter ; les champs dont la forme est libre
//! (corps, en-têtes, valeurs `actual`/`expected`) restent en
//! `serde_json::Value`.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Rapport complet : une entrée par itération (une seule sans données CSV/JSON).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct Report(pub Vec<Iteration>);

impl Report {
    /// Itérations du rapport, dans l'ordre émis par `bru`.
    pub fn iterations(&self) -> &[Iteration] {
        &self.0
    }

    /// Résultats en échec, toutes itérations confondues.
    pub fn failures(&self) -> impl Iterator<Item = &RequestResult> {
        self.0
            .iter()
            .flat_map(|iteration| iteration.results.iter())
            .filter(|result| result.is_failure())
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub iteration_index: u32,
    pub results: Vec<RequestResult>,
    pub summary: Summary,
}

/// Compteurs agrégés calculés par `bru` pour une itération.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub total_requests: u64,
    pub passed_requests: u64,
    pub failed_requests: u64,
    pub error_requests: u64,
    pub skipped_requests: u64,
    pub total_assertions: u64,
    pub passed_assertions: u64,
    pub failed_assertions: u64,
    pub total_tests: u64,
    pub passed_tests: u64,
    pub failed_tests: u64,
    pub total_pre_request_tests: u64,
    pub passed_pre_request_tests: u64,
    pub failed_pre_request_tests: u64,
    pub total_post_response_tests: u64,
    pub passed_post_response_tests: u64,
    pub failed_post_response_tests: u64,
}

/// Résultat de l'exécution d'une requête.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestResult {
    /// Nom déclaré dans le bloc `meta`.
    pub name: String,
    /// Chemin relatif à la racine, sans extension `.bru`.
    pub path: String,
    pub test: RequestFile,
    pub request: RequestInfo,
    pub response: ResponseInfo,
    /// Message d'erreur quand la requête n'a pas pu aboutir.
    #[serde(default)]
    pub error: Option<String>,
    /// Statut global rapporté par `bru` : ne reflète pas les assertions ni
    /// les tests en échec, utiliser [`RequestResult::is_failure`].
    pub status: ResultStatus,
    #[serde(default)]
    pub skipped: bool,
    pub assertion_results: Vec<AssertionResult>,
    pub test_results: Vec<TestResult>,
    pub pre_request_test_results: Vec<TestResult>,
    pub post_response_test_results: Vec<TestResult>,
    #[serde(default)]
    pub should_stop_runner_execution: bool,
    /// Durée d'exécution en secondes.
    pub run_duration: f64,
    pub iteration_index: u32,
}

impl RequestResult {
    /// Verdict d'échec : requête en erreur, ou au moins une assertion ou un
    /// test (principal, pré-requête, post-réponse) en échec.
    pub fn is_failure(&self) -> bool {
        let request_in_error = self.status == ResultStatus::Error || self.error.is_some();
        let assertion_failed = self
            .assertion_results
            .iter()
            .any(|assertion| assertion.status.is_failure());
        let test_failed = self
            .test_results
            .iter()
            .chain(&self.pre_request_test_results)
            .chain(&self.post_response_test_results)
            .any(|test| test.status.is_failure());
        request_in_error || assertion_failed || test_failed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RequestFile {
    /// Fichier `.bru` relatif à la racine de la collection.
    pub filename: String,
}

/// Requête telle qu'envoyée par `bru`, variables résolues.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RequestInfo {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseInfo {
    pub status: ResponseStatus,
    #[serde(default)]
    pub status_text: Option<String>,
    /// `None` quand aucune réponse n'a été reçue ou que la requête est ignorée.
    #[serde(default)]
    pub headers: Option<BTreeMap<String, Value>>,
    /// Corps tel quel : texte, JSON structuré ou `null`.
    #[serde(default)]
    pub data: Value,
    #[serde(default)]
    pub url: Option<String>,
    /// Temps de réponse en millisecondes.
    pub response_time: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssertionResult {
    pub uid: String,
    pub lhs_expr: String,
    pub rhs_expr: String,
    pub rhs_operand: String,
    pub operator: String,
    pub status: ResultStatus,
    #[serde(default)]
    pub error: Option<String>,
}

/// Résultat d'un `test(...)` (bloc `tests`, script pré-requête ou post-réponse).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TestResult {
    pub uid: String,
    pub description: String,
    pub status: ResultStatus,
    #[serde(default)]
    pub error: Option<String>,
    /// `None` si absent, `Some(Value::Null)` si `bru` a émis `null`.
    #[serde(default, deserialize_with = "present_value")]
    pub actual: Option<Value>,
    #[serde(default, deserialize_with = "present_value")]
    pub expected: Option<Value>,
}

/// Statut de réponse : code HTTP, ou mot-clé quand aucune réponse n'existe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseStatus {
    Http(u16),
    /// Aucune réponse reçue (connexion refusée, timeout, etc.).
    Error,
    /// Requête sautée par un script.
    Skipped,
    /// Valeur non reconnue, conservée telle quelle.
    Other(String),
}

impl<'de> Deserialize<'de> for ResponseStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Code(u16),
            Keyword(String),
        }
        Ok(match Raw::deserialize(deserializer)? {
            Raw::Code(code) => Self::Http(code),
            Raw::Keyword(keyword) => match keyword.as_str() {
                "error" => Self::Error,
                "skipped" => Self::Skipped,
                _ => Self::Other(keyword),
            },
        })
    }
}

/// Statut d'un résultat, d'une assertion ou d'un test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultStatus {
    Pass,
    Fail,
    Error,
    Skipped,
    /// Valeur non reconnue, conservée telle quelle.
    Other(String),
}

impl ResultStatus {
    fn is_failure(&self) -> bool {
        matches!(self, Self::Fail | Self::Error)
    }
}

impl<'de> Deserialize<'de> for ResultStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(match raw.as_str() {
            "pass" => Self::Pass,
            "fail" => Self::Fail,
            "error" => Self::Error,
            "skipped" => Self::Skipped,
            _ => Self::Other(raw),
        })
    }
}

/// Distingue un champ présent à `null` d'un champ absent.
fn present_value<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
