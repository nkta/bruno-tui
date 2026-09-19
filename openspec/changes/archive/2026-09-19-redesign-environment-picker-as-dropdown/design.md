## Context

- `src/app/view/mod.rs::view()` dispatche sur `model.focus` : pour
  `Diagnostics`, `History`, `EnvironmentPicker`, `Secrets`, le panneau
  correspondant remplace tout `areas.body` (arbre + détail + réponse
  réunis) ; pour `Tree`/`Detail`/`Response`, les trois panneaux normaux
  se dessinent côte à côte. `EnvironmentPicker` est aujourd'hui dans le
  premier groupe.
- `Areas` (`layout()`) découpe l'écran une fois par frame : `title` (1
  ligne), `body` (arbre/détail/réponse), `status` (barre du bas). Aucun
  panneau flottant n'existe dans le code actuel — aucune trace de
  `ratatui::widgets::Clear` ni de superposition (vérifié par recherche).
- `panels::render_environment_picker` et
  `render_environment_variables` prennent déjà un `Rect` en paramètre et
  n'en présupposent ni la taille ni la position — elles ont été écrites
  pour `areas.body` mais ne dépendent de rien qui soit spécifique à
  cette zone.
- La logique de focus et de session (`Message::ToggleEnvironmentPicker`,
  `Message::FocusTree`, `model.environment_editing`,
  `navigate_environment_picker`) est entièrement dans `update.rs`,
  indépendante du rendu (`view` est pur — `CLAUDE.md`). Voir
  `proposal.md` : ce changement ne la touche pas.

## Goals / Non-Goals

**Goals :**
- Le panneau Environnement se dessine dans une zone flottante ancrée en
  haut à droite, sans faire disparaître l'arbre, le détail et la
  réponse affichés en dessous.

**Non-Goals :**
- Rendre le corps interactif sous le panneau flottant (clic sur l'arbre
  pendant que le menu est ouvert, etc.) : le panneau garde le focus
  exclusif comme aujourd'hui, le corps est visible mais gelé.
- Généraliser un mécanisme de popup réutilisable pour d'autres panneaux
  (Diagnostics, Historique, Secrets restent en plein corps) : seul
  Environnement change ici, à la demande explicite de l'utilisateur.
  Si un futur besoin similaire apparaît pour un autre panneau, ce sera
  un changement séparé qui pourra alors factoriser.

## Decisions

### D1 — Zone flottante calculée à la volée, pas un champ d'`Areas`
`Areas` reste inchangé : il découpe la disposition de base (title/
body/status), qui ne dépend pas de l'ouverture du panneau Environnement.
La zone du menu déroulant est calculée directement dans `view()` au
moment du rendu, à partir de `frame.area()` (ou `areas.title`/
`areas.body`), pas ajoutée comme un champ supplémentaire d'`Areas` —
elle ne participe à aucun calcul de défilement ni de bornage (contrairement
à `areas.tree`/`areas.detail`/`areas.response`, utilisés par `update.rs`
pour garder la sélection visible).

**Alternative écartée** : ajouter un champ `environment_dropdown: Rect` à
`Areas` — rejetée, `Areas` est partagé avec `update` justement pour ce
qui borne le défilement (doc de `layout` : « partagé avec update, qui en
déduit la hauteur des panneaux ») ; le menu déroulant n'a pas de
défilement borné par `update` (`ratatui::widgets::List` gère seul son
défilement autour de la sélection), donc il n'a pas sa place dans ce
contrat.

### D2 — Dimensions et position
Largeur : `(frame.area().width / 3).clamp(28, 44)` — assez pour une
ligne comme `environments/malformed.bru (invalide)` sans être
disproportionné sur un grand terminal. Hauteur : nombre d'entrées à
afficher (environnements, ou variables si la session de variables est
ouverte) plus 2 pour la bordure, plafonné à `frame.area().height -
4` (sous la ligne de titre, avant la barre du bas, avec une marge) pour
ne jamais déborder de l'écran. Position : `x = frame.area().width -
largeur` (flush à droite), `y = 1` (juste sous la ligne de titre, ligne
0).

### D3 — Ordre de rendu et `Clear`
`view()` dessine d'abord le corps normal (arbre/détail/réponse) comme si
`model.focus` valait `Tree` du point de vue du rendu du corps — c'est-à-dire
qu'on n'inspecte plus `model.focus == Focus::EnvironmentPicker` pour
choisir QUOI dessiner en dessous, seulement pour dessiner le menu
PAR-DESSUS ensuite. La zone du menu est d'abord vidée avec
`frame.render_widget(Clear, area)` avant d'y dessiner
`render_environment_picker`, pour ne pas laisser transparaître le texte
du panneau Détail ou Réponse dessiné juste en dessous au même endroit
(comportement standard de `ratatui` pour un popup, déjà documenté dans
ses exemples officiels).

### D4 — Focus visuel des panneaux du dessous pendant que le menu est ouvert
Question : quel panneau apparaît comme « ayant le focus » (bordure mise
en valeur) sous le menu déroulant, puisque `model.focus` vaut
`EnvironmentPicker` et non `Tree`/`Detail`/`Response` ? Décision : aucun
des trois — ils se dessinent avec `focused: false` (comme aujourd'hui
quand le focus est sur un panneau plein-écran type Diagnostics), pour
éviter de suggérrer qu'ils réagiraient au clavier pendant que le menu
est ouvert.

## Risks / Trade-offs

- [Terminal étroit (proche de `MIN_WIDTH` = 60) : le menu à largeur
  minimale 28 laisse peu de place au corps en dessous] → Accepté : le
  corps reste dessiné à sa taille normale en dessous (juste partiellement
  masqué par le menu), aucun recalcul de layout n'est nécessaire, et
  `MIN_WIDTH` garantit déjà un plancher d'affichage exploitable pour les
  trois panneaux.
- [Terminal bas (proche de `MIN_HEIGHT` = 10) : peu de lignes
  disponibles pour le menu] → Mitigation : la hauteur est plafonnée à
  `frame.area().height - 4`, jamais négative pour une taille ≥
  `MIN_HEIGHT` (10), et `ratatui::widgets::List` défile automatiquement
  si le contenu dépasse la hauteur disponible.
- [Premier popup du projet : aucun précédent à suivre] → Mitigation :
  technique standard et documentée de `ratatui` (`Clear` + `Rect`
  positionné manuellement), pas une invention ad hoc.
