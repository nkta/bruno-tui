## Context

Voir `proposal.md` pour le pourquoi. État actuel du code, vérifié avant
d'écrire ce document :

- `detail::detail_text(model)` (`src/app/view/detail.rs`) est le seul
  point qui mélange requête et réponse : pour un nœud requête, il
  construit `request_text_with_session(request, session)` (déjà
  autonome — champs de la requête uniquement) puis, s'il existe un
  résultat d'exécution, ajoute une ligne vide et `result_lines(outcome,
  filter)` à la suite, dans le même `Text`. Pour un dossier ou un nœud
  en erreur, il délègue à `folder_text`/`error_text`, qui ne
  construisent jamais de section résultat.
- `cursor_position_in_detail` (couplage documenté dans
  `openspec/changes/archive/2026-09-16-improve-visual-design/design.md`)
  calcule des décalages de ligne codés en dur, mais uniquement à partir
  de la structure de `request_text_with_session` — jamais de la partie
  résultat, qui vient toujours après. Retirer l'append du résultat ne
  change donc aucun de ces décalages.
- `open_filter` (`src/app/update.rs`) — qui décide si le filtre jq
  (`response-filter`) peut s'ouvrir — ne lit jamais `model.focus` :
  seules la sélection et la présence d'un résultat exploitable
  conditionnent son ouverture. Déplacer l'affichage du résultat vers un
  panneau distinct ne demande donc aucun changement à cette fonction.
- `Focus` (`src/app/model.rs`) a cinq variantes ; `Focus::Detail` est
  utilisé 21 fois dans `src/app/update.rs` et `src/app/view/mod.rs`.
  `NextFocus` (`Tab`) fait actuellement `Tree → Detail`, puis tout focus
  non-Tree revient à `Tree`. `FocusTree` (`Échap`) met
  inconditionnellement `model.focus = Focus::Tree` (sauf session
  d'édition ou sélection visuelle à fermer d'abord) : il ne teste jamais
  la variante de départ, donc `Échap` depuis un futur `Focus::Response`
  fonctionnera sans modification de cette fonction.
- Le défilement, la recherche et la sélection visuelle du détail
  reposent sur `Model::detail_scroll: u16`, `detail_match: Option<(u16,
  Range<usize>)>`, `detail_selection: Option<DetailSelection>` (`struct
  DetailSelection { anchor: u16 }`), et sur les fonctions
  `detail_line_count`, `detail_max_scroll`, `bottom_of_viewport`,
  `selection_range`, `scroll_detail` (`src/app/update.rs`).
  `SearchScope` (`src/app/search.rs`) a deux variantes, `Tree` et
  `Detail`.
- `layout()` (`src/app/view/mod.rs`) calcule `Areas { title, body, tree,
  detail, status }` : `tree` prend 35 % de la largeur (plancher
  `MIN_TREE_WIDTH = 24`), `detail` prend le reste. `MIN_WIDTH = 40`.
- `panels::empty_state_message(area, message)` (privée) centre
  verticalement un message d'une ligne dans une zone, déjà utilisée par
  les panneaux Diagnostics/Historique vides (`visual-theme`).

## Goals / Non-Goals

**Goals:**
- Panneau Réponse indépendant : focus, défilement, recherche, sélection
  visuelle et copie propres, sans affecter le panneau Détail.
- Aucun changement de contenu affiché par l'un ou l'autre panneau, ni de
  comportement du filtre jq ou de l'édition : uniquement leur
  répartition entre deux panneaux au lieu d'un.
- Réutiliser les mécanismes déjà panel-agnostiques (`open_filter`,
  `FocusTree`) sans les toucher quand c'est déjà suffisant.

**Non-Goals:** voir proposal.md (barre de recherche en haut, sélecteur
d'environnement en en-tête, fil d'ariane, onglets). Ni les proportions
exactes de colonnes, qui restent réglables sans changement de spec.

## Decisions

### D1 — Séparer la construction du texte requête et réponse dans `detail.rs`
`request_text_with_session`, `folder_text`, `error_text` restent
inchangées : elles constituent déjà, seules, le contenu du panneau
Détail. `detail_text` cesse d'ajouter le résultat à la suite ; le code
qu'elle en retire devient une nouvelle fonction `response_text(model) ->
Text<'static>`, vide quand la sélection n'a pas de résultat exploitable
(nœud non-requête, ou requête sans exécution). `plain_lines` se
dédouble en une version pour chaque panneau (même principe : dérivées
du `Text` correspondant), utilisées par la recherche et la copie.

### D2 — Un nouveau focus et une nouvelle portée de recherche, pas de nouveau mécanisme
Ajouter `Focus::Response` à l'enum existant et `SearchScope::Response` à
`search.rs`. `NextFocus` devient `Tree → Detail → Response → Tree`
(un seul `match` à trois branches au lieu de deux). `FocusTree` reste
inchangée (elle ne teste jamais la variante de départ, voir Context).
`field-editing` (`e`) continue de ne réagir qu'à `Focus::Detail` : la
condition existante n'a pas besoin de changer, elle exclut déjà
`Focus::Response` par construction (elle ne testait qu'une égalité avec
`Focus::Detail`).

### D3 — État par panneau : champs dupliqués, arithmétique partagée
`Model` gagne `response_scroll: u16`, `response_match: Option<(u16,
Range<usize>)>`, `response_selection: Option<DetailSelection>`
(réutilisation du type existant tel quel — un renommage en un nom plus
générique n'apporterait rien pour une structure à un seul champ, voir
Risks). Plutôt que dupliquer l'arithmétique de `detail_max_scroll`,
`bottom_of_viewport` et `selection_range`, en extraire la partie pure
(sans dépendre de `Model`) :
```
fn max_scroll(line_count: usize, height: u16) -> u16
fn viewport_bottom(scroll: u16, height: u16, line_count: usize) -> u16
```
`scroll_detail`/`scroll_response`, `detail_max_scroll`/`response_max_
scroll`, `bottom_of_viewport`/`response_bottom_of_viewport`,
`selection_range`/`response_selection_range` restent des fonctions
séparées (elles lisent/écrivent des champs différents de `Model` et des
zones différentes de `Areas`), mais délèguent leur calcul à ces deux
fonctions communes.

Alternative écartée : un seul jeu de fonctions paramétré par une enum
« quel panneau » plutôt que deux jeux de fonctions. Rejetée : elle
demanderait un accès générique aux champs de `Model` (via une
indirection ou une paire de closures get/set à chaque appel) pour un
gain de lignes minime, à l'encontre de la préférence du projet pour des
fonctions explicites plutôt qu'une abstraction supplémentaire.

### D4 — `layout()` : trois zones, nouveaux planchers, nouveau minimum
`Areas` gagne un champ `response: Rect`. Nouveaux planchers
`MIN_DETAIL_WIDTH = 18` et `MIN_RESPONSE_WIDTH = 18` (`MIN_TREE_WIDTH`
inchangé à 24) ; `MIN_WIDTH` passe de 40 à **60**, exactement la somme
des trois planchers : à la taille minimale supportée, chaque panneau
est exactement à son plancher, sans marge — au-dessus, `tree` garde sa
règle actuelle (35 % de la largeur totale), et le reste se partage à
parts égales entre `detail` et `response` :
```
let remaining = area.width - tree_width;
let detail_width = (remaining * 50 / 100).max(MIN_DETAIL_WIDTH);
let response_width = (remaining - detail_width).max(MIN_RESPONSE_WIDTH);
```
C'est un changement de comportement observable (`tui-shell`, exigence
« Disposition et barre d'état ») : un terminal entre 40 et 59 colonnes,
utilisable aujourd'hui, affichera désormais le message « agrandir le
terminal ». Alternative écartée : garder `MIN_WIDTH = 40` — rejetée, un
partage à trois panneaux sur 40 colonnes laisserait environ 5 colonnes
utiles à chacun des panneaux latéraux une fois les bordures retirées,
illisible.

### D5 — Panneau Réponse vide : réutiliser `empty_state_message`
`panels::empty_state_message` passe de privée à `pub(crate)` et se
réutilise telle quelle pour le panneau Réponse, avec le message «
aucun résultat », dans les trois cas prévus par la nouvelle exigence
(aucune exécution, dossier sélectionné, nœud en erreur sélectionné).
Aucune nouvelle fonction de centrage : celle qui existe déjà couvre
exactement ce besoin.

### D6 — `response-filter` : aucun changement à `update.rs`
`open_filter` et le reste de la logique de filtre (`confirm_filter`,
saisie, annulation) ne changent pas : ils ne dépendent déjà que de la
sélection et du résultat d'exécution, jamais du panneau focalisé (voir
Context). Seul le point d'affichage change, dans `view/mod.rs` : le
`Paragraph` qui rendait `detail::detail_text` (résultat compris) rend
maintenant `detail::response_text` dans la zone `areas.response`.

## Risks / Trade-offs

- [Doubler l'état de défilement/recherche/sélection double aussi le
  nombre de branches à revoir dans `update.rs` (21 usages actuels de
  `Focus::Detail`)] → mitigation : l'arithmétique commune (D3) limite la
  duplication réelle ; `tasks.md` revoit chaque site un par un plutôt
  qu'un remplacement global, pour repérer les endroits qui doivent
  rester Détail-only par construction (`field-editing`, déjà couvert
  par D2).
- [`MIN_WIDTH` passe de 40 à 60 : un terminal qui fonctionnait
  aujourd'hui peut désormais afficher « trop petit »] → changement
  délibéré, dimensionné pour que les trois panneaux restent lisibles
  plutôt que de dégrader silencieusement l'affichage ; documenté dans
  la exigence modifiée de `tui-shell`.
- [`DetailSelection` réutilisée telle quelle pour la réponse, sous un
  nom qui ne la mentionne plus] → wrinkle de nommage jugée mineure pour
  une structure à un seul champ ; à revoir si elle grossit avec des
  champs propres à un panneau.

## Migration Plan

Aucune migration de données : changement d'état en mémoire et de rendu
uniquement. Les tests de rendu qui supposent `areas.detail` sur toute la
largeur restante après l'arbre (dimensions attendues, positions de
colonne) doivent être mis à jour pour la nouvelle géométrie à trois
zones ; leurs assertions sur le **contenu textuel** ne doivent pas
changer, seulement les coordonnées où ce contenu est cherché.
