## Why

`bru-runner` sait exécuter `bru run` de façon asynchrone et `tui-shell`
sait afficher une collection, mais rien ne les relie : l'interface ne peut
ni lancer une requête ni montrer un résultat. C'est le manque bloquant
pour les deux usages du projet (exploration manuelle, campagnes de TNR),
et la brique qui a le plus de valeur immédiate à ajouter maintenant.

## What Changes

- Une touche (`r`) lance l'exécution du nœud sélectionné dans l'arbre :
  une requête (`bru run <chemin>`) ou un dossier, en récursif
  (`bru run <chemin> -r`). `Entrée` garde son rôle actuel de dépliage,
  décrit dans `specs/tui-shell/spec.md` ("Navigation au clavier") ; ce
  changement ne le modifie pas.
- `BruRunner` est intégré à la boucle `app` : l'issue de chaque exécution
  (`RunEvent`) arrive sur le même canal `mpsc` que les autres `AppEvent`,
  sans jamais bloquer la saisie ni le rendu.
- Pendant une exécution, l'interface affiche un indicateur de progression
  sur le nœud concerné et dans la barre d'état.
- À la fin, un panneau de résultat affiche : statut de réponse (code HTTP,
  erreur de connexion ou requête ignorée), en-têtes, corps, temps de
  réponse, verdicts des assertions et des tests (y compris pré-requête et
  post-réponse), et le verdict global de la requête.
- Dans l'arbre, une requête dont la dernière exécution est en échec porte
  un marqueur visuel distinct du marqueur d'erreur de parsing déjà
  existant (fichier `.bru` invalide).
- Chaque issue non aboutie (`bru` introuvable, aucun rapport produit,
  rapport invalide, exécution annulée) est affichée avec un message
  explicite propre à sa cause, sans confondre les cas.
- Une touche (`Ctrl+X`) annule l'exécution en cours.
- Aucune nouvelle dépendance : uniquement `runner::BruRunner` déjà présent.

Hors périmètre, pour des changements suivants :
- sélection d'un environnement (`--env`) et affichage de ses variables :
  `add-environment-picker` ;
- filtre jq sur la réponse, export curl, historique des exécutions :
  changements sœurs déjà envisagés (`add-response-filter`,
  `add-diagnostics-and-history`) ;
- exécution sur toute la collection sans sélection, exécution planifiée ;
- édition de fichier `.bru`.

## Capabilities

### New Capabilities
- `request-execution`: lancement d'une requête ou d'un dossier depuis
  l'arbre, exécution non bloquante via `bru-runner`, affichage de la
  progression, du résultat détaillé et du verdict, annulation, et
  classification affichée de chaque issue d'échec.

### Modified Capabilities
<!-- Aucune : `tui-shell` garde le sens actuel de `Entrée` (dépliage) et
     `bru-runner` n'est pas modifié, seulement consommé. -->

## Impact

- Code : nouveau sous-module `src/app/run/` (ou fichiers équivalents,
  précisés dans `design.md`) ; extension de `Model`, `Message`,
  `AppEvent`, `update` et `view` existants dans `src/app/` ; `src/main.rs`
  construit et transmet un `BruRunner` à `app::run`. `src/runner/` et
  `src/collection/` inchangés.
- Dépendances : aucune nouvelle.
- Tests : réutilisation du faux `bru` (`tests/fixtures/fake-bru/`) et du
  style `TestBackend` de `tests/app_shell.rs` pour des exécutions
  déterministes ; tests unitaires de `update` pour chaque variante
  d'issue.
- Environnement : `bru` réel non nécessaire pour les tests (faux `bru`
  substitué) ; nécessaire seulement pour un usage réel.
