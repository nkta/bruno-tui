## 1. Palette centralisée

- [x] 1.1 Créer `src/app/view/theme.rs` avec une constante `Style` par
      catégorie (méthode, succès, échec, en cours, erreur de chargement,
      focus, titre, section, libellé, message unique), voir design.md
      tableau de palette. Vérifier que le module compile et que
      `cargo doc` ne signale rien de mort (`#[allow(dead_code)]` interdit
      : chaque constante doit être utilisée dès les tâches suivantes)
- [x] 1.2 Écrire un test unitaire dans `theme.rs` vérifiant que les
      couleurs des catégories « échec d'exécution » et « erreur de
      chargement » sont différentes, et que « en cours d'exécution » et
      « focus actif » sont différentes (les deux confusions identifiées
      dans design.md)

## 2. Arbre (`src/app/view/tree.rs`)

- [x] 2.1 Remplacer les `Style::new().fg(Color::X)` locaux par les
      constantes de `theme.rs` pour la méthode, les marqueurs de succès/
      échec/en cours, et le marqueur d'erreur de chargement (nouvelle
      couleur Magenta). Vérifier que `success_failure_and_running_marks_
      are_distinct_from_the_error_mark` passe toujours (texte inchangé)
- [x] 2.2 Ajouter un test vérifiant que le marqueur d'échec d'exécution
      (`FAILURE_MARK`) et le marqueur d'erreur de chargement
      (`ERROR_MARK`) portent des styles de couleur différents sur la
      même ligne rendue

## 3. Détail (`src/app/view/detail.rs`)

- [x] 3.1 Changer `field()` pour appliquer le style de libellé
      (`theme::LABEL`, Dim) au lieu de Bold, sans changer le nombre de
      `Span` ni de `Line` produits. Vérifier que
      `insert_cursor_positioning_and_bounds` passe toujours sans
      modification (preuve que la structure de ligne n'a pas changé)
- [x] 3.2 Changer `section()` pour ajouter la couleur de section
      (`theme::SECTION`, Cyan) en plus de Bold+souligné existants
- [x] 3.3 Garder `title()` inchangé (Bold seul, cf. design.md) ; ajouter
      un test de rendu vérifiant que les styles de `title()`, `section()`
      et `field()` sont tous les trois différents sur une même vue de
      détail (ex. `post-json`, qui a un titre, une section « En-têtes »,
      et au moins un champ)
- [x] 3.4 Appliquer `theme::LABEL`/`theme::SECTION` de la même façon dans
      `folder_text` et `error_text`, qui ne sont jamais en session
      d'édition (aucune contrainte de `cursor_position_in_detail`) ;
      vérifier que les tests `folder_with_meta_and_without` et
      `error_node` passent toujours sans changement de texte
- [x] 3.5 Vérifier explicitement par un test que
      `request_text_with_session` produit exactement le même nombre de
      lignes avant/après ces changements, pour la fixture `post-json`
      (garde-fou direct sur la contrainte de design.md)

## 4. Panneaux à message unique (`src/app/view/panels.rs`)

- [x] 4.1 Ajouter une fonction utilitaire de centrage vertical (lignes
      vides calculées, cf. design.md) utilisée par `render_diagnostics`
      et `render_history` uniquement quand la liste est vide, avec le
      message stylé en `theme::EMPTY_MESSAGE`. Vérifier par un test
      `TestBackend` que le message « aucune erreur » n'apparaît plus sur
      la première ligne intérieure du panneau quand le terminal fait
      30 lignes, mais reste présent et inchangé textuellement
- [x] 4.2 Même vérification pour « aucune exécution » (historique vide)

## 5. Focus et bordures (`src/app/view/mod.rs`)

- [x] 5.1 Faire pointer `panel()` vers `theme::FOCUS` au lieu de
      `Color::Yellow` en dur, sans changer le comportement (même couleur,
      seulement centralisée). Vérifier que les tests de rendu existants
      sur le focus (bordure) passent toujours

## 6. Non-régression globale

- [x] 6.1 Relire chaque scénario texte déjà spécifié par `tui-shell`,
      `diagnostics-and-history`, `field-editing`, `response-filter`,
      `search-and-yank` et `environment-picker` touchant à l'affichage,
      et confirmer qu'aucun test de rendu existant n'a dû changer son
      assertion de contenu textuel (seules des assertions de style
      peuvent être ajoutées, jamais remplacer une assertion de texte).
      Confirmé par `git diff` : aucune ligne `assert*` existante
      modifiée dans `src/app/view/`, uniquement des ajouts. En prime,
      appliqué `theme::LOAD_ERROR`/`theme::SUCCESS`/`theme::FAILURE`
      dans `panels.rs` (diagnostics, historique, sélection
      d'environnement) au-delà du seul `tree.rs` mentionné en tâche 2.1,
      pour respecter la même exigence de cohérence par catégorie
      partout où elle s'applique
- [x] 6.2 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test`, tous verts (203 tests lib + toutes les suites
      d'intégration), avant de considérer le changement terminé
