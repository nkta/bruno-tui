## Why

Les capacités `bru-runner` et `bru-parser` existent, mais le binaire
`bruno-tui` affiche encore « Hello, world » : rien n'est visible ni
utilisable. Il faut maintenant le squelette de l'interface, qui pose la
boucle d'événements Elm-like imposée par le projet et rend la collection
explorable au clavier. C'est la base sur laquelle viendront ensuite
l'exécution de requêtes et l'affichage des tests en échec.

## What Changes

- Le binaire `bruno-tui` accepte un chemin optionnel (`bruno-tui [CHEMIN]`,
  répertoire courant par défaut), ainsi que `--help` et `--version`.
- Il ouvre une interface plein écran qui :
  - s'affiche immédiatement, puis charge la collection en arrière-plan
    sans jamais bloquer la saisie ;
  - affiche l'arbre de la collection dans l'ordre fourni par le loader,
    dossiers repliés au départ, nœuds en erreur marqués ;
  - se navigue au clavier : déplacement, dépliage, repli, retour au
    parent, premier et dernier élément ;
  - affiche le détail en lecture seule du nœud sélectionné : pour une
    requête, méthode, URL, auth déclarée, en-têtes, paramètres, corps et
    présence des scripts, tests et assertions ; pour un dossier, ses
    méta-données ; pour un nœud en erreur, sa raison ;
  - permet de faire défiler le détail quand il dépasse l'écran ;
  - affiche l'échec de chargement (pas une collection, `bruno.json`
    invalide) sans quitter ;
  - se quitte proprement par `q` ou `Ctrl+C` et restaure le terminal, y
    compris en cas de panique.
- Nouveau module `app` dans la bibliothèque : `AppEvent`, `Message`,
  `Model`, `update` et `view` pur, testables sans terminal réel.
- Ajout de la dépendance `ratatui` avec son backend crossterm, justifiée
  dans `design.md`.

Hors périmètre, pour des changements suivants :
- exécution d'une requête ou d'un dossier, et affichage des résultats ;
- choix d'un environnement et affichage des variables ;
- rechargement de la collection, surveillance du disque ;
- recherche ou filtre dans l'arbre, souris, thèmes, configuration des
  touches ;
- toute édition de fichier `.bru`.

## Capabilities

### New Capabilities
- `tui-shell`: lancement du binaire, boucle d'interface non bloquante,
  chargement de la collection en arrière-plan, arbre navigable au
  clavier, panneau de détail en lecture seule, sortie et restauration du
  terminal.

### Modified Capabilities
<!-- Aucune : `bru-parser` est consommé tel quel via `CollectionLoader`, et
     `bru-runner` n'est pas utilisé dans ce changement. -->

## Impact

- Code : nouveau module `src/app/` déclaré dans `src/lib.rs` ;
  `src/main.rs` réécrit (arguments, runtime tokio, lancement de
  l'interface). `src/collection/` et `src/runner/` inchangés.
- Dépendances : `ratatui` 0.30, sans ses features par défaut sauf le
  backend crossterm. crossterm est utilisé via la réexportation de
  ratatui. Aucun parseur d'arguments ni crate `futures`.
- Tests : tests unitaires de `update` et de la correspondance des
  touches, tests de rendu sur un terminal simulé en mémoire, test
  d'intégration sur la fixture `parser-cases`. La restauration du
  terminal est vérifiée dans un pseudo-terminal.
- Environnement : terminal interactif requis ; `bru` n'est pas
  nécessaire pour ce changement.
