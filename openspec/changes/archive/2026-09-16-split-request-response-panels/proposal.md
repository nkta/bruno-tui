## Why

Nicolas a fourni une image de référence (`exempleTui/Gemini_Generated_
Image_suk0qdsuk0qdsuk0.jpeg`, mockup du client Bruno) où la requête et sa
réponse sont deux panneaux visibles simultanément, côte à côte.
Aujourd'hui, `bruno-tui` les mélange dans un seul panneau « Détail » :
`detail::detail_text()` construit les champs de la requête
(`request_text_with_session`) puis, s'il existe un résultat d'exécution,
lui ajoute à la suite les lignes de résultat (`result_lines`), dans le
même flux défilable. Voir la requête et la dernière réponse en même
temps demande aujourd'hui de faire défiler un long panneau ; l'image
montre que les deux peuvent être côte à côte sans défilement croisé.

## What Changes

- Un nouveau panneau « Réponse », à droite du panneau « Détail »
  existant, affichant le résultat d'exécution de la requête
  sélectionnée (verdict, statut, temps de réponse, corps, assertions et
  tests) : exactement le contenu que `detail::detail_text()` ajoute
  aujourd'hui à la suite des champs de la requête, déplacé dans son
  propre panneau plutôt qu'ajouté au même flux.
- Le panneau « Détail » perd la partie résultat qu'il affichait à la
  suite des champs de la requête : il continue d'afficher les champs de
  la requête (inchangés), le contenu d'un dossier ou d'un nœud en erreur
  exactement comme aujourd'hui.
- Le panneau « Réponse » a son propre focus, son propre défilement, et
  peut être vide (aucune exécution pour la requête sélectionnée, ou nœud
  qui n'est pas une requête) : dans ce cas il affiche un message unique,
  traité comme le prescrit déjà `visual-theme` pour les panneaux à
  message unique.
- `Tab` bascule désormais entre trois panneaux au lieu de deux : Arbre →
  Détail → Réponse → Arbre. `Échap` depuis Détail ou Réponse retourne à
  l'Arbre, comme `Échap` le fait déjà depuis le Détail aujourd'hui.
- Le filtre jq (`response-filter`) et la recherche/sélection/copie
  (`search-and-yank`), qui portaient sur « le panneau de détail »,
  portent maintenant sur le panneau où vit désormais leur cible : le
  filtre jq sur le panneau Réponse (il ne filtrait déjà que le corps de
  réponse) ; la recherche, la sélection visuelle et la copie sur le
  panneau qui a le focus, Détail ou Réponse indifféremment.
- `field-editing` est inchangé : l'édition ne portait déjà que sur les
  champs de la requête, affichés dans le panneau Détail, qui garde
  exactement ce contenu.

Hors périmètre : la barre de recherche en haut de l'écran, le
repositionnement du sélecteur d'environnement dans un en-tête, le fil
d'ariane en bas, les onglets Réponse/Tests/Documentation vus dans
l'image — aucun de ces éléments n'est nécessaire pour que requête et
réponse soient visibles simultanément, qui est le seul objectif de ce
changement. Toute évolution des proportions de colonnes ou du contenu
affiché par chaque panneau, au-delà de ce déplacement, est également
hors périmètre.

## Capabilities

### New Capabilities
Aucune.

### Modified Capabilities
- `tui-shell` : la disposition passe de deux à trois panneaux côte à
  côte (Arbre, Détail, Réponse) ; le panneau de détail ne décrit plus le
  résultat d'exécution, qui devient un panneau à part entière avec sa
  propre exigence ; le défilement et le focus, jusqu'ici définis pour
  « le détail », se définissent désormais pour chacun des deux panneaux
  indépendamment ; `Tab` cycle sur trois panneaux au lieu de deux.
- `response-filter` : le filtre jq s'applique désormais au panneau
  Réponse plutôt qu'au panneau de détail (déplacement de la référence de
  panneau, aucun changement de comportement du filtre lui-même).
- `search-and-yank` : la recherche, la sélection visuelle et la copie,
  définies aujourd'hui uniquement pour « le détail », se généralisent au
  panneau ayant le focus parmi Détail et Réponse, avec un état
  (défilement, motif, sélection) propre à chacun.

## Impact

- Code : `src/app/model.rs` (nouvelle variante `Focus::Response`,
  nouveaux champs `response_scroll`, `response_match`, et l'équivalent
  de l'ancre de sélection visuelle pour la réponse) ; `src/app/update.rs`
  (cycle de `NextFocus`, prise en compte de `Focus::Response` partout où
  `Focus::Detail` conditionne aujourd'hui une action de défilement, de
  recherche, de sélection ou de copie, et le déclenchement du filtre
  jq) ; `src/app/view/mod.rs` (`layout()` passe à trois zones, rendu du
  nouveau panneau) ; `src/app/view/detail.rs` (séparer la construction
  du texte de résultat de celle des champs de la requête, aujourd'hui
  concaténées dans `detail_text`). `src/app/view/panels.rs` et
  `src/app/view/tree.rs` ne devraient pas changer.
- Dépendances : aucune nouvelle crate.
- Tests : les tests de rendu qui supposent deux zones (`layout_respects_
  minimums` et les tests utilisant `layout_for(...).detail`) changent de
  géométrie attendue. Les tests de `field-editing`
  (`cursor_position_in_detail`, positionnement du curseur) ne devraient
  pas changer de contenu attendu, seulement éventuellement de zone
  d'écran. De nouveaux tests couvrent le panneau Réponse vide, son
  défilement indépendant, et la recherche/sélection/copie sur ce
  panneau.
