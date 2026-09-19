## Context

Voir `proposal.md` (Why) pour la demande à l'origine du changement et le
choix explicite de l'utilisateur (vrais widgets bordés, pas des
caractères de bordure dessinés dans le texte).

État actuel du rendu du panneau Détail (`src/app/view/`) :
- `detail.rs` construit un `Text<'static>` (une `Line` par ligne
  logique) et, pour une requête, une table `Vec<FieldLine>` qui associe
  à chaque champ éditable sa position dans cette séquence (`line`,
  `count`, `prefix_width`). Les titres de section (« En-têtes », etc.)
  sont déjà de simples `Line` dans cette même séquence, sans entrée dans
  `FieldLine` — ce sont des lignes « de décor », au même titre que le
  seraient des lignes de bordure.
- `mod.rs::view()` rend ce `Text` avec un seul
  `Paragraph::new(text).scroll((model.detail_scroll, 0))`, avec
  `.wrap(Wrap { trim: false })` sauf pendant la saisie d'un champ
  (`detail_wraps`, qui bascule vers le décalage horizontal d'une seule
  ligne défini par `field-editing`).
- `hit.rs::line_at_row` retrouve la ligne logique sous une position
  d'écran en rejouant, pour chaque ligne, le même calcul de hauteur que
  `Paragraph` (`Paragraph::new(Text::from(line)).wrap(...).line_count(width)`),
  cumulé jusqu'à dépasser `scroll + row`. C'est une resimulation
  indépendante du rendu réel, déjà découplée de `Paragraph` : le
  point d'entrée pour brancher un nouveau mode de rendu sans toucher au
  reste de `field-editing`/`mouse-support`/`search-and-yank`, qui ne
  consomment que `FieldLine`, `detail_scroll` et cette fonction.

C'est cette séparation déjà existante (séquence logique de lignes d'un
côté, rendu de l'autre) qui rend ce changement faisable sans toucher aux
exigences de `field-editing`, `mouse-support` ou `search-and-yank` — voir
`proposal.md`, Capabilities.

## Goals / Non-Goals

**Goals:**
- Chaque section (En-têtes, Paramètres de requête, Paramètres de chemin,
  Corps) rendue avec un vrai `ratatui::widgets::Block` bordé, nom de
  section dans la bordure.
- Défilement, curseur de champ, clic, recherche et sélection au glisser
  continuent de fonctionner sans modification de `field-editing`,
  `mouse-support`, `search-and-yank` — seule la fonction de rendu change.
- Le défilement reste au grain de la ligne (comme aujourd'hui), y
  compris à travers une bordure : une boîte peut être coupée en haut ou
  en bas par le défilement, exactement comme n'importe quel groupe de
  lignes aujourd'hui.

**Non-Goals:**
- Défilement indépendant à l'intérieur d'une boîte (une section trop
  haute pour le panneau se voit coupée par le défilement global, pas par
  une barre de défilement propre à la boîte).
- Encadrer autre chose que les quatre sections listées (champs méta,
  indicateurs de fin, dossier, nœud en erreur, panneau Réponse) : hors
  périmètre, voir `proposal.md`.
- Changer la disposition à trois colonnes ou la largeur des panneaux
  (`tui-shell`, « Disposition et barre d'état ») : inchangée.

## Decisions

### Rendu par tampon virtuel composé, puis découpe à la fenêtre visible
Un seul `Paragraph::scroll` ne peut pas dessiner un `Block` bordé autour
d'un sous-ensemble de ses lignes : c'est un widget de texte plat. Pour
obtenir de vrais `Block` par section tout en gardant un défilement au
grain de la ligne à travers plusieurs sections, le rendu du panneau
Détail :
1. calcule la hauteur de chaque morceau (lignes méta non encadrées,
   hauteur de chaque boîte de section = 2 lignes de bordure + hauteur de
   son contenu, lignes de fin non encadrées) à partir de la même
   séquence logique que `field-editing` utilise déjà (`FieldLine` et la
   liste de `Line`) ;
2. alloue un `ratatui::buffer::Buffer` virtuel de la largeur du panneau
   et de la hauteur totale ainsi calculée (`Rect::new(0, 0, width,
   total_height)`) ;
3. rend chaque morceau dedans à son offset vertical — les lignes méta et
   de fin via `Paragraph`, chaque section via
   `Block::bordered().title(nom)` puis un `Paragraph` dans son
   `.inner(...)`, avec le même bascule wrap/hscroll qu'aujourd'hui
   (`detail_wraps`) ;
4. copie, cellule par cellule, la tranche de ce tampon virtuel comprise
   entre `detail_scroll` et `detail_scroll + hauteur du panneau` dans le
   `Buffer` réel de la frame, à la position du panneau Détail
   (`frame.buffer_mut()`).

`detail_scroll`, `FieldLine`, `hit.rs::line_at_row` et tout ce qui en
dépend restent inchangés : la séquence logique de lignes (étape 1) est
exactement celle qu'ils utilisent déjà, les lignes de bordure de section
s'y ajoutant comme des lignes de décor supplémentaires, au même titre
que l'étaient les titres de section.

- Alternative écartée : bordures dessinées en caractères dans le texte
  (`┌─ En-têtes ─┐` etc. comme `Line` stylées) — la plus simple, aurait
  gardé le rendu en un seul `Paragraph::scroll` sans tampon virtuel, mais
  explicitement écartée par l'utilisateur (pas de vrai widget `Block`).
- Alternative écartée : découpage en zones d'écran (`Rect`) fixes et
  disjointes par section, chacune avec son propre défilement — fidèle à
  une disposition en grille façon GUI, mais un contenu de corps plus
  long que sa zone deviendrait invisible sans défilement propre à cette
  zone (Non-Goal), et la navigation du curseur de champ entre sections
  perdrait la continuité qu'elle a aujourd'hui (un seul défilement, une
  seule table de positions).

### Largeur utile calculée une seule fois, partagée rendu et hit-testing
Une boîte retire 2 colonnes à la largeur disponible pour son contenu (1
de bordure de chaque côté, pas de padding supplémentaire). Le calcul de
hauteur par retour à la ligne (`Paragraph::line_count(width)`, utilisé à
la fois par le rendu et par `hit.rs::line_at_row`) doit recevoir cette
largeur réduite pour les lignes d'une section, la largeur pleine pour les
lignes méta/fin hors section. Cette règle de largeur par ligne SHALL
vivre dans une seule fonction (dérivée de la même table qui associe
chaque ligne à sa section, construite à l'étape 1 ci-dessus), utilisée
identiquement par le rendu et par `hit.rs` — jamais deux formules
séparées, pour ne pas laisser diverger l'endroit cliqué de l'endroit
affiché.
- Alternative écartée : garder la largeur pleine du panneau pour le
  calcul de hauteur et laisser le contenu de la boîte déborder ou se
  couper visuellement sur les 2 colonnes de bordure — rejetée, ça
  romprait la position du curseur de texte en fin de ligne longue.

### Position du curseur de texte : décalage supplémentaire, pas de nouveau modèle
`cursor_position_in_detail` continue de renvoyer une position logique
(ligne, colonne) au sein de la séquence, inchangée. Le placement à
l'écran du curseur terminal (`render_insert_cursor`) ajoute la colonne de
bordure gauche (+1) quand la ligne appartient à une section encadrée, en
plus du calcul déjà existant (`prefix_width`, décalage horizontal,
défilement). C'est un ajustement localisé à cette seule fonction de
placement écran, pas un changement du modèle logique.

## Risks / Trade-offs

- [Risque] Désaccord entre la largeur utilisée pour simuler la hauteur
  d'une ligne (`hit.rs`) et celle réellement utilisée au rendu → éliminé
  par construction en partageant une seule fonction de largeur par ligne
  (décision ci-dessus), jamais deux calculs séparés à maintenir en
  synchronisation.
- [Risque] Recomposer le tampon virtuel à chaque frame a un coût
  (allocation + rendu de chaque section) → contenu typique de quelques
  dizaines à basses centaines de lignes (en-têtes, paramètres, corps
  JSON) ; pas de mesure de performance prévue dans ce changement, à
  reconsidérer seulement si un ralentissement est constaté en usage réel.
- [Risque] Volume important de tests existants à mettre à jour : tout
  test qui compte des lignes exactes du détail
  (`request_detail_line_count_is_unchanged`, recherches par position de
  ligne dans `cursor_position_comes_from_the_field_table`, tests de
  `hit.rs`) verra son décompte changer avec l'ajout des lignes de
  bordure → attendu, pas une régression fonctionnelle ; à traiter
  systématiquement dans les tâches d'implémentation plutôt qu'au fil de
  l'eau, pour ne pas laisser une suite rouge entre deux tâches.
- [Risque] `render_insert_cursor` et tout code qui suppose aujourd'hui
  qu'« une ligne de détail = une ligne d'écran à la même colonne que la
  ligne précédente » (pas de décalage horizontal variable selon la
  section) doit être audité à l'implémentation : le risque n'est pas
  hypothétique, mais son ampleur exacte (combien de call sites) ne sera
  connue qu'en modifiant `mod.rs`/`hit.rs` — signalé ici plutôt que
  deviné.

## Migration Plan

Aucune migration de données : changement de rendu uniquement, aucun
format sur disque modifié. Déployé avec le prochain build du binaire.
Pas de bascule progressive : le rendu encadré remplace l'ancien
directement, protégé par la suite de tests existante (à mettre à jour)
et par les nouveaux scénarios de `visual-theme`.
