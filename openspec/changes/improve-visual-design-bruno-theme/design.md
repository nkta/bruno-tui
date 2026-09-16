## Context

Voir `proposal.md` pour le pourquoi. État actuel du rendu, vérifié dans
le code avant d'écrire ce document :

- `src/app/view/theme.rs` centralise déjà tout le style visuel en
  constantes `Style` (capacité `visual-theme`, changement
  `improve-visual-design`, archivé). `tree.rs`, `detail.rs` et
  `panels.rs` importent ces constantes et ne construisent plus leurs
  propres `Style::new().fg(...)` locaux : changer les valeurs dans
  `theme.rs` suffit à changer la palette partout, sans toucher ces
  trois fichiers.
- `panel()` (`src/app/view/mod.rs`) est le seul endroit qui construit
  encore une bordure de panneau : `Style::new()` (aucune couleur, donc
  la couleur par défaut du terminal) si non focalisé, `theme::FOCUS` si
  focalisé. Il n'existe aujourd'hui aucune constante de bordure par
  défaut ni de fond d'application.
- `view()` (`src/app/view/mod.rs`) dessine directement sur `frame`, sans
  jamais poser de fond explicite : chaque widget (`Paragraph`, `List`,
  `Block`) garde le fond par défaut du terminal (`Color::Reset`) là où
  son style ne fixe pas `.bg`.
- Dans `ratatui`, peindre un `Block::default().style(Style::new().bg(c))`
  sur toute la zone d'un `Frame`, avant les autres widgets, remplit
  chaque cellule de cette zone avec le fond `c`. Les widgets rendus
  ensuite ne l'écrasent pas tant que leur propre `Style` ne fixe pas
  `.bg` : `ratatui` fusionne les styles champ par champ
  (`Style::patch`), un champ `None` du nouveau style laisse la valeur
  déjà posée inchangée. C'est le mécanisme déjà utilisé ailleurs dans
  l'écosystème `ratatui` pour un fond plein écran, sans dépendance
  nouvelle.

## Goals / Non-Goals

**Goals:**
- Un fond d'application unique, posé une seule fois par image de
  `view()`, visible dans tous les états (chargement, erreur, chargé,
  terminal trop petit).
- Une palette resserrée autour des couleurs de l'image de référence
  (verts et oranges), sans perdre aucune des garanties déjà testées de
  `visual-theme` (catégories distinctes, trois niveaux du détail
  distincts).
- Aucun fichier autre que `theme.rs` et `mod.rs` à modifier :
  `tree.rs`, `detail.rs`, `panels.rs` importent déjà les constantes de
  `theme.rs`, donc un changement de valeur y suffit.

**Non-Goals:**
- Aucun changement à `Model`, `Message`, `update.rs`, ni à `layout()` :
  un fond plein écran et un changement de valeurs de couleur n'ont
  besoin d'aucun nouvel état ni d'aucune nouvelle géométrie.
- Aucune configuration de thème par l'utilisateur : palette fixe,
  choisie dans ce changement, comme pour `improve-visual-design`.
- Pas de re-couleur de `FAILURE` (Red), `RUNNING` (Blue) ni
  `LOAD_ERROR` (Magenta) : l'image de référence ne montre aucun état
  d'échec, d'exécution en cours, ni d'erreur de chargement — rien n'y
  motive un changement, et Red reste un signal universel fort pour une
  erreur. Une future itération pourra les revoir séparément si une
  image de référence les couvre.

## Decisions

### Palette : nouvelles valeurs dans les constantes existantes de `theme.rs`, plus deux nouvelles constantes

| Catégorie | Style actuel | Nouveau style | Remarque |
|---|---|---|---|
| Fond d'application (**nouveau**) | — | `Color::Rgb(18, 22, 40)` bleu-nuit | nouvelle constante `BACKGROUND`, posée une fois par `view()` |
| Bordure de panneau non focalisé (**nouveau**) | `Style::new()` (couleur par défaut du terminal) | `Color::Rgb(90, 150, 110)` vert atténué | nouvelle constante `BORDER`, utilisée par `panel()` |
| Méthode HTTP | Cyan | `Color::Rgb(120, 200, 150)` vert clair | repris de la teinte verte de la réponse JSON dans l'image |
| Titre de section (détail) | Cyan + Bold + souligné | `Color::Rgb(120, 200, 150)` (même valeur que `METHOD`) + Bold + souligné | même précédent que l'ancien design, qui réutilisait déjà Cyan pour les deux catégories ; aucune exigence de `visual-theme` n'impose que ces deux-là soient distinctes (seule la distinction titre/section/libellé au sein du détail est exigée) |
| Focus actif (bordure) | Yellow bold | `Color::Rgb(224, 138, 60)` orange bold | aligné sur l'accent orange de l'image (titres, éléments actifs) |
| Succès, échec, en cours, erreur de chargement, titre, libellé, message unique | inchangés | inchangés | voir Non-Goals |

Alternative écartée : garder `FOCUS` en Yellow et n'introduire l'orange
que pour `SECTION` — rejetée parce que l'image associe l'orange
spécifiquement aux éléments **actifs/sélectionnés**, ce que `FOCUS`
représente directement (bordure du panneau qui a le focus) ; `SECTION`
n'est pas un état actif, il reste dans la famille verte comme `METHOD`.

### Fond d'application : peint une fois, au tout début de `view()`, inconditionnellement
Ajouter en tout premier dans `view()` :
```
frame.render_widget(
    Block::default().style(Style::new().bg(theme::BACKGROUND)),
    frame.area(),
);
```
avant même la vérification de taille minimale, pour que l'état « terminal
trop petit » porte aussi le fond — une interface qui bascule entre deux
fonds selon la taille du terminal serait un signal visuel involontaire.

Alternative écartée : poser le fond seulement dans la branche
`CollectionState::Loaded` — rejetée parce que `Loading`, `Failed` et
l'écran « terminal trop petit » resteraient sur le fond par défaut du
terminal, contredisant l'exigence d'un fond cohérent sur tout l'écran.

### Bordure par défaut : nouvelle constante `theme::BORDER`, appliquée dans `panel()`
`panel()` change de :
```
let style = if focused { theme::FOCUS } else { Style::new() };
```
à :
```
let style = if focused { theme::FOCUS } else { theme::BORDER };
```
Signature et appelants de `panel()` inchangés. Le titre du panneau (déjà
passé à `Block::title()`) hérite du même style que la bordure dans
`ratatui` tant qu'il n'a pas son propre style explicite ; aucun autre
changement n'est donc nécessaire pour que les titres de panneau portent
aussi la couleur de bordure. À vérifier empiriquement par un test
`TestBackend` (tâche dédiée) : si le titre ne suit pas la bordure telle
qu'observée dans le rendu, appliquer un style explicite au titre plutôt
que de changer cette décision.

## Risks / Trade-offs

- [`Color::Rgb` n'est utilisé nulle part aujourd'hui dans `bruno-tui`
  (toutes les couleurs actuelles sont des variantes nommées à 16
  couleurs) ; un terminal limité à 16 ou 256 couleurs ne rendra pas la
  valeur RGB exacte] → dégradation gracieuse acceptée : `crossterm`
  convertit vers la couleur la plus proche disponible, aucun crash ni
  perte de lisibilité, seule la teinte exacte varie. Aucune dépendance
  nouvelle : `Color::Rgb` fait déjà partie de `ratatui::style` utilisé
  par le projet.
- [Réutiliser la même valeur de couleur pour `METHOD` et `SECTION`
  pourrait sembler une régression par rapport à une palette où chaque
  catégorie a sa propre teinte] → aucune exigence de `visual-theme` ne
  l'impose ; c'est déjà le comportement du changement précédent (les
  deux étaient en Cyan) et cette convention est reconduite, pas
  introduite.
- [Le fond plein écran posé avant `layout()` doit être fusionné
  correctement par tous les widgets rendus par-dessus (`Paragraph`,
  `List`, surbrillances `REVERSED` de recherche/sélection)] →
  mitigation : `Style::patch` de `ratatui` ne remplace que les champs
  explicitement fixés par le style du widget ; aucun widget existant ne
  fixe `.bg` en dur aujourd'hui sauf les surbrillances de recherche/
  sélection/curseur, qui doivent rester visibles par-dessus le nouveau
  fond — à vérifier par les tests de rendu existants
  (`selection_and_match_are_highlighted_distinctly_on_screen`), qui ne
  doivent pas casser.

## Migration Plan

Aucune migration : changement de style pur, aucun format sur disque ni
aucune donnée persistée n'est concerné. Les tests de rendu existants qui
vérifient le texte affiché doivent continuer à passer sans modification ;
ceux qui vérifient une couleur précise (palette de `theme.rs`, bordure
par défaut) seront mis à jour pour refléter la nouvelle palette.
