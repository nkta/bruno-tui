//! Contenu du panneau Statut (capacité `status-panel`).
//!
//! Fonction pure dérivée du résultat conservé et de l'exécution en cours :
//! aucune information qui n'a pas été rapportée par `bru` n'est inventée
//! (pas de libellé déduit du code, pas de taille estimée depuis le corps).

use std::path::Path;

use ratatui::text::{Line, Span};
use serde_json::Value;

use super::theme;
use super::tree::{FAILURE_MARK, SUCCESS_MARK};
use crate::app::model::{ActiveRun, Model, RequestOutcome};
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

/// Lignes intérieures du panneau Statut : trois en forme complète, une
/// en forme compacte.
pub fn status_panel_lines(model: &Model, compact: bool) -> Vec<Line<'static>> {
    let request_path = match model.selected_node() {
        Some(TreeNode::Request(request)) => Some(request.path.as_path()),
        _ => None,
    };
    let outcome = request_path.and_then(|path| model.run.outcomes.get(path));
    let running = request_path.is_some_and(|path| {
        model
            .run
            .active
            .as_ref()
            .is_some_and(|active| run_concerns(active, path))
    });
    match (running, outcome) {
        (true, previous) => running_lines(previous, compact),
        (false, Some(outcome)) => result_lines(&outcome.result, compact),
        (false, None) => vec![Line::from(Span::styled("—", theme::LABEL))],
    }
}

fn badge(text: String, class: StatusClass) -> Span<'static> {
    Span::styled(format!(" {text} "), theme::status_badge(class))
}

/// Indicateur « en cours » et, atténué et sans verdict, le résultat
/// précédent : sur la ligne de l'indicateur en forme compacte, sur les
/// deux lignes suivantes sinon, pour tenir dans une colonne étroite.
fn running_lines(previous: Option<&RequestOutcome>, compact: bool) -> Vec<Line<'static>> {
    let badge = Span::styled(
        " en cours ",
        theme::RUNNING.fg.map_or(theme::RUNNING, theme::badge_on),
    );
    let Some(outcome) = previous else {
        return vec![Line::from(badge)];
    };
    let response = &outcome.result.response;
    let summary = format!(
        "{} · {} ms",
        badge_text(&response.status),
        response.response_time
    );
    if compact {
        return vec![Line::from(vec![
            badge,
            Span::styled(format!(" {summary}"), theme::LABEL),
        ])];
    }
    vec![
        Line::from(badge),
        Line::from(Span::styled("précédent :", theme::LABEL)),
        Line::from(Span::styled(summary, theme::LABEL)),
    ]
}

fn result_lines(result: &RequestResult, compact: bool) -> Vec<Line<'static>> {
    let response = &result.response;
    let badge = badge(badge_text(&response.status), classify(&response.status));
    let (mark, verdict, verdict_style) = if result.is_failure() {
        (FAILURE_MARK, "échec", theme::FAILURE)
    } else {
        (SUCCESS_MARK, "réussi", theme::SUCCESS)
    };
    let time = format!("{} ms", response.response_time);

    if compact {
        return vec![Line::from(vec![
            badge,
            Span::raw(" "),
            Span::styled(mark, verdict_style),
            Span::raw(format!(" {time}")),
        ])];
    }

    let mut first = vec![badge];
    if let Some(label) = response.status_text.as_deref().filter(|t| !t.is_empty()) {
        first.push(Span::raw(format!(" {label}")));
    }

    let second = match content_length(result) {
        Some(bytes) => format!("{time} · {}", format_size(bytes)),
        None => time,
    };

    let (passed, total) = check_counts(result);
    let counts = if total == 0 {
        "aucune vérification".to_owned()
    } else {
        format!("{passed}/{total}")
    };
    let third = Line::from(vec![
        Span::styled(format!("{mark} {verdict}"), verdict_style),
        Span::raw(format!(" · {counts}")),
    ]);

    vec![Line::from(first), Line::raw(second), third]
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

    #[test]
    fn fully_successful_request() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        let lines = status_panel_lines(&model, false);
        assert_eq!(plain(&lines), " 200  OK\n6 ms · 21 o\n✓ réussi · 2/2");
        assert_eq!(
            lines[0].spans[0].style,
            theme::status_badge(StatusClass::Success)
        );
        assert_eq!(lines[2].spans[0].style, theme::SUCCESS);
    }

    #[test]
    fn ok_status_with_failing_checks() {
        let mut model = runner_probe_model();
        select(&mut model, "ok.bru");
        let lines = status_panel_lines(&model, false);
        assert_eq!(plain(&lines), " 200  OK\n14 ms · 32 o\n● échec · 2/4");
        assert_eq!(
            lines[0].spans[0].style,
            theme::status_badge(StatusClass::Success)
        );
        assert_eq!(lines[2].spans[0].style, theme::FAILURE);
    }

    #[test]
    fn request_without_response() {
        let mut model = runner_probe_model();
        select(&mut model, "folder/down.bru");
        let lines = status_panel_lines(&model, false);
        let text = plain(&lines);
        assert_eq!(
            text,
            " aucune réponse \n0 ms\n● échec · aucune vérification"
        );
        assert!(!text.contains("ECONNREFUSED"), "{text}");
        assert_eq!(
            lines[0].spans[0].style,
            theme::status_badge(StatusClass::Failure)
        );
    }

    #[test]
    fn skipped_request() {
        let mut model = runner_probe_model();
        select(&mut model, "skip.bru");
        let lines = status_panel_lines(&model, false);
        let text = plain(&lines);
        assert!(
            text.starts_with(" ignorée  request skipped via pre-request script\n"),
            "{text}"
        );
        assert!(!text.contains("échec"), "{text}");
        assert_eq!(lines[0].spans[0].style, theme::LABEL);
    }

    #[test]
    fn request_failed_before_sending_keeps_error_out_of_the_panel() {
        let report: crate::runner::Report = serde_json::from_str(include_str!(
            "../../../tests/fixtures/reports/pre-request-error.json"
        ))
        .expect("pre-request-error.json doit se désérialiser");
        let result = report.iterations()[0].results[0].clone();
        let lines = result_lines(&result, false);
        let text = plain(&lines);
        assert!(text.starts_with(" aucune réponse "), "{text}");
        assert!(text.contains("● échec"), "{text}");
        let error = result.error.as_deref().expect("erreur rapportée");
        assert!(!text.contains(error), "{text}");
    }

    #[test]
    fn compact_form_is_one_line_badge_mark_time() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        let lines = status_panel_lines(&model, true);
        assert_eq!(plain(&lines), " 200  ✓ 6 ms");
    }

    #[test]
    fn no_result_shows_a_neutral_dash() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        model.run.outcomes.clear();
        let lines = status_panel_lines(&model, false);
        assert_eq!(plain(&lines), "—");
        assert_eq!(lines[0].spans[0].style, theme::LABEL);

        let mut folder_model = loaded_model((100, 30));
        select(&mut folder_model, "grp");
        assert_eq!(plain(&status_panel_lines(&folder_model, false)), "—");
    }

    #[test]
    fn running_shows_indicator_and_dimmed_previous_result() {
        let mut model = runner_probe_model();
        select(&mut model, "folder/down.bru");
        model.run.active = Some(active("folder", true));
        let lines = status_panel_lines(&model, false);
        assert_eq!(
            plain(&lines),
            " en cours \nprécédent :\naucune réponse · 0 ms"
        );
        assert_eq!(lines[0].spans[0].style.bg, theme::RUNNING.fg);
        assert_eq!(lines[1].spans[0].style, theme::LABEL);
        assert_eq!(lines[2].spans[0].style, theme::LABEL);
        assert_eq!(
            plain(&status_panel_lines(&model, true)),
            " en cours  aucune réponse · 0 ms"
        );

        // Première exécution : pas de résultat précédent.
        model.run.outcomes.clear();
        assert_eq!(plain(&status_panel_lines(&model, false)), " en cours ");
    }

    #[test]
    fn unrelated_run_does_not_change_the_panel() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        model.run.active = Some(active("ok.bru", false));
        assert!(plain(&status_panel_lines(&model, false)).starts_with(" 200 "));
    }

    #[test]
    fn no_other_header_value_or_body_leaks_into_the_panel() {
        let mut result = with_headers(&[
            ("set-cookie", "session=secret-cookie".into()),
            ("content-length", "21".into()),
        ]);
        result.response.data = Value::String("secret-body".into());
        let text = plain(&result_lines(&result, false));
        assert!(!text.contains("secret"), "{text}");
        assert!(!text.contains("session"), "{text}");
        assert!(text.contains("21 o"), "{text}");
    }
}
