//! Session d'édition des requêtes, dossiers et collections (`improve-direct-editing`, `add-entry-management`).

use ratatui::text::Line;

use crate::app::message::InputKey;
use crate::app::model::{
    EditSession, EditState, EditableField, Focus, InputTarget, Model, SavedEdit, StatusMessage,
    field_enabled, field_value, field_value_committed, section_entries,
};
use crate::app::text_input::TextInput;
use crate::app::update::Command;
use crate::app::update::output::resolve_editor;
use crate::app::update::tree::read_only_collection;
use crate::app::view::detail::detail_text_fields_boxes;
use crate::app::view::{inner, layout_for};
use crate::collection::TreeNode;
use crate::writer::{EntrySection, FieldEdit, FileStamp};

/// Ouvre une session d'édition sur la requête, le dossier ou la collection sélectionné,
/// dans l'état Sélection de champ.
pub(crate) fn start_edit(model: &mut Model) {
    if model.focus != Focus::Detail || model.editing.is_some() {
        return;
    }
    if read_only_collection(model) {
        model.last_status = Some(StatusMessage::ReadOnlyCollection);
        return;
    }
    let Some(collection) = model.loaded() else {
        return;
    };
    let root = &collection.root;
    let (path, view) = if model.selected_collection {
        let path = std::path::PathBuf::from("collection.bru");
        let Some(Ok(meta)) = collection.settings.as_ref() else {
            return;
        };
        (path, meta.to_request_view())
    } else {
        match model.selected_node() {
            Some(TreeNode::Request(request)) => {
                if let Some(kind) = request.view.non_http_label() {
                    model.last_status = Some(StatusMessage::ReadOnlyRequestType(kind));
                    return;
                }
                (request.path.clone(), request.view.clone())
            }
            Some(TreeNode::Folder(folder)) => {
                let path = folder.path.join("folder.bru");
                let Some(Ok(meta)) = folder.meta.as_ref() else {
                    return;
                };
                (path, meta.to_request_view())
            }
            _ => return,
        }
    };
    let full_path = root.join(&path);
    let Ok(stamp) = FileStamp::capture(&full_path) else {
        return;
    };
    model.editing = Some(EditSession::new(path, stamp, view));
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// `Entrée` dans le détail : ouvre la session, ou commence la saisie du
/// champ sous le curseur (`improve-direct-editing`, D3), ou ouvre l'éditeur
/// externe pour un bloc de texte.
pub(crate) fn enter_in_detail(model: &mut Model) -> Command {
    match &model.editing {
        None => {
            start_edit(model);
            Command::None
        }
        Some(session) if session.state == EditState::FieldSelect => begin_input(model),
        Some(session) if matches!(session.state, EditState::MethodPicker { .. }) => {
            confirm_method_picker(model);
            Command::None
        }
        Some(_) => Command::None,
    }
}

/// Déplace la présélection du sélecteur de méthode, bornée aux
/// extrémités de `ast::METHODS` (`add-method-editing`).
pub(crate) fn move_method_picker(model: &mut Model, delta: i8) {
    let Some(session) = &mut model.editing else {
        return;
    };
    let EditState::MethodPicker { selected } = &mut session.state else {
        return;
    };
    let count = crate::collection::ast::METHODS.len();
    if delta < 0 {
        *selected = selected.saturating_sub(1);
    } else if delta > 0 {
        *selected = (*selected + 1).min(count - 1);
    }
}

/// Valide la présélection du sélecteur de méthode : applique le
/// changement si elle diffère de la méthode courante, puis referme le
/// sélecteur vers l'état Sélection de champ (`add-method-editing`).
pub(crate) fn confirm_method_picker(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    let EditState::MethodPicker { selected } = session.state else {
        return;
    };
    let Some(&method) = crate::collection::ast::METHODS.get(selected) else {
        return;
    };
    if !method.eq_ignore_ascii_case(&session.preview.method) {
        try_commit(model, FieldEdit::Method(method.to_owned()));
    }
    close_input(model);
}

/// Déplace le curseur de champ en sélection de champ.
pub(crate) fn move_field_cursor(model: &mut Model, delta: i8) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let count = session.fields.len();
    if count == 0 {
        return;
    }
    if delta < 0 {
        session.cursor = session.cursor.saturating_sub(delta.unsigned_abs() as usize);
    } else if delta > 0 {
        session.cursor = (session.cursor + delta as usize).min(count - 1);
    }
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Bascule l'état activé/désactivé de l'en-tête ou du paramètre courant.
pub(crate) fn toggle_field(model: &mut Model) {
    let Some(session) = &model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let Some(field) = session.current_field() else {
        return;
    };
    let enabled = !field_enabled(session, &field);
    let edit = match field {
        EditableField::HeaderValue(index) => FieldEdit::HeaderEnabled { index, enabled },
        EditableField::QueryParamValue(index) => FieldEdit::QueryParamEnabled { index, enabled },
        EditableField::PathParamValue(index) => FieldEdit::PathParamEnabled { index, enabled },
        EditableField::VarsPreRequestValue(index) => {
            FieldEdit::VarsPreRequestEnabled { index, enabled }
        }
        EditableField::VarsPostResponseValue(index) => {
            FieldEdit::VarsPostResponseEnabled { index, enabled }
        }
        EditableField::AssertValue(index) => FieldEdit::AssertEnabled { index, enabled },
        EditableField::SettingsValue(_)
        | EditableField::Method
        | EditableField::Url
        | EditableField::AddRow(_)
        | EditableField::BodyText
        | EditableField::ScriptPreRequest
        | EditableField::ScriptPostResponse
        | EditableField::Tests
        | EditableField::Docs => return,
    };
    try_commit(model, edit);
}

/// `Entrée` en sélection de champ : saisie de la valeur du champ sous le
/// curseur, ajout d'une entrée sur une ligne « + Ajouter », ou ouverture
/// de l'éditeur externe pour un bloc de texte.
pub(crate) fn begin_input(model: &mut Model) -> Command {
    let Some(session) = &model.editing else {
        return Command::None;
    };
    if session.state != EditState::FieldSelect {
        return Command::None;
    }
    let Some(field) = session.current_field() else {
        return Command::None;
    };
    match field {
        EditableField::AddRow(section) => {
            start_add(model, section);
            Command::None
        }
        EditableField::Method => {
            open_method_picker(model);
            Command::None
        }
        EditableField::ScriptPreRequest
        | EditableField::ScriptPostResponse
        | EditableField::Tests
        | EditableField::Docs => {
            let extension = match field {
                EditableField::Docs => "md",
                _ => "js",
            };
            let text = field_value_committed(session, &field).to_string();
            let program = resolve_editor(model.editor.as_deref());
            Command::EditTextInEditor {
                program,
                text,
                extension: extension.to_string(),
                field,
            }
        }
        _ => {
            let input = TextInput::new(
                field_value(session, &field),
                field == EditableField::BodyText,
            );
            open_input(model, input, InputTarget::Field);
            Command::None
        }
    }
}

/// Passe la session en saisie avec `target`.
pub(crate) fn open_input(model: &mut Model, input: TextInput, target: InputTarget) {
    if let Some(session) = &mut model.editing {
        session.state = EditState::Input(input);
        session.target = target;
        session.hscroll = 0;
    }
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Ouvre le sélecteur de méthode, la méthode courante de la session
/// présélectionnée (`add-method-editing`).
pub(crate) fn open_method_picker(model: &mut Model) {
    let Some(session) = &mut model.editing else {
        return;
    };
    let current = session.preview.method.as_str();
    let selected = crate::collection::ast::METHODS
        .iter()
        .position(|candidate| current.eq_ignore_ascii_case(candidate))
        .unwrap_or(0);
    session.state = EditState::MethodPicker { selected };
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Revient à la sélection de champ, sans rien valider.
pub(crate) fn close_input(model: &mut Model) {
    if let Some(session) = &mut model.editing {
        session.state = EditState::FieldSelect;
        session.target = InputTarget::Field;
        session.hscroll = 0;
    }
    scroll_edit_into_view(model);
}

/// `a` en sélection de champ, ou `Entrée` sur une ligne d'ajout : commence
/// la saisie de la clé d'une nouvelle entrée de `section`. Le curseur de
/// champ se place sur la ligne d'ajout, où s'affiche l'entrée provisoire.
pub(crate) fn start_add(model: &mut Model, section: EntrySection) {
    let Some(session) = &mut model.editing else {
        return;
    };
    if session.state != EditState::FieldSelect {
        return;
    }
    let return_cursor = session.cursor;
    if let Some(position) = session
        .fields
        .iter()
        .position(|f| *f == EditableField::AddRow(section))
    {
        session.cursor = position;
    }
    open_input(
        model,
        TextInput::new("", false),
        InputTarget::NewKey {
            section,
            return_cursor,
        },
    );
}

/// `a` : ajout dans la section de l'entrée ou de la ligne d'ajout sous le
/// curseur ; sans effet sur l'URL et le corps.
pub(crate) fn add_entry(model: &mut Model) {
    let Some(section) = model
        .editing
        .as_ref()
        .filter(|s| s.state == EditState::FieldSelect)
        .and_then(EditSession::current_field)
        .and_then(|field| field.section())
    else {
        return;
    };
    if section == EntrySection::Settings {
        return;
    }
    start_add(model, section);
}

/// `d` : supprime l'entrée sous le curseur, sans confirmation ; le curseur
/// garde son indice, borné à la dernière position.
pub(crate) fn delete_entry(model: &mut Model) {
    let Some((section, index)) = model
        .editing
        .as_ref()
        .filter(|s| s.state == EditState::FieldSelect)
        .and_then(EditSession::current_field)
        .and_then(|field| field.entry())
    else {
        return;
    };
    if section == EntrySection::Settings {
        return;
    }
    try_commit(model, FieldEdit::RemoveEntry { section, index });
    scroll_edit_into_view(model);
}

/// `c` : commence la saisie de la clé de l'entrée sous le curseur,
/// pré-remplie avec la clé actuelle.
pub(crate) fn start_rename(model: &mut Model) {
    let Some(session) = model
        .editing
        .as_ref()
        .filter(|s| s.state == EditState::FieldSelect)
    else {
        return;
    };
    let Some((section, index)) = session.current_field().and_then(|field| field.entry()) else {
        return;
    };
    if section == EntrySection::Settings {
        return;
    }
    let Some(key) = section_entries(&session.preview, section)
        .get(index)
        .map(|kv| kv.key.clone())
    else {
        return;
    };
    open_input(
        model,
        TextInput::new(&key, false),
        InputTarget::RenameKey { section, index },
    );
}

/// Issue d'une validation de saisie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Validation {
    /// Aucune saisie en cours, ou saisie validée : retour en sélection.
    Done,
    /// Clé d'ajout validée : la saisie continue sur la valeur.
    Continued,
    /// Modification refusée : la saisie reste ouverte.
    Refused,
}

/// Valide la saisie en cours (`Tab`, `Entrée` sur une ligne, `Ctrl+S`),
/// selon sa cible. `key_only` : à l'étape de la clé d'un ajout, ajoute
/// l'entrée avec une valeur vide au lieu de passer à la valeur.
pub(crate) fn validate_input(model: &mut Model, key_only: bool) -> Validation {
    let Some(session) = &model.editing else {
        return Validation::Done;
    };
    let EditState::Input(input) = &session.state else {
        return Validation::Done;
    };
    let text = input.text().to_owned();
    match session.target.clone() {
        InputTarget::Field => {
            let Some(field) = session.current_field() else {
                close_input(model);
                return Validation::Done;
            };
            if text != field_value_committed(session, &field)
                && let Some(edit) = value_edit(field, text)
                && !try_commit(model, edit)
            {
                return Validation::Refused;
            }
            close_input(model);
            Validation::Done
        }
        InputTarget::NewKey {
            section,
            return_cursor,
        } => {
            let add = FieldEdit::AddEntry {
                section,
                key: text.clone(),
                value: String::new(),
                enabled: true,
            };
            if key_only {
                if !try_commit(model, add) {
                    return Validation::Refused;
                }
                finish_add(model, section);
                return Validation::Done;
            }
            if let Err(error) = check_edit(model, add) {
                refuse(model, &error);
                return Validation::Refused;
            }
            open_input(
                model,
                TextInput::new("", false),
                InputTarget::NewValue {
                    section,
                    key: text,
                    return_cursor,
                },
            );
            Validation::Continued
        }
        InputTarget::NewValue { section, key, .. } => {
            let add = FieldEdit::AddEntry {
                section,
                key,
                value: text,
                enabled: true,
            };
            if !try_commit(model, add) {
                return Validation::Refused;
            }
            finish_add(model, section);
            Validation::Done
        }
        InputTarget::RenameKey { section, index } => {
            let unchanged = section_entries(&session.preview, section)
                .get(index)
                .is_some_and(|kv| kv.key == text);
            if !unchanged
                && !try_commit(
                    model,
                    FieldEdit::RenameKey {
                        section,
                        index,
                        key: text,
                    },
                )
            {
                return Validation::Refused;
            }
            close_input(model);
            Validation::Done
        }
    }
}

/// Termine un ajout réussi : sélection de champ, curseur sur la nouvelle
/// entrée (la dernière de sa section).
pub(crate) fn finish_add(model: &mut Model, section: EntrySection) {
    if let Some(session) = &mut model.editing {
        let count = section_entries(&session.preview, section).len();
        let field = match section {
            EntrySection::Headers => EditableField::HeaderValue(count.saturating_sub(1)),
            EntrySection::QueryParams => EditableField::QueryParamValue(count.saturating_sub(1)),
            EntrySection::PathParams => EditableField::PathParamValue(count.saturating_sub(1)),
            EntrySection::VarsPreRequest => {
                EditableField::VarsPreRequestValue(count.saturating_sub(1))
            }
            EntrySection::VarsPostResponse => {
                EditableField::VarsPostResponseValue(count.saturating_sub(1))
            }
            EntrySection::Assert => EditableField::AssertValue(count.saturating_sub(1)),
            EntrySection::Settings => EditableField::SettingsValue(count.saturating_sub(1)),
        };
        if let Some(position) = session.fields.iter().position(|f| *f == field) {
            session.cursor = position;
        }
    }
    close_input(model);
}

/// Modification de la valeur d'un champ.
pub(crate) fn value_edit(field: EditableField, value: String) -> Option<FieldEdit> {
    Some(match field {
        EditableField::Url => FieldEdit::Url(value),
        EditableField::HeaderValue(index) => FieldEdit::HeaderValue { index, value },
        EditableField::QueryParamValue(index) => FieldEdit::QueryParamValue { index, value },
        EditableField::PathParamValue(index) => FieldEdit::PathParamValue { index, value },
        EditableField::VarsPreRequestValue(index) => {
            FieldEdit::VarsPreRequestValue { index, value }
        }
        EditableField::VarsPostResponseValue(index) => {
            FieldEdit::VarsPostResponseValue { index, value }
        }
        EditableField::AssertValue(index) => FieldEdit::AssertValue { index, value },
        EditableField::SettingsValue(index) => FieldEdit::SettingsValue { index, value },
        EditableField::BodyText => FieldEdit::BodyText(value),
        EditableField::ScriptPreRequest => FieldEdit::ScriptPreRequest(value),
        EditableField::ScriptPostResponse => FieldEdit::ScriptPostResponse(value),
        EditableField::Tests => FieldEdit::Tests(value),
        EditableField::Docs => FieldEdit::Docs(value),
        EditableField::Method | EditableField::AddRow(_) => return None,
    })
}

/// Annule la saisie en cours : `pending` n'est pas touché, le champ
/// reprend la valeur qu'il avait au début de la saisie ; un ajout est
/// abandonné en entier et le curseur revient d'où il était parti.
pub(crate) fn cancel_input(model: &mut Model) {
    if let Some(session) = &mut model.environment_editing {
        session.cancel_input();
        model.last_status = None;
        return;
    }
    let Some(session) = &mut model.editing else {
        return;
    };
    match session.state {
        EditState::Input(_) => {
            if let InputTarget::NewKey { return_cursor, .. }
            | InputTarget::NewValue { return_cursor, .. } = session.target
            {
                session.cursor = return_cursor.min(session.fields.len().saturating_sub(1));
            }
        }
        EditState::MethodPicker { .. } => {}
        EditState::FieldSelect => return,
    }
    model.last_status = None;
    close_input(model);
}

/// Touche d'édition du tampon pendant une saisie.
pub(crate) fn input_key(model: &mut Model, key: InputKey) {
    if model.environment_editing.is_some() {
        super::environment_input_key(model, key);
        return;
    }
    let Some(session) = &mut model.editing else {
        return;
    };
    let EditState::Input(input) = &mut session.state else {
        return;
    };
    match key {
        InputKey::Char(c) => input.insert(c),
        InputKey::Backspace => input.backspace(),
        InputKey::Delete => input.delete(),
        InputKey::Left => input.left(),
        InputKey::Right => input.right(),
        InputKey::Up => input.up(),
        InputKey::Down => input.down(),
        InputKey::Home => input.home(),
        InputKey::End => input.end(),
        InputKey::Enter if input.is_multiline() => input.newline(),
        InputKey::Enter => {
            validate_input(model, false);
            return;
        }
    }
    model.last_status = None;
    scroll_edit_into_view(model);
}

/// Journal de la session augmenté de `edit`, avec une seule coalescence :
/// deux modifications de valeur consécutives du même champ.
pub(crate) fn with_edit(pending: &[FieldEdit], edit: FieldEdit) -> Vec<FieldEdit> {
    let mut edits = pending.to_vec();
    let same_value_target = match (edits.last(), &edit) {
        (Some(FieldEdit::Url(_)), FieldEdit::Url(_))
        | (Some(FieldEdit::BodyText(_)), FieldEdit::BodyText(_))
        | (Some(FieldEdit::ScriptPreRequest(_)), FieldEdit::ScriptPreRequest(_))
        | (Some(FieldEdit::ScriptPostResponse(_)), FieldEdit::ScriptPostResponse(_))
        | (Some(FieldEdit::Tests(_)), FieldEdit::Tests(_))
        | (Some(FieldEdit::Docs(_)), FieldEdit::Docs(_)) => true,
        (
            Some(FieldEdit::HeaderValue { index: a, .. }),
            FieldEdit::HeaderValue { index: b, .. },
        )
        | (
            Some(FieldEdit::QueryParamValue { index: a, .. }),
            FieldEdit::QueryParamValue { index: b, .. },
        )
        | (
            Some(FieldEdit::PathParamValue { index: a, .. }),
            FieldEdit::PathParamValue { index: b, .. },
        ) => a == b,
        _ => false,
    };
    if same_value_target {
        edits.pop();
    }
    edits.push(edit);
    edits
}

/// Retourne l'AST du fichier actuellement édité (`request`, `folder.bru` ou `collection.bru`),
/// si la sélection et le chemin correspondent à la session.
pub(crate) fn current_editing_ast<'a>(
    model: &'a Model,
    session_path: &std::path::Path,
) -> Option<&'a crate::collection::BruFile> {
    if model.selected_collection {
        if session_path == std::path::Path::new("collection.bru") {
            return model
                .loaded()?
                .settings
                .as_ref()?
                .as_ref()
                .ok()?
                .ast
                .as_ref();
        }
    } else {
        match model.selected_node() {
            Some(TreeNode::Request(request)) if request.path == session_path => {
                return request.ast.as_ref();
            }
            Some(TreeNode::Folder(folder)) if folder.path.join("folder.bru") == session_path => {
                return folder.meta.as_ref()?.as_ref().ok()?.ast.as_ref();
            }
            _ => {}
        }
    }
    None
}

/// Aperçu de la requête éditée avec `edit` ajouté au journal.
pub(crate) fn check_edit(
    model: &Model,
    edit: FieldEdit,
) -> Result<(Vec<FieldEdit>, crate::collection::RequestView), crate::writer::EditError> {
    let session = model
        .editing
        .as_ref()
        .ok_or(crate::writer::EditError::Unreadable)?;
    let ast =
        current_editing_ast(model, &session.path).ok_or(crate::writer::EditError::Unreadable)?;
    let edits = with_edit(&session.pending, edit);
    let view = crate::writer::preview(ast, &edits)?;
    Ok((edits, view))
}

/// Affiche le refus d'une modification, sans jamais citer le texte saisi.
pub(crate) fn refuse(model: &mut Model, error: &crate::writer::EditError) {
    model.last_status = Some(StatusMessage::EditRefused(error.to_string()));
}

/// Ajoute `edit` au journal s'il est accepté par `bru-writer` : aperçu et
/// positions recalculés, session marquée modifiée. Sinon, message de refus
/// et journal inchangé.
pub(crate) fn try_commit(model: &mut Model, edit: FieldEdit) -> bool {
    match check_edit(model, edit) {
        Ok((edits, view)) => {
            if let Some(session) = &mut model.editing {
                session.pending = edits;
                session.set_preview(view);
                session.dirty = true;
            }
            model.last_status = None;
            true
        }
        Err(error) => {
            refuse(model, &error);
            false
        }
    }
}

/// Fait défiler le détail pour que le champ sous le curseur (en sélection)
/// ou la ligne du curseur de texte (en saisie) soit visible, et ajuste le
/// décalage horizontal de la valeur en saisie (`improve-direct-editing`,
/// D7).
pub(crate) fn scroll_edit_into_view(model: &mut Model) {
    let Some(areas) = layout_for(model.size, model.zoomed_panel()) else {
        return;
    };
    let area = inner(areas.detail);
    let (height, width) = (area.height.max(1), area.width);
    let Some(session) = &model.editing else {
        return;
    };
    let Some(field) = session.current_field() else {
        return;
    };
    let (_, field_lines, _) = detail_text_fields_boxes(model);
    let Some(location) = field_lines.iter().find(|l| l.field == field) else {
        return;
    };
    let (target_line, hscroll) = match &session.state {
        EditState::FieldSelect | EditState::MethodPicker { .. } => (location.line, 0),
        EditState::Input(input) => {
            let (line, _) = input.cursor_line_col();
            let column = u16::try_from(Line::raw(input.text_before_cursor_on_line()).width())
                .unwrap_or(u16::MAX);
            let available = width
                .saturating_sub(u16::try_from(location.prefix_width).unwrap_or(u16::MAX))
                .max(1);
            let mut hscroll = session.hscroll;
            if column < hscroll {
                hscroll = column;
            } else if column >= hscroll.saturating_add(available) {
                hscroll = column - available + 1;
            }
            (location.line + line, hscroll)
        }
    };
    let target = u16::try_from(target_line).unwrap_or(u16::MAX);
    let mut scroll = model.detail_scroll;
    if target < scroll {
        scroll = target;
    } else if target >= scroll.saturating_add(height) {
        scroll = target - height + 1;
    }
    model.detail_scroll = scroll;
    if let Some(session) = &mut model.editing {
        session.hscroll = hscroll;
    }
}

/// Une session porte des modifications non enregistrées : tout changement
/// de requête sélectionnée est refusé (`improve-direct-editing`, D6).
pub(crate) fn selection_locked(model: &Model) -> bool {
    model.editing.as_ref().is_some_and(EditSession::has_unsaved)
}

/// Déclenche la sauvegarde des modifications si la session est modifiée (D7).
pub(crate) fn save_edit(model: &mut Model) -> Command {
    let Some(session) = &model.editing else {
        return Command::None;
    };
    if !session.dirty {
        return Command::None;
    }
    let Some(ast) = current_editing_ast(model, &session.path) else {
        return Command::None;
    };
    Command::SaveEdit {
        path: session.path.clone(),
        ast: ast.clone(),
        stamp: session.stamp,
        edits: session.pending.clone(),
    }
}

/// Applique le résultat d'une sauvegarde (D7).
pub(crate) fn edit_saved(
    model: &mut Model,
    path: std::path::PathBuf,
    result: Result<SavedEdit, crate::writer::WriteError>,
) {
    let Some(session) = &mut model.editing else {
        return;
    };
    // Garde contre un événement obsolète (chemin ne correspondant plus à la session active)
    if session.path != path {
        return;
    }
    match result {
        Ok(saved) => {
            session.stamp = saved.stamp;
            session.pending.clear();
            session.dirty = false;
            session.set_preview(saved.view.clone());
            model.last_saved_stamps.insert(path.clone(), saved.stamp);
            model.replace_file_meta_or_request_node(&path, saved.ast, saved.view);
        }
        Err(error) => {
            model.last_status = Some(StatusMessage::SaveError(error.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::super::*;
    use super::*;
    use crate::app::model::{
        EditState, EditableField, Exit, Focus, InputTarget, PendingConfirm, SavedEdit,
        StatusMessage,
    };
    use crate::app::test_support::{loaded_model, select};
    use crate::app::view::detail::request_text_and_fields;
    use crate::collection::{RequestView, TreeNode};
    use crate::writer::EntrySection;

    #[test]
    fn start_edit_opens_session_only_in_detail_on_request() {
        let mut model = loaded_model((100, 30));

        // 1. Sur une requête mais avec Focus::Tree : sans effet
        select(&mut model, "simple-get.bru");
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // 2. Sur un dossier sans folder.bru avec Focus::Detail : sans effet
        select(&mut model, "misc");
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // Sur un dossier avec folder.bru valide : ouverture
        select(&mut model, "grp");
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_some());
        assert_eq!(
            model.editing.as_ref().unwrap().path,
            std::path::PathBuf::from("grp/folder.bru")
        );
        model.editing = None;

        // 3. Sur un nœud en erreur avec Focus::Detail : sans effet
        select(&mut model, "broken.bru");
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());

        // 4. Sur une requête valide avec Focus::Detail : ouverture
        select(&mut model, "simple-get.bru");
        assert_eq!(model.focus, Focus::Detail);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_some());
        let session = model.editing.as_ref().unwrap();
        assert_eq!(session.path, std::path::PathBuf::from("simple-get.bru"));
        assert_eq!(session.cursor, 0);
        assert_eq!(session.state, EditState::FieldSelect);
        assert!(session.pending.is_empty());
        assert!(!session.dirty);
        assert!(!session.fields.is_empty());

        // 5. Si une session est déjà ouverte, StartEdit ne fait rien
        let stamp = session.stamp;
        update(&mut model, Message::StartEdit);
        assert_eq!(model.editing.as_ref().unwrap().stamp, stamp);

        // 6. Sur une requête valide avec Focus::Response : sans effet
        // (l'édition ne porte que sur le panneau Détail,
        // `split-request-response-panels`). Focus déjà sur Détail depuis
        // le cas 4 : un seul Tab suffit pour atteindre la réponse.
        model.editing = None;
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Response);
        update(&mut model, Message::StartEdit);
        assert!(model.editing.is_none());
    }

    #[test]
    fn move_field_cursor_navigation_and_bounds() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        let count = model.editing.as_ref().unwrap().fields.len();
        assert!(count >= 3, "au moins 3 champs attendus");

        // Curseur initial à 0
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);

        // Flèche haut sur la borne 0 : sans effet
        update(&mut model, Message::Up);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);

        // Parcours vers le bas
        for expected in 1..count {
            update(&mut model, Message::Down);
            assert_eq!(model.editing.as_ref().unwrap().cursor, expected);
        }

        // Flèche bas sur la dernière borne : sans effet
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 1);

        // Remontée vers le haut
        update(&mut model, Message::Up);
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 2);

        // Via Message::MoveFieldCursor direct
        update(&mut model, Message::MoveFieldCursor(1));
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 1);

        update(&mut model, Message::MoveFieldCursor(-1));
        assert_eq!(model.editing.as_ref().unwrap().cursor, count - 2);
    }

    #[test]
    fn toggle_field_toggles_header_and_params_and_marks_dirty() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "scripted.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Champ 0 : Méthode -> ToggleField sans effet
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);
        assert!(matches!(
            model.editing.as_ref().unwrap().fields[0],
            EditableField::Method
        ));
        update(&mut model, Message::ToggleField);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());
        assert!(!model.editing.as_ref().unwrap().dirty);

        // Champ 2 : En-tête Accept (initialement activé dans scripted.bru) ;
        // Méthode puis URL précèdent les en-têtes.
        update(&mut model, Message::Down);
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 2);
        assert!(matches!(
            model.editing.as_ref().unwrap().fields[2],
            EditableField::HeaderValue(0)
        ));

        assert!(field_enabled(
            model.editing.as_ref().unwrap(),
            &EditableField::HeaderValue(0)
        ));

        // Bascule : devient désactivé
        update(&mut model, Message::ToggleField);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);
        assert!(!field_enabled(
            model.editing.as_ref().unwrap(),
            &EditableField::HeaderValue(0)
        ));

        // Seconde bascule : redevient activé
        update(&mut model, Message::ToggleField);
        assert!(field_enabled(
            model.editing.as_ref().unwrap(),
            &EditableField::HeaderValue(0)
        ));

        // Test sur paramètre de requête : multiline.bru a des query params
        let mut model2 = loaded_model((100, 30));
        select(&mut model2, "multiline.bru");
        update(&mut model2, Message::NextFocus);
        update(&mut model2, Message::StartEdit);
        // Trouver le premier QueryParamValue
        let param_cursor = model2
            .editing
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|f| matches!(f, EditableField::QueryParamValue(_)))
            .expect("query param attendu");
        model2.editing.as_mut().unwrap().cursor = param_cursor;

        let param_field = model2.editing.as_ref().unwrap().fields[param_cursor];
        let init_enabled = field_enabled(model2.editing.as_ref().unwrap(), &param_field);
        update(&mut model2, Message::ToggleField);
        assert!(model2.editing.as_ref().unwrap().dirty);
        assert_eq!(
            field_enabled(model2.editing.as_ref().unwrap(), &param_field),
            !init_enabled
        );

        // Test sur corps : post-json.bru a un corps
        let mut model3 = loaded_model((100, 30));
        select(&mut model3, "post-json.bru");
        update(&mut model3, Message::NextFocus);
        update(&mut model3, Message::StartEdit);
        let body_cursor = model3
            .editing
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|f| matches!(f, EditableField::BodyText))
            .expect("corps attendu");
        model3.editing.as_mut().unwrap().cursor = body_cursor;
        update(&mut model3, Message::ToggleField);
        assert!(!model3.editing.as_ref().unwrap().dirty);
        assert!(model3.editing.as_ref().unwrap().pending.is_empty());
    }

    #[test]
    fn input_transitions_and_value_preservation() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Curseur sur Méthode par défaut ; on se place sur URL.
        assert_eq!(model.editing.as_ref().unwrap().cursor, 0);
        update(&mut model, Message::Down);
        assert_eq!(model.editing.as_ref().unwrap().cursor, 1);
        let initial_url =
            { field_value(model.editing.as_ref().unwrap(), &EditableField::Url).to_string() };
        let expected_len = initial_url.chars().count();

        // Début de la saisie (Entrée)
        update(&mut model, Message::Enter);
        match &model.editing.as_ref().unwrap().state {
            EditState::Input(input) => {
                assert_eq!(input.cursor(), expected_len);
                assert_eq!(input.text(), initial_url);
            }
            EditState::FieldSelect | EditState::MethodPicker { .. } => panic!("saisie attendue"),
        }

        // Validation sans frappe (Tab) : valeur conservée, pas de dirty
        update(&mut model, Message::ValidateInput);
        assert_eq!(
            model.editing.as_ref().unwrap().state,
            EditState::FieldSelect
        );
        assert!(!model.editing.as_ref().unwrap().dirty);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        assert_eq!(
            field_value(model.editing.as_ref().unwrap(), &EditableField::Url,),
            initial_url.as_str()
        );

        // Saisie puis validation par Entrée sur un champ à une ligne (URL)
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(
            model.editing.as_ref().unwrap().state,
            EditState::FieldSelect
        );
        assert!(!model.editing.as_ref().unwrap().dirty);
    }

    #[test]
    fn input_typing_cursor_movement_and_commit_at_validation() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        // 1. Aller-retour sans modification de valeur mais avec déplacement curseur : dirty inchangé
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Left));
        update(&mut model, Message::InputKey(InputKey::Right));
        update(&mut model, Message::ValidateInput);
        assert!(!model.editing.as_ref().unwrap().dirty);
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        // 2. Frappe, suppression, insertion en milieu de chaîne
        update(&mut model, Message::Enter);
        // Curseur initial à la fin de "https://{{host}}/ping"
        // Ajout d'un caractère à la fin
        update(&mut model, Message::InputKey(InputKey::Char('s')));
        // Déplacement à gauche
        update(&mut model, Message::InputKey(InputKey::Left));
        update(&mut model, Message::InputKey(InputKey::Left));
        // Insertion en milieu de chaîne
        update(&mut model, Message::InputKey(InputKey::Char('X')));
        // Retour arrière : supprime le X qu'on vient d'insérer
        update(&mut model, Message::InputKey(InputKey::Backspace));

        // Pendant la saisie, pending n'est PAS mis à jour à chaque frappe
        assert!(model.editing.as_ref().unwrap().pending.is_empty());

        // Validation de la saisie (Tab)
        update(&mut model, Message::ValidateInput);
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);

        assert_eq!(
            field_value(model.editing.as_ref().unwrap(), &EditableField::Url,),
            "https://{{host}}/pings"
        );
    }

    #[test]
    fn input_enter_on_body_inserts_newline() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "post-json.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        let body_cursor = model
            .editing
            .as_ref()
            .unwrap()
            .fields
            .iter()
            .position(|f| matches!(f, EditableField::BodyText))
            .expect("corps attendu");
        model.editing.as_mut().unwrap().cursor = body_cursor;

        // Début de la saisie du corps
        update(&mut model, Message::Enter);

        // Entrée insère un saut de ligne et la saisie continue
        update(&mut model, Message::InputKey(InputKey::Enter));
        match &model.editing.as_ref().unwrap().state {
            EditState::Input(input) => {
                assert!(input.text().ends_with('\n'), "saut de ligne inséré");
            }
            EditState::FieldSelect | EditState::MethodPicker { .. } => {
                panic!("devrait rester en saisie sur le corps")
            }
        }

        // Validation par Tab
        update(&mut model, Message::ValidateInput);
        assert_eq!(
            model.editing.as_ref().unwrap().state,
            EditState::FieldSelect
        );
        assert!(model.editing.as_ref().unwrap().dirty);
        assert_eq!(model.editing.as_ref().unwrap().pending.len(), 1);
    }

    #[test]
    fn save_edit_when_clean_returns_none_when_dirty_returns_command() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        // 1. Session propre -> Command::None
        let cmd = update(&mut model, Message::SaveEdit);
        assert!(matches!(cmd, Command::None));

        // 2. Modification -> session dirty
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);
        assert!(model.editing.as_ref().unwrap().dirty);

        // 3. Session modifiée -> Command::SaveEdit avec path, ast, stamp, edits
        let cmd = update(&mut model, Message::SaveEdit);
        match cmd {
            Command::SaveEdit {
                path,
                ast,
                stamp,
                edits,
            } => {
                assert_eq!(path, std::path::Path::new("simple-get.bru"));
                assert_eq!(stamp, model.editing.as_ref().unwrap().stamp);
                assert_eq!(edits, model.editing.as_ref().unwrap().pending);
                assert!(!edits.is_empty());
                // ast correspond bien à la requête
                let req_node = match model.selected_node().unwrap() {
                    TreeNode::Request(r) => r,
                    _ => panic!("requête attendue"),
                };
                assert_eq!(Some(&ast), req_node.ast.as_ref());
            }
            _ => panic!("Command::SaveEdit attendu"),
        }

        // L'émission de Command::SaveEdit ne vide PAS dirty ni pending
        assert!(model.editing.as_ref().unwrap().dirty);
        assert!(!model.editing.as_ref().unwrap().pending.is_empty());
    }

    #[test]
    fn edit_saved_success_updates_node_and_resets_dirty_and_pending() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // Modification
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);

        let initial_stamp = model.editing.as_ref().unwrap().stamp;
        let new_source = "get {\n  url: https://updated.example.com\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");
        let new_view = RequestView::from_ast(&new_ast).expect("view");

        // Simuler un nouveau FileStamp avec une date différente
        let new_stamp = {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/collections/writer-cases/headers.bru");
            crate::writer::FileStamp::capture(&path).expect("stamp")
        };

        let saved = SavedEdit {
            stamp: new_stamp,
            ast: new_ast.clone(),
            view: new_view.clone(),
        };

        let path = std::path::PathBuf::from("simple-get.bru");
        update(
            &mut model,
            Message::EditSaved {
                path,
                result: Ok(saved),
            },
        );

        let session = model.editing.as_ref().unwrap();
        assert!(!session.dirty);
        assert!(session.pending.is_empty());
        assert_eq!(session.stamp, new_stamp);
        assert_ne!(session.stamp, initial_stamp);

        // Le nœud dans le modèle est bien mis à jour
        let req_node = match model.selected_node().unwrap() {
            TreeNode::Request(r) => r,
            _ => panic!("requête attendue"),
        };
        assert_eq!(req_node.view.url, "https://updated.example.com");
        assert_eq!(req_node.ast.as_ref(), Some(&new_ast));
    }

    #[test]
    fn edit_saved_failure_retains_modifications_and_sets_status() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        // Modification
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);

        let pending_before = model.editing.as_ref().unwrap().pending.clone();
        let path = std::path::PathBuf::from("simple-get.bru");
        let error = crate::writer::WriteError::Stale { path: path.clone() };

        update(
            &mut model,
            Message::EditSaved {
                path,
                result: Err(error),
            },
        );

        // Session inchangée : dirty et pending conservés
        let session = model.editing.as_ref().unwrap();
        assert!(session.dirty);
        assert_eq!(session.pending, pending_before);

        // Message d'erreur dans last_status
        match &model.last_status {
            Some(StatusMessage::SaveError(msg)) => {
                assert!(msg.contains("modifié depuis son chargement"));
            }
            _ => panic!("StatusMessage::SaveError attendu"),
        }
    }

    #[test]
    fn edit_saved_stale_event_ignored() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL

        // Modification
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('x')));
        update(&mut model, Message::ValidateInput);

        let pending_before = model.editing.as_ref().unwrap().pending.clone();
        let stamp_before = model.editing.as_ref().unwrap().stamp;

        // Événement pour un autre chemin que la session active
        let other_path = std::path::PathBuf::from("other.bru");
        let new_source = "get {\n  url: https://other.example.com\n}\n";
        let new_ast = crate::collection::BruFile::parse(new_source.to_string()).expect("parse");
        let new_view = RequestView::from_ast(&new_ast).expect("view");
        let saved = SavedEdit {
            stamp: stamp_before,
            ast: new_ast,
            view: new_view,
        };

        update(
            &mut model,
            Message::EditSaved {
                path: other_path,
                result: Ok(saved),
            },
        );

        // Session inchangée
        let session = model.editing.as_ref().unwrap();
        assert!(session.dirty);
        assert_eq!(session.pending, pending_before);
        assert_eq!(session.stamp, stamp_before);
        assert!(model.last_status.is_none());
    }

    #[test]
    fn escape_in_session_closes_immediately_if_clean_and_asks_confirm_if_dirty() {
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);

        // 1. Session non modifiée (!dirty) -> Échap (FocusTree) ferme immédiatement la session
        assert!(model.editing.is_some());
        assert!(!model.editing.as_ref().unwrap().dirty);
        update(&mut model, Message::FocusTree);
        assert!(model.editing.is_none());
        assert!(model.confirm.is_none());

        // 2. Session modifiée (dirty) -> Échap (FocusTree) demande confirmation DiscardEdit
        update(&mut model, Message::StartEdit);
        update(&mut model, Message::Down); // Méthode -> URL
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('a')));
        update(&mut model, Message::ValidateInput);
        assert!(model.editing.as_ref().unwrap().dirty);

        update(&mut model, Message::FocusTree);
        assert_eq!(model.confirm, Some(PendingConfirm::DiscardEdit));
        assert!(model.editing.is_some(), "session toujours active");

        // Refus de confirmation -> session reste intacte
        update(&mut model, Message::ConfirmNo);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_some());
        assert!(model.editing.as_ref().unwrap().dirty);

        // Nouvelle tentative d'Échap puis acceptation
        update(&mut model, Message::FocusTree);
        assert_eq!(model.confirm, Some(PendingConfirm::DiscardEdit));
        update(&mut model, Message::ConfirmYes);
        assert!(model.confirm.is_none());
        assert!(model.editing.is_none(), "session fermée après confirmation");
    }

    #[test]
    fn quit_and_force_quit_ask_confirm_if_dirty_and_exit_directly_otherwise() {
        let mut model = loaded_model((100, 30));

        // 1. Sans session d'édition : Quit ferme directement
        update(&mut model, Message::Quit);
        assert!(matches!(model.exit, Some(Exit::Normal)));
        assert!(model.confirm.is_none());

        // Réinitialisation de exit
        model.exit = None;

        // 2. Session ouverte propre (!dirty) : Quit ferme directement
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        assert!(!model.editing.as_ref().unwrap().dirty);

        update(&mut model, Message::Quit);
        assert!(matches!(model.exit, Some(Exit::Normal)));
        assert!(model.confirm.is_none());

        model.exit = None;

        // 3. Session modifiée (dirty) : Quit demande confirmation QuitWithUnsavedEdit
        update(&mut model, Message::Down); // Méthode -> URL
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Char('a')));
        update(&mut model, Message::ValidateInput);
        assert!(model.editing.as_ref().unwrap().dirty);

        update(&mut model, Message::Quit);
        assert!(model.exit.is_none(), "ne doit pas quitter immédiatement");
        assert_eq!(model.confirm, Some(PendingConfirm::QuitWithUnsavedEdit));

        // Refus : application et session restent actives
        update(&mut model, Message::ConfirmNo);
        assert!(model.confirm.is_none());
        assert!(model.exit.is_none());
        assert!(model.editing.is_some());

        // 4. Session modifiée avec ForceQuit (Ctrl+C hors confirm) : demande confirmation
        update(&mut model, Message::ForceQuit);
        assert!(model.exit.is_none());
        assert_eq!(model.confirm, Some(PendingConfirm::QuitWithUnsavedEdit));

        // Acceptation : sortie
        update(&mut model, Message::ConfirmYes);
        assert!(matches!(model.exit, Some(Exit::Normal)));
    }

    // --- Édition directe (`improve-direct-editing`) ---------------------

    /// Modèle avec une session ouverte sur `path`, focus Détail.
    fn edit_session(path: &str, size: (u16, u16)) -> Model {
        let mut model = loaded_model(size);
        select(&mut model, path);
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_some(), "session ouverte sur {path}");
        model
    }

    fn move_to_field(model: &mut Model, field: EditableField) {
        let index = model
            .editing
            .as_ref()
            .and_then(|s| s.fields.iter().position(|f| *f == field))
            .expect("champ présent");
        while model.editing.as_ref().map(|s| s.cursor) != Some(index) {
            update(model, Message::Down);
        }
    }

    fn type_input(model: &mut Model, text: &str) {
        for c in text.chars() {
            update(model, Message::InputKey(InputKey::Char(c)));
        }
    }

    fn press(model: &mut Model, key: InputKey, times: usize) {
        for _ in 0..times {
            update(model, Message::InputKey(key));
        }
    }

    fn input_of(model: &Model) -> &TextInput {
        match &model.editing.as_ref().expect("session").state {
            EditState::Input(input) => input,
            EditState::FieldSelect | EditState::MethodPicker { .. } => panic!("saisie attendue"),
        }
    }

    fn shown_value(model: &Model, field: EditableField) -> String {
        field_value(model.editing.as_ref().expect("session"), &field).to_owned()
    }

    fn session(model: &Model) -> &EditSession {
        model.editing.as_ref().expect("session")
    }

    #[test]
    fn enter_opens_the_session_and_e_is_an_alias() {
        // Ouverture par Entrée, sur le premier champ, en sélection de champ.
        let model = edit_session("simple-get.bru", (100, 30));
        assert_eq!(session(&model).cursor, 0);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Alias `e`.
        let mut model = loaded_model((100, 30));
        select(&mut model, "simple-get.bru");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::StartEdit);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Sans effet sur un dossier sans folder.bru ni sur un nœud en erreur.
        let mut model = loaded_model((100, 30));
        select(&mut model, "misc");
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Enter);
        assert!(model.editing.is_none());
        select(&mut model, "broken.bru");
        update(&mut model, Message::Enter);
        assert!(model.editing.is_none());

        // Ouvre la session sur un dossier doté de folder.bru.
        select(&mut model, "grp");
        update(&mut model, Message::Enter);
        assert!(model.editing.is_some());
        assert_eq!(
            model.editing.as_ref().unwrap().path,
            std::path::PathBuf::from("grp/folder.bru")
        );
    }

    #[test]
    fn enter_keeps_its_meaning_outside_the_detail() {
        // Arbre : déplie le dossier, comme `→`.
        let mut model = loaded_model((100, 30));
        select(&mut model, "grp");
        update(&mut model, Message::Enter);
        assert!(model.tree.expanded.contains(Path::new("grp")));

        // Panneau Environnement : `→` active l'entrée, sans changer le
        // focus (`add-environment-panel-and-edit-popup`).
        let mut model = loaded_model((100, 30));
        model.current_environment = Some("local".into());
        model.focus = Focus::EnvironmentPicker;
        model.environment_selected = 0;
        update(&mut model, Message::Right);
        assert_eq!(model.focus, Focus::EnvironmentPicker);
        assert_eq!(model.current_environment, None);

        // Réponse : onglet suivant, comme `→`.
        let mut model = loaded_model((100, 30));
        model.focus = Focus::Response;
        update(&mut model, Message::Enter);
        let mut expected = loaded_model((100, 30));
        expected.focus = Focus::Response;
        update(&mut expected, Message::Right);
        assert_eq!(model.response_tab, expected.response_tab);
    }

    #[test]
    fn validated_url_edit_marks_the_session_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        let url_cursor = session(&model).cursor;
        update(&mut model, Message::Enter);
        assert_eq!(
            input_of(&model).cursor(),
            "https://{{host}}/ping".chars().count()
        );
        type_input(&mut model, "/v2");
        // Rien n'est versé dans `pending` avant la validation.
        assert!(session(&model).pending.is_empty());
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(
            shown_value(&model, EditableField::Url),
            "https://{{host}}/ping/v2"
        );
        assert!(session(&model).dirty);
        assert_eq!(session(&model).cursor, url_cursor, "même champ");
    }

    /// Ouverture avec présélection, extrémité haute, navigation, validation,
    /// réouverture avec la nouvelle présélection, annulation sans effet
    /// (`add-method-editing`).
    #[test]
    fn method_picker_opens_navigates_confirms_and_cancels() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        assert_eq!(session(&model).cursor, 0);
        assert!(matches!(session(&model).fields[0], EditableField::Method));

        // Ouverture : présélection sur la méthode courante (GET, indice 0).
        update(&mut model, Message::Enter);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 0 }
        );
        assert!(!session(&model).has_unsaved());

        // Raccourci global sans effet pendant le sélecteur.
        update(&mut model, Message::RunSelected);
        assert!(model.run.active.is_none());
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 0 }
        );

        // Extrémité haute : Up ne bouge pas depuis la première entrée.
        update(&mut model, Message::Up);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 0 }
        );

        // Navigation vers POST (indice 1) puis validation.
        update(&mut model, Message::Down);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 1 }
        );
        update(&mut model, Message::Enter);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(shown_value(&model, EditableField::Method), "POST");
        assert!(session(&model).dirty);

        // Réouverture : la nouvelle méthode est présélectionnée.
        update(&mut model, Message::Enter);
        assert_eq!(
            session(&model).state,
            EditState::MethodPicker { selected: 1 }
        );

        // Annulation : le sélecteur se ferme sans changer la méthode, quelle
        // que soit la présélection au moment de la fermeture.
        update(&mut model, Message::Down);
        update(&mut model, Message::CancelInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(shown_value(&model, EditableField::Method), "POST");
    }

    /// Valider la présélection déjà en place ne marque pas la session
    /// modifiée (`add-method-editing`).
    #[test]
    fn method_picker_validation_without_change_does_not_mark_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        update(&mut model, Message::Enter);
        update(&mut model, Message::Enter);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(!session(&model).dirty);
        assert!(!session(&model).has_unsaved());
    }

    #[test]
    fn escape_cancels_the_input_without_confirmation() {
        let mut model = edit_session("post-json.bru", (100, 30));
        move_to_field(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Enter);
        type_input(&mut model, "xyz");
        update(&mut model, Message::CancelInput);
        assert!(model.confirm.is_none());
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(
            shown_value(&model, EditableField::HeaderValue(0)),
            "application/json"
        );
        assert!(!session(&model).dirty);
        assert!(!session(&model).has_unsaved());
    }

    #[test]
    fn cancel_after_a_previous_validation_restores_the_validated_value() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "b");
        update(&mut model, Message::ValidateInput);
        update(&mut model, Message::Enter);
        type_input(&mut model, "zzz");
        update(&mut model, Message::CancelInput);
        assert_eq!(
            shown_value(&model, EditableField::Url),
            "https://{{host}}/pingb"
        );
        assert!(session(&model).dirty);
    }

    #[test]
    fn body_enter_inserts_a_line_and_tab_validates() {
        let mut model = edit_session("post-json.bru", (100, 30));
        move_to_field(&mut model, EditableField::BodyText);
        update(&mut model, Message::Enter);
        update(&mut model, Message::InputKey(InputKey::Enter));
        type_input(&mut model, "ajout");
        assert!(matches!(session(&model).state, EditState::Input(_)));
        update(&mut model, Message::ValidateInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(shown_value(&model, EditableField::BodyText).ends_with("\najout"));
        assert!(session(&model).dirty);
    }

    #[test]
    fn unchanged_value_does_not_mark_the_session_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        update(&mut model, Message::Enter);
        type_input(&mut model, "x");
        press(&mut model, InputKey::Backspace, 1);
        update(&mut model, Message::ValidateInput);
        assert!(!session(&model).dirty);
        assert!(session(&model).pending.is_empty());
    }

    #[test]
    fn has_unsaved_covers_validated_and_in_progress_edits() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        assert!(!session(&model).has_unsaved(), "saisie inchangée");
        type_input(&mut model, "x");
        assert!(session(&model).has_unsaved(), "saisie modifiée");
        assert!(!session(&model).dirty);
        update(&mut model, Message::ValidateInput);
        assert!(session(&model).has_unsaved(), "modification validée");
    }

    #[test]
    fn free_text_cursor_through_update() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);

        // Insertion au milieu : après « https://{{host}} ».
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Right, "https://{{host}}".len());
        type_input(&mut model, "-v2");
        assert_eq!(input_of(&model).text(), "https://{{host}}-v2/ping");

        // Début et Fin.
        press(&mut model, InputKey::Home, 1);
        type_input(&mut model, "x");
        press(&mut model, InputKey::End, 1);
        type_input(&mut model, "y");
        assert_eq!(input_of(&model).text(), "xhttps://{{host}}-v2/pingy");

        // Suppression arrière et avant.
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Right, 2);
        press(&mut model, InputKey::Backspace, 1);
        press(&mut model, InputKey::Delete, 1);
        assert_eq!(input_of(&model).text(), "xtps://{{host}}-v2/pingy");
        assert_eq!(input_of(&model).cursor(), 1);

        // Extrémités.
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Left, 1);
        press(&mut model, InputKey::Backspace, 1);
        assert_eq!(input_of(&model).text(), "xtps://{{host}}-v2/pingy");
        assert_eq!(input_of(&model).cursor(), 0);

        // ↑/↓ sans effet sur un champ à une ligne.
        press(&mut model, InputKey::Down, 1);
        assert_eq!(input_of(&model).cursor(), 0);

        // Caractère multi-octet.
        press(&mut model, InputKey::End, 1);
        type_input(&mut model, "é");
        press(&mut model, InputKey::Backspace, 1);
        assert_eq!(input_of(&model).text(), "xtps://{{host}}-v2/pingy");
    }

    #[test]
    fn body_vertical_moves_and_line_join_through_update() {
        let mut model = edit_session("post-json.bru", (100, 30));
        move_to_field(&mut model, EditableField::BodyText);
        update(&mut model, Message::Enter);
        let lines_before = input_of(&model).text().split('\n').count();
        // Curseur en fin de corps (dernière ligne `}`) ; ↑ remonte d'une ligne.
        let (last, _) = input_of(&model).cursor_line_col();
        press(&mut model, InputKey::Up, 1);
        assert_eq!(input_of(&model).cursor_line_col().0, last - 1);
        press(&mut model, InputKey::Down, 1);
        assert_eq!(input_of(&model).cursor_line_col().0, last);
        // Jointure : début de la dernière ligne, Retour arrière.
        press(&mut model, InputKey::Home, 1);
        press(&mut model, InputKey::Backspace, 1);
        assert_eq!(
            input_of(&model).text().split('\n').count(),
            lines_before - 1
        );
    }

    #[test]
    fn save_from_input_validates_then_saves() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        // Sauvegarde sans modification : aucune écriture.
        assert!(matches!(
            update(&mut model, Message::SaveEdit),
            Command::None
        ));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "!");
        match update(&mut model, Message::SaveEdit) {
            Command::SaveEdit { edits, .. } => {
                assert_eq!(edits, vec![FieldEdit::Url("https://{{host}}/ping!".into())]);
            }
            other => panic!("Command::SaveEdit attendu : {other:?}"),
        }
        assert_eq!(session(&model).state, EditState::FieldSelect);
        // Hors session, Ctrl+S est sans effet.
        let mut model = loaded_model((100, 30));
        assert!(matches!(
            update(&mut model, Message::SaveEdit),
            Command::None
        ));
    }

    #[test]
    fn quitting_during_a_modified_input_asks_and_keeps_the_input() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "abc");
        press(&mut model, InputKey::Left, 1);

        // `q` en saisie est du texte.
        update(&mut model, Message::InputKey(InputKey::Char('q')));
        assert!(model.confirm.is_none());
        assert!(model.exit.is_none());
        assert_eq!(input_of(&model).text(), "https://{{host}}/pingabqc");
        let cursor = input_of(&model).cursor();

        update(&mut model, Message::ForceQuit);
        assert_eq!(model.confirm, Some(PendingConfirm::QuitWithUnsavedEdit));
        // La confirmation reprend le clavier : plus de capture de texte.
        assert_eq!(model.text_capture(), None);
        update(&mut model, Message::FocusTree);
        assert!(model.exit.is_none());
        assert_eq!(input_of(&model).text(), "https://{{host}}/pingabqc");
        assert_eq!(input_of(&model).cursor(), cursor);
        assert_eq!(
            model.text_capture(),
            Some(crate::app::message::TextCapture::Input)
        );
    }

    #[test]
    fn selection_change_is_refused_while_the_session_is_modified() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, "!");
        update(&mut model, Message::ValidateInput);
        let selected = model.tree.selected;

        // Navigation dans l'arbre.
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::NextFocus);
        assert_eq!(model.focus, Focus::Tree);
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
        assert!(session(&model).dirty);

        // Recherche dans l'arbre.
        model.last_status = None;
        update(&mut model, Message::StartSearch);
        for c in "inherit".chars() {
            update(&mut model, Message::SearchInput(c));
        }
        update(&mut model, Message::ConfirmSearch);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
        assert!(!model.tree.expanded.contains(Path::new("grp")));

        // Navigation croisée depuis les diagnostics.
        model.last_status = None;
        model.focus = Focus::Diagnostics;
        model.diagnostics_selected = 1;
        update(&mut model, Message::Right);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
        assert!(session(&model).dirty);

        // Une navigation qui ne change pas de nœud reste permise.
        model.last_status = None;
        model.focus = Focus::Tree;
        update(&mut model, Message::Left);
        assert!(model.last_status.is_none());
        assert!(model.editing.is_some());
    }

    #[test]
    fn clean_session_is_closed_on_selection_change() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        update(&mut model, Message::Enter);
        update(&mut model, Message::CancelInput);
        model.focus = Focus::Tree;
        let before = model.tree.selected;
        update(&mut model, Message::Down);
        assert_ne!(model.tree.selected, before);
        assert!(model.editing.is_none());
    }

    #[test]
    fn field_cursor_and_text_cursor_stay_visible() {
        // Terminal bas : le corps de post-json est hors écran au départ.
        let mut model = edit_session("post-json.bru", (100, 12));
        let height = layout_for(model.size, model.zoomed_panel())
            .map(|a| inner(a.detail).height)
            .expect("layout");
        assert_eq!(model.detail_scroll, 0);
        move_to_field(&mut model, EditableField::BodyText);
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let (_, lines, _) =
            request_text_and_fields(request, model.editing.as_ref(), model.shown_request_tab());
        let body = lines
            .iter()
            .find(|l| l.field == EditableField::BodyText)
            .expect("corps")
            .line as u16;
        assert!(model.detail_scroll <= body && body < model.detail_scroll + height);

        // Nouvelle ligne en bas du corps : le détail suit le curseur.
        update(&mut model, Message::Enter);
        press(&mut model, InputKey::Enter, 3);
        let (line, _) = input_of(&model).cursor_line_col();
        let target = body + line as u16;
        assert!(
            model.detail_scroll <= target && target < model.detail_scroll + height,
            "ligne {target} visible (défilement {})",
            model.detail_scroll
        );

        // URL plus large que le panneau : décalage horizontal.
        let mut model = edit_session("simple-get.bru", (60, 20));
        let width = layout_for(model.size, model.zoomed_panel())
            .map(|a| inner(a.detail).width)
            .expect("layout");
        move_to_field(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        type_input(&mut model, &"a".repeat(usize::from(width) * 2));
        let hscroll = session(&model).hscroll;
        assert!(hscroll > 0);
        press(&mut model, InputKey::Home, 1);
        assert_eq!(session(&model).hscroll, 0);
        press(&mut model, InputKey::End, 1);
        assert_eq!(session(&model).hscroll, hscroll);
        update(&mut model, Message::ValidateInput);
        assert_eq!(session(&model).hscroll, 0);
    }

    /// Déplace le curseur de champ sur `field`, vers le haut ou le bas.
    fn go_to(model: &mut Model, field: EditableField) {
        let index = session(model)
            .fields
            .iter()
            .position(|f| *f == field)
            .expect("champ présent");
        while session(model).cursor > index {
            update(model, Message::Up);
        }
        while session(model).cursor < index {
            update(model, Message::Down);
        }
    }

    fn target_of(model: &Model) -> &InputTarget {
        &session(model).target
    }

    /// Ajoute `key: value` à `section` depuis sa ligne d'ajout.
    fn add_via_row(model: &mut Model, section: EntrySection, key: &str, value: &str) {
        go_to(model, EditableField::AddRow(section));
        update(model, Message::Enter);
        type_input(model, key);
        update(model, Message::ValidateInput);
        type_input(model, value);
        update(model, Message::InputKey(InputKey::Enter));
        assert_eq!(session(model).state, EditState::FieldSelect, "ajout validé");
    }

    #[test]
    fn a_on_an_existing_header_adds_at_the_end_of_the_section() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Add);
        assert!(matches!(
            target_of(&model),
            InputTarget::NewKey {
                section: EntrySection::Headers,
                ..
            }
        ));
        type_input(&mut model, "X-Trace");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert!(matches!(target_of(&model), InputTarget::NewValue { .. }));
        assert!(session(&model).has_unsaved());
        type_input(&mut model, "abc");
        update(&mut model, Message::InputKey(InputKey::Enter));

        let session = session(&model);
        assert_eq!(session.state, EditState::FieldSelect);
        let last = session.preview.headers.last().expect("en-tête");
        assert_eq!((last.key.as_str(), last.value.as_str()), ("X-Trace", "abc"));
        assert!(last.enabled);
        assert_eq!(session.current_field(), Some(EditableField::HeaderValue(2)));
        assert!(session.dirty);
    }

    #[test]
    fn adding_a_query_param_from_its_row_updates_the_url() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "page", "2");
        let session = session(&model);
        assert_eq!(session.preview.url, "https://{{host}}/ping?page=2");
        assert_eq!(session.preview.query_params[0].key, "page");
        assert_eq!(
            session.current_field(),
            Some(EditableField::QueryParamValue(0))
        );
    }

    #[test]
    fn adding_into_an_empty_path_section() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::PathParams, "id", "42");
        let session = session(&model);
        assert_eq!(session.preview.path_params[0].key, "id");
        assert_eq!(
            session.current_field(),
            Some(EditableField::PathParamValue(0))
        );
        assert_eq!(session.preview.url, "https://{{host}}/ping");
    }

    #[test]
    fn escape_abandons_the_whole_add_and_restores_the_cursor() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(1));
        let cursor = session(&model).cursor;
        update(&mut model, Message::Add);
        type_input(&mut model, "X-Trace");
        update(&mut model, Message::ValidateInput);

        // Une clé validée verrouille la sélection de requête.
        assert!(selection_locked(&model));

        type_input(&mut model, "abc");
        update(&mut model, Message::CancelInput);
        let session = session(&model);
        assert_eq!(session.state, EditState::FieldSelect);
        assert_eq!(session.cursor, cursor);
        assert_eq!(session.preview.headers.len(), 2);
        assert!(session.pending.is_empty());
        assert!(!session.dirty);
        assert!(!session.has_unsaved());
    }

    #[test]
    fn a_d_c_and_space_have_no_effect_where_not_applicable() {
        let mut model = edit_session("post-json.bru", (100, 30));
        // URL
        update(&mut model, Message::Add);
        update(&mut model, Message::Rename);
        update(&mut model, Message::Delete);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).pending.is_empty());
        // Corps
        go_to(&mut model, EditableField::BodyText);
        update(&mut model, Message::Add);
        update(&mut model, Message::Delete);
        update(&mut model, Message::Rename);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).pending.is_empty());
        // Ligne d'ajout : `a` ajoute, mais `d`, `c` et Espace sont sans effet.
        go_to(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Delete);
        update(&mut model, Message::Rename);
        update(&mut model, Message::ToggleField);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).pending.is_empty());
        update(&mut model, Message::Add);
        assert!(matches!(target_of(&model), InputTarget::NewKey { .. }));
    }

    #[test]
    fn d_deletes_immediately_and_keeps_the_cursor_index() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::Headers, "A", "1");
        add_via_row(&mut model, EntrySection::Headers, "B", "2");
        add_via_row(&mut model, EntrySection::Headers, "C", "3");
        go_to(&mut model, EditableField::HeaderValue(1));
        let cursor = session(&model).cursor;
        update(&mut model, Message::Delete);

        let session = session(&model);
        let keys: Vec<&str> = session
            .preview
            .headers
            .iter()
            .map(|h| h.key.as_str())
            .collect();
        assert_eq!(keys, ["A", "C"]);
        assert_eq!(session.cursor, cursor);
        assert_eq!(session.current_field(), Some(EditableField::HeaderValue(1)));
        assert!(session.dirty);
    }

    #[test]
    fn deleting_a_query_param_updates_the_url_and_locks_the_selection() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "page", "2");
        add_via_row(&mut model, EntrySection::QueryParams, "size", "10");
        assert_eq!(
            session(&model).preview.url,
            "https://{{host}}/ping?page=2&size=10"
        );
        go_to(&mut model, EditableField::QueryParamValue(1));
        update(&mut model, Message::Delete);
        assert_eq!(session(&model).preview.url, "https://{{host}}/ping?page=2");

        let selected = model.tree.selected;
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::NextFocus);
        update(&mut model, Message::Down);
        assert_eq!(model.tree.selected, selected);
        assert!(matches!(model.last_status, Some(StatusMessage::EditLocked)));
    }

    #[test]
    fn c_renames_a_query_param_and_updates_the_url() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "p", "2");
        update(&mut model, Message::Rename);
        assert_eq!(input_of(&model).text(), "p");
        assert_eq!(input_of(&model).cursor(), 1);
        press(&mut model, InputKey::Backspace, 1);
        type_input(&mut model, "page");
        update(&mut model, Message::InputKey(InputKey::Enter));

        let session = session(&model);
        assert_eq!(session.state, EditState::FieldSelect);
        let param = &session.preview.query_params[0];
        assert_eq!((param.key.as_str(), param.value.as_str()), ("page", "2"));
        assert_eq!(session.preview.url, "https://{{host}}/ping?page=2");
    }

    #[test]
    fn cancelled_rename_keeps_the_key_and_the_session_clean() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(1));
        update(&mut model, Message::Rename);
        type_input(&mut model, "zz");
        assert!(session(&model).has_unsaved());
        update(&mut model, Message::CancelInput);
        let session = session(&model);
        assert_eq!(session.preview.headers[1].key, "X-Debug");
        assert!(!session.preview.headers[1].enabled);
        assert!(!session.dirty);
        assert!(!session.has_unsaved());
    }

    #[test]
    fn settings_refuses_structural_edits_and_accepts_value_edits() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        let source = "meta {\n  name: ping\n  type: http\n  seq: 7\n}\n\nget {\n  url: https://{{host}}/ping\n  body: none\n  auth: none\n}\n\nsettings {\n  encodeUrl: true\n}\n";
        let ast = crate::collection::BruFile::parse(source.to_owned()).expect("AST");
        let view = RequestView::from_ast(&ast).expect("vue");
        if let CollectionState::Loaded(col) = &mut model.collection {
            for node in &mut col.tree {
                if let TreeNode::Request(req) = node
                    && req.path == std::path::Path::new("simple-get.bru")
                {
                    req.ast = Some(ast.clone());
                    req.view = view.clone();
                }
            }
        }
        let session_mut = model.editing.as_mut().expect("session");
        session_mut.fields.push(EditableField::SettingsValue(0));
        session_mut
            .preview
            .extra
            .settings
            .push(crate::collection::KeyValue {
                key: "encodeUrl".into(),
                value: "true".into(),
                enabled: true,
            });
        go_to(&mut model, EditableField::SettingsValue(0));

        // Toggle (Espace) est sans effet
        update(&mut model, Message::ToggleField);
        assert!(!session(&model).dirty);

        // Delete ('d') est sans effet
        update(&mut model, Message::Delete);
        assert!(!session(&model).dirty);

        // Rename ('c') est sans effet
        update(&mut model, Message::Rename);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Add ('a') est sans effet
        update(&mut model, Message::Add);
        assert_eq!(session(&model).state, EditState::FieldSelect);

        // Mais Value edit (Enter) est autorisé !
        update(&mut model, Message::Enter);
        assert!(matches!(session(&model).state, EditState::Input(_)));
        press(&mut model, InputKey::Backspace, 4);
        type_input(&mut model, "false");
        update(&mut model, Message::ValidateInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert!(session(&model).dirty);
        assert_eq!(session(&model).preview.extra.settings[0].value, "false");
    }

    #[test]
    fn rename_keeps_value_and_disabled_state() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(1));
        update(&mut model, Message::Rename);
        press(&mut model, InputKey::Backspace, 5);
        type_input(&mut model, "Verbose");
        update(&mut model, Message::ValidateInput);
        let header = &session(&model).preview.headers[1];
        assert_eq!(header.key, "X-Verbose");
        assert_eq!(header.value, "1");
        assert!(!header.enabled);
    }

    #[test]
    fn ctrl_s_during_the_key_step_adds_an_empty_value_and_saves() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        go_to(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Enter);
        type_input(&mut model, "X-Trace");
        let command = update(&mut model, Message::SaveEdit);
        let Command::SaveEdit { edits, .. } = command else {
            panic!("sauvegarde attendue");
        };
        assert_eq!(
            edits,
            [FieldEdit::AddEntry {
                section: EntrySection::Headers,
                key: "X-Trace".into(),
                value: String::new(),
                enabled: true,
            }]
        );
        assert_eq!(session(&model).state, EditState::FieldSelect);
    }

    #[test]
    fn ctrl_s_on_an_invalid_key_refuses_without_saving() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        go_to(&mut model, EditableField::AddRow(EntrySection::Headers));
        update(&mut model, Message::Enter);
        type_input(&mut model, "X:Trace");
        let command = update(&mut model, Message::SaveEdit);
        assert!(matches!(command, Command::None));
        assert_eq!(input_of(&model).text(), "X:Trace");
        assert!(matches!(target_of(&model), InputTarget::NewKey { .. }));
        let Some(StatusMessage::EditRefused(reason)) = &model.last_status else {
            panic!("refus attendu");
        };
        assert!(!reason.contains("X:Trace"), "{reason}");
        assert!(session(&model).pending.is_empty());
        assert!(!session(&model).dirty);
    }

    #[test]
    fn invalid_key_on_enter_keeps_the_input_open() {
        let mut model = edit_session("scripted.bru", (100, 30));
        go_to(&mut model, EditableField::HeaderValue(0));
        update(&mut model, Message::Add);
        type_input(&mut model, "X Trace");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert!(matches!(target_of(&model), InputTarget::NewKey { .. }));
        assert_eq!(input_of(&model).text(), "X Trace");
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::EditRefused(_))
        ));
        assert_eq!(session(&model).preview.headers.len(), 2);
    }

    #[test]
    fn refused_query_value_stays_in_input_then_escape_restores() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "q", "2");
        let pending = session(&model).pending.clone();
        update(&mut model, Message::Enter);
        type_input(&mut model, "#b");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(input_of(&model).text(), "2#b");
        assert!(matches!(
            model.last_status,
            Some(StatusMessage::EditRefused(_))
        ));
        assert_eq!(session(&model).pending, pending);

        update(&mut model, Message::CancelInput);
        assert_eq!(session(&model).state, EditState::FieldSelect);
        assert_eq!(shown_value(&model, EditableField::QueryParamValue(0)), "2");
    }

    #[test]
    fn duplicate_header_key_is_accepted() {
        let mut model = edit_session("scripted.bru", (100, 30));
        add_via_row(&mut model, EntrySection::Headers, "Accept", "text/plain");
        let accepts = session(&model)
            .preview
            .headers
            .iter()
            .filter(|h| h.key == "Accept")
            .count();
        assert_eq!(accepts, 2);
    }

    #[test]
    fn validating_the_url_resynchronises_query_params() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::QueryParams, "page", "2");
        go_to(&mut model, EditableField::Url);
        update(&mut model, Message::Enter);
        press(&mut model, InputKey::Backspace, 1);
        type_input(&mut model, "3");
        update(&mut model, Message::InputKey(InputKey::Enter));
        assert_eq!(session(&model).preview.query_params[0].value, "3");
    }

    #[test]
    fn edit_saved_after_an_add_replaces_the_preview() {
        let mut model = edit_session("simple-get.bru", (100, 30));
        add_via_row(&mut model, EntrySection::Headers, "A", "1");
        let Some(TreeNode::Request(request)) = model.selected_node() else {
            panic!("requête attendue");
        };
        let ast = request.ast.clone().expect("ast");
        let pending = session(&model).pending.clone();
        let bytes = crate::writer::preview(&ast, &pending).expect("aperçu");
        let source = "meta {\n  name: ping\n  type: http\n  seq: 7\n}\n\nget {\n  url: https://{{host}}/ping\n  body: none\n  auth: none\n}\n\nheaders {\n  A: 1\n}\n";
        let saved_ast = crate::collection::BruFile::parse(source.to_owned()).expect("AST");
        let saved_view = RequestView::from_ast(&saved_ast).expect("vue");
        assert_eq!(bytes, saved_view);
        let stamp = session(&model).stamp;
        update(
            &mut model,
            Message::EditSaved {
                path: "simple-get.bru".into(),
                result: Ok(SavedEdit {
                    stamp,
                    ast: saved_ast,
                    view: saved_view.clone(),
                }),
            },
        );
        let session = session(&model);
        assert!(session.pending.is_empty());
        assert!(!session.dirty);
        assert_eq!(session.preview, saved_view);
        assert!(session.fields.contains(&EditableField::HeaderValue(0)));
    }
}
