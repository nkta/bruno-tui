//! Résumé du statut de la réponse, affiché sur la bordure haute du
//! panneau Réponse (capacité `status-panel`).
//!
//! Fonction pure dérivée du résultat conservé et de l'exécution en cours :
//! aucune information qui n'a pas été rapportée par `bru` n'est inventée
//! (pas de libellé déduit du code, pas de taille estimée depuis le corps).

use std::path::Path;

use ratatui::text::{Line, Span};
use serde_json::Value;

use super::theme;
use super::tree::{FAILURE_MARK, SUCCESS_MARK};
use crate::app::model::{ActiveRun, Model};
use crate::collection::TreeNode;
use crate::runner::report::{RequestResult, ResponseStatus, ResultStatus};

/// Classe d'un statut de réponse, qui détermine la couleur du badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusClass {
    /// 2xx.
    Success,
    /// 3xx.
    Redirect,
    /// 4xx.
    ClientError,
    /// 5xx, ou aucune réponse reçue.
    Failure,
    /// Requête ignorée, code hors de 200–599, valeur non reconnue.
    Neutral,
}

/// Classe d'un statut de réponse (`visual-theme`).
pub fn classify(status: &ResponseStatus) -> StatusClass {
    match status {
        ResponseStatus::Http(200..=299) => StatusClass::Success,
        ResponseStatus::Http(300..=399) => StatusClass::Redirect,
        ResponseStatus::Http(400..=499) => StatusClass::ClientError,
        ResponseStatus::Http(500..=599) | ResponseStatus::Error => StatusClass::Failure,
        ResponseStatus::Http(_) | ResponseStatus::Skipped | ResponseStatus::Other(_) => {
            StatusClass::Neutral
        }
    }
}

/// Texte du badge : code HTTP, ou mot-clé quand il n'y en a pas.
fn badge_text(status: &ResponseStatus) -> String {
    match status {
        ResponseStatus::Http(code) => code.to_string(),
        ResponseStatus::Error => "aucune réponse".to_owned(),
        ResponseStatus::Skipped => "ignorée".to_owned(),
        ResponseStatus::Other(other) => other.clone(),
    }
}

/// Taille du corps d'après l'en-tête `content-length`, seule source
/// fiable : `bru` ne rapporte pas de taille (`design.md`, D5).
pub fn content_length(result: &RequestResult) -> Option<u64> {
    let headers = result.response.headers.as_ref()?;
    let (_, value) = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))?;
    match value {
        Value::String(text) => text.trim().parse().ok(),
        Value::Number(number) => number.as_u64(),
        _ => None,
    }
}

/// Taille lisible, base 1024, une décimale et virgule décimale.
pub fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = KIB * KIB;
    let scaled = |unit: u64, suffix: &str| {
        let value = format!("{:.1}", bytes as f64 / unit as f64);
        format!("{} {suffix}", value.replace('.', ","))
    };
    if bytes < KIB {
        format!("{bytes} o")
    } else if bytes < MIB {
        scaled(KIB, "Ko")
    } else {
        scaled(MIB, "Mo")
    }
}

/// Vérifications réussies et vérifications non ignorées, toutes
/// catégories confondues.
pub fn check_counts(result: &RequestResult) -> (usize, usize) {
    let statuses = result
        .assertion_results
        .iter()
        .map(|assertion| &assertion.status)
        .chain(
            result
                .test_results
                .iter()
                .chain(&result.pre_request_test_results)
                .chain(&result.post_response_test_results)
                .map(|test| &test.status),
        )
        .filter(|status| **status != ResultStatus::Skipped);
    statuses.fold((0, 0), |(passed, total), status| {
        (
            passed + usize::from(*status == ResultStatus::Pass),
            total + 1,
        )
    })
}

/// Vrai si l'exécution en cours va remplacer le résultat de `path` :
/// cible exacte, ou exécution récursive d'un dossier ancêtre.
pub fn run_concerns(active: &ActiveRun, path: &Path) -> bool {
    active.target == path || (active.recursive && path.starts_with(&active.target))
}

/// Résumé sur une ligne, à droite des onglets de la réponse comme dans
/// Bruno bureau : statut coloré, temps, taille (si connue) et verdict ;
/// indicateur « en cours » pendant une exécution ; vide sans résultat.
pub fn status_summary(model: &Model) -> Line<'static> {
    let request_path = match model.selected_node() {
        Some(TreeNode::Request(request)) => Some(request.path.as_path()),
        _ => None,
    };
    let running = request_path.is_some_and(|path| {
        model
            .run
            .active
            .as_ref()
            .is_some_and(|active| run_concerns(active, path))
    });
    if running {
        return Line::from(Span::styled(
            " en cours ",
            theme::RUNNING.fg.map_or(theme::RUNNING, theme::badge_on),
        ));
    }
    match request_path.and_then(|path| model.run.outcomes.get(path)) {
        Some(outcome) => result_summary(&outcome.result),
        None => Line::default(),
    }
}

/// Résumé d'un résultat : statut coloré et son libellé, temps, taille
/// (seulement d'après `content-length`), puis verdict et décompte des
/// vérifications. Rien d'autre du résultat (en-têtes, corps, erreur).
fn result_summary(result: &RequestResult) -> Line<'static> {
    let response = &result.response;
    let mut status_value = badge_text(&response.status);
    if let Some(label) = response.status_text.as_deref().filter(|t| !t.is_empty()) {
        status_value.push(' ');
        status_value.push_str(label);
    }
    let mut spans = vec![
        Span::styled(
            status_value,
            class_style(classify(&response.status)).add_modifier(ratatui::style::Modifier::BOLD),
        ),
        Span::raw(format!("  {} ms", response.response_time)),
    ];
    if let Some(bytes) = content_length(result) {
        spans.push(Span::raw(format!("  {}", format_size(bytes))));
    }
    let (passed, total) = check_counts(result);
    let (mark, style) = if result.is_failure() {
        (FAILURE_MARK, theme::FAILURE)
    } else {
        (SUCCESS_MARK, theme::SUCCESS)
    };
    let verdict = if total == 0 {
        format!("  {mark}")
    } else {
        format!("  {mark} {passed}/{total}")
    };
    spans.push(Span::styled(verdict, style));
    Line::from(spans)
}

/// Couleur de texte d'une classe de statut.
fn class_style(class: StatusClass) -> ratatui::style::Style {
    match class {
        StatusClass::Success => theme::SUCCESS,
        StatusClass::Redirect => theme::REDIRECT,
        StatusClass::ClientError => theme::CLIENT_ERROR,
        StatusClass::Failure => theme::FAILURE,
        StatusClass::Neutral => theme::LABEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{loaded_model, runner_probe_model, select};
    use crate::runner::RunId;
    use crate::runner::report::{AssertionResult, TestResult};

    fn plain(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect::<Vec<String>>()
            .join("\n")
    }

    fn outcome_of<'a>(model: &'a Model, path: &str) -> &'a RequestResult {
        &model.run.outcomes[Path::new(path)].result
    }

    fn active(target: &str, recursive: bool) -> ActiveRun {
        ActiveRun {
            id: RunId(1),
            target: target.into(),
            recursive,
            handle: None,
        }
    }

    #[test]
    fn classify_maps_every_class() {
        for (status, class) in [
            (ResponseStatus::Http(200), StatusClass::Success),
            (ResponseStatus::Http(299), StatusClass::Success),
            (ResponseStatus::Http(301), StatusClass::Redirect),
            (ResponseStatus::Http(404), StatusClass::ClientError),
            (ResponseStatus::Http(500), StatusClass::Failure),
            (ResponseStatus::Http(599), StatusClass::Failure),
            (ResponseStatus::Error, StatusClass::Failure),
            (ResponseStatus::Http(101), StatusClass::Neutral),
            (ResponseStatus::Http(600), StatusClass::Neutral),
            (ResponseStatus::Skipped, StatusClass::Neutral),
            (ResponseStatus::Other("x".into()), StatusClass::Neutral),
        ] {
            assert_eq!(classify(&status), class, "{status:?}");
        }
    }

    fn with_headers(pairs: &[(&str, Value)]) -> RequestResult {
        let model = runner_probe_model();
        let mut result = outcome_of(&model, "green.bru").clone();
        result.response.headers = Some(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), v.clone()))
                .collect(),
        );
        result
    }

    #[test]
    fn content_length_is_read_only_from_a_valid_header() {
        let size = |pairs: &[(&str, Value)]| content_length(&with_headers(pairs));
        assert_eq!(size(&[("content-length", "21".into())]), Some(21));
        assert_eq!(size(&[("Content-Length", "21".into())]), Some(21));
        assert_eq!(size(&[("content-length", 1536.into())]), Some(1536));
        assert_eq!(size(&[("content-length", "abc".into())]), None);
        assert_eq!(size(&[("content-length", "-1".into())]), None);
        assert_eq!(size(&[("content-type", "text/html".into())]), None);
        let mut no_headers = with_headers(&[]);
        no_headers.response.headers = None;
        assert_eq!(content_length(&no_headers), None);
    }

    #[test]
    fn size_is_formatted_in_french_units() {
        assert_eq!(format_size(21), "21 o");
        assert_eq!(format_size(1023), "1023 o");
        assert_eq!(format_size(1536), "1,5 Ko");
        assert_eq!(format_size(3 * 1024 * 1024 + 512 * 1024), "3,5 Mo");
    }

    #[test]
    fn check_counts_ignore_skipped_checks() {
        let model = runner_probe_model();
        let mut result = outcome_of(&model, "green.bru").clone();
        let test = |status| TestResult {
            uid: "t".into(),
            description: "t".into(),
            status,
            error: None,
            actual: None,
            expected: None,
        };
        result.assertion_results = vec![AssertionResult {
            uid: "a".into(),
            lhs_expr: "res.status".into(),
            rhs_expr: "eq 200".into(),
            rhs_operand: "200".into(),
            operator: "eq".into(),
            status: ResultStatus::Pass,
            error: None,
        }];
        result.test_results = vec![test(ResultStatus::Pass), test(ResultStatus::Skipped)];
        result.pre_request_test_results = vec![test(ResultStatus::Pass)];
        result.post_response_test_results = vec![test(ResultStatus::Fail)];
        assert_eq!(check_counts(&result), (3, 4));
    }

    #[test]
    fn run_concerns_exact_target_or_recursive_ancestor() {
        assert!(run_concerns(
            &active("folder", true),
            Path::new("folder/down.bru")
        ));
        assert!(!run_concerns(
            &active("folder", true),
            Path::new("folder2/x.bru")
        ));
        assert!(!run_concerns(
            &active("folder", false),
            Path::new("folder/down.bru")
        ));
        assert!(!run_concerns(
            &active("ok.bru", false),
            Path::new("green.bru")
        ));
        assert!(run_concerns(&active("ok.bru", false), Path::new("ok.bru")));
    }

    fn summary(model: &Model) -> String {
        plain(&[status_summary(model)])
    }

    #[test]
    fn fully_successful_request() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        assert_eq!(summary(&model), "200 OK  6 ms  21 o  ✓ 2/2");
        let line = status_summary(&model);
        assert_eq!(
            line.spans[0].style,
            theme::SUCCESS.add_modifier(ratatui::style::Modifier::BOLD)
        );
        assert_eq!(line.spans.last().map(|s| s.style), Some(theme::SUCCESS));
    }

    #[test]
    fn ok_status_with_failing_checks() {
        let mut model = runner_probe_model();
        select(&mut model, "ok.bru");
        assert_eq!(summary(&model), "200 OK  14 ms  32 o  ● 2/4");
        let line = status_summary(&model);
        assert_eq!(line.spans.last().map(|s| s.style), Some(theme::FAILURE));
    }

    #[test]
    fn request_without_response() {
        let mut model = runner_probe_model();
        select(&mut model, "folder/down.bru");
        let text = summary(&model);
        assert_eq!(text, "aucune réponse  0 ms  ●");
        assert!(!text.contains("ECONNREFUSED"), "{text}");
        assert_eq!(
            status_summary(&model).spans[0].style,
            theme::FAILURE.add_modifier(ratatui::style::Modifier::BOLD)
        );
    }

    #[test]
    fn skipped_request() {
        let mut model = runner_probe_model();
        select(&mut model, "skip.bru");
        let text = summary(&model);
        assert!(
            text.starts_with("ignorée request skipped via pre-request script  0 ms"),
            "{text}"
        );
        assert!(text.ends_with('✓'), "{text}");
    }

    #[test]
    fn request_failed_before_sending_keeps_error_out_of_the_summary() {
        let report: crate::runner::Report = serde_json::from_str(include_str!(
            "../../../tests/fixtures/reports/pre-request-error.json"
        ))
        .expect("pre-request-error.json doit se désérialiser");
        let result = report.iterations()[0].results[0].clone();
        let text = plain(&[result_summary(&result)]);
        assert!(text.starts_with("aucune réponse"), "{text}");
        assert!(text.contains('●'), "{text}");
        let error = result.error.as_deref().expect("erreur rapportée");
        assert!(!text.contains(error), "{text}");
    }

    #[test]
    fn no_result_gives_an_empty_summary() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        model.run.outcomes.clear();
        assert_eq!(summary(&model), "");

        let mut folder_model = loaded_model((100, 30));
        select(&mut folder_model, "grp");
        assert_eq!(summary(&folder_model), "");
    }

    #[test]
    fn running_shows_the_indicator() {
        let mut model = runner_probe_model();
        select(&mut model, "folder/down.bru");
        model.run.active = Some(active("folder", true));
        assert_eq!(summary(&model), " en cours ");
        assert_eq!(status_summary(&model).spans[0].style.bg, theme::RUNNING.fg);
        model.run.outcomes.clear();
        assert_eq!(summary(&model), " en cours ");
    }

    #[test]
    fn unrelated_run_does_not_change_the_summary() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        model.run.active = Some(active("ok.bru", false));
        assert!(summary(&model).starts_with("200 OK"));
    }

    #[test]
    fn no_other_header_value_or_body_leaks_into_the_summary() {
        let mut result = with_headers(&[
            ("set-cookie", "session=secret-cookie".into()),
            ("content-length", "21".into()),
        ]);
        result.response.data = Value::String("secret-body".into());
        let text = plain(&[result_summary(&result)]);
        assert!(!text.contains("secret"), "{text}");
        assert!(!text.contains("session"), "{text}");
        assert!(text.contains("21 o"), "{text}");
    }
}
