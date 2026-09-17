//! Tampon de saisie d'un champ, sans I/O (design D2 de
//! `improve-direct-editing`).
//!
//! Le curseur est une position en caractères, jamais en octets : un
//! caractère accentué ou non latin se traverse et se supprime en une seule
//! opération. Les positions ligne/colonne sont dérivées à la demande du
//! tampon et du curseur, sans double représentation à synchroniser.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInput {
    buffer: String,
    /// Position en caractères, dans `0..=nombre de caractères`.
    cursor: usize,
    /// Le corps accepte les sauts de ligne et le déplacement vertical.
    multiline: bool,
    /// Valeur au début de la saisie, pour `is_modified`.
    initial: String,
}

impl TextInput {
    /// Ouvre une saisie sur `value`, curseur en fin de valeur.
    pub fn new(value: &str, multiline: bool) -> Self {
        Self {
            buffer: value.to_owned(),
            cursor: value.chars().count(),
            multiline,
            initial: value.to_owned(),
        }
    }

    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// Position du curseur, en caractères depuis le début du texte.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    /// Le texte diffère de la valeur au début de la saisie.
    pub fn is_modified(&self) -> bool {
        self.buffer != self.initial
    }

    /// Ligne (à partir de 0) et colonne (en caractères) du curseur.
    pub fn cursor_line_col(&self) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        for c in self.buffer.chars().take(self.cursor) {
            if c == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Texte de la ligne courante situé avant le curseur.
    pub fn text_before_cursor_on_line(&self) -> String {
        let before: String = self.buffer.chars().take(self.cursor).collect();
        match before.rfind('\n') {
            Some(index) => before[index + 1..].to_owned(),
            None => before,
        }
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_index)
            .map_or(self.buffer.len(), |(index, _)| index)
    }

    fn char_count(&self) -> usize {
        self.buffer.chars().count()
    }

    /// Longueurs, en caractères, de chaque ligne du texte.
    fn line_lengths(&self) -> Vec<usize> {
        self.buffer.split('\n').map(|l| l.chars().count()).collect()
    }

    /// Position en caractères du début de la ligne `line`.
    fn line_start(lengths: &[usize], line: usize) -> usize {
        lengths.iter().take(line).map(|len| len + 1).sum()
    }

    pub fn insert(&mut self, c: char) {
        if c == '\n' && !self.multiline {
            return;
        }
        let at = self.byte_index(self.cursor);
        self.buffer.insert(at, c);
        self.cursor += 1;
    }

    /// Saut de ligne, sans effet sur un champ à une ligne.
    pub fn newline(&mut self) {
        self.insert('\n');
    }

    /// Supprime le caractère avant le curseur.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let at = self.byte_index(self.cursor - 1);
        self.buffer.remove(at);
        self.cursor -= 1;
    }

    /// Supprime le caractère après le curseur.
    pub fn delete(&mut self) {
        if self.cursor >= self.char_count() {
            return;
        }
        let at = self.byte_index(self.cursor);
        self.buffer.remove(at);
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.char_count());
    }

    /// Début de la ligne courante (du texte entier en une ligne).
    pub fn home(&mut self) {
        let (line, _) = self.cursor_line_col();
        self.cursor = Self::line_start(&self.line_lengths(), line);
    }

    /// Fin de la ligne courante (du texte entier en une ligne).
    pub fn end(&mut self) {
        let (line, _) = self.cursor_line_col();
        let lengths = self.line_lengths();
        self.cursor = Self::line_start(&lengths, line) + lengths.get(line).copied().unwrap_or(0);
    }

    /// Ligne précédente, même colonne ou fin de ligne ; sans effet en une
    /// ligne ou sur la première ligne.
    pub fn up(&mut self) {
        if !self.multiline {
            return;
        }
        let (line, col) = self.cursor_line_col();
        if line == 0 {
            return;
        }
        self.move_to_line(line - 1, col);
    }

    /// Ligne suivante, même colonne ou fin de ligne ; sans effet en une
    /// ligne ou sur la dernière ligne.
    pub fn down(&mut self) {
        if !self.multiline {
            return;
        }
        let (line, col) = self.cursor_line_col();
        let lengths = self.line_lengths();
        if line + 1 >= lengths.len() {
            return;
        }
        self.move_to_line(line + 1, col);
    }

    fn move_to_line(&mut self, line: usize, col: usize) {
        let lengths = self.line_lengths();
        let len = lengths.get(line).copied().unwrap_or(0);
        self.cursor = Self::line_start(&lengths, line) + col.min(len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str, cursor: usize, multiline: bool) -> TextInput {
        let mut input = TextInput::new(value, multiline);
        input.cursor = cursor;
        input
    }

    #[test]
    fn new_places_cursor_at_end_and_is_unmodified() {
        let input = TextInput::new("café", false);
        assert_eq!(input.cursor(), 4);
        assert!(!input.is_modified());
    }

    #[test]
    fn insert_in_the_middle() {
        let mut input = at("http://api/users", 10, false);
        for c in "-v2".chars() {
            input.insert(c);
        }
        assert_eq!(input.text(), "http://api-v2/users");
        assert_eq!(input.cursor(), 13);
        assert!(input.is_modified());
    }

    #[test]
    fn home_and_end_on_single_line() {
        let mut input = at("http://api", 4, false);
        input.home();
        input.insert('x');
        input.end();
        input.insert('y');
        assert_eq!(input.text(), "xhttp://apiy");
    }

    #[test]
    fn home_and_end_stay_on_the_current_line_of_a_body() {
        let mut input = at("ab\ncde\nf", 4, true);
        input.home();
        assert_eq!(input.cursor(), 3);
        input.end();
        assert_eq!(input.cursor(), 6);
        assert_eq!(input.cursor_line_col(), (1, 3));
    }

    #[test]
    fn backspace_then_delete() {
        let mut input = at("abcd", 2, false);
        input.backspace();
        input.delete();
        assert_eq!(input.text(), "ad");
        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn extremities_have_no_effect() {
        let mut input = at("abc", 0, false);
        input.left();
        input.backspace();
        assert_eq!((input.text(), input.cursor()), ("abc", 0));
        input.end();
        input.right();
        input.delete();
        assert_eq!((input.text(), input.cursor()), ("abc", 3));
    }

    #[test]
    fn multibyte_characters_count_as_one() {
        let mut input = TextInput::new("café", false);
        input.backspace();
        assert_eq!(input.text(), "caf");
        let mut input = at("été", 1, false);
        input.right();
        assert_eq!(input.cursor(), 2);
        input.delete();
        assert_eq!(input.text(), "ét");
    }

    #[test]
    fn vertical_moves_clamp_to_line_length() {
        let mut input = TextInput::new("{\n  \"a\": 1\n}", true);
        input.up();
        assert_eq!(input.cursor_line_col(), (1, 1));
        input.end();
        assert_eq!(input.cursor_line_col(), (1, 8));
        input.down();
        assert_eq!(input.cursor_line_col(), (2, 1));
        input.down();
        assert_eq!(input.cursor_line_col(), (2, 1));
        input.up();
        input.up();
        assert_eq!(input.cursor_line_col(), (0, 1));
        input.up();
        assert_eq!(input.cursor_line_col(), (0, 1));
    }

    #[test]
    fn vertical_moves_do_nothing_on_single_line() {
        let mut input = at("abc", 1, false);
        input.up();
        input.down();
        assert_eq!(input.cursor(), 1);
    }

    #[test]
    fn horizontal_moves_cross_line_breaks() {
        let mut input = at("ab\ncd", 2, true);
        input.right();
        assert_eq!(input.cursor_line_col(), (1, 0));
        input.left();
        assert_eq!(input.cursor_line_col(), (0, 2));
    }

    #[test]
    fn join_and_split_lines() {
        let mut input = at("ab\ncd", 3, true);
        input.backspace();
        assert_eq!(input.text(), "abcd");
        input.newline();
        assert_eq!(input.text(), "ab\ncd");
        assert_eq!(input.cursor_line_col(), (1, 0));
        let mut input = at("ab\ncd", 2, true);
        input.delete();
        assert_eq!(input.text(), "abcd");
    }

    #[test]
    fn newline_has_no_effect_on_single_line() {
        let mut input = at("ab", 1, false);
        input.newline();
        input.insert('\n');
        assert_eq!(input.text(), "ab");
        assert!(!input.is_modified());
    }

    #[test]
    fn modified_back_to_initial_is_unmodified() {
        let mut input = TextInput::new("ab", false);
        input.insert('c');
        input.backspace();
        assert!(!input.is_modified());
    }

    #[test]
    fn text_before_cursor_on_line() {
        let input = at("ab\ncdé", 5, true);
        assert_eq!(input.text_before_cursor_on_line(), "cd");
    }
}
