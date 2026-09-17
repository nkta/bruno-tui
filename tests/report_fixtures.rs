//! Désérialisation du rapport `bru` sur les fixtures réelles de
//! `tests/fixtures/reports/` (voir le README associé).

use std::collections::BTreeMap;

use bruno_tui::runner::report::{
    AssertionResult, Report, RequestFile, RequestResult, ResponseStatus, ResultStatus, Summary,
    TestResult,
};
use serde_json::{Value, json};

const MIXED: &str = include_str!("fixtures/reports/mixed.json");

fn mixed() -> Report {
    serde_json::from_str(MIXED).expect("mixed.json doit se désérialiser")
}

fn result<'a>(report: &'a Report, name: &str) -> &'a RequestResult {
    report.iterations()[0]
        .results
        .iter()
        .find(|result| result.name == name)
        .unwrap_or_else(|| panic!("résultat {name} absent de la fixture"))
}

fn python_headers(content_type: &str, length: &str) -> BTreeMap<String, Value> {
    [
        ("server", "SimpleHTTP/0.6 Python/3.10.12"),
        ("date", "Mon, 14 Sep 2026 14:23:53 GMT"),
        ("content-type", content_type),
        ("content-length", length),
        ("last-modified", "Mon, 14 Sep 2026 14:23:13 GMT"),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), json!(value)))
    .collect()
}

#[test]
fn iteration_and_summary() {
    let report = mixed();
    assert_eq!(report.iterations().len(), 1);
    let iteration = &report.iterations()[0];
    assert_eq!(iteration.iteration_index, 0);
    let names: Vec<&str> = iteration.results.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["down", "ok", "json", "green", "skip"]);
    assert_eq!(
        iteration.summary,
        Summary {
            total_requests: 5,
            passed_requests: 1,
            failed_requests: 2,
            error_requests: 1,
            skipped_requests: 1,
            total_assertions: 3,
            passed_assertions: 2,
            failed_assertions: 1,
            total_tests: 3,
            passed_tests: 2,
            failed_tests: 1,
            total_pre_request_tests: 1,
            passed_pre_request_tests: 1,
            failed_pre_request_tests: 0,
            total_post_response_tests: 1,
            passed_post_response_tests: 0,
            failed_post_response_tests: 1,
        }
    );
}

#[test]
fn connection_error_has_no_http_response() {
    let report = mixed();
    let down = result(&report, "down");
    assert_eq!(down.path, "folder/down");
    assert_eq!(
        down.test,
        RequestFile {
            filename: "folder/down.bru".into()
        }
    );
    assert_eq!(down.request.method.as_deref(), Some("GET"));
    assert_eq!(
        down.request.url.as_deref(),
        Some("http://127.0.0.1:18799/nope")
    );
    assert_eq!(down.request.headers, Some(BTreeMap::new()));
    assert_eq!(down.response.status, ResponseStatus::Error);
    assert_eq!(down.response.status_text, None);
    assert_eq!(down.response.headers, None);
    assert_eq!(down.response.data, Value::Null);
    assert_eq!(down.response.url, None);
    assert_eq!(down.response.response_time, 0);
    assert_eq!(
        down.error.as_deref(),
        Some("connect ECONNREFUSED 127.0.0.1:18799")
    );
    assert_eq!(down.status, ResultStatus::Error);
    assert!(!down.skipped);
    assert!(down.assertion_results.is_empty());
    assert!(down.test_results.is_empty());
    assert!(down.pre_request_test_results.is_empty());
    assert!(down.post_response_test_results.is_empty());
    assert!(!down.should_stop_runner_execution);
    assert_eq!(down.run_duration, 0.068983992);
    assert_eq!(down.iteration_index, 0);
}

#[test]
fn http_response_with_text_body_assertions_and_tests() {
    let report = mixed();
    let ok = result(&report, "ok");
    assert_eq!(ok.path, "ok");
    assert_eq!(ok.test.filename, "ok.bru");
    assert_eq!(
        ok.request.url.as_deref(),
        Some("http://127.0.0.1:18765/index.html")
    );
    assert_eq!(ok.response.status, ResponseStatus::Http(200));
    assert_eq!(ok.response.status_text.as_deref(), Some("OK"));
    assert_eq!(ok.response.headers, Some(python_headers("text/html", "32")));
    assert_eq!(ok.response.data, json!("<html><body>probe</body></html>\n"));
    assert_eq!(
        ok.response.url.as_deref(),
        Some("http://127.0.0.1/index.html")
    );
    assert_eq!(ok.response.response_time, 14);
    assert_eq!(ok.error, None);
    assert_eq!(ok.status, ResultStatus::Pass);
    assert_eq!(ok.run_duration, 0.049533291);

    assert_eq!(
        ok.assertion_results,
        [
            AssertionResult {
                uid: "J5SvrxThWDLhAEfi4SaRA".into(),
                lhs_expr: "res.status".into(),
                rhs_expr: "eq 200".into(),
                rhs_operand: "200".into(),
                operator: "eq".into(),
                status: ResultStatus::Pass,
                error: None,
            },
            AssertionResult {
                uid: "rBsfBKZCg7VlvHNCdkP1z".into(),
                lhs_expr: "res.status".into(),
                rhs_expr: "eq 404".into(),
                rhs_operand: "404".into(),
                operator: "eq".into(),
                status: ResultStatus::Fail,
                error: Some("expected 200 to equal 404".into()),
            },
        ]
    );
    assert_eq!(
        ok.test_results,
        [
            TestResult {
                uid: "N-5r7kenQ0iSHowbE3XBR".into(),
                description: "fails".into(),
                status: ResultStatus::Fail,
                error: Some("expected 200 to equal 500".into()),
                actual: Some(json!(200)),
                expected: Some(json!(500)),
            },
            TestResult {
                uid: "o96Oit9ERrO_dx9Zcthf1".into(),
                description: "passes".into(),
                status: ResultStatus::Pass,
                error: None,
                actual: None,
                expected: None,
            },
        ]
    );
}

#[test]
fn json_body_and_pre_post_tests() {
    let report = mixed();
    let json_result = result(&report, "json");
    assert_eq!(json_result.response.data, json!({"a": [1, 2], "b": null}));
    assert_eq!(
        json_result.response.headers,
        Some(python_headers("application/json", "21"))
    );
    assert_eq!(json_result.response.response_time, 9);
    assert_eq!(json_result.status, ResultStatus::Pass);
    assert!(json_result.assertion_results.is_empty());
    assert!(json_result.test_results.is_empty());
    assert_eq!(
        json_result.pre_request_test_results,
        [TestResult {
            uid: "u6dp_0vWddxSzmkrpC6kp".into(),
            description: "pre ok".into(),
            status: ResultStatus::Pass,
            error: None,
            actual: None,
            expected: None,
        }]
    );
    assert_eq!(
        json_result.post_response_test_results,
        [TestResult {
            uid: "0fH9pgXwFmDpxM8a2QdZR".into(),
            description: "post ko".into(),
            status: ResultStatus::Fail,
            error: Some("expected 2 to equal 3".into()),
            actual: Some(json!(2)),
            expected: Some(json!(3)),
        }]
    );
}

#[test]
fn skipped_request() {
    let report = mixed();
    let skip = result(&report, "skip");
    assert_eq!(skip.status, ResultStatus::Skipped);
    assert!(skip.skipped);
    assert_eq!(skip.response.status, ResponseStatus::Skipped);
    assert_eq!(
        skip.response.status_text.as_deref(),
        Some("request skipped via pre-request script")
    );
    assert_eq!(skip.response.headers, None);
    assert_eq!(skip.response.url, None);
    assert_eq!(skip.response.data, Value::Null);
    assert_eq!(skip.error, None);
}

#[test]
fn unknown_status_values_are_kept() {
    let mut raw: Value = serde_json::from_str(MIXED).expect("fixture JSON");
    raw[0]["results"][1]["status"] = json!("flaky");
    raw[0]["results"][1]["response"]["status"] = json!("timeout");
    let report: Report = serde_json::from_value(raw).expect("valeurs inconnues tolérées");
    let ok = result(&report, "ok");
    assert_eq!(ok.status, ResultStatus::Other("flaky".into()));
    assert_eq!(ok.response.status, ResponseStatus::Other("timeout".into()));
}

#[test]
fn unknown_fields_are_ignored_at_every_level() {
    let mut raw: Value = serde_json::from_str(MIXED).expect("fixture JSON");
    let extra = json!({"nested": [1, 2]});
    let iteration = &mut raw[0];
    iteration["futureField"] = extra.clone();
    iteration["summary"]["futureField"] = extra.clone();
    let ok = &mut iteration["results"][1];
    ok["futureField"] = extra.clone();
    ok["test"]["futureField"] = extra.clone();
    ok["request"]["futureField"] = extra.clone();
    ok["response"]["futureField"] = extra.clone();
    ok["assertionResults"][0]["futureField"] = extra.clone();
    ok["testResults"][0]["futureField"] = extra.clone();

    let enriched: Report = serde_json::from_value(raw).expect("champs inconnus tolérés");
    assert_eq!(enriched, mixed());
}

#[test]
fn failure_verdict() {
    let report = mixed();
    // `pass` avec assertion et test en échec
    assert!(result(&report, "ok").is_failure());
    // `pass` avec seul un test post-réponse en échec
    assert!(result(&report, "json").is_failure());
    // requête en erreur
    assert!(result(&report, "down").is_failure());
    // ignorée
    assert!(!result(&report, "skip").is_failure());
    // tout vert
    assert!(!result(&report, "green").is_failure());

    let failures: Vec<&str> = report.failures().map(|r| r.name.as_str()).collect();
    assert_eq!(failures, ["down", "ok", "json"]);
}

const PRE_REQUEST_ERROR: &str = include_str!("fixtures/reports/pre-request-error.json");

fn pre_request_error() -> Report {
    serde_json::from_str(PRE_REQUEST_ERROR).expect("pre-request-error.json doit se désérialiser")
}

#[test]
fn request_failed_before_sending() {
    let report = pre_request_error();
    assert_eq!(report.iterations().len(), 1);
    let iteration = &report.iterations()[0];
    assert_eq!(iteration.iteration_index, 0);
    assert_eq!(
        iteration.summary,
        Summary {
            total_requests: 1,
            passed_requests: 0,
            failed_requests: 0,
            error_requests: 1,
            skipped_requests: 0,
            total_assertions: 0,
            passed_assertions: 0,
            failed_assertions: 0,
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            total_pre_request_tests: 0,
            passed_pre_request_tests: 0,
            failed_pre_request_tests: 0,
            total_post_response_tests: 0,
            passed_post_response_tests: 0,
            failed_post_response_tests: 0,
        }
    );

    let boom = result(&report, "boom");
    assert_eq!(boom.path, "boom");
    assert_eq!(
        boom.test,
        RequestFile {
            filename: "boom.bru".into()
        }
    );
    // Requête jamais envoyée : `bru` rapporte méthode, URL et en-têtes à `null`.
    assert_eq!(boom.request.method, None);
    assert_eq!(boom.request.url, None);
    assert_eq!(boom.request.headers, None);
    assert_eq!(boom.response.status, ResponseStatus::Error);
    assert_eq!(boom.response.status_text, None);
    assert_eq!(boom.response.headers, None);
    assert_eq!(boom.response.data, Value::Null);
    assert_eq!(boom.response.url, None);
    assert_eq!(boom.response.response_time, 0);
    assert_eq!(boom.error.as_deref(), Some("pre-request failure (fixture)"));
    assert_eq!(boom.status, ResultStatus::Error);
    assert!(!boom.skipped);
    assert!(boom.assertion_results.is_empty());
    assert!(boom.test_results.is_empty());
    assert!(boom.pre_request_test_results.is_empty());
    assert!(boom.post_response_test_results.is_empty());
    assert!(!boom.should_stop_runner_execution);
    assert_eq!(boom.run_duration, 0.043209747);
    assert_eq!(boom.iteration_index, 0);
    assert!(boom.is_failure());
}
