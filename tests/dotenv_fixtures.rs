//! Lecture `.env` comparée aux valeurs produites par le vrai `dotenv.parse`.
//!
//! Valeurs attendues relevées avec `dotenv` 16.6.1, embarqué par `bru`
//! 2.13.2 (`@usebruno/cli/node_modules/dotenv`), par :
//! `node -e 'console.log(JSON.stringify(require(dotenv).parse(
//! fs.readFileSync(fichier, "utf8"))))'` sur chaque fixture.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bruno_tui::secrets::dotenv::parse;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/dotenv")
        .join(name)
}

fn parsed(name: &str) -> BTreeMap<String, String> {
    let bytes = std::fs::read(fixture(name)).expect("lecture de la fixture");
    parse(&String::from_utf8_lossy(&bytes))
        .into_iter()
        .map(|(key, value)| (key, value.expose().to_owned()))
        .collect()
}

fn expected(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn cases_match_dotenv_parse() {
    assert_eq!(
        parsed("cases.env"),
        expected(&[
            ("BASIC", "basic"),
            ("EXPORTED", "exported"),
            ("export", "export-as-key"),
            ("INDENTED", "indented value"),
            ("COLON", "colon-value"),
            ("EMPTY", ""),
            ("EMPTY_THEN_NEXT", ""),
            ("NEXT", "next"),
            ("SINGLE", "single # not comment"),
            ("DOUBLE", "double\nwith newline"),
            ("BACKTICK", "back 'tick' \"q\""),
            ("LITERAL_ESCAPE", "single\\nkept"),
            ("INLINE", "value"),
            ("HASH_IN_DOUBLE", "a # b"),
            ("MULTI", "line one\nline two"),
            ("UNCLOSED", "\"unclosed"),
            ("AFTER_UNCLOSED", "after"),
            ("ESCAPED_QUOTE", "say \\\"hi\\\""),
            ("TRAILING_QUOTED", "\"a\" trailing"),
            ("TWO_QUOTED", "a\" \"b"),
            ("DOTTED.key-name", "dotted"),
            ("DUP", "second"),
            ("EQUALS", "a=b=c"),
            ("QUOTE_THEN_COMMENT", "x"),
            ("SPACE_BEFORE_EQ", "spaced"),
            ("TAB_VALUE", "tabbed"),
            ("UNICODE", "héllo wörld"),
        ])
    );
}

#[test]
fn line_endings_match_dotenv_parse() {
    assert_eq!(
        parsed("cases-crlf.env"),
        expected(&[("CRLF", "crlf"), ("CR_ONLY", "cr"), ("LAST", "last")])
    );
}

#[test]
fn backtracking_quirks_match_dotenv_parse() {
    assert_eq!(
        parsed("backtracking.env"),
        expected(&[
            ("ESC_SINGLE", "'x\\'y"),
            ("BLANK_THEN_QUOTED", "on next line"),
            ("COLON_NL", "TAKEN=1"),
            ("DQ_ESC", "a\\"),
            ("AFTER_DQ", "2"),
            ("BS_END", "a\\\\"),
            ("SPACED_EXPORT", "v"),
            ("QUOTE_INSIDE", "a\"b"),
        ])
    );
}
