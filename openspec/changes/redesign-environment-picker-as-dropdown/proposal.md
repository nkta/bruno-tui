## Why

L'utilisateur a testé `environment-editing` : le panneau Environnement
(`E`) remplace aujourd'hui tout l'écran principal (Collection, Détail,
Réponse disparaissent le temps qu'il est ouvert) —
`src/app/view/mod.rs` route `Focus::EnvironmentPicker` vers
`panels::render_environment_picker(model, frame, areas.body)`, qui
dessine dans toute la zone `body`. Ce n'est pas ce qu'il attend : il
veut un menu déroulant compact ancré en haut à droite de l'écran, qui se
superpose sans faire disparaître le reste de l'interface.

## What Changes

- Le panneau Environnement (liste des environnements, puis vue des
  variables au drill-down) est redessiné dans une petite zone flottante
  ancrée en haut à droite de l'écran, sous la ligne de titre — pas dans
  `areas.body`. L'arbre, le détail et la réponse restent affichés
  derrière, tels qu'ils étaient avant l'ouverture (gelés, non
  interactifs tant que le focus est sur le panneau).
- Ouverture (`E`), fermeture (`Échap` depuis la liste), drill-down
  (`Entrée` sur un environnement valide), retour à la liste (`Échap`
  depuis la vue des variables), édition d'une valeur, sauvegarde
  (`Ctrl+S`/`s`) : aucun changement de comportement, seule la zone
  d'affichage change. `update.rs` n'a pas besoin d'être modifié — la
  logique de focus et de session (`model.environment_editing`,
  `Message::FocusTree`) est déjà indépendante de la zone de rendu.
- Hors périmètre : support souris dans le menu déroulant (déjà hors
  périmètre d'`environment-editing`) ; redimensionnement dynamique
  au-delà d'un plafond raisonnable de lignes visibles (au-delà, la
  liste défile comme aujourd'hui, `ratatui::widgets::List` gère déjà le
  défilement automatique autour de la sélection).

## Capabilities

### Modified Capabilities

- `environment-editing` : l'exigence « Affichage des variables d'un
  environnement » est reformulée pour décrire un menu déroulant flottant
  ancré en haut à droite plutôt qu'un remplacement plein écran ; les
  scénarios de saisie et de sauvegarde ne changent pas de comportement,
  seule la zone d'affichage est concernée.

## Impact

- `src/app/view/mod.rs` : la branche `Focus::EnvironmentPicker` du
  dispatch de `view()` change — elle ne remplace plus le rendu du corps
  (`areas.body`) ; le corps normal (arbre/détail/réponse) se dessine
  toujours, puis le menu déroulant se dessine par-dessus, dans une zone
  calculée (largeur/hauteur bornées, ancrée en haut à droite),
  vidée au préalable (`ratatui::widgets::Clear`) puisque du contenu y a
  déjà été dessiné en dessous.
- `src/app/view/panels.rs` : `render_environment_picker` et
  `render_environment_variables` sont réutilisées telles quelles — elles
  dessinent déjà dans le `Rect` qu'on leur donne, sans hypothèse sur sa
  taille ni sa position.
- Aucun changement à `src/app/model.rs`, `src/app/update.rs`, ni à
  `src/writer/`.
- Aucune nouvelle dépendance : `ratatui::widgets::Clear` fait déjà partie
  de `ratatui`.
