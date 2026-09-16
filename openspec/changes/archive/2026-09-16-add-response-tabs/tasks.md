## 1. `Model` : enum `ResponseTab` (D3)

- [x] 1.1 Ajouter `enum ResponseTab { Body, Headers, Tests }` à
      `src/app/model.rs` (`Copy`, `Clone`, `Debug`, `PartialEq`, `Eq`,
      `Default` sur `Body`), avec `fn next(self) -> Self` et
      `fn previous(self) -> Self` (cycle circulaire à trois valeurs)
- [x] 1.2 Ajouté `Model::response_tab: ResponseTab`, initialisé à
      `ResponseTab::Body` dans `Model::new`. Vérifié : `cargo build`
      compile

## 2. `detail.rs` : bandeau, onglets, barre d'onglets (D1, D2)

- [x] 2.1 Extrait de `result_lines` la fonction `status_band(outcome)
      -> Vec<Line<'static>>` (section « Résultat », Verdict, Statut,
      Erreur éventuelle, Temps de réponse), sans les en-têtes
- [x] 2.2 Extrait `body_tab_lines(outcome, filter) -> Vec<Line<'static>>`
      (ligne de filtre + résultat/erreur, ou corps brut), sans le
      `section("Corps de réponse")` qui l'introduisait
- [x] 2.3 Extrait `headers_tab_lines(outcome) -> Vec<Line<'static>>`
      (en-têtes de réponse), sans le `section("En-têtes de réponse")`
      qui les introduisait
- [x] 2.4 Extrait `tests_tab_lines(outcome) -> Vec<Line<'static>>` (les
      quatre blocs `push_checks` existants, inchangés)
- [x] 2.5 Ajouté `tab_bar(active: ResponseTab) -> Line<'static>` :
      libellés « Corps », « En-têtes », « Tests » séparés par deux
      espaces, l'actif en `theme::SECTION`, les autres en `theme::LABEL`
- [x] 2.6 Réécrit `response_text(model)` : `status_band`, ligne vide,
      `tab_bar(model.response_tab)`, ligne vide, puis
      `body_tab_lines`/`headers_tab_lines`/`tests_tab_lines` selon
      `model.response_tab`. `result_lines` supprimée
- [x] 2.7 Mis à jour `filter_applied_replaces_body_with_formatted_
      result_and_filter_line` : les assertions sur « Corps de réponse »
      cherchent désormais le corps brut (`[1,2]`) directement. Mis à
      jour aussi `result_detail` (aide de test) : active l'onglet Tests
      pour que les tests `result_section_for_*` continuent de vérifier
      les assertions/tests, désormais dans cet onglet

## 3. `update.rs` : bascule d'onglet et réinitialisation (D4, D5)

- [x] 3.1 Ajouté `previous_response_tab`/`next_response_tab` (et
      `reset_response_tab_view_state`) dans `src/app/update.rs` :
      appliquent `ResponseTab::previous`/`next` à `model.response_tab`,
      puis réinitialisent `response_scroll`, `response_match`,
      `response_selection`
- [x] 3.2 Dans la branche `Focus::Response` du `match` de navigation
      générique, `Message::Left`/`Message::Right` interceptés vers ces
      deux fonctions avant de déléguer le reste à `scroll_response`
- [x] 3.3 `clear_detail_view_state` réinitialise aussi
      `model.response_tab = ResponseTab::Body`
- [x] 3.4 `open_filter` réinitialise `model.response_tab =
      ResponseTab::Body` après ses gardes existantes, sur le chemin de
      succès

## 4. Tests du cycle d'onglets et de la réinitialisation

- [x] 4.1 Test ajouté : `response_tab_cycles_forward_with_right`
- [x] 4.2 Test ajouté : `response_tab_cycles_backward_with_left`
- [x] 4.3 Test ajouté : `changing_selection_resets_the_active_response_
      tab`
- [x] 4.4 Test ajouté : `opening_the_filter_switches_to_the_body_tab`
- [x] 4.5 Test ajouté : `changing_response_tab_resets_scroll_match_and_
      selection`

## 5. Rendu et non-régression

- [x] 5.1 Test ajouté : `response_status_band_stays_visible_across_
      tabs` (Résultat/Verdict/Statut visibles sur les trois onglets)
- [x] 5.2 Test ajouté : `response_tab_content_is_shown_only_when_active`
      (corps, en-têtes, assertions/tests jamais affichés ensemble)
- [x] 5.3 Relus les tests existants de `response-filter` et de
      recherche/sélection/copie dans la réponse : tous passent sans
      modification (fixtures `green.bru`/`json.bru`, corps dans
      l'onglet Corps, actif par défaut). **Écart trouvé en
      implémentant**, hors de ce périmètre direct : le test
      d'intégration `launching_a_request_shows_its_result`
      (`tests/app_run.rs`) vérifiait un message d'assertion en échec
      désormais dans l'onglet Tests, invisible dans l'onglet Corps actif
      par défaut. Corrigé en envoyant `Tab`, `Tab`, `→`, `→` avant la
      vérification, pour porter le focus sur la réponse puis atteindre
      l'onglet Tests — conforme à `response-tabs`, pas une régression
- [x] 5.4 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis `cargo
      test`, tous verts (221 tests lib + toutes les suites
      d'intégration)

## 6. Mise en forme du corps par défaut via jq (D6)

- [x] 6.1 Ajouté `filter::pretty_print(data: &Value) -> String`
      (`src/app/filter.rs`), extrayant la configuration de mise en forme
      déjà utilisée par `evaluate` (`pretty_print_config`, factorisée)
- [x] 6.2 `detail::body_lines` : une chaîne reste affichée telle quelle,
      un objet ou un tableau passe par `filter::pretty_print` au lieu
      de la sérialisation compacte de `serde_json`
- [x] 6.3 Mis à jour les tests dont les assertions supposaient un corps
      compact sur une ligne (`response_search_finds_a_match_without_
      moving_the_detail_scroll`, `filter_applied_replaces_body_with_
      formatted_result_and_filter_line`, `response_tab_content_is_
      shown_only_when_active`) pour chercher un fragment stable de la
      forme indentée (`"a": [`) plutôt que le format compact disparu
- [x] 6.4 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis `cargo
      test`, tous verts (221 tests lib + toutes les suites
      d'intégration)
