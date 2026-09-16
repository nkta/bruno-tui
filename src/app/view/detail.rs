//! Contenu du panneau de détail.
//!
//! Fonction pure utilisée par le rendu et par `update`, qui s'en sert pour
//! borner le défilement. Les valeurs sont affichées telles qu'écrites dans
//! les fichiers : aucune variable n'est résolue.

use std::ops::Range;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use serde_json::Value;

use super::tree::file_name;
use crate::app::filter::{FilterResult, FilterState};
use crate::app::model::{Model, RequestOutcome};
use crate::app::update::selection_range;
use crate::collection::{
    AuthMode, BodyContent, BodyKind, ErrorNode, FileMeta, FolderNode, KeyValue, RequestNode,
    TreeNode,
};
use crate::runner::report::{AssertionResult, ResponseStatus, ResultStatus, TestResult};

/// Texte brut du détail, une entrée par ligne logique (`Text.lines`),
/// spans concaténés. Utilisé par la recherche et par la copie : c'est
/// cette même unité de ligne que `update::detail_line_count` compte pour
/// le défilement (voir sa documentation pour la raison du choix).
pub fn plain_lines(model: &Model) -> Vec<String> {
    detail_text(model)
        .lines
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

/// Texte du détail du nœud sélectionné ; vide sans sélection.
pub fn detail_text(model: &Model) -> Text<'static> {
    match model.selected_node() {
        Some(TreeNode::Request(request)) => {
            let mut text = request_text(request);
            if let Some(outcome) = model.run.outcomes.get(&request.path) {
                let filter = model.filter.as_ref().filter(|f| f.target == request.path);
                text.lines.push(Line::default());
                text.lines.extend(result_lines(outcome, filter));
            }
            text
        }
        Some(TreeNode::Folder(folder)) => folder_text(folder),
        Some(TreeNode::Error(error)) => error_text(error),
        None => Text::default(),
    }
}

fn title(text: String) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::new().add_modifier(Modifier::BOLD),
    ))
}

fn section(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_owned(),
        Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ))
}

fn field(label: &str, value: impl Into<String>) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label} : "),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.into()),
    ])
}

fn yes_no(value: bool) -> &'static str {
    if value { "oui" } else { "non" }
}

fn entries(lines: &mut Vec<Line<'static>>, values: &[KeyValue]) {
    if values.is_empty() {
        lines.push(Line::raw("  aucun"));
    }
    for entry in values {
        let text = format!("  {}: {}", entry.key, entry.value);
        if entry.enabled {
            lines.push(Line::raw(text));
        } else {
            lines.push(Line::styled(
                format!("{text} (désactivé)"),
                Style::new().add_modifier(Modifier::DIM),
            ));
        }
    }
}

/// Mode d'auth tel qu'écrit dans le fichier.
pub fn auth_label(mode: &AuthMode) -> &str {
    match mode {
        AuthMode::NoAuth => "none",
        AuthMode::Inherit => "inherit",
        AuthMode::Basic => "basic",
        AuthMode::Bearer => "bearer",
        AuthMode::Digest => "digest",
        AuthMode::Ntlm => "ntlm",
        AuthMode::OAuth2 => "oauth2",
        AuthMode::AwsV4 => "awsv4",
        AuthMode::Wsse => "wsse",
        AuthMode::ApiKey => "apikey",
        AuthMode::Other(other) => other,
    }
}

/// Type de corps tel qu'écrit dans le fichier.
pub fn body_label(kind: &BodyKind) -> &str {
    match kind {
        BodyKind::Json => "json",
        BodyKind::Text => "text",
        BodyKind::Xml => "xml",
        BodyKind::Sparql => "sparql",
        BodyKind::Graphql => "graphql",
        BodyKind::FormUrlEncoded => "formUrlEncoded",
        BodyKind::MultipartForm => "multipartForm",
        BodyKind::File => "file",
        BodyKind::Other(other) => other,
    }
}

fn request_text(request: &RequestNode) -> Text<'static> {
    let view = &request.view;
    let node_name = view.name.clone().unwrap_or_else(|| {
        request
            .path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let mut lines = vec![
        title(node_name),
        field("Chemin", request.path.display().to_string()),
        Line::default(),
        Line::from(vec![
            Span::styled(
                format!("{} ", view.method),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::raw(view.url.clone()),
        ]),
        field(
            "Auth",
            view.auth
                .as_ref()
                .map_or("non déclarée", auth_label)
                .to_owned(),
        ),
        Line::default(),
        section("En-têtes"),
    ];
    entries(&mut lines, &view.headers);
    lines.push(section("Paramètres de requête"));
    entries(&mut lines, &view.query_params);
    lines.push(section("Paramètres de chemin"));
    entries(&mut lines, &view.path_params);

    lines.push(Line::default());
    match &view.body {
        None => lines.push(field("Corps", "aucun")),
        Some(body) => {
            lines.push(field("Corps", body_label(&body.kind).to_owned()));
            match &body.content {
                BodyContent::Text(text) => {
                    lines.extend(text.split('\n').map(|l| Line::raw(format!("  {l}"))));
                }
                BodyContent::Entries(values) => entries(&mut lines, values),
                BodyContent::Missing => lines.push(Line::raw("  bloc absent")),
            }
        }
    }

    lines.push(Line::default());
    lines.push(field(
        "Script pré-requête",
        yes_no(view.has_pre_request_script),
    ));
    lines.push(field(
        "Script post-réponse",
        yes_no(view.has_post_response_script),
    ));
    lines.push(field("Tests", yes_no(view.has_tests)));
    lines.push(field("Assertions", yes_no(view.has_assert)));
    if !view.assertions.is_empty() {
        entries(&mut lines, &view.assertions);
    }
    Text::from(lines)
}

/// Représentation d'une valeur JSON pour l'affichage : sans guillemets pour
/// une chaîne, telle quelle sinon.
fn json_display(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn status_word(status: &ResultStatus) -> String {
    match status {
        ResultStatus::Pass => "succès".to_owned(),
        ResultStatus::Fail => "échec".to_owned(),
        ResultStatus::Error => "erreur".to_owned(),
        ResultStatus::Skipped => "ignoré".to_owned(),
        ResultStatus::Other(other) => other.clone(),
    }
}

fn assertion_line(assertion: &AssertionResult) -> Line<'static> {
    let mut text = format!(
        "  {} {} {} : {}",
        assertion.lhs_expr,
        assertion.operator,
        assertion.rhs_expr,
        status_word(&assertion.status)
    );
    if let Some(error) = &assertion.error {
        text.push_str(&format!(" — {error}"));
    }
    Line::raw(text)
}

fn test_line(test: &TestResult) -> Line<'static> {
    let mut text = format!("  {} : {}", test.description, status_word(&test.status));
    if let Some(error) = &test.error {
        text.push_str(&format!(" — {error}"));
    }
    Line::raw(text)
}

/// Ajoute une sous-section titrée ; « aucun » si `items` est vide.
fn push_checks<I: IntoIterator<Item = Line<'static>>>(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    items: I,
) {
    lines.push(section(title));
    let mut any = false;
    for line in items {
        lines.push(line);
        any = true;
    }
    if !any {
        lines.push(Line::raw("  aucun"));
    }
}

fn body_lines(data: &Value) -> Vec<Line<'static>> {
    if data.is_null() {
        return vec![Line::raw("  aucun")];
    }
    let text = json_display(data);
    text.split('\n')
        .map(|l| Line::raw(format!("  {l}")))
        .collect()
}

/// Ligne affichant le filtre en cours d'édition ou appliqué.
fn filter_line(draft: &str, editing: bool) -> Line<'static> {
    let mut spans = vec![
        Span::styled("Filtre : ", Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(draft.to_owned()),
    ];
    if editing {
        spans.push(Span::raw("█"));
    }
    Line::from(spans)
}

/// Section « Résultat » d'une requête exécutée : verdict, réponse, puis
/// chaque assertion et test (y compris pré-requête et post-réponse).
fn result_lines(outcome: &RequestOutcome, filter: Option<&FilterState>) -> Vec<Line<'static>> {
    let result = &outcome.result;
    let mut lines = vec![section("Résultat")];
    lines.push(field(
        "Verdict",
        if result.is_failure() {
            "échec"
        } else {
            "réussi"
        },
    ));
    match &result.response.status {
        ResponseStatus::Http(code) => lines.push(field("Statut", code.to_string())),
        ResponseStatus::Error => {
            lines.push(field("Statut", "aucune réponse"));
            if let Some(error) = &result.error {
                lines.push(field("Erreur", error.clone()));
            }
        }
        ResponseStatus::Skipped => lines.push(field("Statut", "ignorée")),
        ResponseStatus::Other(other) => lines.push(field("Statut", other.clone())),
    }
    lines.push(field(
        "Temps de réponse",
        format!("{} ms", result.response.response_time),
    ));

    lines.push(section("En-têtes de réponse"));
    match &result.response.headers {
        Some(headers) if !headers.is_empty() => {
            for (key, value) in headers {
                lines.push(Line::raw(format!("  {key}: {}", json_display(value))));
            }
        }
        _ => lines.push(Line::raw("  aucun")),
    }

    let filter_active = filter.is_some_and(|f| f.editing || f.applied.is_some());
    if let Some(f) = filter.filter(|_| filter_active) {
        lines.push(filter_line(&f.draft, f.editing));
        match &f.applied {
            Some(FilterResult::Output(outputs)) => {
                for out in outputs {
                    for line in out.lines() {
                        lines.push(Line::raw(format!("  {line}")));
                    }
                }
            }
            Some(FilterResult::Error(error)) => {
                for line in error.lines() {
                    lines.push(Line::styled(
                        format!("  {line}"),
                        Style::new().fg(Color::Red),
                    ));
                }
            }
            None => {
                lines.extend(body_lines(&result.response.data));
            }
        }
    } else {
        lines.push(section("Corps de réponse"));
        lines.extend(body_lines(&result.response.data));
    }

    push_checks(
        &mut lines,
        "Assertions",
        result.assertion_results.iter().map(assertion_line),
    );
    push_checks(
        &mut lines,
        "Tests",
        result.test_results.iter().map(test_line),
    );
    push_checks(
        &mut lines,
        "Tests pré-requête",
        result.pre_request_test_results.iter().map(test_line),
    );
    push_checks(
        &mut lines,
        "Tests post-réponse",
        result.post_response_test_results.iter().map(test_line),
    );

    lines
}

fn meta_lines(lines: &mut Vec<Line<'static>>, meta: &FileMeta) {
    lines.push(field("  Auth", yes_no(meta.has_auth)));
    lines.push(field("  En-têtes", yes_no(meta.has_headers)));
    lines.push(field(
        "  Script pré-requête",
        yes_no(meta.has_pre_request_script),
    ));
    lines.push(field(
        "  Script post-réponse",
        yes_no(meta.has_post_response_script),
    ));
    lines.push(field("  Tests", yes_no(meta.has_tests)));
}

fn folder_text(folder: &FolderNode) -> Text<'static> {
    let mut lines = vec![
        title(folder.name.clone()),
        field("Chemin", folder.path.display().to_string()),
        field(
            "Seq",
            folder
                .seq
                .map_or_else(|| "aucun".to_owned(), |seq| seq.to_string()),
        ),
        field("Enfants", folder.children.len().to_string()),
        Line::default(),
    ];
    match &folder.meta {
        None => lines.push(field("folder.bru", "absent")),
        Some(Ok(meta)) => {
            lines.push(field("folder.bru", "présent"));
            meta_lines(&mut lines, meta);
        }
        Some(Err(error)) => {
            lines.push(field("folder.bru", "en erreur"));
            lines.push(Line::raw(format!("  {error}")));
        }
    }
    Text::from(lines)
}

/// Fond distinct des lignes d'une sélection visuelle active.
const SELECTION_TINT: Style = Style::new().bg(Color::Rgb(40, 55, 75));
/// Fond distinct d'une correspondance de recherche, différent de la teinte
/// de sélection et du surlignage `REVERSED` déjà utilisé pour le nœud
/// sélectionné dans l'arbre.
const MATCH_HIGHLIGHT: Style = Style::new().bg(Color::Rgb(96, 78, 0));

/// `detail_text` avec la teinte de sélection visuelle et la surbrillance
/// de la dernière correspondance de recherche appliquées, pour le rendu
/// seulement : `detail_text` reste la source utilisée pour le calcul de
/// défilement et pour la copie.
pub fn render_text(model: &Model) -> Text<'static> {
    let mut text = detail_text(model);
    if let Some(range) = selection_range(model) {
        for (index, line) in text.lines.iter_mut().enumerate() {
            if range.contains(&(index as u16)) {
                *line = tint_line(std::mem::take(line), SELECTION_TINT);
            }
        }
    }
    if let Some((line_index, range)) = &model.detail_match
        && let Some(line) = text.lines.get_mut(usize::from(*line_index))
    {
        *line = highlight_match(line, range.clone());
    }
    text
}

fn tint_line(line: Line<'static>, tint: Style) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| Span::styled(span.content, span.style.patch(tint)))
            .collect::<Vec<_>>(),
    )
}

/// Découpe `line` en spans avant/motif/après selon `range` (position en
/// octets dans le texte concaténé de la ligne), en conservant le style
/// d'origine de chaque portion et en y ajoutant le fond de surbrillance sur
/// le motif. `range` vient de `find_detail_match`, qui le construit par
/// `str::find` : toujours une frontière de caractère valide.
fn highlight_match(line: &Line<'static>, range: Range<usize>) -> Line<'static> {
    let mut spans = Vec::new();
    let mut pos = 0usize;
    for span in &line.spans {
        let text = span.content.as_ref();
        let span_start = pos;
        let span_end = pos + text.len();
        pos = span_end;
        let hl_start = range.start.max(span_start);
        let hl_end = range.end.min(span_end);
        if hl_start >= hl_end {
            spans.push(Span::styled(text.to_owned(), span.style));
            continue;
        }
        let local_start = hl_start - span_start;
        let local_end = hl_end - span_start;
        if local_start > 0 {
            spans.push(Span::styled(text[..local_start].to_owned(), span.style));
        }
        spans.push(Span::styled(
            text[local_start..local_end].to_owned(),
            span.style.patch(MATCH_HIGHLIGHT),
        ));
        if local_end < text.len() {
            spans.push(Span::styled(text[local_end..].to_owned(), span.style));
        }
    }
    Line::from(spans)
}

fn error_text(error: &ErrorNode) -> Text<'static> {
    Text::from(vec![
        title(file_name(&error.path)),
        field("Chemin", error.path.display().to_string()),
        field("Erreur", error.error.to_string()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::test_support::{loaded_model, select};

    fn plain(text: &Text<'_>) -> String {
        text.lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn detail_of(path: &str) -> String {
        let mut model = loaded_model((100, 30));
        select(&mut model, path);
        plain(&detail_text(&model))
    }

    #[test]
    fn post_json_request() {
        let text = detail_of("post-json.bru");
        for expected in [
            "POST https://{{host}}/items",
            "Content-Type: application/json",
            "Corps : json",
            "\"s\": \"}\"",
            "Chemin : post-json.bru",
        ] {
            assert!(text.contains(expected), "`{expected}` absent :\n{text}");
        }
    }

    #[test]
    fn scripted_request() {
        let text = detail_of("scripted.bru");
        for expected in [
            "X-Debug: 1 (désactivé)",
            "Accept: application/json\n",
            "Script pré-requête : oui",
            "Script post-réponse : oui",
            "Tests : oui",
            "Assertions : oui",
            "res.status: eq 200",
            "Auth : bearer",
        ] {
            assert!(text.contains(expected), "`{expected}` absent :\n{text}");
        }
    }

    #[test]
    fn inherited_auth_is_shown_as_written() {
        let text = detail_of("grp/inherit.bru");
        assert!(text.contains("Auth : inherit"), "{text}");
        assert!(!text.contains("bearer"), "{text}");
    }

    #[test]
    fn error_node() {
        let text = detail_of("broken.bru");
        assert!(text.starts_with("broken.bru\n"), "{text}");
        assert!(text.contains("Chemin : broken.bru"), "{text}");
        assert!(text.contains("`headers`") && text.contains("13"), "{text}");
        assert!(!text.contains("s3cr3t"), "{text}");
    }

    #[test]
    fn folder_with_invalid_meta() {
        let text = detail_of("badmeta");
        assert!(text.contains("folder.bru : en erreur"), "{text}");
        assert!(text.contains("`meta`"), "{text}");
        assert!(text.contains("Enfants : 1"), "{text}");
    }

    #[test]
    fn folder_with_meta_and_without() {
        let text = detail_of("grp");
        assert!(
            text.contains("Seq : 3") && text.contains("Enfants : 4"),
            "{text}"
        );
        assert!(
            text.contains("folder.bru : présent") && text.contains("Auth : oui"),
            "{text}"
        );
        assert!(detail_of("misc").contains("folder.bru : absent"));
    }

    #[test]
    fn simple_get_without_body() {
        let text = detail_of("simple-get.bru");
        assert!(text.contains("GET https://{{host}}/ping"), "{text}");
        assert!(
            text.contains("Corps : aucun") && text.contains("Auth : none"),
            "{text}"
        );
        assert!(text.contains("En-têtes\n  aucun"), "{text}");
    }

    use crate::runner::report::{RequestFile, RequestInfo, RequestResult, ResponseInfo};

    fn base_result() -> RequestResult {
        RequestResult {
            name: "ping".to_owned(),
            path: "simple-get".to_owned(),
            test: RequestFile {
                filename: "simple-get.bru".to_owned(),
            },
            request: RequestInfo {
                method: "GET".into(),
                url: "https://x/ping".into(),
                headers: Default::default(),
            },
            response: ResponseInfo {
                status: ResponseStatus::Http(200),
                status_text: Some("OK".into()),
                headers: Some(Default::default()),
                data: Value::Null,
                url: Some("https://x/ping".into()),
                response_time: 12,
            },
            error: None,
            status: ResultStatus::Pass,
            skipped: false,
            assertion_results: Vec::new(),
            test_results: Vec::new(),
            pre_request_test_results: Vec::new(),
            post_response_test_results: Vec::new(),
            should_stop_runner_execution: false,
            run_duration: 0.01,
            iteration_index: 0,
        }
    }

    fn result_detail(result: RequestResult) -> String {
        let mut model = loaded_model((100, 30));
        model.run.outcomes.insert(
            "simple-get.bru".into(),
            RequestOutcome {
                result,
                exit_code: Some(0),
            },
        );
        select(&mut model, "simple-get.bru");
        plain(&detail_text(&model))
    }

    #[test]
    fn result_section_for_a_fully_successful_request() {
        let text = result_detail(RequestResult {
            response: ResponseInfo {
                data: serde_json::json!({"a": [1, 2]}),
                ..base_result().response
            },
            ..base_result()
        });
        assert!(text.contains("Résultat"), "{text}");
        assert!(text.contains("Verdict : réussi"), "{text}");
        assert!(text.contains("Statut : 200"), "{text}");
        assert!(text.contains("Temps de réponse : 12 ms"), "{text}");
    }

    #[test]
    fn result_section_for_a_failing_assertion() {
        let text = result_detail(RequestResult {
            status: ResultStatus::Pass,
            assertion_results: vec![AssertionResult {
                uid: "1".into(),
                lhs_expr: "res.status".into(),
                rhs_expr: "eq 404".into(),
                rhs_operand: "404".into(),
                operator: "eq".into(),
                status: ResultStatus::Fail,
                error: Some("expected 200 to equal 404".into()),
            }],
            ..base_result()
        });
        assert!(text.contains("Verdict : échec"), "{text}");
        assert!(text.contains("expected 200 to equal 404"), "{text}");
    }

    #[test]
    fn result_section_for_a_failing_post_response_test() {
        let text = result_detail(RequestResult {
            post_response_test_results: vec![TestResult {
                uid: "1".into(),
                description: "post ko".into(),
                status: ResultStatus::Fail,
                error: Some("expected 2 to equal 3".into()),
                actual: None,
                expected: None,
            }],
            ..base_result()
        });
        assert!(text.contains("Verdict : échec"), "{text}");
        assert!(text.contains("post ko"), "{text}");
        assert!(text.contains("expected 2 to equal 3"), "{text}");
    }

    #[test]
    fn result_section_for_no_response() {
        let text = result_detail(RequestResult {
            status: ResultStatus::Error,
            error: Some("connect ECONNREFUSED 127.0.0.1:18799".into()),
            response: ResponseInfo {
                status: ResponseStatus::Error,
                status_text: None,
                headers: None,
                data: Value::Null,
                url: None,
                response_time: 0,
            },
            ..base_result()
        });
        assert!(text.contains("Verdict : échec"), "{text}");
        assert!(text.contains("Statut : aucune réponse"), "{text}");
        assert!(
            text.contains("connect ECONNREFUSED 127.0.0.1:18799"),
            "{text}"
        );
        assert!(!text.contains("Statut : 200"), "{text}");
    }

    #[test]
    fn result_section_for_a_skipped_request() {
        let text = result_detail(RequestResult {
            status: ResultStatus::Skipped,
            skipped: true,
            response: ResponseInfo {
                status: ResponseStatus::Skipped,
                status_text: Some("request skipped via pre-request script".into()),
                headers: None,
                data: Value::Null,
                url: None,
                response_time: 0,
            },
            ..base_result()
        });
        assert!(text.contains("Statut : ignorée"), "{text}");
        assert!(!text.contains("Verdict : échec"), "{text}");
    }

    #[test]
    fn filter_applied_replaces_body_with_formatted_result_and_filter_line() {
        use crate::app::message::Message;
        use crate::app::test_support::{runner_probe_model, screen_lines};
        use crate::app::update::update;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");

        // Avant filtrage : la section s'appelle "Corps de réponse" et contient le JSON brut
        let text_before = plain(&detail_text(&model));
        assert!(text_before.contains("Corps de réponse"), "{text_before}");
        assert!(!text_before.contains("Filtre :"), "{text_before}");

        // Applique le filtre `.a` sur json.bru
        update(&mut model, Message::OpenFilter);
        for c in ".a".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);

        // Vérification par TestBackend
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 50)).expect("terminal");
        terminal
            .draw(|frame| super::super::view(&model, frame))
            .expect("rendu");
        let screen = screen_lines(terminal.backend()).join("\n");

        assert!(screen.contains("Filtre : .a"), "{screen}");
        assert!(screen.contains("["), "{screen}");
        assert!(screen.contains("1,"), "{screen}");
        assert!(screen.contains("2"), "{screen}");
        assert!(screen.contains("]"), "{screen}");
        // "Corps de réponse" a été remplacé par le filtre
        assert!(!screen.contains("Corps de réponse"), "{screen}");

        // Si le filtre ne correspond pas au nœud affiché, le détail reste inchangé
        model.filter.as_mut().unwrap().target = std::path::PathBuf::from("autre.bru");
        let text_other = plain(&detail_text(&model));
        assert!(text_other.contains("Corps de réponse"), "{text_other}");
        assert!(!text_other.contains("Filtre :"), "{text_other}");
    }

    #[test]
    fn filter_error_shows_visible_error_message_and_survives_absurd_terminal_size() {
        use crate::app::message::Message;
        use crate::app::test_support::{runner_probe_model, screen_lines};
        use crate::app::update::update;

        let mut model = runner_probe_model();
        select(&mut model, "json.bru");

        // Applique un filtre invalide `.a.b |`
        update(&mut model, Message::OpenFilter);
        for c in ".a.b |".chars() {
            update(&mut model, Message::FilterInput(c));
        }
        update(&mut model, Message::ConfirmFilter);

        // Vérification par TestBackend sur terminal 120x50
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 50)).expect("terminal");
        terminal
            .draw(|frame| super::super::view(&model, frame))
            .expect("rendu");
        let screen = screen_lines(terminal.backend()).join("\n");

        assert!(screen.contains("Filtre : .a.b |"), "{screen}");
        // Un message d'erreur de syntaxe doit être visible
        let text = plain(&detail_text(&model));
        assert!(
            text.contains("erreur de syntaxe")
                || text.contains("syntax error")
                || text.contains("attendait")
                || text.contains("attendu"),
            "message d'erreur attendu dans le détail :\n{text}"
        );

        // Vérifier que le message d'erreur est affiché en rouge dans le buffer
        let buffer = terminal.backend().buffer();
        let mut found_red = false;
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.fg == ratatui::style::Color::Red && !cell.symbol().trim().is_empty() {
                    found_red = true;
                    break;
                }
            }
            if found_red {
                break;
            }
        }
        assert!(found_red, "le message d'erreur doit être stylé en rouge");

        // Sur un terminal 1x1, aucun panic ne doit survenir
        let mut small_terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(1, 1)).expect("terminal");
        small_terminal
            .draw(|frame| super::super::view(&model, frame))
            .expect("rendu 1x1");
    }
}
