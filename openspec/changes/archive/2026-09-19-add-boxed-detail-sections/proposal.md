## Why

L'utilisateur a fourni une image de référence (`exempleTui/`) et souhaite
que le panneau Détail se rapproche de ce style : chaque section de la
requête (En-têtes, Paramètres de requête, Paramètres de chemin, Corps)
dans sa propre boîte à bordure, avec le nom de la section intégré dans le
cadre, plutôt que le simple titre souligné utilisé aujourd'hui
(`improve-edit-field-legibility`, archivable). L'utilisateur a
explicitement choisi la version fidèle à l'image — de vrais widgets
bordés, pas des caractères de bordure dessinés dans le texte — après
avoir vu les deux options et leur coût respectif.

## What Changes

- Les sections « En-têtes », « Paramètres de requête », « Paramètres de
  chemin » et « Corps » du détail d'une requête sont chacune présentées
  dans un cadre (`ratatui::widgets::Block`) avec le nom de la section
  affiché dans la bordure, à la place du titre souligné actuel
  (`section()` dans `src/app/view/detail.rs`).
- Les champs méta en tête du détail (Nom, Chemin, Méthode, URL, Auth) et
  les indicateurs de fin (scripts, tests, assertions) restent des lignes
  simples, non encadrées, comme aujourd'hui — l'image de référence ne les
  encadre pas non plus.
- Le panneau Réponse n'est pas concerné : il reste un panneau unique à
  onglets (`response-tabs`), comme sur l'image de référence.
- **Aucun changement de comportement observable** pour la navigation, la
  saisie, le clic, le défilement, la recherche ou la copie : un champ
  reste au même endroit logique dans la séquence de lignes du détail: la
  boîte n'ajoute que des lignes de bordure autour d'un groupe de lignes
  déjà existant, exactement comme le fait déjà la ligne de titre de
  section aujourd'hui. `field-editing`, `mouse-support` et
  `search-and-yank` ne changent donc pas de contrat : ce sont des
  capacités qui raisonnent déjà en séquence ordonnée de lignes, jamais en
  coordonnées d'écran directes — voir `design.md` pour la justification
  détaillée.
- Le rendu du panneau Détail change en revanche en profondeur : au lieu
  d'un unique `Paragraph` défilé sur un `Text` plat, le panneau est
  composé de plusieurs widgets `Block` indépendants assemblés dans un
  tampon virtuel puis découpés à la fenêtre visible (voir `design.md`,
  c'est le cœur technique de ce changement).

## Capabilities

### New Capabilities

Aucune.

### Modified Capabilities

- `visual-theme` : la présentation du détail en fiche lisible (exigence
  déjà existante sur la hiérarchie titre/section/champ) est précisée pour
  les quatre sections listées ci-dessus, désormais encadrées, avec le nom
  de section dans la bordure plutôt qu'en ligne soulignée.

`field-editing`, `mouse-support`, `search-and-yank` et `tui-shell` ne
sont pas modifiés : leurs exigences portent sur une séquence ordonnée de
lignes et sur des comportements observables (champ visible après
déplacement, clic ouvrant une saisie, défilement d'une ligne, etc.), qui
restent vrais avec des sections encadrées. Voir `design.md` pour le détail
de cette analyse et le risque qu'elle couvre.

## Impact

- Code principalement dans `src/app/view/` : `detail.rs` (construction
  des lignes et des bordures de section), `mod.rs` (rendu du panneau,
  aujourd'hui un seul `Paragraph::scroll`), `hit.rs` (inchangé en
  principe, à vérifier à l'implémentation — voir `design.md`).
- Changement de rendu substantiel (tampon virtuel puis découpe), mais
  aucune nouvelle dépendance : `ratatui::buffer::Buffer` est déjà utilisé
  en interne par le framework.
- Aucun changement de format sur disque, aucun changement d'API du
  writer/runner/parser.
- Risque principal : un désaccord entre la séquence logique de lignes
  (utilisée par `field-editing`/`mouse-support`/`search-and-yank`) et ce
  qui est effectivement dessiné à l'écran romprait silencieusement le
  clic, la recherche ou le défilement. `design.md` fixe la règle qui
  élimine ce risque (une seule source de vérité pour la séquence de
  lignes, le rendu ne fait que la mettre en forme).
