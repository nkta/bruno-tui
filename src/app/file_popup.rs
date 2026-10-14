//! Popups des opérations de fichiers (`docs/edition-phase3.md` § 4) :
//! nouvelle requête, nouveau dossier, renommer, cloner.
//!
//! État et logique purs, sans I/O : le popup valide la saisie avec les
//! règles de Bruno et produit l'opération ([`FileOp`]) que la boucle
//! exécute. Les champs texte reçoivent directement la frappe ; `↑↓`, `Tab`
//! changent de ligne, `←→` et `Espace` changent un choix ou la case.

use std::path::PathBuf;

use super::message::InputKey;
use super::text_input::TextInput;
use crate::collection::ast::METHODS;
use crate::writer::files::{FileOp, NewRequest, RequestKind, new_request_content};
use crate::writer::names::{
    MAX_NAME_LENGTH, sanitize_name, validate_name, validate_request_filename,
};

/// Opération du popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePopupKind {
    NewRequest,
    NewFolder,
    Rename,
    Clone,
}

/// Ligne du popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupField {
    Name,
    /// Nom du fichier (sans `.bru`) ou du répertoire.
    Filename,
    /// Type de la nouvelle requête : ‹ http › / ‹ graphql ›.
    Kind,
    Method,
    Url,
    /// Case « Renommer aussi le fichier ».
    RenameFile,
}

/// Popup ouvert sur l'arbre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePopup {
    pub kind: FilePopupKind,
    /// L'élément créé, renommé ou cloné est un dossier.
    pub is_folder: bool,
    /// Complément du titre : « dans Posts » ou nom du fichier visé.
    pub context: String,
    pub fields: Vec<PopupField>,
    pub cursor: usize,
    pub name: TextInput,
    pub filename: TextInput,
    /// Le fichier a été saisi : il ne suit plus le nom.
    pub filename_touched: bool,
    pub request_kind: RequestKind,
    /// Indice dans `METHODS`.
    pub method: usize,
    pub url: TextInput,
    pub rename_file: bool,
    /// Refus de la dernière validation, affiché dans le popup.
    pub error: Option<String>,
    /// Répertoire de création, relatif à la racine.
    pub dir: PathBuf,
    /// Élément renommé ou cloné, relatif à la racine.
    pub target: PathBuf,
    /// Source chargé de l'élément visé (requête, ou `folder.bru`).
    pub expected: Option<String>,
    pub seq: u32,
    original_name: String,
    original_filename: String,
}

impl FilePopup {
    fn base(kind: FilePopupKind, is_folder: bool, context: String) -> Self {
        Self {
            kind,
            is_folder,
            context,
            fields: Vec::new(),
            cursor: 0,
            name: TextInput::new("", false),
            filename: TextInput::new("", false),
            filename_touched: false,
            request_kind: RequestKind::Http,
            method: 0,
            url: TextInput::new("", false),
            rename_file: true,
            error: None,
            dir: PathBuf::new(),
            target: PathBuf::new(),
            expected: None,
            seq: 1,
            original_name: String::new(),
            original_filename: String::new(),
        }
    }

    /// Nouvelle requête dans `dir` (`context` : nom affiché du dossier).
    pub fn new_request(dir: PathBuf, context: String, seq: u32) -> Self {
        Self {
            fields: vec![
                PopupField::Name,
                PopupField::Filename,
                PopupField::Kind,
                PopupField::Method,
                PopupField::Url,
            ],
            dir,
            seq,
            ..Self::base(FilePopupKind::NewRequest, false, context)
        }
    }

    /// Nouveau dossier dans `dir`.
    pub fn new_folder(dir: PathBuf, context: String, seq: u32) -> Self {
        Self {
            fields: vec![PopupField::Name, PopupField::Filename],
            dir,
            seq,
            ..Self::base(FilePopupKind::NewFolder, true, context)
        }
    }

    /// Renommer `target` (nom affiché `name`, nom de fichier `filename`
    /// sans extension).
    pub fn rename(
        target: PathBuf,
        is_folder: bool,
        name: &str,
        filename: &str,
        expected: Option<String>,
    ) -> Self {
        let context = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            fields: vec![
                PopupField::Name,
                PopupField::Filename,
                PopupField::RenameFile,
            ],
            name: TextInput::new(name, false),
            filename: TextInput::new(filename, false),
            target,
            expected,
            original_name: name.to_owned(),
            original_filename: filename.to_owned(),
            ..Self::base(FilePopupKind::Rename, is_folder, context)
        }
    }

    /// Cloner `target`, pré-rempli avec « <nom> copie ».
    pub fn clone_of(
        target: PathBuf,
        is_folder: bool,
        name: &str,
        expected: Option<String>,
        seq: u32,
    ) -> Self {
        let context = target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let copy = format!("{name} copie");
        Self {
            fields: vec![PopupField::Name, PopupField::Filename],
            filename: TextInput::new(&sanitize_name(&copy), false),
            name: TextInput::new(&copy, false),
            target,
            expected,
            seq,
            ..Self::base(FilePopupKind::Clone, is_folder, context)
        }
    }

    /// Titre du cadre.
    pub fn title(&self) -> String {
        let what = match (self.kind, self.is_folder) {
            (FilePopupKind::NewRequest, _) => "Nouvelle requête — dans",
            (FilePopupKind::NewFolder, _) => "Nouveau dossier — dans",
            (FilePopupKind::Rename, _) => "Renommer —",
            (FilePopupKind::Clone, _) => "Cloner —",
        };
        format!(" {what} {} ", self.context)
    }

    pub fn current_field(&self) -> PopupField {
        self.fields
            .get(self.cursor)
            .copied()
            .unwrap_or(PopupField::Name)
    }

    /// Libellé de la case : fichier ou dossier.
    pub fn rename_file_label(&self) -> &'static str {
        if self.is_folder {
            "Renommer aussi le dossier"
        } else {
            "Renommer aussi le fichier"
        }
    }

    /// Libellé de la ligne du nom de fichier.
    pub fn filename_label(&self) -> &'static str {
        if self.is_folder { "Dossier" } else { "Fichier" }
    }

    /// Méthode affichée, en majuscules.
    pub fn method_label(&self) -> String {
        METHODS
            .get(self.method)
            .map(|method| method.to_ascii_uppercase())
            .unwrap_or_default()
    }

    /// Libellé du type de requête.
    pub fn kind_label(&self) -> &'static str {
        self.request_kind.as_str()
    }

    /// Verbe de validation, pour l'aide (`Ctrl+S créer`).
    pub fn verb(&self) -> &'static str {
        match self.kind {
            FilePopupKind::NewRequest | FilePopupKind::NewFolder | FilePopupKind::Clone => "créer",
            FilePopupKind::Rename => "appliquer",
        }
    }

    pub fn next_field(&mut self) {
        self.cursor = (self.cursor + 1) % self.fields.len().max(1);
    }

    pub fn previous_field(&mut self) {
        let count = self.fields.len().max(1);
        self.cursor = (self.cursor + count - 1) % count;
    }

    /// Vrai sur la dernière ligne : `Entrée` y crée ou applique.
    pub fn on_last_field(&self) -> bool {
        self.cursor + 1 >= self.fields.len()
    }

    /// Champ texte sous le curseur.
    pub fn text_input(&self) -> Option<&TextInput> {
        match self.current_field() {
            PopupField::Name => Some(&self.name),
            PopupField::Filename => Some(&self.filename),
            PopupField::Url => Some(&self.url),
            _ => None,
        }
    }

    fn text_input_mut(&mut self) -> Option<&mut TextInput> {
        match self.current_field() {
            PopupField::Name => Some(&mut self.name),
            PopupField::Filename => Some(&mut self.filename),
            PopupField::Url => Some(&mut self.url),
            _ => None,
        }
    }

    /// Change le choix ou la case sous le curseur (`→`, `Espace` : suivant ;
    /// `←` : précédent).
    pub fn cycle(&mut self, forward: bool) {
        match self.current_field() {
            PopupField::Kind => {
                self.request_kind = match self.request_kind {
                    RequestKind::Http => RequestKind::Graphql,
                    RequestKind::Graphql => RequestKind::Http,
                };
            }
            PopupField::Method => {
                let count = METHODS.len();
                self.method = if forward {
                    (self.method + 1) % count
                } else {
                    (self.method + count - 1) % count
                };
            }
            PopupField::RenameFile => self.rename_file = !self.rename_file,
            _ => {}
        }
    }

    /// Touche d'édition hors navigation entre lignes.
    pub fn key(&mut self, key: InputKey) {
        let field = self.current_field();
        let Some(input) = self.text_input_mut() else {
            match key {
                InputKey::Char(' ') | InputKey::Right => self.cycle(true),
                InputKey::Left => self.cycle(false),
                _ => {}
            }
            return;
        };
        let before = input.text().to_owned();
        match key {
            InputKey::Char(c) => input.insert(c),
            InputKey::Backspace => input.backspace(),
            InputKey::Delete => input.delete(),
            InputKey::Left => input.left(),
            InputKey::Right => input.right(),
            InputKey::Home => input.home(),
            InputKey::End => input.end(),
            InputKey::Up | InputKey::Down | InputKey::Enter => {}
        }
        let changed = input.text() != before;
        if changed {
            self.error = None;
            match field {
                PopupField::Filename => self.filename_touched = true,
                PopupField::Name if !self.filename_touched => {
                    self.filename = TextInput::new(&sanitize_name(self.name.text()), false);
                }
                _ => {}
            }
        }
    }

    /// Valide la saisie. `Ok(None)` : rien à faire (renommage sans
    /// changement). `Err` : message de refus, le popup reste ouvert.
    pub fn submit(&self) -> Result<Option<FileOp>, String> {
        let name = self.name.text().trim().to_owned();
        let filename = self.filename.text().trim().to_owned();
        if name.is_empty() {
            return Err("Le nom est obligatoire.".to_owned());
        }
        if name.encode_utf16().count() > MAX_NAME_LENGTH {
            return Err("Le nom ne peut pas dépasser 255 caractères.".to_owned());
        }
        let check_filename = |filename: &str| {
            if self.is_folder {
                validate_name(filename)
            } else {
                validate_request_filename(filename)
            }
            .map_err(|error| error.to_string())
        };
        let op = match self.kind {
            FilePopupKind::NewRequest => {
                check_filename(&filename)?;
                let url = self.url.text().trim().to_owned();
                let request = NewRequest {
                    name,
                    filename,
                    kind: self.request_kind,
                    method: METHODS
                        .get(self.method)
                        .copied()
                        .unwrap_or("get")
                        .to_owned(),
                    url,
                    seq: self.seq,
                };
                // Refus éventuel de l'URL, affiché dans le popup.
                new_request_content(&request).map_err(|error| error.to_string())?;
                FileOp::CreateRequest {
                    dir: self.dir.clone(),
                    request,
                }
            }
            FilePopupKind::NewFolder => {
                check_filename(&filename)?;
                FileOp::CreateFolder {
                    dir: self.dir.clone(),
                    name,
                    dirname: filename,
                    seq: self.seq,
                }
            }
            FilePopupKind::Rename => {
                let rename_file = self.rename_file && filename != self.original_filename;
                if rename_file {
                    check_filename(&filename)?;
                }
                if name == self.original_name && !rename_file {
                    return Ok(None);
                }
                let filename = rename_file.then_some(filename);
                if self.is_folder {
                    FileOp::RenameFolder {
                        path: self.target.clone(),
                        expected: self.expected.clone(),
                        name,
                        dirname: filename,
                    }
                } else {
                    FileOp::RenameRequest {
                        path: self.target.clone(),
                        expected: self.expected.clone().unwrap_or_default(),
                        name,
                        filename,
                    }
                }
            }
            FilePopupKind::Clone => {
                check_filename(&filename)?;
                if self.is_folder {
                    FileOp::CloneFolder {
                        path: self.target.clone(),
                        name,
                        dirname: filename,
                        seq: self.seq,
                    }
                } else {
                    FileOp::CloneRequest {
                        path: self.target.clone(),
                        expected: self.expected.clone().unwrap_or_default(),
                        name,
                        filename,
                        seq: self.seq,
                    }
                }
            }
        };
        Ok(Some(op))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(popup: &mut FilePopup, text: &str) {
        for c in text.chars() {
            popup.key(InputKey::Char(c));
        }
    }

    #[test]
    fn filename_follows_the_name_until_edited() {
        let mut popup = FilePopup::new_request(PathBuf::from("Posts"), "Posts".into(), 4);
        typed(&mut popup, "Lire: un article.");
        assert_eq!(popup.filename.text(), "Lire- un article");
        popup.next_field();
        popup.key(InputKey::Backspace);
        assert!(popup.filename_touched);
        popup.previous_field();
        typed(&mut popup, "!");
        assert_eq!(popup.filename.text(), "Lire- un articl");
    }

    #[test]
    fn new_request_builds_the_bruno_request() {
        let mut popup = FilePopup::new_request(PathBuf::from("Posts"), "Posts".into(), 4);
        typed(&mut popup, "Lire");
        popup.next_field();
        popup.next_field();
        popup.key(InputKey::Char(' '));
        popup.next_field();
        popup.key(InputKey::Right);
        popup.next_field();
        typed(&mut popup, "{{base_url}}/graphql");
        assert!(popup.on_last_field());
        assert_eq!(
            popup.submit(),
            Ok(Some(FileOp::CreateRequest {
                dir: PathBuf::from("Posts"),
                request: NewRequest {
                    name: "Lire".into(),
                    filename: "Lire".into(),
                    kind: RequestKind::Graphql,
                    method: "post".into(),
                    url: "{{base_url}}/graphql".into(),
                    seq: 4,
                },
            }))
        );
    }

    #[test]
    fn invalid_input_is_refused_with_bruno_messages() {
        let mut popup = FilePopup::new_request(PathBuf::new(), "collection".into(), 1);
        assert_eq!(popup.submit(), Err("Le nom est obligatoire.".into()));
        typed(&mut popup, "folder");
        assert_eq!(
            popup.submit(),
            Err("Les noms de fichier « collection » et « folder » sont réservés par Bruno.".into())
        );
        popup.next_field();
        typed(&mut popup, "?");
        assert_eq!(
            popup.submit(),
            Err("Caractère spécial interdit dans le nom : « ? ».".into())
        );
        let mut popup = FilePopup::new_request(PathBuf::new(), "x".into(), 1);
        typed(&mut popup, "a");
        popup.cursor = 4;
        typed(&mut popup, "http://moi@h/posts/:id");
        assert!(
            popup
                .submit()
                .is_err_and(|e| e.starts_with("URL non prise en charge"))
        );
        let mut popup = FilePopup::new_request(PathBuf::new(), "x".into(), 1);
        typed(&mut popup, "a");
        popup.cursor = 4;
        typed(&mut popup, "{{u}}/posts/:id?full=1");
        assert!(popup.submit().is_ok_and(|op| op.is_some()));
        let mut popup = FilePopup::new_request(PathBuf::new(), "x".into(), 1);
        typed(&mut popup, "a");
        popup.cursor = 4;
        typed(&mut popup, "{{u}}/x?{{k}}=1");
        assert_eq!(
            popup.submit(),
            Err("paramètre « {{k}} » : nom à écrire entre guillemets, non pris en charge (crée la requête sans, puis complète l'URL avec m)".into())
        );
    }

    #[test]
    fn rename_without_change_does_nothing_and_checkbox_keeps_the_file() {
        let mut popup = FilePopup::rename(
            PathBuf::from("get-post.bru"),
            false,
            "Lire",
            "get-post",
            Some("src".into()),
        );
        assert_eq!(popup.title(), " Renommer — get-post.bru ");
        assert_eq!(popup.submit(), Ok(None));
        typed(&mut popup, " tout");
        assert_eq!(popup.filename.text(), "Lire tout");
        popup.cursor = 2;
        popup.key(InputKey::Char(' '));
        assert!(!popup.rename_file);
        assert_eq!(
            popup.submit(),
            Ok(Some(FileOp::RenameRequest {
                path: PathBuf::from("get-post.bru"),
                expected: "src".into(),
                name: "Lire tout".into(),
                filename: None,
            }))
        );
        popup.key(InputKey::Char(' '));
        assert!(matches!(
            popup.submit(),
            Ok(Some(FileOp::RenameRequest { filename: Some(f), .. })) if f == "Lire tout"
        ));
    }

    #[test]
    fn clone_is_prefilled_with_copie() {
        let popup = FilePopup::clone_of(PathBuf::from("Posts"), true, "Articles", None, 6);
        assert_eq!(popup.name.text(), "Articles copie");
        assert_eq!(popup.filename.text(), "Articles copie");
        assert_eq!(
            popup.submit(),
            Ok(Some(FileOp::CloneFolder {
                path: PathBuf::from("Posts"),
                name: "Articles copie".into(),
                dirname: "Articles copie".into(),
                seq: 6,
            }))
        );
    }
}
