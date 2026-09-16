//! Filtrage jq du corps de réponse.
//!
//! Ce module implémente l'évaluation d'un filtre jq sur le corps JSON
//! d'une réponse de requête (`serde_json::Value`), ainsi que la structure
//! d'état associée.
//!
//! # Coordination avec `add-search-and-yank` et `add-field-editing`
//!
//! Ce module s'intègre au mécanisme de capture modale du texte introduit
//! par `add-search-and-yank` (D2 de son design) via `to_message(event,
//! capture: Option<TextCapture>)`, documenté au D3 et dans le Contexte de
//! `design.md`. `TextCapture` est une énumération ouverte à laquelle ce
//! changement ajoute la variante `Filter`, traitée par `filter_capture_message`
//! dans `message.rs` et activée par `Model::text_capture()`. L'exclusion mutuelle
//! entre recherche, édition et filtrage est garantie par construction car
//! une capture active intercepte toutes les touches avant la consultation de
//! la table hors-saisie.

use std::path::PathBuf;

/// Résultat de l'application d'un filtre jq.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterResult {
    /// Une entrée par valeur produite, déjà mise en forme.
    Output(Vec<String>),
    /// Message d'erreur de compilation ou d'exécution.
    Error(String),
}

/// État du filtre associé à une requête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterState {
    /// Chemin de la requête pour laquelle ce filtre est ouvert.
    pub target: PathBuf,
    /// Vrai si la ligne de saisie du filtre est active.
    pub editing: bool,
    /// Texte en cours de frappe.
    pub draft: String,
    /// Filtre validé et son résultat, `None` avant toute validation
    /// réussie ou après annulation.
    pub applied: Option<FilterResult>,
}

impl FilterState {
    /// Crée un nouvel état de filtre en cours d'édition pour une requête cible.
    pub fn new(target: PathBuf) -> Self {
        Self {
            target,
            editing: true,
            draft: String::new(),
            applied: None,
        }
    }
}

/// Évalue un filtre jq sur une valeur JSON sans aucune I/O.
pub fn evaluate(filter_src: &str, data: &serde_json::Value) -> FilterResult {
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs()
        .chain(jaq_std::funs())
        .chain(jaq_json::funs());

    let loader = jaq_core::load::Loader::new(defs);
    let arena = jaq_core::load::Arena::default();

    let modules = match loader.load(
        &arena,
        jaq_core::load::File {
            code: filter_src,
            path: (),
        },
    ) {
        Ok(modules) => modules,
        Err(errs) => return FilterResult::Error(format_load_errors(errs)),
    };

    let compiler = jaq_core::Compiler::default().with_funs(funs);
    let filter = match compiler.compile(modules) {
        Ok(filter) => filter,
        Err(errs) => return FilterResult::Error(format_compile_errors(errs)),
    };

    let input = match serde_json::from_value::<jaq_json::Val>(data.clone()) {
        Ok(val) => val,
        Err(err) => {
            return FilterResult::Error(format!("erreur de conversion de la réponse : {err}"));
        }
    };

    let ctx = jaq_core::Ctx::<jaq_core::data::JustLut<jaq_json::Val>>::new(
        &filter.lut,
        jaq_core::Vars::new([]),
    );

    let pp = pretty_print_config();

    let mut outputs = Vec::new();
    for item in filter.id.run((ctx, input)) {
        let val = match item {
            Ok(v) => v,
            Err(exn) => {
                let msg = match exn.get_err() {
                    Ok(err) => err.to_string(),
                    Err(exn) => match exn.get_halt() {
                        Ok(code) => format!("halt({code})"),
                        Err(_) => "erreur d'exécution".to_string(),
                    },
                };
                return FilterResult::Error(msg);
            }
        };
        let mut buf = Vec::new();
        if let Err(err) = jaq_json::write::write(&mut buf, &pp, 0, &val) {
            return FilterResult::Error(format!("erreur de formatage : {err}"));
        }
        outputs.push(String::from_utf8_lossy(&buf).into_owned());
    }

    FilterResult::Output(outputs)
}

fn pretty_print_config() -> jaq_json::write::Pp {
    jaq_json::write::Pp {
        indent: Some("  ".to_string()),
        sep_space: true,
        sort_keys: false,
        ..Default::default()
    }
}

/// Met en forme lisible une valeur JSON structurée (objet ou tableau),
/// avec le même moteur et la même indentation que l'évaluation d'un
/// filtre (`evaluate`), pour l'affichage du corps de réponse par
/// défaut, sans filtre actif (`response-tabs`). Retombe sur la
/// sérialisation compacte de `serde_json` si la valeur ne peut pas être
/// convertie ou mise en forme — ne devrait pas arriver en pratique pour
/// une valeur déjà désérialisée par `bru-runner`.
pub fn pretty_print(data: &serde_json::Value) -> String {
    let Ok(val) = serde_json::from_value::<jaq_json::Val>(data.clone()) else {
        return data.to_string();
    };
    let mut buf = Vec::new();
    if jaq_json::write::write(&mut buf, &pretty_print_config(), 0, &val).is_err() {
        return data.to_string();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn format_load_errors(errors: jaq_core::load::Errors<&str, ()>) -> String {
    let mut msgs = Vec::new();
    for (_file, err) in errors {
        match err {
            jaq_core::load::Error::Lex(lex_errs) => {
                for (exp, found) in lex_errs {
                    msgs.push(format!(
                        "erreur lexicale : attendu {exp:?}, trouvé '{found}'"
                    ));
                }
            }
            jaq_core::load::Error::Parse(parse_errs) => {
                for (exp, found) in parse_errs {
                    let found_str = if found.is_empty() {
                        "fin de saisie"
                    } else {
                        found
                    };
                    msgs.push(format!(
                        "erreur de syntaxe : attendu {}, trouvé '{found_str}'",
                        exp.as_str()
                    ));
                }
            }
            jaq_core::load::Error::Io(io_errs) => {
                for (path, err) in io_errs {
                    msgs.push(format!("erreur d'import pour {path} : {err}"));
                }
            }
        }
    }
    if msgs.is_empty() {
        "erreur de syntaxe dans le filtre".to_string()
    } else {
        msgs.join("\n")
    }
}

fn format_compile_errors(errors: jaq_core::compile::Errors<&str, ()>) -> String {
    let mut msgs = Vec::new();
    for (_file, errs) in errors {
        for (sym, undef) in errs {
            msgs.push(format!("symbole indéfini ({}) : '{sym}'", undef.as_str()));
        }
    }
    if msgs.is_empty() {
        "erreur de compilation dans le filtre".to_string()
    } else {
        msgs.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn evaluate_valid_extract_and_multi_output() {
        let data = json!({"a": [1, 2], "b": null});

        // Test 2.1: `.a` produit `[1, 2]` mis en forme
        let res_a = evaluate(".a", &data);
        match res_a {
            FilterResult::Output(outputs) => {
                assert_eq!(outputs.len(), 1);
                assert_eq!(outputs[0], "[\n  1,\n  2\n]");
            }
            FilterResult::Error(err) => panic!("erreur inattendue : {err}"),
        }

        // Test 2.1: `.a[]` produit deux sorties `1` et `2`
        let res_iter = evaluate(".a[]", &data);
        match res_iter {
            FilterResult::Output(outputs) => {
                assert_eq!(outputs.len(), 2);
                assert_eq!(outputs[0], "1");
                assert_eq!(outputs[1], "2");
            }
            FilterResult::Error(err) => panic!("erreur inattendue : {err}"),
        }
    }

    #[test]
    fn evaluate_syntax_error_and_runtime_error() {
        // Test 2.2: filtre syntaxiquement invalide `.a.b |`
        let data = json!({"a": [1, 2], "b": null});
        let res_invalid = evaluate(".a.b |", &data);
        match res_invalid {
            FilterResult::Error(msg) => {
                assert!(!msg.is_empty(), "le message d'erreur ne doit pas être vide");
            }
            FilterResult::Output(_) => panic!("le filtre invalide aurait dû échouer"),
        }

        // Test 2.2: filtre valide mais incompatible `map(.)` sur chaîne HTML de mixed.json
        let html_data = json!("<html><body>probe</body></html>\n");
        let res_incompatible = evaluate("map(.)", &html_data);
        match res_incompatible {
            FilterResult::Error(msg) => {
                assert!(!msg.is_empty(), "le message d'erreur ne doit pas être vide");
            }
            FilterResult::Output(_) => panic!("map(.) sur une chaîne aurait dû échouer"),
        }
    }

    #[test]
    fn evaluate_string_length() {
        // Test 2.3: `. | length` sur une chaîne produit sa longueur
        let html_data = json!("<html><body>probe</body></html>\n");
        let res = evaluate(". | length", &html_data);
        match res {
            FilterResult::Output(outputs) => {
                assert_eq!(outputs.len(), 1);
                assert_eq!(outputs[0], "32");
            }
            FilterResult::Error(err) => panic!("erreur inattendue : {err}"),
        }
    }
}
