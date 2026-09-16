## Why

L'arbre de `parser-cases` tient sur un écran, mais une vraie collection
Bruno compte des centaines de requêtes réparties sur plusieurs niveaux de
dossiers. Sans recherche, atteindre une requête précise impose de déplier
et défiler à l'aveugle. Et sans copie, une URL ou un corps repéré dans le
détail doit être retapé à la main dans un autre outil. Le projet Go `brio`
(https://github.com/luca-trifilio/brio) traite ces deux besoins avec des
touches vim (`/`, `v`, `y`) ; ce changement en reprend l'esprit, pas le
code, sur l'interface déjà posée par `tui-shell`.

## What Changes

- Recherche `/` dans le panneau ayant le focus, avec une ligne de saisie
  dédiée qui capture les caractères tapés (les touches habituelles de
  navigation redeviennent du texte le temps de la saisie) :
  - dans l'arbre : recherche par sous-chaîne insensible à la casse sur le
    nom affiché de chaque nœud (tout l'arbre, y compris les dossiers
    repliés) ; une correspondance déplie ses dossiers ancêtres et
    sélectionne le nœud ;
  - dans le détail : recherche par sous-chaîne insensible à la casse dans
    le texte affiché ; une correspondance fait défiler la ligne trouvée en
    haut du panneau et la surligne.
  - `Entrée` valide et saute à la première correspondance après la
    position courante (recherche circulaire) ; `Échap` annule sans
    déplacer la sélection ni le défilement ; une recherche sans résultat
    est signalée dans la barre d'état, sans modifier l'affichage.
  - `n` répète dans le même sens, `N` dans le sens opposé, tant qu'un
    motif a été validé au moins une fois, quel que soit le panneau ayant
    le focus au moment de l'appui.
- Sélection visuelle et copie dans le panneau de détail :
  - `v` ancre une sélection sur la ligne actuellement en haut du panneau
    visible ; les touches de défilement déjà existantes (`↑↓ j k Page
    suivante/précédente Début Fin`) étendent la sélection jusqu'à la ligne
    en bas du panneau visible au lieu de simplement défiler ; un second
    `v` ou `Échap` annule la sélection sans copier.
  - `y` copie dans le presse-papiers du système : le texte des lignes
    sélectionnées si une sélection est active, sinon la seule ligne
    actuellement en haut du panneau visible (le « champ » pointé). La
    sélection est levée après la copie.
  - Si le presse-papiers du système est inaccessible (pas de serveur
    d'affichage, presse-papiers occupé), l'échec est signalé dans la
    barre d'état sans jamais paniquer ni bloquer la boucle d'événements.
- Nouvelle dépendance `arboard` (accès au presse-papiers du système),
  sondée et justifiée dans `design.md`.

Hors périmètre, pour des changements suivants :
- filtre qui masque les nœuds non correspondants dans l'arbre (cette
  capacité ne fait que sauter d'une correspondance à l'autre, sans rien
  cacher) ;
- édition d'un champ (capacité `add-field-editing`) ;
- historique des exécutions, filtre jq sur les réponses, thème ;
- sélection ou copie dans le panneau de l'arbre (seuls les noms y sont
  cherchés, pas copiés) ;
- collage depuis le presse-papiers.

## Capabilities

### New Capabilities
- `search-and-yank`: recherche de sous-chaîne dans l'arbre et le détail
  avec saut à la correspondance suivante ou précédente, sélection
  visuelle de lignes dans le détail, et copie vers le presse-papiers du
  système.

### Modified Capabilities
<!-- Aucune. Le contrat de `tui-shell` (specs/tui-shell/spec.md) n'est pas
     modifié : toutes les touches et tous les comportements qui y sont
     décrits (navigation de l'arbre, défilement du détail, focus, sortie)
     restent inchangés en dehors d'une saisie de recherche active. Voir
     design.md pour la vérification touche par touche. -->

## Impact

- Code : nouveau sous-module dans `src/app/` pour l'état et la logique de
  recherche et de sélection (nom exact dans `design.md`), extension de
  `Message`/`to_message` (`message.rs`), de `Model`/`update`
  (`model.rs`/`update.rs`) et du rendu (`view/`). `src/collection/` et
  `src/runner/` inchangés.
- Dépendances : ajout d'`arboard` (presse-papiers), sans les fonctions
  image, justifié dans `design.md`.
- Tests : unitaires sur la traduction des touches et sur `update` (dans le
  style de `message.rs`/`update.rs` existants), rendu `TestBackend` pour
  la mise en évidence de la sélection et de la correspondance recherchée.
  Aucun test n'exige de serveur d'affichage réel : l'écriture vers le
  presse-papiers est isolée derrière une interface testable par une
  fausse implémentation.
- Environnement : sur une machine sans serveur d'affichage (CI, SSH sans
  transfert X), la copie échoue proprement ; le reste de l'interface n'est
  pas affecté.
