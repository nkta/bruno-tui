## Why

Les deux usages de bruno-tui (exploration d'API, campagnes de TNR) reposent
sur l'exécution de requêtes, et le projet impose de la déléguer
intégralement à `bru run --reporter-json`. Il faut donc une brique
fondatrice, indépendante de l'UI, qui lance `bru` sans bloquer la boucle
d'événements et restitue le rapport sous forme de types Rust fiables,
avant de pouvoir construire la moindre vue d'exécution.

## What Changes

- Nouveau module `runner` (bibliothèque interne, sans UI) qui :
  - construit et lance `bru run` en processus asynchrone (tokio) à la
    racine d'une collection, sur une ou plusieurs cibles (requête,
    dossier, récursif), avec environnement optionnel ;
  - récupère le rapport JSON **sans l'écrire sur disque** (le rapport
    contient en-têtes et corps de réponse, donc potentiellement des
    secrets) ;
  - désérialise le rapport en types Rust fidèles à la structure observée
    sur `bru` 2.13.2 (itérations, résultats par requête, assertions,
    tests, résumé) ;
  - distingue « la campagne a tourné et comporte des échecs » (cas
    nominal, code de sortie non nul) de « `bru` n'a pas pu produire de
    rapport » (binaire absent, chemin inexistant, hors racine de
    collection, JSON invalide) ;
  - publie le résultat via un `mpsc` pour s'intégrer à la boucle
    `AppEvent`, et permet l'annulation d'une exécution en cours.
- Fixtures de rapports issus de vrais `bru run` dans
  `tests/fixtures/reports/`, et collection de fixture pour les
  régénérer.
- Ajout des dépendances tokio, serde, serde_json, thiserror et
  `command-fds` pour transmettre le rapport par un descripteur hérité
  (justifiées dans `design.md`).

Non inclus : affichage (vues ratatui), progression requête par requête
en temps réel, support Windows natif, parsing des fichiers `.bru`.

## Capabilities

### New Capabilities
- `bru-runner`: lancement asynchrone et annulable de `bru run`,
  récupération du rapport JSON sans persistance disque, modèle de
  rapport typé et classification des erreurs d'exécution.

### Modified Capabilities
<!-- Aucune : aucun spec existant dans openspec/specs/. -->

## Impact

- Code : nouveau module `src/runner/` (commande, transport du rapport,
  types du rapport, erreurs), exposé via un nouveau `src/lib.rs` pour
  être testable depuis `tests/`. `src/main.rs` inchangé.
- Dépendances : `Cargo.toml` passe de zéro à cinq dépendances.
- Environnement : requiert `bru` (Bruno CLI, Node) dans le `PATH` ;
  version de référence 2.13.2. Cible Unix (Linux/WSL, macOS).
- Tests : fixtures JSON versionnées ; test d'intégration contre le vrai
  `bru` ignoré si le binaire est absent.
