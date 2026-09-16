## 1. Palette (`src/app/view/theme.rs`)

- [x] 1.1 Ajouter la constante `BACKGROUND` (`Style` avec
      `Color::Rgb(18, 22, 40)` en fond) et la constante `BORDER`
      (`Style` avec `Color::Rgb(90, 150, 110)` en avant-plan), voir
      design.md tableau de palette
- [x] 1.2 Changer `METHOD` de Cyan vers `Color::Rgb(120, 200, 150)`
- [x] 1.3 Changer `SECTION` pour utiliser la même valeur `Color::Rgb(120,
      200, 150)` que `METHOD`, en conservant Bold + souligné
- [x] 1.4 Changer `FOCUS` de Yellow bold vers `Color::Rgb(224, 138, 60)`
      bold
- [x] 1.5 Mettre à jour le test
      `every_category_has_a_foreground_color_except_pure_text_styles`
      pour inclure `BORDER` dans la liste des styles avec `fg`, et
      ajouter une assertion que `BACKGROUND.bg` est bien défini
      (`Some`)
- [x] 1.6 Vérifier que `categories_that_could_be_confused_have_distinct_colors`
      passe toujours sans modification (FAILURE/LOAD_ERROR et
      RUNNING/FOCUS restent distincts après les changements de 1.2-1.4)
- [x] 1.7 `cargo test --lib theme::` vert

## 2. Fond d'application (`src/app/view/mod.rs`)

- [x] 2.1 Ajouter au tout début de `view()` le rendu d'un
      `Block::default().style(theme::BACKGROUND)` sur `frame.area()`,
      avant la vérification de taille minimale (voir design.md,
      décision « Fond d'application »)
- [x] 2.2 Ajouter un test `TestBackend` vérifiant que le fond
      (`Cell::bg`) est identique à `theme::BACKGROUND.bg` en un point
      de la ligne de titre, un point d'un panneau, et un point de la
      barre d'état, sur l'état `Loaded`
- [x] 2.3 Ajouter la même vérification de fond pour l'état `Loading`,
      l'état `Failed`, et l'écran « terminal trop petit » (`too_small_
      and_absurd_sizes`), pour confirmer que le fond est posé
      inconditionnellement
- [x] 2.4 Vérifier que `selection_and_match_are_highlighted_distinctly_
      on_screen` passe toujours sans modification (les surbrillances de
      recherche/sélection restent visibles par-dessus le nouveau fond)

## 3. Bordure par défaut (`panel()`, `src/app/view/mod.rs`)

- [x] 3.1 Faire pointer `panel()` vers `theme::BORDER` au lieu de
      `Style::new()` pour un panneau non focalisé
- [x] 3.2 Ajouter un test `TestBackend` vérifiant qu'un caractère de
      bordure d'un panneau non focalisé porte `theme::BORDER.fg`, et
      qu'un panneau focalisé porte toujours `theme::FOCUS.fg` (non-
      régression du focus, valeur mise à jour)
- [x] 3.3 Vérifié empiriquement (`panel_border_and_title_use_border_or_
      focus_color`) : le titre d'un panneau porte bien la même couleur
      que sa bordure sans changement de code supplémentaire — `ratatui`
      applique `border_style` au titre par défaut, confirmant la
      décision de design.md

## 4. Non-régression globale

- [x] 4.1 Relire chaque scénario texte déjà spécifié par `tui-shell`,
      `diagnostics-and-history`, `field-editing`, `response-filter`,
      `search-and-yank` et `environment-picker` touchant à l'affichage,
      et confirmer par `git diff` qu'aucune assertion de contenu
      textuel existante n'a changé (seules des assertions de style/fond
      ajoutées)
- [x] 4.2 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test`, tous verts (206 tests lib), avant de considérer le
      changement terminé
