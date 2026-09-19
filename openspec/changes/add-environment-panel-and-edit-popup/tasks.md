## 1. Mise en page : panneau permanent

- [x] 1.1 Ajouter `Areas.environment: Rect` dans `src/app/view/mod.rs`,
      au-dessus de `response_status`, même largeur, hauteur fixe à deux
      paliers (`ENV_PANEL_HEIGHT` = 6, `ENV_PANEL_COMPACT_HEIGHT` = 3,
      seuil `STATUS_PANEL_FULL_MIN_TERMINAL_HEIGHT` réutilisé) —
      `response_status`/`response` se décalent et rétrécissent d'autant
      (design D1). Vérifier par un test que la somme des hauteurs
      (environment + response_status + response) égale `body_height`
      pour plusieurs tailles d'écran, y compris `MIN_WIDTH`×`MIN_HEIGHT`.
- [x] 1.2 Dans `view()`, dessiner le panneau Environnement dans tous les
      cas où une collection est chargée (plus seulement quand il a le
      focus), avec la bordure focalisée seulement si
      `model.focus == Focus::EnvironmentPicker`. Vérifier par un test de
      rendu que le panneau est visible focus sur `Tree`.
- [x] 1.3 Retirer le branchement `Focus::EnvironmentPicker` de la
      superposition en haut à droite (le menu déroulant flottant de
      `redesign-environment-picker-as-dropdown` disparaît, remplacé par
      la zone permanente). Vérifier par `cargo build` et par les tests
      existants de rendu (à adapter, voir tâche 5.2).

## 2. Activer un environnement : Entrée rejoint Droite

- [x] 2.1 Dans `navigate_environment_picker` (`src/app/update.rs`),
      traiter `Message::Enter` comme `Message::Right`
      (`select_environment_picker`). Vérifier par un test que `Entrée`
      active un environnement valide, désactive sur « Aucun », ne fait
      rien sur une entrée en erreur.
- [x] 2.2 `select_environment_picker` ne change plus `model.focus` (le
      panneau étant permanent, il n'y a plus rien à refermer). Vérifier
      par un test que le focus reste sur `Focus::EnvironmentPicker`
      après activation par `Entrée` ou `Droite`.

## 3. Popup d'édition : ouverture et fermeture

- [x] 3.1 Dans `update.rs`, ajouter une branche pour
      `Message::StartEdit` quand `model.focus == Focus::EnvironmentPicker`
      et la sélection courante est une entrée valide : ouvre
      `EnvironmentEditSession` (même construction qu'avant pour
      `Message::Enter`, migrée ici). Sans effet sur « Aucun » ou une
      entrée en erreur. Vérifier par un test.
- [x] 3.2 `Échap` avec `model.environment_editing.is_some()` et aucune
      saisie en cours ferme le popup (`model.environment_editing = None`),
      focus reste sur le panneau — logique déjà existante, vérifier
      qu'elle survit au changement de déclencheur (test existant à
      rejouer, pas de nouveau code attendu ici).

## 4. Popup d'édition : rendu en tableau et clic pour éditer

- [x] 4.1 Remplacer `render_environment_variables`
      (`src/app/view/panels.rs`, liste en ligne) par un rendu en popup
      centré : `Rect` calculé à partir du nombre de variables (largeur
      et hauteur bornées à l'écran, comme
      `redesign-environment-picker-as-dropdown` l'a fait pour le menu),
      `Clear` puis un tableau à deux colonnes (Clé, Valeur) — 
      `ratatui::widgets::Table` ou rendu manuel par `Line`/colonnes,
      selon ce qui reste le plus simple à maintenir avec le style déjà
      utilisé (variable désactivée atténuée). Vérifier par un test de
      rendu que les deux colonnes s'affichent et que le popup est centré.
- [x] 4.2 Ajouter la résolution du clic dans le popup : un clic gauche
      relâché dans la zone du tableau, sur la ligne d'une variable
      activée, démarre la saisie de sa valeur (résolution par position
      relative, sans suivre un défilement interne — design D6). Sans
      effet sur une variable désactivée. Vérifier par un test simulant
      un clic sur une ligne du popup.
- [x] 4.3 Vérifier par un test que le clic pendant une saisie en cours
      sur une AUTRE variable valide d'abord la saisie courante avant de
      démarrer la nouvelle (cohérent avec la règle déjà existante
      « Clic pendant une session d'édition » de `mouse-support`).

## 5. Détection de double-clic sur le panneau

- [x] 5.1 Ajouter `last_environment_click: Option<(Instant, usize)>` à
      `MouseState` (`model.rs`). Vérifier par `cargo build`.
- [x] 5.2 Dans `update.rs`, sur un clic gauche relâché dans la zone du
      panneau Environnement (hors saisie) : résoudre l'entrée cliquée
      par position (sans défilement, design D6), l'activer (comme
      `Entrée`) ; si `last_environment_click` porte la même entrée avec
      un écart inférieur à 400 ms, ouvrir en plus le popup d'édition ;
      dans tous les cas, mettre à jour `last_environment_click` avec
      l'horodatage et l'entrée de ce clic. Vérifier par des tests :
      simple clic (active seulement), double-clic rapproché (active et
      ouvre le popup), deux clics espacés de plus de 400 ms (pas de
      popup), deux clics rapprochés sur des entrées différentes (pas de
      popup).

## 6. Vérification globale

- [x] 6.1 Mettre à jour les tests existants qui supposaient le menu
      déroulant flottant (`environment_picker_panel_and_permanent_indicator`,
      `environment_variables_view_rendering_and_cursor`,
      `environment_variables_help_line`,
      `environment_dropdown_area_stays_within_screen_bounds` — cette
      dernière n'a plus lieu d'être, la zone étant désormais fixe) pour
      refléter le panneau permanent et le popup centré.
- [x] 6.2 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test` passent sans erreur ni avertissement.
- [x] 6.3 Vérification manuelle : lancer l'app, confirmer que le
      panneau Environnement est visible sans rien appuyer ; `Entrée` sur
      un environnement l'active sans rien fermer ; `e` ouvre le popup
      d'édition en tableau ; cliquer sur une ligne du popup démarre sa
      saisie ; double-clic sur une entrée du panneau active et ouvre le
      popup ; `Échap` ferme le popup ; `Ctrl+S` sauvegarde.
