## Context

Voir `proposal.md` pour le pourquoi. État actuel du rendu, vérifié dans
le code avant d'écrire ce document :

- Des couleurs existent déjà, mais partiellement et sans registre commun :
  méthode HTTP en Cyan (`src/app/view/tree.rs`), succès en Green, échec
  d'exécution en Red, en cours en Yellow, nœud en erreur de chargement
  **également en Red** — donc indiscernable d'un échec d'exécution par la
  couleur seule (les deux ne se distinguent aujourd'hui que par leur
  symbole, `✗` contre `●`/`✓`/`…`). Le panneau ayant le focus est bordé en
  Yellow bold (`panel()`, `src/app/view/mod.rs`) — la même couleur que le
  marqueur « en cours d'exécution ».
- Dans le détail (`src/app/view/detail.rs`), `field()` met le **libellé**
  en gras et laisse la **valeur** en style par défaut : visuellement,
  c'est le libellé qui ressort, pas la donnée que l'utilisateur vient
  lire. `section()` est gras+souligné, sans couleur. `title()` est gras
  seul. Les trois niveaux ne se distinguent aujourd'hui que par
  gras/souligné, jamais par la couleur.
- **Contrainte majeure découverte en lisant le code** :
  `cursor_position_in_detail` (`src/app/view/detail.rs`) calcule la
  position du curseur en mode Insert (`add-field-editing`) avec des
  décalages de ligne **codés en dur** (`line_index = 3` pour l'URL,
  `7 + idx` pour un en-tête, etc.), strictement couplés au nombre exact
  de lignes produites par `request_text_with_session`. Le test
  `insert_cursor_positioning_and_bounds` (`src/app/view/mod.rs`) fixe
  `cursor.y == inner_area.y + 3`, preuve directe de ce couplage. Changer
  le nombre ou l'ordre des lignes de `request_text_with_session` sans
  répercuter le même changement dans `cursor_position_in_detail` casse
  silencieusement le positionnement du curseur d'édition.
- `layout()` (`src/app/view/mod.rs`) calcule des zones de taille fixe
  (arbre 35 % de la largeur, panneaux sur toute la hauteur du corps) ;
  plusieurs tests (`layout_respects_minimums`, tests de rendu avec
  largeur de terminal calculée dynamiquement pour le titre) dépendent de
  cette géométrie exacte.

## Goals / Non-Goals

**Goals:**
- Un petit nombre de couleurs, choisies une fois et réutilisées
  partout où la même catégorie d'information apparaît.
- Un renversement du rapport de contraste libellé/valeur dans le détail :
  la valeur doit ressortir plus que son libellé, pas l'inverse.
- Trois niveaux de style clairement distincts dans le détail : titre,
  section, champ.
- Une présentation délibérée des panneaux à message unique (diagnostics
  et historique vides), sans changer le message affiché.
- Rien de tout cela ne doit changer le texte brut déjà vérifié par les
  tests de rendu existants : uniquement le style des `Span`/`Line` déjà
  produits.

**Non-Goals:**
- Aucun changement à `Model`, `Message`, ou `update.rs` : tout est
  dérivable du modèle existant au moment du rendu.
- Aucun changement au **nombre ou à l'ordre des lignes** produites par
  `request_text_with_session` : la contrainte `cursor_position_in_detail`
  ci-dessus rend ce refactor risqué et hors de portée de ce changement.
  Une future capacité pourra s'y attaquer séparément, si besoin, en
  commençant par supprimer ce couplage par calcul de décalages.
- Aucun changement à `layout()` (proportions arbre/détail, hauteur des
  panneaux) : les tests de géométrie existants et le couplage avec le
  défilement (`detail_max_scroll`, `body_height`) rendent ce chantier
  distinct de la présentation visuelle proprement dite.
- Aucune configuration de thème par l'utilisateur (couleurs fixes,
  choisies dans ce changement).

## Decisions

### Palette : une constante par catégorie, dans un seul module
Ajouter un petit module `src/app/view/theme.rs` avec des constantes
`Style` (pas seulement des `Color`, pour inclure `Modifier` où utile) :

| Catégorie | Style retenu | Remarque |
|---|---|---|
| Méthode HTTP | Cyan (inchangé) | déjà correct, centralisé |
| Succès d'exécution | Green (inchangé) | |
| Échec d'exécution | Red (inchangé) | |
| En cours d'exécution | **Blue** (changé, était Yellow) | libère Yellow pour le focus seul |
| Erreur de chargement/parsing | **Magenta** (changé, était Red) | distinct de l'échec d'exécution |
| Focus actif (bordure) | Yellow bold (inchangé) | |
| Titre de nœud (détail) | Bold (inchangé) | reste le plus sobre des trois niveaux |
| Titre de section (détail) | Bold + souligné + Cyan (changé, était sans couleur) | |
| Libellé de champ (détail) | **Dim** (changé, était Bold) | la valeur redevient le point focal |
| Message d'un panneau à message unique | Dim + italique si le terminal le supporte | |

Alternative écartée : garder l'échec d'exécution et l'erreur de
chargement dans la même couleur (Red pour les deux), en s'appuyant
uniquement sur le symbole pour les distinguer — rejetée parce que la
spec `visual-theme` demande une couleur cohérente et distincte par
catégorie, et que la couleur se remarque avant le symbole en balayage
visuel.

Chaque fonction de rendu (`tree.rs`, `detail.rs`, `panels.rs`) importe
ces constantes au lieu de construire ses propres `Style::new().fg(...)`
: garantit qu'une catégorie ne peut pas dériver silencieusement d'un
panneau à l'autre.

### Libellé/valeur : inverser le style, pas la structure
`field(label, value)` change uniquement le style appliqué à son premier
`Span` (`Modifier::DIM` au lieu de `Modifier::BOLD`) ; sa signature, le
nombre de `Span`/`Line` produits, et tous ses appels restent identiques.
Aucun impact sur `cursor_position_in_detail`, qui ne dépend que du texte,
jamais du style.

### Panneaux à message unique : centrage vertical par lignes vides calculées
Pour `render_diagnostics`/`render_history` (`panels.rs`) quand la liste
est vide, calculer le nombre de lignes vides à insérer avant le message
(`(hauteur_intérieure.saturating_sub(1)) / 2`) pour le centrer
verticalement dans la zone, en plus d'un style Dim. Alternative écartée :
changer la hauteur du panneau pour qu'il s'ajuste à son contenu réel —
rejetée par la contrainte Non-Goals sur `layout()` (la disposition
générale ne bouge pas dans ce changement) et parce qu'un panneau qui
change de taille selon son contenu perturberait le repère visuel de
l'utilisateur d'un appui à l'autre sur `D`/`H`.

### Portée exclue du panneau de détail d'une requête : styles seulement
Puisque `request_text_with_session` ne doit pas changer de structure
(Non-Goals), le travail sur le détail se limite à : styliser `title()`
(inchangé, Bold), `section()` (ajout Cyan), `field()` (Dim sur le
libellé), et les lignes de `editable_entries`/`entries` (garder leur
structure, ajuster uniquement leur style pour cohérence avec le nouveau
libellé Dim). `folder_text` et `error_text` (jamais en session
d'édition, donc sans contrainte de décalage codé en dur) peuvent recevoir
la même hiérarchie de style sans restriction supplémentaire.

## Risks / Trade-offs

- [Recolorer l'erreur de chargement en Magenta et l'exécution en cours en
  Blue change une association déjà connue des utilisateurs actuels de la
  branche `add-diagnostics-and-history`/`add-request-run`] → aucun
  utilisateur externe à ce jour (outil non publié) ; les tests de rendu
  qui vérifient une couleur précise (s'il y en a) seront mis à jour dans
  le cadre de ce changement, pas laissés en échec.
- [Un terminal ou thème utilisateur qui ne distingue pas bien Cyan/Blue/
  Magenta selon sa palette 16 couleurs] → mitigation partielle : les
  marqueurs textuels (`✗`, `✓`, `●`, `…`) déjà distincts restent en place
  à côté de la couleur, qui est un renfort et non l'unique signal.
- [Centrer verticalement un message par lignes vides calculées est
  correct pour du texte sur une seule ligne mais deviendrait fragile pour
  un message multi-lignes] → mitigation : n'appliquer ce traitement
  qu'aux messages actuellement à une ligne (« aucune erreur », « aucune
  exécution ») ; un futur message plus long devra revoir ce calcul,
  documenté en commentaire à l'endroit du calcul.

## Migration Plan

Aucune migration : changement de style pur, aucun format sur disque ni
aucune donnée persistée n'est concerné. Les tests de rendu existants
qui vérifient le texte affiché doivent continuer à passer sans
modification ; ceux qui vérifient une couleur précise (rares aujourd'hui)
seront mis à jour pour refléter la nouvelle palette.
