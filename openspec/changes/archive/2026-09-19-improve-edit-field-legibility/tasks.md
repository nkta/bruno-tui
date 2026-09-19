## 1. Styles

- [x] 1.1 Ajouter `theme::EDITABLE_BODY` dans `src/app/view/theme.rs` (fond
      légèrement distinct de `theme::BACKGROUND`, cf. design.md « Corps
      éditable ») et vérifier `cargo build` sans avertissement.

## 2. Entrées clé/valeur (en-têtes, paramètres)

- [x] 2.1 Faire construire à `entries()` (`src/app/view/detail.rs:108`,
      lecture seule) une `Line` à deux spans (clé stylée `theme::LABEL`,
      valeur au style normal) au lieu d'une chaîne concaténée unique, en
      conservant le préfixe `"  "`, le `": "` et le texte brut
      concaténé identique (`plain_lines` inchangé) ; vérifier avec un
      test qui compare le texte brut avant/après sur une requête ayant
      des en-têtes (ex. fixture `parser-cases/post-json.bru`).
- [x] 2.2 Faire de même pour `editable_entries()`
      (`src/app/view/detail.rs:211`) : entrée existante, entrée
      désactivée (garder le style DIM déjà appliqué en plus du style de
      clé), clé en cours de renommage (`InputTarget::RenameKey`), entrée
      provisoire pendant un ajout (`InputTarget::NewKey`/`NewValue`).
      Vérifier que `prefix_width` (utilisé pour la position du curseur de
      texte) reste calculé sur la même largeur affichée qu'avant.
- [x] 2.3 Ajouter/adapter les tests de `src/app/view/detail.rs` (module
      `tests`) couvrant les scénarios de la spec `visual-theme` : « En-tête
      affiché hors session », « En-tête désactivé pendant une session
      d'édition », « Entrée provisoire pendant un ajout » — vérifier que
      le style de la clé diffère de celui de la valeur dans chaque cas.

## 3. Corps éditable

- [x] 3.1 Appliquer `theme::EDITABLE_BODY` à chaque `Line` du corps dans
      `request_text_and_fields` (`src/app/view/detail.rs:394-426`),
      conditionné par le même booléen déjà calculé
      (`EditableField::list_for(view).contains(&field)`), avant le
      `tint_line(..., FIELD_CURSOR_STYLE)` existant sur la ligne sous le
      curseur de session ; vérifier que le curseur de session reste
      visible par-dessus (patch de style).
- [x] 3.2 Ajouter un test couvrant le scénario « Corps éditable distingué
      du texte de détail » (fixture avec `body:json`, ex.
      `writer-cases/json-body.bru`) et le scénario « Corps sans type
      éditable non affecté » (fixture avec `body: none`, ex.
      `runner-probe/ok.bru`) : vérifier la présence/absence du style
      `EDITABLE_BODY` sur les lignes concernées.

## 4. Non-régression et validation

- [x] 4.1 Exécuter `cargo fmt`, `cargo clippy -- -D warnings`, puis
      `cargo test`, et vérifier qu'ils passent sans erreur ni
      avertissement.
- [x] 4.2 Lancer `bruno-tui` sur `tests/fixtures/collections/parser-cases`
      (requête `post-json`, qui a des en-têtes et un corps JSON) et
      vérifier visuellement : clé d'en-tête distincte de sa valeur, corps
      visuellement identifiable comme zone de saisie, comportement des
      touches d'édition (`field-editing`) inchangé.
      Vérifié manuellement par l'utilisateur : "ok pas mal".
