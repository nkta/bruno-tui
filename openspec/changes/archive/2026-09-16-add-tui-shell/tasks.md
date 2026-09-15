## 1. Mise en place

- [x] 1.1 Ajouter à `Cargo.toml` `ratatui` 0.30.2 avec `default-features = false` et les features `crossterm`, `layout-cache`, `unstable-rendered-line-info` (D1) ; vérifier `cargo build` et que `cargo tree -i crossterm` ne montre qu'une version de crossterm
- [x] 1.2 Créer le squelette `src/app/{mod.rs, event.rs, message.rs, model.rs, update.rs, cli.rs, view/mod.rs, view/tree.rs, view/detail.rs}` avec commentaires de module en français, déclarer `pub mod app;` dans `src/lib.rs` ; vérifier `cargo build` et `cargo clippy -- -D warnings`

## 2. Ligne de commande

- [x] 2.1 Implémenter `cli::parse` (D8) : `Run(PathBuf)` avec répertoire courant par défaut, `-h`/`--help`, `-V`/`--version`, `UsageError` pour option inconnue ou deux positionnels, texte d'usage ; vérifier par tests unitaires chacun de ces cas, dont un chemin non UTF-8

## 3. Événements et messages

- [x] 3.1 Implémenter `AppEvent` et `Message` (D3, D4) et `to_message` : flèches, `j/k/h/l`, `g/G`, `Début/Fin`, `Page précédente/suivante`, `Entrée` → `Right`, `Tab`, `Échap`, `q`, `Ctrl+C`, redimensionnement, chargement terminé, terminal fermé ; vérifier par tests unitaires chaque correspondance, qu'un `KeyEventKind::Release` donne `None` et que `c` sans `Ctrl` donne `None`
- [x] 3.2 Implémenter dans `event.rs` le thread de lecture du terminal (`crossterm::event::read` → `blocking_send`, arrêt sur envoi impossible, `TerminalClosed` sur erreur de lecture) ; vérifier par relecture qu'il ne capture aucune ressource autre que l'émetteur, et par `cargo clippy -- -D warnings`

## 4. Modèle et mise à jour

- [x] 4.1 Implémenter `Model`, `CollectionState`, `TreeState`, `Row`, `Focus`, `Exit` (D5) et le calcul des lignes visibles à partir de `Collection` et des dossiers dépliés ; vérifier par test unitaire sur `parser-cases` chargée par `BruLoader` que les lignes initiales sont les 11 nœuds de premier niveau, dans l'ordre du spec, avec `Groupe` sélectionné
- [x] 4.2 Implémenter `update` pour la navigation de l'arbre (`Up`, `Down`, `Home`, `End`, `Right`, `Left`) avec préservation de la sélection au repli ; vérifier par tests unitaires les scénarios du spec : deux `Right` sur `Groupe` → `x` sélectionné, `Left` depuis `x` → `Groupe` déplié, second `Left` → replié, `Up` sur le premier nœud sans effet, `Right` sur une requête sans effet
- [x] 4.3 Implémenter le calcul de `offset` à partir de `layout(size)` (D5, D7) pour `Down`, `Up`, `Home`, `End`, `Resize` et après chargement ; vérifier par tests unitaires sur un terminal de 12 lignes que `End` rend le dernier nœud visible, que `Home` remet `offset` à 0, et qu'un `Resize` plus petit garde la sélection visible
- [x] 4.4 Implémenter les messages de focus et de défilement du détail (`NextFocus`, `FocusTree`, `Up`/`Down`/`PageUp`/`PageDown`/`Home`/`End` quand le détail a le focus), la borne par `Paragraph::line_count` (D6) et la remise à zéro au changement de sélection ; vérifier par tests unitaires : `End` puis `Down` sur `scripted` dans un terminal de 12 lignes ne dépasse pas la borne, `FocusTree` puis `Down` change la sélection, changer de nœud remet le défilement à 0
- [x] 4.5 Implémenter `CollectionLoaded` (état `Loaded` ou `Failed`, calcul des lignes), `Quit`, `ForceQuit` et `TerminalClosed` ; vérifier par tests unitaires que `Quit` pendant `Loading` fixe `exit` à `Normal`, qu'un `LoadError::NotACollection` passe en `Failed`, et qu'une collection vide donne zéro ligne sans panique

## 5. Rendu

- [x] 5.1 Implémenter `view/detail.rs` : `detail_text` pour requête, dossier et nœud en erreur (D6), avec libellés d'auth tels qu'écrits ; vérifier par tests unitaires sur `parser-cases` que le texte de `post-json` contient `POST`, `https://{{host}}/items`, `Content-Type`, `json` et `"s": "}"`, que `scripted` marque `X-Debug` désactivé et les quatre blocs présents, que `inherit` affiche `inherit`, que `broken.bru` cite `headers` et la ligne 13, et que `badmeta` affiche l'erreur de `folder.bru` et 1 enfant
- [x] 5.2 Implémenter `view/mod.rs` et `view/tree.rs` : `layout`, titre, arbre (glyphes, indentation, méthode alignée, marqueur d'erreur, nom de fichier avec extension pour les erreurs), détail, focus visible, barre d'état, message sous 40×10 (D7) ; vérifier par tests `TestBackend` 100×30 que la première colonne de l'arbre initial contient les 11 noms du spec dans l'ordre avec `✗` sur `badmeta`, `broken.bru` et `no-method.bru`, que la ligne de `ping` contient `GET`, qu'un état `Failed` sur `NotACollection` affiche le chemin et la raison, qu'une collection vide affiche « collection vide », que 30×8 affiche le message d'agrandissement et que 1×1 se rend sans panique
- [x] 5.3 Vérifier par relecture que la signature de `view` ne prend le modèle qu'en référence immuable (`&Model`) et n'appelle ni `update` ni le loader, et par `grep` qu'aucun appel `std::fs`, `std::process`, `println!` ou `eprintln!` n'existe dans `src/app/view/`

## 6. Boucle et binaire

- [x] 6.1 Implémenter `app::run` générique sur le backend (D9) : chargement par `spawn_blocking`, premier rendu immédiat, traitement des événements jusqu'à `exit` ; vérifier par test d'intégration `tests/app_shell.rs` avec `TestBackend` : le titre affiche `Chargement de` avant le chargement, puis `parser-cases` après l'événement de chargement, et `run` retourne `Exit::Normal` après un `q` injecté
- [x] 6.2 Ajouter à `tests/app_shell.rs` le test du chargement bloqué : loader bloqué sur une barrière, `q` injecté, `run` retourne en moins d'une seconde alors que le loader est toujours bloqué, puis libération de la barrière ; vérifier `cargo test --test app_shell`
- [x] 6.3 Réécrire `src/main.rs` selon l'ordre de D9 : arguments, détection de terminal, runtime construit à la main, `try_init`, thread de lecture, `run`, `try_restore`, `shutdown_background`, codes de sortie 0, 1, 2 ; vérifier `cargo build`, puis `bruno-tui --help` (code 0), `bruno-tui a b` (code 2) et `bruno-tui tests/fixtures/collections/parser-cases > /dev/null` (code 1, message sur la sortie d'erreur)

## 7. Vérifications transverses

- [x] 7.1 Vérification manuelle dans un pseudo-terminal avec `script` : lancer `bruno-tui tests/fixtures/collections/parser-cases`, naviguer, déplier `Groupe`, consulter `post-json`, quitter par `q` ; vérifier le code 0, la présence de la séquence de sortie d'écran alternatif dans la capture, et que `git status` ne montre aucun fichier créé ou modifié sous `tests/fixtures/`
- [x] 7.2 Vérifier par relecture que le hook de panique de `ratatui::try_init` est installé avant tout autre code susceptible de paniquer dans la boucle, et par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests et hors `main` dans `src/app/`, et qu'aucun `std::process`, écriture `std::fs`, `println!` ou `eprintln!` n'existe dans `src/app/`
- [x] 7.3 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
