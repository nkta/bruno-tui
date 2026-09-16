//! Formatage d'une entrée de bloc dictionnaire et d'un contenu de bloc
//! texte, et application des remplacements résolus sur le source brut.
//!
//! Le format produit est symétrique de la lecture de `bru-parser` :
//! indentation de deux espaces pour un bloc dictionnaire, forme `'''` pour
//! une valeur contenant un saut de ligne, réindentation de deux espaces
//! pour un contenu de corps texte.

use super::edit::{self, FieldEdit};
use super::error::EditError;
use crate::collection::BruFile;

/// Fin de ligne dominante du fichier : `\r\n` si le source en contient au
/// moins une, sinon `\n`. Une seule détection par fichier.
pub(crate) fn detect_eol(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Formate une entrée de bloc dictionnaire (indentation de deux espaces).
/// Une valeur sans saut de ligne tient sur une ligne ; une valeur avec au
/// moins un saut de ligne prend la forme `'''`, lignes intérieures
/// indentées de quatre espaces.
pub(crate) fn format_entry(key: &str, value: &str, disabled: bool, eol: &str) -> String {
    let prefix = if disabled { "~" } else { "" };
    if value.contains('\n') {
        let mut out = format!("  {prefix}{key}: '''{eol}");
        for line in value.split('\n') {
            out.push_str("    ");
            out.push_str(line);
            out.push_str(eol);
        }
        out.push_str("  '''");
        out.push_str(eol);
        out
    } else {
        format!("  {prefix}{key}: {value}{eol}")
    }
}

/// Réindente un contenu de corps texte de deux espaces par ligne, l'inverse
/// de `dedent` à la lecture.
pub(crate) fn format_body_content(text: &str, eol: &str) -> String {
    let mut out = String::new();
    for line in text.split('\n') {
        out.push_str("  ");
        out.push_str(line);
        out.push_str(eol);
    }
    out
}

/// Résout `edits` puis applique les remplacements sur `ast.raw()` par
/// concaténation. Fonction pure : aucune I/O, aucun effet de bord.
pub(crate) fn serialize(ast: &BruFile, edits: &[FieldEdit]) -> Result<Vec<u8>, EditError> {
    let replacements = edit::resolve(ast, edits)?;
    let raw = ast.raw();
    let mut out = String::with_capacity(raw.len());
    let mut cursor = 0;
    for replacement in &replacements {
        out.push_str(&raw[cursor..replacement.span.start]);
        out.push_str(&replacement.bytes);
        cursor = replacement.span.end;
    }
    out.push_str(&raw[cursor..]);
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::BruFile;

    #[test]
    fn eol_detection() {
        assert_eq!(detect_eol("a\nb\n"), "\n");
        assert_eq!(detect_eol("a\r\nb\r\n"), "\r\n");
        assert_eq!(detect_eol(""), "\n");
    }

    #[test]
    fn single_line_entry_lf() {
        assert_eq!(
            format_entry("Accept", "application/json", false, "\n"),
            "  Accept: application/json\n"
        );
    }

    #[test]
    fn single_line_entry_crlf() {
        assert_eq!(
            format_entry("Accept", "application/json", false, "\r\n"),
            "  Accept: application/json\r\n"
        );
    }

    #[test]
    fn disabled_entry_gets_tilde_prefix() {
        assert_eq!(format_entry("X-Debug", "1", true, "\n"), "  ~X-Debug: 1\n");
    }

    #[test]
    fn multiline_entry_round_trips_through_the_parser() {
        let value = "line one\nline two\nline three";
        let formatted = format_entry("q", value, false, "\n");
        let source = format!("params:query {{\n{formatted}}}\n");
        let file = BruFile::parse(source).expect("source valide");
        assert_eq!(file.entry("params:query", "q"), Some(value));
    }

    #[test]
    fn body_content_round_trips_through_the_parser() {
        let text = "{\n  \"a\": 1\n}";
        let formatted = format_body_content(text, "\n");
        let source = format!("body:json {{\n{formatted}}}\n");
        let file = BruFile::parse(source).expect("source valide");
        assert_eq!(file.text("body:json").as_deref(), Some(text));
    }

    #[test]
    fn serialize_without_edits_is_identity() {
        let source = "meta {\n  name: a\n}\n\nget {\n  url: http://x\n}\n";
        let file = BruFile::parse(source.to_owned()).expect("source valide");
        let bytes = serialize(&file, &[]).expect("sérialisation");
        assert_eq!(bytes, source.as_bytes());
    }
}
