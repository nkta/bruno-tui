## Why

`bru-parser` marque déjà chaque fichier `.bru` invalide dans l'arbre, et
`add-request-run` va bientôt donner à chaque requête un verdict
d'exécution. Mais les deux restent dispersés : il faut déplier l'arbre
nœud par nœud pour trouver un fichier en erreur, et rien ne garde trace
des exécutions passées d'une session de TNR. Ce changement ajoute un
point de vue agrégé sur les deux, sans toucher à la façon dont ils sont
produits.

## What Changes

- Un panneau **Diagnostics** (touche `D`) liste, en un seul endroit, tous
  les nœuds en erreur de la collection chargée : fichiers `.bru`
  malformés et dossiers dont `folder.bru` est invalide, chacun avec son
  chemin et sa raison. Un badge (`⚠ N`) reste visible en permanence dans
  la barre de titre dès que `N` est supérieur à zéro.
- Choisir une entrée du panneau et valider (`Entrée`/`→`) ramène la
  sélection sur ce nœud dans l'arbre et rend le focus à l'arbre.
- Un panneau **Historique** (touche `H`) liste les exécutions terminées
  de la session, les plus récentes en premier : horodatage, cible,
  verdict global, durée. Le journal est borné à 200 entrées ; au-delà, la
  plus ancienne est retirée.
- **Rejouer** (`r`, la même touche que pour lancer une exécution depuis
  l'arbre) une entrée de l'historique relance une exécution sur sa cible
  d'origine, avec le même mode récursif.
- `Échap` referme l'un ou l'autre panneau et rend le focus à l'arbre.

Hors périmètre, volontairement :
- persistance de l'historique sur disque : le journal ne survit pas à la
  fermeture de l'application. Les en-têtes et corps de réponse peuvent
  porter des secrets ; les y exposer plus longtemps que la session en
  cours, sans mécanisme de rédaction, n'est pas acceptable en l'état. Une
  persistance éventuelle est laissée à un futur changement, une fois la
  question de la rédaction posée séparément ;
- recherche ou filtre dans l'historique ou les diagnostics ;
- export de l'historique ou des diagnostics ;
- agrégat de statut sur les nœuds dossier de l'arbre (nombre de succès et
  d'échecs d'un sous-arbre) : seule la vue agrégée du panneau Diagnostics
  existe, dans l'esprit de ce que `add-request-run` a lui-même exclu pour
  les résultats d'exécution.

## Capabilities

### New Capabilities
- `diagnostics-and-history`: panneau agrégeant les fichiers `.bru` en
  erreur avec badge de comptage et navigation croisée vers l'arbre ;
  journal borné des exécutions de la session avec rejeu.

### Modified Capabilities
<!-- Aucune : `tui-shell` garde son contrat actuel, ce changement ajoute
     deux panneaux et deux touches (`D`, `H`) non utilisées ailleurs.
     `bru-runner` n'est pas modifié. `request-execution` (changement
     sœur `add-request-run`, non encore archivé) n'est pas modifié : ce
     changement lit son modèle de données (`RunState`, `RunEvent`) sans
     y toucher. -->

## Impact

- Code : nouveau sous-module `src/app/diagnostics.rs` (ou équivalent,
  précisé dans `design.md`) ; extension de `Model`, `Message`, `update`
  et `view` (`Focus`, barre de titre pour le badge, deux nouveaux
  panneaux plein corps). `src/collection/` et `src/runner/` inchangés.
- Dépendances : aucune nouvelle dépendance.
- Tests : unitaires sur `update` (ajout au journal, troncature à 200,
  rejeu, navigation croisée diagnostics → arbre) ; rendu `TestBackend` du
  badge et des deux panneaux, sur la fixture `parser-cases` (qui contient
  déjà des nœuds en erreur) complétée par des exécutions simulées via le
  faux `bru`.
- Dépendance de conception : ce changement s'appuie sur le contrat de
  données de `add-request-run` (`Model.run: RunState`, `RunEvent`) tel
  que documenté dans son `design.md`. Si ce contrat change avant
  l'implémentation des deux changements, `design.md` de celui-ci devra
  être mis à jour en conséquence.
