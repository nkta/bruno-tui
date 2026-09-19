## Why

`redesign-environment-picker-as-dropdown` (archivé) a transformé le
panneau Environnement plein écran en menu déroulant flottant, ouvert par
`E` et refermé par `Échap`. Test utilisateur : ça fonctionne, mais deux
problèmes restent :

1. Un menu qu'il faut rouvrir à chaque fois n'est pas ce qui est attendu
   pour « afficher l'environnement » — il faut une vraie place
   permanente en haut à droite, toujours visible, comme les panneaux
   Collection/Détail/Réponse/Statut.
2. `Entrée` sert aujourd'hui à démarrer l'édition d'un environnement
   (drill-down vers ses variables) — l'utilisateur attend qu'`Entrée`
   active l'environnement (comme `Droite` aujourd'hui), et qu'un
   raccourci séparé (`e`, cohérent avec l'édition d'une requête) ouvre
   l'édition, sous la forme d'un popup dédié avec un tableau clé/valeur,
   utilisable au clavier ET à la souris.

## What Changes

- **Panneau permanent** : le panneau Environnement obtient une vraie
  zone dans la mise en page, au-dessus du panneau Statut (qui reste,
  rétréci d'autant ; Réponse rétrécit à son tour). Toujours affiché,
  quel que soit le focus — comme Collection/Détail/Réponse/Statut.
  `E` donne le focus au panneau (au lieu de l'ouvrir/le fermer) ; `Tab`
  continue de cycler entre les panneaux, Environnement y compris.
- **Activer un environnement** : `Entrée` sur une entrée valide
  l'active désormais, exactement comme `Droite` aujourd'hui (les deux
  coexistent). Contrairement à l'ancien comportement, activer un
  environnement ne referme plus rien ni ne change le focus : le panneau
  étant permanent, il n'y a plus rien à refermer.
- **Éditer un environnement** : `e` sur une entrée valide (ni « Aucun »,
  ni une entrée en erreur) ouvre un popup d'édition centré, distinct du
  panneau permanent, superposé au reste de l'écran. Le popup affiche un
  tableau à deux colonnes (Clé, Valeur) des variables de l'environnement
  visé, navigable aux flèches, `Entrée` sur une ligne démarre la saisie
  de sa valeur (comme aujourd'hui), `Échap` annule la saisie puis ferme
  le popup, `Ctrl+S`/`s` sauvegarde. Un double-clic sur une entrée du
  panneau permanent ouvre le même popup (en plus de l'avoir activée,
  comme le ferait un simple clic).
- **Souris dans le popup** : un clic sur une ligne du tableau démarre la
  saisie de sa valeur, comme le clic sur un en-tête de requête
  (`mouse-support`) — jusqu'ici hors périmètre pour l'édition
  d'environnement, désormais couvert.
- La session d'édition (`EnvironmentEditSession`, `model.rs`) ne change
  pas : mêmes champs, même logique de saisie/validation/sauvegarde.
  Seuls son déclenchement (`e` plutôt que `Entrée`) et son rendu
  (popup avec tableau plutôt que liste en ligne dans le panneau) changent.

## Capabilities

### Modified Capabilities

- `environment-editing` : nouvelle place du panneau (permanent, dans la
  mise en page) ; `e` remplace `Entrée` pour ouvrir l'édition ;
  `Entrée` rejoint `Droite` pour activer ; le rendu de l'édition devient
  un popup à tableau plutôt qu'une liste en ligne ; ajout du clic pour
  démarrer une saisie dans le popup et du double-clic pour l'ouvrir
  depuis le panneau.

## Impact

- `src/app/view/mod.rs` : `Areas` gagne un champ `environment: Rect`
  (au-dessus de `response_status`, même largeur) ; `layout()` répartit
  la hauteur entre Environnement (hauteur fixe, pas liée au contenu —
  `layout()` reste indépendant du modèle, comme aujourd'hui pour
  Statut) ; le panneau se dessine désormais dans tous les cas où une
  collection est chargée, plus seulement quand il a le focus ; le popup
  d'édition, quand ouvert, se dessine par-dessus tout le reste
  (technique déjà utilisée par `redesign-environment-picker-as-dropdown`
  : `Clear` + `Rect` positionné).
- `src/app/view/panels.rs` : `render_environment_variables` (liste en
  ligne) est remplacée par un rendu en tableau (`ratatui::widgets::Table`
  ou `Row`/colonnes manuelles).
- `src/app/update.rs` : `navigate_environment_picker` traite désormais
  `Message::Enter` comme `Message::Right` (active) ; le déclenchement de
  l'édition passe de `Message::Enter` à `Message::StartEdit` (déjà la
  touche `e`, déjà routée par focus ailleurs dans l'app — ajout d'une
  branche pour `Focus::EnvironmentPicker`) ; nouvelle détection de
  double-clic sur le panneau (fenêtre de temps courte entre deux clics
  sur la même entrée) ; nouveau clic-pour-éditer dans le popup.
- `src/app/model.rs` : `MouseState` gagne un champ pour mémoriser le
  dernier clic sur le panneau Environnement (horodatage + entrée visée),
  nécessaire à la détection de double-clic.
- Aucun changement à `src/writer/environment.rs` ni au format `.bru`.
