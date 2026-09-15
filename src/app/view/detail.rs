//! Contenu du panneau de détail.
//!
//! Fonction pure utilisée par le rendu et par `update`, qui s'en sert pour
//! borner le défilement. Les valeurs sont affichées telles qu'écrites dans
//! les fichiers : aucune variable n'est résolue.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};

use super::tree::file_name;
use crate::app::model::Model;
use crate::collection::{
    AuthMode, BodyContent, BodyKind, ErrorNode, FileMeta, FolderNode, KeyValue, RequestNode,
    TreeNode,
};

/// Texte du détail du nœud sélectionné ; vide sans sélection.
pub fn detail_text(model: &Model) -> Text<'static> {
    match model.selected_node() {
        Some(TreeNode::Request(request)) => request_text(request),
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
}
