## 1. Fixture réelle

- [x] 1.1 Créer `tests/fixtures/collections/runner-probe-errored/` (`bruno.json` copié de `runner-probe`, `boom.bru` avec `script:pre-request` faisant `throw new Error("pre-request failure (fixture)")`, cf. design D3) ; vérifier que `tests/parser_fixtures.rs` passe toujours
- [x] 1.2 Étendre `scripts/gen-report-fixtures.sh` pour produire aussi `tests/fixtures/reports/pre-request-error.json` depuis `runner-probe-errored` (design D4) ; vérifier en lançant le script que `mixed.json` n'a pas d'autre diff que dates/`uid`/durées (ou le restaurer) et que la nouvelle fixture contient `"request": {"method": null, "url": null, "headers": null, "data": null}` et `"status": "error"`
- [x] 1.3 Documenter `pre-request-error.json` dans `tests/fixtures/reports/README.md` (version `bru`, date, commande, code de sortie, cas couvert) ; vérifier par relecture qu'aucun secret n'y figure

## 2. Modèle de rapport

- [x] 2.1 Dans `tests/report_fixtures.rs`, ajouter le test de désérialisation de `pre-request-error.json` (requête sans méthode/URL/en-têtes, réponse `Error`, statut `Error`, message, listes vides, résumé, verdict en échec) ; vérifier qu'il échoue avant correction
- [x] 2.2 Passer `RequestInfo::method`, `url` et `headers` en `Option` avec `#[serde(default)]` et commentaire expliquant le cas « requête non envoyée » (design D1) ; vérifier que le test 2.1 passe
- [x] 2.3 Mettre à jour les constructions de `RequestInfo` (`base_result` dans `src/app/view/detail.rs`, `sample_request_result` dans `src/app/update.rs`) et les assertions existantes sur `request` dans `tests/report_fixtures.rs` ; vérifier que `cargo test` compile et passe

## 3. Affichage de l'erreur

- [x] 3.1 Ajouter dans `src/app/view/detail.rs` un test rendant le bandeau d'un résultat issu de `pre-request-error.json` (« Verdict : échec », « Statut : aucune réponse », message d'erreur, visible sur les onglets En-têtes et Tests) et un test vérifiant l'absence de ligne « Erreur » pour un résultat HTTP 200 sans erreur
- [x] 3.2 Dans `status_band`, afficher `field("Erreur", …)` dès que `result.error` est présent, hors de la branche `ResponseStatus::Error` (design D2) ; vérifier que les tests 3.1 et `result_section_for_no_response` passent

## 4. Vrai bru et validation

- [x] 4.1 Ajouter dans `tests/runner_real_bru.rs` (ignoré par défaut) un test exécutant `runner-probe-errored` et attendant une issue Terminée avec un résultat en erreur et son message ; vérifier avec `cargo test --test runner_real_bru -- --ignored`
- [x] 4.2 Vérifier `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` sans erreur, puis `openspec validate fix-errored-request-report --strict`
