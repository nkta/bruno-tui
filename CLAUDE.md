# CLAUDE.md — bruno-tui

## Projet

TUI en Rust pour les collections Bruno (fichiers `.bru`). Deux usages :
exploration manuelle d'API, et lancement de campagnes de TNR avec
affichage des tests en échec.

Dépôt : <https://github.com/nkta/bruno-tui>
Binaire : `bruno-tui`

## Principes non négociables

- L'exécution d'une requête est TOUJOURS déléguée à
  `bru run --reporter-json`. Ne jamais réimplémenter l'interpolation de
  variables, les scripts pre/post ou les blocs `tests`/`assert`.
- Le parser `.bru` sert uniquement à afficher et éditer. Il ne résout
  aucune variable et ne connaît pas l'héritage d'auth.
- Aucune écriture dans un fichier `.bru` tant que la capacité
  d'édition n'a pas été spécifiée et approuvée. Lecture seule par défaut.
- Tout type désérialisé depuis le reporter JSON est couvert par un test
  de fixture issu d'un vrai `bru run`. Pas de structure devinée.
- Les secrets ne sont jamais écrits sur disque ni loggués.

## Stack

ratatui + crossterm (UI), tokio (process asynchrone), serde/serde_json
(reporter). Pas d'autre framework TUI. Toute nouvelle dépendance doit
être justifiée dans le `design.md` du changement en cours.

## Architecture

Elm-like : `Model` / `Message` / `update` / `view`.

- Un seul enum `AppEvent` pour toutes les entrées de la boucle.
- La boucle d'événements ne bloque jamais : les exécutions `bru run`
  passent par un `mpsc::channel`.
- Le chargement des collections passe par le trait `CollectionLoader`
  (Bruno `.bru` aujourd'hui, OpenCollection YAML prévu).
- `view` est pur : pas d'I/O, pas de logique métier.

## Conventions

- Français pour les commentaires et la documentation, anglais pour le
  code (identifiants, noms de types).
- `cargo fmt`, puis `cargo clippy -- -D warnings`, puis `cargo test`
  doivent passer avant tout commit. Pas d'exception.
- Pas de `unwrap()` ni `expect()` hors tests et hors `main`.
  Erreurs propagées avec `thiserror`, contexte ajouté à la frontière.
- Les tests d'intégration utilisent les fixtures de `tests/fixtures/`,
  jamais une collection réelle du disque.

## Workflow

Ce projet suit OpenSpec. Avant toute modification de code, vérifier
qu'un changement correspondant existe dans `openspec/changes/`.
Si la demande ne rentre dans aucun changement actif, le dire et
proposer `/opsx:propose` plutôt que de coder directement.
