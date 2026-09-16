## Why

`bru-writer` sait désormais sérialiser une modification de champ vers un
fichier `.bru` sans rien perdre du reste du fichier, mais rien ne
l'appelle : l'interface reste en lecture seule. Il faut maintenant la
capacité d'édition dans `tui-shell`, avec l'ergonomie vim demandée
(modes Normal/Insert, barre de mode) plutôt qu'un éditeur de texte intégré
ou un lancement d'`$EDITOR`, pour rester cohérent avec le style
d'exploration déjà en place et avec le périmètre volontairement restreint
de `bru-writer` (édition de champs déjà présents, pas de structure).

## What Changes

- Le panneau de détail devient éditable pour une requête sélectionnée :
  une touche (`e`) ouvre une **session d'édition** sur cette requête,
  superposée au focus « Détail » existant. Tant qu'aucune session n'est
  ouverte, la navigation et l'affichage restent identiques à `tui-shell`.
- **Mode Normal** (par défaut dans une session) : un curseur de champ se
  déplace entre les champs éditables (URL, valeur de chaque en-tête et
  paramètre existant, contenu du corps si son type est éditable par
  `bru-writer`) ; `Espace` bascule l'état activé/désactivé d'un en-tête ou
  paramètre sous le curseur ; `w` sauvegarde les modifications en attente.
- **Mode Insert** : entré par `i` sur le champ sous le curseur, édition de
  texte avec curseur de caractère (insertion, suppression, déplacement) ;
  `Échap` revient en Normal en conservant la valeur modifiée **en
  mémoire**, sans écrire sur disque.
- Un indicateur d'état modifié et une barre de mode explicite (Normal /
  Insert, nom du champ courant) sont affichés, dans l'esprit de `brio`
  (https://github.com/luca-trifilio/brio, esprit vim uniquement — aucun
  texte ni code recopié, licence non vérifiée).
- La sauvegarde appelle `bru-writer` (`RequestWriter`) avec l'instantané
  de fraîcheur capturé au chargement de la requête ; un refus (fichier
  modifié entre-temps, erreur disque) affiche un message clair et ne perd
  pas la modification en mémoire.
- Fermer une session avec des modifications non sauvegardées, ou quitter
  l'application dans ce cas, demande confirmation.
- **BREAKING** (comportement observable, pas de rupture d'API externe) :
  la sémantique d'`Échap` dans le panneau de détail devient conditionnelle
  — elle referme d'abord une session d'édition active avant de rendre le
  focus à l'arbre, au lieu d'y aller directement dans tous les cas.

Hors périmètre, volontairement, pour ce changement :
- tout ce que `bru-writer` refuse déjà : ajout/suppression d'entrée,
  changement de type de corps, changement de mode d'auth, corps de forme
  formulaire (`formUrlEncoded`, `multipartForm`, `file`) ;
- édition du nom de la requête, de la méthode, du `seq` — champs de
  structure ou de méta-données, pas de contenu de champ au sens de
  `bru-writer` ;
- édition de `collection.bru`, `folder.bru`, `environments/*.bru` ;
- annulation multi-niveaux (undo/redo) au-delà de la conservation en
  mémoire d'une session non sauvegardée ;
- édition pendant qu'une exécution est active sur la même requête (cf.
  `add-request-run`) ou édition du résultat d'une exécution passée ;
- mode Command (`:`) façon vim : reporté à un futur changement si le
  besoin apparaît, une seule touche suffit ici pour chaque action.

## Capabilities

### New Capabilities
- `field-editing` : session d'édition de champ sur une requête depuis le
  panneau de détail, modes Normal/Insert avec barre de mode, curseur de
  champ et curseur de texte, sauvegarde explicite via `bru-writer`,
  confirmation avant de perdre des modifications non sauvegardées.

### Modified Capabilities
- `tui-shell` : la touche `Échap` dans le panneau de détail referme
  d'abord une session d'édition active avant de rendre le focus à
  l'arbre ; quitter l'application (`q`, `Ctrl+C`) avec une session
  d'édition non sauvegardée demande confirmation avant de fermer le
  terminal.

## Impact

- Code : `src/app/model.rs` (session d'édition, curseur, état modifié),
  `src/app/message.rs` (variante `Insert` ajoutée à l'énumération
  `TextCapture`, nouveaux messages de saisie), `src/app/update.rs`
  (nouvelle variante de `Command` pour l'écriture, traitement hors I/O),
  `src/app/view/detail.rs` et `src/app/view/mod.rs` (rendu de la session,
  barre de mode), `src/app/mod.rs` (exécution de `Command::SaveEdit` via
  `spawn_blocking`). `src/writer/`, `src/collection/`, `src/runner/`
  inchangés.
- Dépendances : aucune nouvelle crate.
- Coordination avec les changements sœurs en cours : ce changement
  suppose le contrat de `add-bru-writer` (`RequestWriter`, `BruWriter`,
  `FieldEdit`, `FileStamp`, `WriteError`) et l'énumération `TextCapture`
  de `to_message` introduite par `add-search-and-yank` (D2) pour sa
  propre saisie de recherche — ce changement y ajoute la variante
  `Insert` plutôt que de redéfinir un mécanisme concurrent, ce qui est
  documenté dans `design.md` (D4).
- Tests : unitaires sur `update` (transitions Normal/Insert, saisie,
  sauvegarde réussie/refusée, confirmation), rendu `TestBackend` pour la
  barre de mode et le curseur de champ, fixtures dédiées dans
  `tests/fixtures/collections/writer-cases/` (réutilisées telles que
  `add-bru-writer` les a définies).
