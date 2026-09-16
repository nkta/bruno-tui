## 1. `Model` et types (D2, D3)

- [x] 1.1 Ajouter `Focus::Response` à l'enum `Focus`
      (`src/app/model.rs`)
- [x] 1.2 Ajouter `SearchScope::Response` à l'enum `SearchScope`
      (`src/app/search.rs`)
- [x] 1.3 Ajouter à `Model` : `response_scroll: u16`, `response_match:
      Option<(u16, Range<usize>)>`, `response_selection:
      Option<DetailSelection>`, initialisés comme leurs équivalents
      `detail_*` dans `Model::new`. Vérifié : `cargo build` compile
      (avertissements `dead_code` résolus par les tâches suivantes)

## 2. `detail.rs` : séparer requête et réponse (D1)

- [x] 2.1 Extraire de `detail_text` le code qui ajoute la ligne vide et
      `result_lines(outcome, filter)` à la suite des champs de la
      requête ; en faire une nouvelle fonction publique `response_text
      (model: &Model) -> Text<'static>`, vide (`Text::default()`) quand
      la sélection n'a pas de résultat exploitable
- [x] 2.2 `detail_text` ne construit plus que les champs de la requête,
      du dossier ou du nœud en erreur (appel direct à
      `request_text_with_session`/`folder_text`/`error_text`, sans plus
      jamais consulter `model.run.outcomes`). Vérifié par un nouveau
      test `detail_text_no_longer_contains_the_execution_result`
- [x] 2.3 Ajouter `response_plain_lines(model: &Model) -> Vec<String>`,
      miroir de `plain_lines` mais sur `response_text`
- [x] 2.4 Vérifié que `request_detail_line_count_is_unchanged` et les
      tests de `cursor_position_in_detail` passent toujours sans
      modification (garde-fou direct sur la contrainte de design.md,
      Context)

## 3. `update.rs` : arithmétique de défilement partagée (D3)

- [x] 3.1 Extrait de `detail_max_scroll` et `bottom_of_viewport` les
      fonctions pures `max_scroll(line_count: usize, height: u16) ->
      u16` et `viewport_bottom(scroll: u16, height: u16, line_count:
      usize) -> u16` ; `detail_max_scroll` et `bottom_of_viewport` les
      utilisent sans changer leur signature ni leur résultat. Vérifié :
      `insert_cursor_positioning_and_bounds` et les autres tests de
      défilement du détail passent toujours sans modification
- [x] 3.2 Ajouté `response_line_count`, `response_max_scroll`,
      `response_bottom_of_viewport`, `response_selection_range`,
      miroirs exacts des fonctions du détail mais sur les champs
      `response_*` de `Model` et la zone `areas.response`, réutilisant
      `max_scroll`/`viewport_bottom` de 3.1
- [x] 3.3 Ajouté `scroll_response(model: &mut Model, message: Message)`,
      miroir de `scroll_detail`

## 4. `update.rs` : focus et navigation (D2)

- [x] 4.1 `NextFocus` : `Tree → Detail → Response → Tree`. Test ajouté :
      `next_focus_cycles_through_tree_detail_and_response`
- [x] 4.2 Dans le `match model.focus` de la navigation générique,
      ajouté `Focus::Response => scroll_response(model, navigation)`
- [x] 4.3 Dans `run_selected`, ajouté `Focus::Response` à côté de
      `Focus::Tree | Focus::Detail` pour le calcul de la cible de
      lancement
- [x] 4.4 Vérifié que `start_edit` (`e`) reste inchangée ; test ajouté
      dans `start_edit_opens_session_only_in_detail_on_request` (cas 6)
      confirmant que `e` est sans effet quand la réponse a le focus.
      **Écart trouvé en implémentant** : `Message::FocusTree` (`Échap`)
      ne vérifiait que `model.detail_selection`, jamais
      `model.response_selection` — une sélection visuelle active dans
      la réponse aurait été ignorée et le focus serait passé directement
      à l'arbre sans l'annuler d'abord, contrairement à l'exigence déjà
      committée dans le delta `search-and-yank` (« Un second appui sur
      `v`, ou `Échap`, SHALL annuler la sélection »). Corrigé dans le
      même geste que 6.1/6.2 (voir groupe 6) ; test ajouté :
      `toggle_visual_then_escape_in_response_does_not_change_focus_or_
      detail_selection`

## 5. `update.rs` : recherche (D2)

- [x] 5.1 Dans `confirm_search`, ajouté `Focus::Response =>
      SearchScope::Response`
- [x] 5.2 Ajouté `search_response(model: &mut Model, pattern: &str,
      forward: bool)`, miroir de `search_detail` (réutilise
      `find_detail_match`, déjà générique sur `&[String]`) ; `run_search`
      branché sur `SearchScope::Response => search_response(...)`
- [x] 5.3 Test ajouté :
      `response_search_finds_a_match_without_moving_the_detail_scroll`

## 6. `update.rs` : sélection visuelle et copie (D2)

- [x] 6.1 `toggle_visual` (`v`) : accepte `Focus::Response` en plus de
      `Focus::Detail`, lit/écrit `response_selection`/`response_scroll`
      dans ce cas. `Message::FocusTree` corrigé en même temps (voir
      4.4) pour annuler la sélection du panneau focalisé, Détail ou
      Réponse, avant de rendre le focus à l'arbre
- [x] 6.2 `yank` (`y`) : même généralisation, via
      `response_plain_lines`/`response_selection_range`/
      `response_selection` quand `model.focus == Focus::Response`
- [x] 6.3 Tests ajoutés :
      `visual_selections_in_detail_and_response_are_independent`,
      `toggle_visual_then_escape_in_response_does_not_change_focus_or_
      detail_selection`

## 7. `update.rs` : réinitialisation à la sélection

- [x] 7.1 `clear_detail_view_state` réinitialise aussi `response_scroll`,
      `response_selection`, `response_match`. Test ajouté :
      `changing_selection_clears_both_detail_and_response_view_state`
- [x] 7.2 Confirmé par relecture : `open_filter`/`confirm_filter`
      n'ont nécessité aucune modification (D6) ; seul le point de rendu
      a changé (groupe 9)

## 8. `view/mod.rs` : disposition à trois zones (D4)

- [x] 8.1 Ajouté `response: Rect` à `Areas` ; constantes
      `MIN_DETAIL_WIDTH = 18`, `MIN_RESPONSE_WIDTH = 18` ; `MIN_WIDTH`
      changé de 40 à 60
- [x] 8.2 Calcul à trois zones implémenté dans `layout()` (`tree`
      inchangé à 35 % avec son plancher, le reste partagé 50/50 entre
      `detail` et `response` avec leurs propres planchers)
- [x] 8.3 `layout_respects_minimums` mis à jour pour la nouvelle
      géométrie (60 colonnes, trois largeurs). Aucune assertion de
      contenu textuel modifiée (confirmé par `git diff`, voir 10.1)
- [x] 8.4 Test ajouté : `too_small_just_below_the_new_minimum_width`
      (59 colonnes → trop petit, 60 → trois panneaux affichés)

## 9. `view/mod.rs` et `panels.rs` : rendu du panneau Réponse (D5, D6)

- [x] 9.1 `panels::empty_state_message` rendue `pub(crate)`
- [x] 9.2 Ajouté `render_response(model, frame, area)` dans
      `src/app/view/mod.rs`, et `detail::render_response_text` (miroir
      de `render_text`) pour la teinte de sélection et la surbrillance
      de recherche
- [x] 9.3 `render_response` appelée dans `view()` à côté du rendu du
      détail, dans `areas.response`
- [x] 9.4 Test ajouté : `response_panel_shows_result_separately_from_
      detail` (verdict/corps dans la réponse, absents du détail ;
      « aucun résultat » pour requête non exécutée, dossier, nœud en
      erreur)

## 10. Non-régression globale

- [x] 10.1 Relu chaque scénario texte déjà spécifié par `tui-shell`,
      `field-editing`, `diagnostics-and-history`, `environment-picker`
      touchant à l'affichage. Confirmé par `git diff` : seules deux
      lignes d'assertion existantes ont changé, toutes deux de
      géométrie (`layout_respects_minimums`, largeur 40→60), aucune
      assertion de contenu textuel modifiée
- [x] 10.2 Vérifié que les scénarios de `response-filter` passent sans
      modification de leur logique (`open_filter_availability_
      conditions` et les autres tests de filtre) ; seules les
      assertions qui lisaient `detail_text` pour vérifier le corps/le
      filtre ont été redirigées vers `response_text`, sans changer ce
      qu'elles vérifient
- [x] 10.3 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis `cargo
      test`, tous verts (214 tests lib + toutes les suites
      d'intégration)
