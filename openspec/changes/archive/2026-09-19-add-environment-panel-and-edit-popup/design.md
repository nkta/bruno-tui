## Context

- `layout()`/`layout_for` (`src/app/view/mod.rs`) sont des fonctions
  pures de géométrie, prenant seulement une taille d'écran — jamais le
  modèle. Elles sont appelées à des dizaines d'endroits dans
  `update.rs`/`hit.rs` (bornage de défilement, hit-testing) avec
  seulement `model.size`. Le panneau Statut a une hauteur fixe à deux
  paliers (`STATUS_PANEL_HEIGHT` = 5, `STATUS_PANEL_COMPACT_HEIGHT` = 3,
  bascule à `STATUS_PANEL_FULL_MIN_TERMINAL_HEIGHT` = 20 lignes de
  terminal), jamais liée au nombre d'éléments à afficher.
- `update.rs` lit déjà l'horloge réelle directement dans son propre
  arbre d'appel (`SystemTime::now()` pour `HistoryEntry.started_at`,
  ligne 778 et ailleurs) malgré le principe « `update` ne fait aucune
  I/O » du module — lire l'heure n'y est pas traité comme une E/S
  interdite. C'est le précédent direct pour la détection de double-clic
  ci-dessous.
- `EnvironmentEditSession` (`model.rs`) porte déjà tout l'état
  nécessaire à l'édition (variables en mémoire, curseur, état
  Sélection/Saisie, `dirty`) — rien n'y change ici, seul son
  déclenchement et son rendu changent (voir `proposal.md`).
- Le seul mécanisme de superposition existant (`Clear` + `Rect`
  positionné manuellement) vient de
  `redesign-environment-picker-as-dropdown` — réutilisé tel quel pour le
  popup d'édition, en plus grand et centré plutôt qu'ancré en coin.
- Le clic gauche est aujourd'hui le seul bouton traité
  (`MouseEventKind::Down/Up(MouseButton::Left)` uniquement,
  `message.rs`) ; aucun double-clic n'existe nulle part dans le code.

## Goals / Non-Goals

**Goals :**
- Panneau Environnement toujours visible, dans une vraie zone de la
  mise en page.
- `Entrée` active ; `e` (ou double-clic) ouvre l'édition dans un popup
  séparé, tableau clé/valeur, utilisable au clavier et à la souris.

**Non-Goals :**
- Généraliser la détection de double-clic à d'autres panneaux : scopée
  au panneau Environnement, seul endroit qui en a besoin aujourd'hui.
- Gérer le clic-souris avec précision quand la liste (panneau ou popup)
  défile au-delà de ce qui tient à l'écran : voir D6 (limitation
  acceptée).
- Changer `EnvironmentEditSession`, la sauvegarde, ou le format `.bru` :
  inchangés, voir `proposal.md`.

## Decisions

### D1 — Hauteur du panneau Environnement fixe, pas liée au contenu
Comme le panneau Statut, la hauteur du panneau Environnement dans
`layout()` est fixe à deux paliers
(`ENV_PANEL_HEIGHT` = 6, `ENV_PANEL_COMPACT_HEIGHT` = 3, même seuil de
bascule `STATUS_PANEL_FULL_MIN_TERMINAL_HEIGHT`), jamais calculée à
partir du nombre d'environnements.

**Alternative écartée** : hauteur dépendant du nombre d'environnements
(comme le menu déroulant de `redesign-environment-picker-as-dropdown`)
— rejetée, `layout()` devrait alors recevoir le modèle, un changement de
signature qui toucherait des dizaines d'appels dans `update.rs`/`hit.rs`
qui n'ont aujourd'hui besoin que de `model.size`. Le
`ratatui::widgets::List` défile déjà tout seul autour de la sélection
si le contenu dépasse la hauteur disponible — un palier fixe suffit,
sur le même principe que Statut.

### D2 — `e` ouvre l'édition, `Entrée` active
`Message::StartEdit` (déjà la touche `e`, déjà routée par focus ailleurs
dans l'app pour l'édition d'une requête) gagne une branche pour
`Focus::EnvironmentPicker` : sur une entrée valide, ouvre
`EnvironmentEditSession` comme le faisait `Entrée` avant ce changement.
`navigate_environment_picker` traite désormais `Message::Enter` comme
`Message::Right` (`select_environment_picker`) : les deux activent.
Activer un environnement ne change plus le focus ni ne « ferme » quoi
que ce soit — le panneau étant permanent, il n'y a plus de fermeture à
faire (différence avec le comportement d'avant ce changement, où
`Droite` renvoyait le focus à l'arbre).

### D3 — Le popup d'édition remplace la vue en ligne, sans toucher à la session
`render_environment_variables` (liste en ligne dans le panneau) est
remplacée par un rendu en popup centré (tableau à deux colonnes, Clé et
Valeur). `EnvironmentEditSession` n'est pas modifiée : mêmes champs,
mêmes transitions Sélection/Saisie. Seule la fonction de rendu change,
plus la position/dimension de la zone qu'on lui passe (centrée, pas la
zone du panneau permanent).

### D4 — Détection de double-clic : horodatage lu directement dans `update`
`MouseState` (`model.rs`) gagne
`last_environment_click: Option<(Instant, usize)>` (horodatage et
indice de l'entrée cliquée). Sur un clic gauche relâché dans la zone du
panneau Environnement, `update.rs` compare `Instant::now()` (lu
directement, comme déjà fait pour `HistoryEntry.started_at` — voir
Context) à `last_environment_click` : même indice d'entrée et écart
inférieur à `DOUBLE_CLICK_WINDOW` (400 ms) SHALL ouvrir le popup
d'édition, en plus d'activer l'entrée comme le ferait un simple clic ;
sinon, le clic active seulement l'entrée et met à jour
`last_environment_click`.

**Alternative écartée** : porter l'horodatage dans `MouseInput`
(rempli à la frontière impure `event.rs`/`message.rs`, `update` restant
dépendant d'une donnée reçue plutôt que de l'horloge) — plus « pur » en
apparence, mais s'écarte du précédent déjà établi par
`HistoryEntry.started_at`, pour un bénéfice de testabilité marginal : un
test peut déjà construire deux clics séparés par un `thread::sleep`
court ou comparer un `last_environment_click` pré-rempli avec un
`Instant` du passé, sans flakiness (fenêtre de 400 ms très large par
rapport à la durée d'un test).

### D5 — Clic pour éditer dans le popup
Dans le popup, un clic gauche sur une ligne du tableau démarre la
saisie de sa valeur — même règle que le clic sur un en-tête de requête
(`mouse-support`, « Saisie d'un champ au clic »), étendue ici au popup
d'environnement (hors périmètre du proposal `environment-editing`
d'origine, explicitement repris ici à la demande de l'utilisateur).

### D6 — Résolution du clic par position, sans tenir compte du défilement interne
Le panneau permanent et le popup résolvent l'entrée/ligne cliquée par
sa position relative dans la zone (`ligne_cliquée - zone.y`), sans tenir
compte d'un éventuel défilement interne du `List`/tableau au-delà de ce
qui tient à l'écran. Limitation acceptée : correcte tant que tout le
contenu tient dans la zone (cas courant — peu d'environnements, popup
dimensionné au nombre de variables) ; au clavier, la navigation reste
correcte dans tous les cas, y compris au-delà de ce qui tient à l'écran.

**Alternative écartée** : suivre l'offset de défilement via un
`ListState`/`TableState` persistant dans le modèle pour une résolution
exacte même en défilement — rejetée pour ce changement, complexité
disproportionnée par rapport au besoin réel (peu d'environnements et de
variables dans les collections visées) ; à revisiter si un besoin
concret apparaît.

## Risks / Trade-offs

- [Empiler Environnement (3) et Statut (3) au-dessus de Réponse, à
  `MIN_HEIGHT` inchangé (10), aurait laissé Réponse sans aucune ligne
  intérieure — cassant l'invariant déjà testé « Réponse garde au moins
  une ligne intérieure » (`status-panel`)] → Corrigé en relevant
  `MIN_HEIGHT` à 11 (au lieu d'accepter la régression) : à ce plancher,
  Environnement et Statut compacts (3 chacun) laissent Réponse à 3 lignes
  (1 ligne intérieure), l'invariant existant reste vrai. Détail
  d'implémentation découvert en écrivant les tests de layout, pas une
  décision de conception a priori.
- [Double-clic : première implémentation de ce genre d'interaction dans
  le projet] → Mitigation : fenêtre de temps large (400 ms, standard),
  scopée à un seul panneau, aucune généralisation prématurée (voir
  Non-Goals).
- [Clic sans suivi du défilement : imprécis si le contenu déborde] →
  Documenté en D6, limitation assumée plutôt que cachée.
