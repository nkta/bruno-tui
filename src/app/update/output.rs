//! Sélection visuelle, copie, commande curl, éditeur externe et export.

use crate::app::clipboard::ClipboardError;
use crate::app::model::{
    ClipboardSource, DetailSelection, ExportInputState, Focus, Model, PendingConfirm, StatusMessage,
};
use crate::app::update::{Command, response_selection_range, selection_range};
use crate::app::view::detail::{plain_lines, response_plain_lines};
use crate::collection::TreeNode;

/// Bascule la sélection visuelle dans le panneau courant (Détail ou Réponse
/// courant). Chaque panneau porte sa propre sélection, indépendamment de
/// l'autre (`split-request-response-panels`).
pub(super) fn toggle_visual(model: &mut Model) {
    match model.focus {
        Focus::Detail => {
            if model.detail_selection.take().is_some() {
                return;
            }
            model.detail_selection = Some(DetailSelection {
                anchor: model.detail_scroll,
                head: None,
            });
        }
        Focus::Response => {
            if model.response_selection.take().is_some() {
                return;
            }
            model.response_selection = Some(DetailSelection {
                anchor: model.response_scroll,
                head: None,
            });
        }
        _ => {}
    }
}

/// `y` : copie la sélection visuelle si elle est active, sinon la seule
/// ligne au sommet du panneau. La sélection copiée n'est levée qu'à la
/// réception d'une copie réussie : un échec la laisse intacte. Sans effet hors focus Détail ou Réponse
/// (l'arbre et la saisie de recherche n'ont pas de notion de ligne à
/// copier), à partir du panneau qui a le focus.
pub(super) fn yank(model: &mut Model) -> Command {
    let (lines, range, scroll) = match model.focus {
        Focus::Detail => (
            plain_lines(model),
            selection_range(model),
            model.detail_scroll,
        ),
        Focus::Response => (
            response_plain_lines(model),
            response_selection_range(model),
            model.response_scroll,
        ),
        _ => return Command::None,
    };
    let selection = range.is_some().then_some(model.focus);
    let text = match range {
        Some(range) => {
            let selected: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(index, _)| range.contains(&(*index as u16)))
                .map(|(_, line)| line.as_str())
                .collect();
            selected.join("\n")
        }
        None => lines.get(usize::from(scroll)).cloned().unwrap_or_default(),
    };
    let token = model.next_clipboard_token;
    model.next_clipboard_token += 1;
    model.pending_clipboard_token = Some(token);
    model.pending_clipboard_source = selection.map(ClipboardSource::Selection);
    Command::CopyToClipboard { token, text }
}

/// Résultat d'une copie précédemment lancée. Un jeton qui ne correspond
/// plus au dernier émis est ignoré (résultat en retard sur une copie plus
/// récente, cf. `Model::pending_clipboard_token`).
pub(super) fn apply_clipboard_result(
    model: &mut Model,
    token: u64,
    result: Result<(), ClipboardError>,
) {
    if model.pending_clipboard_token != Some(token) {
        return;
    }
    model.pending_clipboard_token = None;
    let source = model.pending_clipboard_source.take();
    if let (Ok(()), Some(ClipboardSource::Selection(focus))) = (&result, source) {
        match focus {
            Focus::Detail => model.detail_selection = None,
            Focus::Response => model.response_selection = None,
            _ => {}
        }
    }
    model.last_status = Some(match result {
        Ok(()) => match source {
            Some(ClipboardSource::Curl { masked }) => StatusMessage::CopiedCurl { masked },
            _ => StatusMessage::Copied,
        },
        Err(error) => StatusMessage::ClipboardError(error.to_string()),
    });
}

/// Renvoie la requête résolue du résultat d'exécution courant si elle a une URL.
pub(super) fn current_result_request(model: &Model) -> Option<&crate::runner::report::RequestInfo> {
    let TreeNode::Request(request_node) = model.selected_node()? else {
        return None;
    };
    let outcome = model.run.outcomes.get(&request_node.path)?;
    outcome.result.request.url.as_ref()?;
    Some(&outcome.result.request)
}

/// Copie dans le presse-papiers la commande curl issue du dernier résultat d'exécution.
pub(super) fn copy_curl(model: &mut Model, masked: bool) -> Command {
    let Some(request) = current_result_request(model) else {
        model.last_status = Some(StatusMessage::NoCurlResult);
        return Command::None;
    };
    let Some(curl_command) = crate::runner::curl::build_curl(request, masked) else {
        model.last_status = Some(StatusMessage::NoCurlResult);
        return Command::None;
    };
    let token = model.next_clipboard_token;
    model.next_clipboard_token += 1;
    model.pending_clipboard_token = Some(token);
    model.pending_clipboard_source = Some(ClipboardSource::Curl { masked });
    Command::CopyToClipboard {
        token,
        text: curl_command,
    }
}

/// Copie complète (démasquée) de la commande curl, avec confirmation si des secrets sont présents.
pub(super) fn copy_curl_full(model: &mut Model) -> Command {
    let Some(request) = current_result_request(model) else {
        model.last_status = Some(StatusMessage::NoCurlResult);
        return Command::None;
    };
    if crate::runner::curl::has_sensitive_headers(request) {
        model.confirm = Some(PendingConfirm::CopyUnmaskedCurl);
        Command::None
    } else {
        copy_curl(model, false)
    }
}

/// Résout le programme éditeur à lancer :
/// 1. `cli_editor` si défini et non vide ;
/// 2. `VISUAL` si définie et non vide ;
/// 3. `EDITOR` si définie et non vide ;
/// 4. `"nano"`.
///
/// Paramétrée par une commande CLI optionnelle et une fonction de recherche
/// de variable d'environnement pour rester pure et testable sans dépendre de l'état du processus.
pub fn resolve_editor_with<F>(cli_editor: Option<&str>, lookup: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    if let Some(cli) = cli_editor.filter(|c| !c.is_empty()) {
        return cli.to_string();
    }
    if let Some(visual) = lookup("VISUAL").filter(|v| !v.is_empty()) {
        return visual;
    }
    if let Some(editor) = lookup("EDITOR").filter(|e| !e.is_empty()) {
        return editor;
    }
    "nano".to_string()
}

/// Résout le programme éditeur à lancer depuis l'environnement réel du processus.
pub fn resolve_editor(cli_editor: Option<&str>) -> String {
    resolve_editor_with(cli_editor, |key| std::env::var(key).ok())
}

/// Prépare l'exportation du corps de réponse et ouvre la saisie du chemin.
pub(super) fn start_export_response_body(model: &mut Model) {
    let Some(TreeNode::Request(request)) = model.selected_node() else {
        model.last_status = Some(StatusMessage::NoResponseBody);
        return;
    };
    let Some(outcome) = model.run.outcomes.get(&request.path) else {
        model.last_status = Some(StatusMessage::NoResponseBody);
        return;
    };
    let Some(bytes) = crate::app::export::raw_response_bytes(&outcome.result.response.data) else {
        model.last_status = Some(StatusMessage::NoResponseBody);
        return;
    };
    let ext = crate::app::export::detect_response_extension(
        outcome.result.response.headers.as_ref(),
        &outcome.result.response.data,
    );
    let req_name = request.view.name.as_deref().unwrap_or_else(|| {
        request
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("requete")
    });
    let collection_root = model.loaded().map(|c| c.root.as_path());
    let default_path = crate::app::export::default_export_path(
        collection_root,
        req_name,
        ext,
        std::time::SystemTime::now(),
    );
    model.export_input = Some(ExportInputState::new(
        &default_path.to_string_lossy(),
        bytes,
    ));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::message::Message;
    use crate::app::model::{Focus, PendingConfirm, StatusMessage};
    use crate::app::test_support::{loaded_model, runner_probe_model, select};
    use crate::app::text_input::TextInput;
    use crate::app::update::test_support::sample_request_result;
    use crate::app::update::update;

    #[test]
    fn resolve_editor_precedence() {
        // cli_editor est prioritaire sur VISUAL et EDITOR
        assert_eq!(
            resolve_editor_with(Some("subl -w"), |key| match key {
                "VISUAL" => Some("code -w".into()),
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "subl -w"
        );
        // Si cli_editor est vide, VISUAL prend le relais
        assert_eq!(
            resolve_editor_with(Some(""), |key| match key {
                "VISUAL" => Some("code -w".into()),
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "code -w"
        );

        // VISUAL est prioritaire si définie et non vide
        assert_eq!(
            resolve_editor_with(None, |key| match key {
                "VISUAL" => Some("code -w".into()),
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "code -w"
        );

        // EDITOR est utilisé si VISUAL n'est pas définie ou est vide
        assert_eq!(
            resolve_editor_with(None, |key| match key {
                "VISUAL" => None,
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "vim"
        );
        assert_eq!(
            resolve_editor_with(None, |key| match key {
                "VISUAL" => Some("".into()),
                "EDITOR" => Some("vim".into()),
                _ => None,
            }),
            "vim"
        );

        // "nano" par défaut si ni VISUAL ni EDITOR ne sont définies ou sont vides
        assert_eq!(
            resolve_editor_with(None, |key| match key {
                "VISUAL" => None,
                "EDITOR" => None,
                _ => None,
            }),
            "nano"
        );
        assert_eq!(
            resolve_editor_with(None, |key| match key {
                "VISUAL" => Some("".into()),
                "EDITOR" => Some("".into()),
                _ => None,
            }),
            "nano"
        );
    }

    #[test]
    fn open_response_in_editor_command_and_status() {
        let mut model = runner_probe_model();

        // Avec corps exploitable (ok.bru)
        select(&mut model, "ok.bru");
        match update(&mut model, Message::OpenResponseInEditor) {
            Command::OpenInEditor { program, text } => {
                assert_eq!(program, resolve_editor(model.editor.as_deref()));
                assert_eq!(text, "<html><body>probe</body></html>\n");
            }
            other => panic!("Command::OpenInEditor attendu, obtenu {other:?}"),
        }

        // Sans corps exploitable : dossier sélectionné
        select(&mut model, "folder");
        assert!(matches!(
            update(&mut model, Message::OpenResponseInEditor),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoResponseBody)
        ));

        // Sans corps exploitable : corps null (folder/down.bru)
        select(&mut model, "folder/down.bru");
        model.last_status = None;
        assert!(matches!(
            update(&mut model, Message::OpenResponseInEditor),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoResponseBody)
        ));

        // Sans corps exploitable : requête jamais exécutée
        select(&mut model, "ok.bru");
        model.run.outcomes.clear();
        model.last_status = None;
        assert!(matches!(
            update(&mut model, Message::OpenResponseInEditor),
            Command::None
        ));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoResponseBody)
        ));
    }

    #[test]
    fn toggle_visual_then_escape_does_not_change_scroll_or_copy() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        let scroll_before = model.detail_scroll;
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::FocusTree);
        assert!(model.detail_selection.is_none());
        assert_eq!(model.detail_scroll, scroll_before);
        // Le premier Échap annule seulement la sélection : le focus reste
        // sur le détail.
        assert_eq!(model.focus, Focus::Detail);
    }

    #[test]
    fn toggle_visual_then_escape_in_response_does_not_change_focus_or_detail_selection() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        let scroll_before = model.response_scroll;
        update(&mut model, Message::ToggleVisual);
        assert!(model.response_selection.is_some());
        update(&mut model, Message::FocusTree);
        assert!(model.response_selection.is_none());
        assert_eq!(model.response_scroll, scroll_before);
        // Le premier Échap annule seulement la sélection : le focus reste
        // sur la réponse, et la sélection du détail n'est pas concernée.
        assert_eq!(model.focus, Focus::Response);
        assert!(model.detail_selection.is_none());
    }

    #[test]
    fn a_second_toggle_visual_cancels_the_selection_without_copying() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        let scroll_before = model.detail_scroll;
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_none());
        assert_eq!(model.detail_scroll, scroll_before);
    }

    #[test]
    fn toggle_visual_extends_to_the_true_last_line_on_end() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::ToggleVisual);
        update(&mut model, Message::End);
        let range = selection_range(&model).expect("sélection active");
        let last_line = (plain_lines(&model).len() - 1) as u16;
        assert_eq!(*range.end(), last_line);
        assert_eq!(*range.start(), 0);
    }

    #[test]
    fn selecting_another_node_clears_the_visual_selection() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus); // Tab : focus Détail
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::NextFocus); // Tab : focus Réponse, sélection intacte
        update(&mut model, Message::NextFocus); // Tab : focus Arbre, sélection intacte
        assert_eq!(model.focus, Focus::Tree);
        assert!(model.detail_selection.is_some());
        update(&mut model, Message::Down); // change le nœud sélectionné
        assert!(model.detail_selection.is_none());
    }

    #[test]
    fn yank_without_selection_copies_the_top_line() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let expected = plain_lines(&model)[0].clone();
        match update(&mut model, Message::Yank) {
            Command::CopyToClipboard { token, text } => {
                assert_eq!(text, expected);
                assert_eq!(model.pending_clipboard_token, Some(token));
            }
            other => panic!("CopyToClipboard attendu, obtenu {other:?}"),
        }
    }

    #[test]
    fn yank_with_selection_joins_lines_and_clears_selection() {
        let mut model = loaded_model((100, 12));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::ToggleVisual);
        update(&mut model, Message::End);
        let expected = plain_lines(&model).join("\n");
        let token = match update(&mut model, Message::Yank) {
            Command::CopyToClipboard { text, token } => {
                assert_eq!(text, expected);
                token
            }
            other => panic!("CopyToClipboard attendu, obtenu {other:?}"),
        };
        // Levée seulement une fois la copie réussie.
        assert!(model.detail_selection.is_some());
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(model.detail_selection.is_none());
    }

    #[test]
    fn yank_outside_detail_focus_does_nothing() {
        let mut model = loaded_model((100, 30));
        assert!(matches!(update(&mut model, Message::Yank), Command::None));
    }

    #[test]
    fn visual_selections_in_detail_and_response_are_independent() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");

        // Sélection visuelle dans le détail.
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::ToggleVisual);
        assert!(model.detail_selection.is_some());

        // Bascule vers la réponse et active sa propre sélection.
        update(&mut model, Message::NextFocus); // Réponse
        assert!(model.response_selection.is_none());
        update(&mut model, Message::ToggleVisual);
        assert!(model.response_selection.is_some());

        // Les deux sélections coexistent, chacune bornée à son panneau.
        assert!(model.detail_selection.is_some());
        assert!(model.response_selection.is_some());

        // Copier depuis la réponse ne lève, une fois la copie réussie, que
        // la sélection de la réponse.
        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("copie attendue");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(model.response_selection.is_none());
        assert!(model.detail_selection.is_some());
    }

    #[test]
    fn changing_selection_clears_both_detail_and_response_view_state() {
        let mut model = runner_probe_model();
        select(&mut model, "green.bru");
        update(&mut model, Message::NextFocus); // Détail
        model.detail_scroll = 2;
        model.detail_match = Some((2, 0..1));
        update(&mut model, Message::NextFocus); // Réponse
        model.response_scroll = 3;
        model.response_match = Some((3, 0..1));

        update(&mut model, Message::FocusTree);
        assert_eq!(
            model.focus,
            Focus::Tree,
            "aucune sélection à annuler d'abord"
        );
        update(&mut model, Message::Down);

        assert_eq!(model.detail_scroll, 0);
        assert!(model.detail_selection.is_none());
        assert!(model.detail_match.is_none());
        assert_eq!(model.response_scroll, 0);
        assert!(model.response_selection.is_none());
        assert!(model.response_match.is_none());
    }

    #[test]
    fn clipboard_success_and_failure_update_the_status() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("CopyToClipboard attendu");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(matches!(model.last_status, Some(StatusMessage::Copied)));

        let Command::CopyToClipboard { token, .. } = update(&mut model, Message::Yank) else {
            panic!("CopyToClipboard attendu");
        };
        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Err(crate::app::clipboard::test_support::error_for_test()),
            },
        );
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::ClipboardError(_))
        ));
    }

    #[test]
    fn a_stale_clipboard_result_is_ignored() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        let Command::CopyToClipboard { token: first, .. } = update(&mut model, Message::Yank)
        else {
            panic!("CopyToClipboard attendu");
        };
        let Command::CopyToClipboard { .. } = update(&mut model, Message::Yank) else {
            panic!("CopyToClipboard attendu");
        };
        // Le résultat de la première copie arrive après la seconde : il
        // doit être ignoré, `pending_clipboard_token` pointe déjà sur la
        // seconde.
        update(
            &mut model,
            Message::ClipboardResult {
                token: first,
                result: Ok(()),
            },
        );
        assert!(model.last_status.is_none());
        assert!(model.pending_clipboard_token.is_some());
    }

    #[test]
    fn copy_curl_in_response_panel() {
        let mut model = runner_probe_model();
        select(&mut model, "ok.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse
        assert_eq!(model.focus, Focus::Response);

        // 'c' (Message::Rename) copie la commande curl masquée
        let cmd = update(&mut model, Message::Rename);
        let Command::CopyToClipboard { token, text } = cmd else {
            panic!("CopyToClipboard attendu");
        };
        assert!(text.starts_with("curl -X GET 'http://127.0.0.1:18765/index.html'"));

        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::CopiedCurl { masked: true })
        ));
    }

    #[test]
    fn copy_curl_unmasked_requires_confirmation_when_secrets_present() {
        use serde_json::json;
        use std::collections::BTreeMap;

        let mut model = runner_probe_model();
        select(&mut model, "ok.bru");
        update(&mut model, Message::NextFocus); // Détail
        update(&mut model, Message::NextFocus); // Réponse

        // Injecter un en-tête d'auth dans le résultat
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), json!("Bearer mon-secret"));
        let outcome = model
            .run
            .outcomes
            .get_mut(std::path::Path::new("ok.bru"))
            .expect("outcome ok.bru présent");
        outcome.result.request.headers = Some(headers);

        // 'u' demande confirmation
        let cmd = update(&mut model, Message::CopyCurlFull);
        assert!(matches!(cmd, Command::None));
        assert_eq!(model.confirm, Some(PendingConfirm::CopyUnmaskedCurl));

        // 'y' confirme la copie démasquée
        let cmd = update(&mut model, Message::ConfirmYes);
        let Command::CopyToClipboard { token, text } = cmd else {
            panic!("CopyToClipboard attendu");
        };
        assert!(text.contains("Bearer mon-secret"));
        assert!(!text.contains("********"));

        update(
            &mut model,
            Message::ClipboardResult {
                token,
                result: Ok(()),
            },
        );
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::CopiedCurl { masked: false })
        ));
    }

    #[test]
    fn copy_curl_without_result_shows_status_message() {
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Response;

        let cmd = update(&mut model, Message::Rename);
        assert!(matches!(cmd, Command::None));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoCurlResult)
        ));

        let cmd = update(&mut model, Message::CopyCurlFull);
        assert!(matches!(cmd, Command::None));
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::NoCurlResult)
        ));
    }

    #[test]
    fn test_export_response_body_trigger_and_navigation() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");

        // 1. En focus Tree, Message::ExportResponseBody ne fait rien
        model.focus = Focus::Tree;
        let cmd = update(&mut model, Message::ExportResponseBody);
        assert!(matches!(cmd, Command::None));
        assert!(model.export_input.is_none());

        // 2. En focus Response sans résultat d'exécution -> StatusMessage::NoResponseBody
        model.focus = Focus::Response;
        let cmd = update(&mut model, Message::ExportResponseBody);
        assert!(matches!(cmd, Command::None));
        assert_eq!(model.last_status, Some(StatusMessage::NoResponseBody));
        assert!(model.export_input.is_none());

        // 3. Avec résultat d'exécution et corps JSON
        let mut result = sample_request_result("post-json.bru");
        result.response.data = serde_json::json!({"status": "ok"});
        model.run.outcomes.insert(
            PathBuf::from("post-json.bru"),
            crate::app::model::RequestOutcome::new(result, Some(1)),
        );

        let cmd = update(&mut model, Message::ExportResponseBody);
        assert!(matches!(cmd, Command::None));
        assert!(model.export_input.is_some());
        let initial_text = model.export_input.as_ref().unwrap().input.text().to_owned();
        assert!(initial_text.ends_with(".json"));

        // 4. Saisie interactive
        update(&mut model, Message::ExportPathInput('x'));
        assert_eq!(
            model.export_input.as_ref().unwrap().input.text(),
            format!("{initial_text}x")
        );
        update(&mut model, Message::ExportPathBackspace);
        assert_eq!(
            model.export_input.as_ref().unwrap().input.text(),
            initial_text
        );

        // 5. Annulation
        update(&mut model, Message::CancelExportPath);
        assert!(model.export_input.is_none());
    }

    #[test]
    fn test_export_response_body_confirm_flow() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        model.focus = Focus::Response;

        let mut result = sample_request_result("post-json.bru");
        result.response.data = serde_json::json!({"message": "bonjour"});
        model.run.outcomes.insert(
            PathBuf::from("post-json.bru"),
            crate::app::model::RequestOutcome::new(result, Some(1)),
        );

        // Démarre l'export
        update(&mut model, Message::ExportResponseBody);
        assert!(model.export_input.is_some());

        // Chemin non existant
        let temp_dir = std::env::temp_dir();
        let non_existing_path = temp_dir.join("bruno_test_export_inexistant_12345.json");
        if non_existing_path.exists() {
            let _ = std::fs::remove_file(&non_existing_path);
        }

        // On remplace le texte par le chemin non existant
        model.export_input.as_mut().unwrap().input =
            TextInput::new(non_existing_path.to_str().unwrap(), false);

        let cmd = update(&mut model, Message::ConfirmExportPath);
        assert!(model.export_input.is_none());
        match cmd {
            Command::ExportResponseBody { path, bytes } => {
                assert_eq!(path, non_existing_path);
                assert!(String::from_utf8(bytes).unwrap().contains("bonjour"));
            }
            other => panic!("Command::ExportResponseBody attendu, obtenu {other:?}"),
        }

        // Cas avec fichier existant : confirmation d'écrasement
        let existing_path = temp_dir.join("bruno_test_export_existant_12345.json");
        std::fs::write(&existing_path, b"deja present").unwrap();

        update(&mut model, Message::ExportResponseBody);
        model.export_input.as_mut().unwrap().input =
            TextInput::new(existing_path.to_str().unwrap(), false);

        let cmd = update(&mut model, Message::ConfirmExportPath);
        assert!(matches!(cmd, Command::None));
        assert!(model.export_input.is_none());
        assert!(matches!(
            model.confirm,
            Some(PendingConfirm::OverwriteExport { .. })
        ));

        // Refus (ConfirmNo)
        let cmd = update(&mut model, Message::ConfirmNo);
        assert!(matches!(cmd, Command::None));
        assert!(model.confirm.is_none());

        // Réessai et acceptation (ConfirmYes)
        update(&mut model, Message::ExportResponseBody);
        model.export_input.as_mut().unwrap().input =
            TextInput::new(existing_path.to_str().unwrap(), false);
        update(&mut model, Message::ConfirmExportPath);
        let cmd = update(&mut model, Message::ConfirmYes);
        assert!(model.confirm.is_none());
        match cmd {
            Command::ExportResponseBody { path, .. } => {
                assert_eq!(path, existing_path);
            }
            other => panic!("Command::ExportResponseBody attendu, obtenu {other:?}"),
        }

        let _ = std::fs::remove_file(&existing_path);
    }

    #[test]
    fn test_export_response_body_async_result() {
        let mut model = loaded_model((100, 30));
        let path = PathBuf::from("/tmp/test_export.json");

        // Résultat OK
        update(
            &mut model,
            Message::ResponseBodyExported {
                path: path.clone(),
                result: Ok(()),
            },
        );
        assert_eq!(
            model.last_status,
            Some(StatusMessage::ResponseExported(path.clone()))
        );

        // Résultat Erreur
        update(
            &mut model,
            Message::ResponseBodyExported {
                path,
                result: Err("permission non accordée".into()),
            },
        );
        assert_eq!(
            model.last_status,
            Some(StatusMessage::ExportError("permission non accordée".into()))
        );
    }
}
