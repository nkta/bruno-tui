## 1. Diagnostics agrégés

- [ ] 1.1 Créer `src/app/diagnostics.rs` avec `DiagnosticEntry` et la fonction `diagnostics(&Collection) -> Vec<DiagnosticEntry<'_>>` parcourant l'arbre pour collecter `ErrorNode` et `FolderNode` à méta-données invalides, adresse comprise (D1) ; déclarer le module dans `src/app/mod.rs` ; vérifier par tests unitaires sur la fixture `parser-cases` que la liste contient exactement `broken.bru`, `no-method.bru` et `badmeta`, dans l'ordre de l'arbre, avec la raison attendue pour chacun
- [ ] 1.2 Vérifier par un test unitaire qu'une collection sans nœud en erreur donne une liste vide

## 2. Modèle : focus, sélections et historique

- [ ] 2.1 Ajouter `Diagnostics` et `History` à `Focus`, `diagnostics_selected: usize` et `history_selected: usize` à `Model`, initialisés à 0 (D2) ; vérifier `cargo build`
- [ ] 2.2 Ajouter `HistoryEntry`, `HistoryOutcome` et `HISTORY_LIMIT = 200` à `src/app/model.rs`, et `history: VecDeque<HistoryEntry>` à `Model`, initialisé vide (D2) ; vérifier par un test unitaire que `Model::new` démarre avec un historique vide

## 3. Messages et touches

- [ ] 3.1 Ajouter `Message::ToggleDiagnostics` et `Message::ToggleHistory`, et les touches `D`/`H` dans `key_message` (D5) ; vérifier par tests unitaires les deux correspondances, et qu'aucune touche existante (`tui-shell`, `add-request-run`, `add-search-and-yank`) n'est modifiée
- [ ] 3.2 Implémenter le basculement de focus pour `ToggleDiagnostics`/`ToggleHistory` (interrupteur : ferme si déjà ouvert, ouvre sinon), sans effet tant qu'aucune collection n'est chargée (D5) ; vérifier par tests unitaires les deux sens du basculement et l'absence d'effet en état `Loading`/`Failed`

## 4. Navigation dans les deux panneaux

- [ ] 4.1 Implémenter le déplacement de `diagnostics_selected`/`history_selected` pour `Up`/`Down`/`Home`/`End`/`PageUp`/`PageDown` selon `model.focus`, borné à la longueur de la liste courante (D5) ; vérifier par tests unitaires les extrémités et le défilement de page sur une collection dont les diagnostics comportent au moins trois entrées
- [ ] 4.2 Implémenter la navigation croisée : `Message::Right` quand `focus == Diagnostics` déplie les dossiers ancêtres de l'entrée sélectionnée, rafraîchit les lignes, sélectionne le nœud dans l'arbre et repasse le focus à `Tree` (D4) ; vérifier par test unitaire sur `badmeta` (dossier) et sur `broken.bru` (fichier à la racine) que la sélection et le dépliage sont corrects après l'appel

## 5. Journal des exécutions

- [ ] 5.1 Implémenter, dans le traitement de `Message::RunFinished` (ajouté par `add-request-run`), la construction d'une `HistoryEntry` selon D3 (verdict via `RequestResult::is_failure()`, jamais via `Summary` de `bru`; durée = somme des `run_duration`) et son insertion en tête de `model.history`, avec retrait de la plus ancienne au-delà de `HISTORY_LIMIT` ; vérifier par tests unitaires, avec le faux `bru` (`tests/fixtures/fake-bru/fake-bru.sh`) : une exécution réussie ajoute une entrée `Completed` correcte, une exécution dont le rapport contient un `pass` avec assertion en échec ajoute une entrée dont `failed >= 1`, une annulation ajoute une entrée `Cancelled`, une erreur de lancement ajoute une entrée `Failed`
- [ ] 5.2 Vérifier par un test unitaire que 201 exécutions successives laissent le journal à 200 entrées, la plus ancienne ayant été retirée

## 6. Rejeu

- [ ] 6.1 Implémenter `Message::RunSelected` quand `focus == History` : relance une exécution sur la cible et le mode récursif de l'entrée sélectionnée, selon les mêmes règles que le lancement depuis l'arbre (aucun effet si une exécution est déjà en cours) ; vérifier par tests unitaires les deux cas (aucune exécution en cours → nouvelle `Command::StartRun` avec la bonne cible ; exécution en cours → `Command::None`)
- [ ] 6.2 Vérifier par un test unitaire que rejouer une entrée ne modifie pas l'entrée déjà présente dans `model.history`, et que la nouvelle exécution produit sa propre entrée distincte une fois terminée

## 7. Rendu

- [ ] 7.1 Ajouter le badge `⚠ N` à `title_line` quand `diagnostics(collection)` n'est pas vide, absent sinon (D6) ; vérifier par test `TestBackend` sur `parser-cases` que le badge affiche `⚠ 3` et par un test sur une collection sans erreur qu'aucun badge n'apparaît
- [ ] 7.2 Implémenter le rendu du panneau de diagnostics sur `areas.body` (liste des entrées avec chemin et raison, message dédié si vide) et du panneau d'historique (liste des entrées avec horodatage, cible, verdict, durée, message dédié si vide) dans un nouveau `src/app/view/panels.rs` ; vérifier par tests `TestBackend` le contenu affiché pour chacun, avec et sans entrées
- [ ] 7.3 Ajouter les deux entrées de `status_line` pour `Focus::Diagnostics`/`Focus::History` (D6) ; vérifier par test `TestBackend` que la barre d'état affiche les touches attendues dans chaque état

## 8. Vérifications transverses

- [ ] 8.1 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests dans les fichiers ajoutés ou modifiés, et qu'aucun appel `std::fs` d'écriture n'apparaît dans `src/app/diagnostics.rs` ni dans le code du journal (garantit l'absence de persistance exigée par le spec)
- [ ] 8.2 Écrire un test d'intégration, sur le modèle de `tests/app_shell.rs`, qui exécute une requête via le faux `bru`, vérifie l'apparition de l'entrée dans le panneau d'historique rendu, puis rejoue cette entrée et vérifie qu'une seconde exécution démarre
- [ ] 8.3 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
