## 1. Mise en place du crate

- [x] 1.1 Ajouter à `Cargo.toml` `tokio` (features `process`, `rt-multi-thread`, `macros`, `sync`, `io-util`), `serde` (`derive`), `serde_json`, `thiserror`, `command-fds` (`tokio`) ; vérifier que `cargo build` réussit
- [x] 1.2 Créer `src/lib.rs` (`pub mod runner;`) et le squelette `src/runner/{mod.rs, request.rs, process.rs, report.rs, error.rs}` avec commentaires de module en français ; vérifier `cargo build` et `cargo clippy -- -D warnings`

## 2. Fixtures réelles

- [x] 2.1 Créer la collection `tests/fixtures/collections/runner-probe/` (`bruno.json`, requête avec assertions et tests en succès/échec, requête vers un port fermé, requête ignorée via `bru.runner.skipRequest()`, requête à corps JSON avec tests pré-requête et post-réponse) ; vérifier que `bru run -r` la parcourt sans erreur de parsing
- [x] 2.2 Écrire `scripts/gen-report-fixtures.sh` (serveur `python3 -m http.server` local, `bru run -r --reporter-json`, arrêt du serveur) ; l'exécuter et vérifier que `tests/fixtures/reports/mixed.json` contient les cas `pass`, `fail`, `error` et `skipped`
- [x] 2.3 Ajouter `tests/fixtures/reports/README.md` indiquant la version de `bru` (2.13.2), la commande et la date de capture ; vérifier par relecture qu'aucun secret réel ni en-tête sensible n'est présent dans les fixtures

## 3. Modèle de rapport

- [x] 3.1 Implémenter dans `report.rs` `Report`, `Iteration`, `Summary`, `RequestResult`, `RequestInfo`, `ResponseInfo`, `AssertionResult`, `TestResult` (camelCase, champs optionnels, `serde_json::Value` pour corps/en-têtes/`actual`/`expected`, sans `deny_unknown_fields`) ; vérifier par un test que `mixed.json` se désérialise et que chaque champ de chaque type est comparé aux valeurs de la fixture
- [x] 3.2 Implémenter `ResponseStatus` (`Http(u16) | Error | Skipped | Other(String)`) et `ResultStatus` (`Pass | Fail | Error | Skipped | Other(String)`) ; vérifier par tests : `200`, `"error"`, `"skipped"` issus de la fixture, et une valeur inconnue conservée dans `Other`
- [x] 3.3 Vérifier par un test qu'un rapport de fixture enrichi d'un champ inconnu à chaque niveau se désérialise toujours
- [x] 3.4 Implémenter `RequestResult::is_failure()` et `Report::failures()` selon la règle du spec ; vérifier par tests sur la fixture : `pass` avec assertion en échec → échec, `pass` avec seul test post-réponse en échec → échec, `error` → échec, `skipped` → pas échec, tout vert → pas échec

## 4. Requête et erreurs

- [x] 4.1 Implémenter `RunRequest`, `SecretString` (Debug/Display masqués) et la construction des arguments `bru run [cibles] [-r] [--env X] [--env-var k=v]... --reporter-json /dev/fd/3` ; vérifier par tests unitaires la liste d'arguments (collection entière, dossier récursif + env, plusieurs cibles) et que `format!("{:?}", req)` ne contient pas la valeur secrète
- [x] 4.2 Implémenter `RunError` avec `thiserror` (`BruNotFound`, `Spawn`, `NoReport`, `InvalidReport`, `Io`, `UnsupportedPlatform`) et `RunOutcome` ; vérifier par un test que le `Display` de `InvalidReport` ne contient pas le contenu du rapport

## 5. Exécution asynchrone

- [x] 5.1 Créer le faux `bru` `tests/fixtures/fake-bru/fake-bru.sh` piloté par variable d'environnement (écrit une fixture sur fd 3 et sort en 1 ; n'écrit rien et sort en 0 ; écrit du JSON invalide ; écrit « Path not found » sur stdout et sort en 5 ; dort longtemps) ; vérifier chaque mode à la main en shell
- [x] 5.2 Implémenter dans `process.rs` le lancement : pipe `std::io::pipe`, mappage fd 3 via `command-fds`, `current_dir` = racine, `stdin` null, `kill_on_drop(true)`, fermeture de l'extrémité écriture côté parent, lecture du rapport en `spawn_blocking`, lecture concurrente stdout/stderr bornée à 64 Kio ; vérifier par test avec le faux `bru` qu'une fixture écrite sur fd 3 revient en `Completed` avec le code 1
- [x] 5.3 Implémenter la classification (D3) ; vérifier par tests avec le faux `bru` : rien écrit + code 0 → `NoReport`, code 5 → `NoReport` dont `output` contient `Path not found`, JSON invalide → `InvalidReport`, programme inexistant → `BruNotFound`
- [x] 5.4 Implémenter `BruRunner::start` (retour immédiat, identifiant croissant, un unique `RunEvent` émis sur le `mpsc`) ; vérifier par tests que `start` retourne avant la fin d'un faux `bru` qui dort, et que deux runs concurrents émettent chacun un événement avec le bon identifiant
- [x] 5.5 Implémenter l'annulation (`RunHandle::cancel` et drop du handle) avec `kill` + `wait` ; vérifier par tests que l'issue est `Cancelled`, qu'aucun rapport partiel n'est émis et que le PID du faux `bru` n'existe plus après l'événement
- [x] 5.6 Ajouter la garde `cfg(not(unix))` émettant `UnsupportedPlatform` ; vérifier `cargo check` sur la cible hôte (le cas non-Unix est vérifié par relecture)

## 6. Vérifications transverses

- [x] 6.1 Test d'intégration `#[ignore]` lançant le vrai `bru` sur `runner-probe` avec serveur local ; vérifier `cargo test -- --ignored` : issue `Completed`, 1 itération, verdicts attendus
- [x] 6.2 Test « aucun fichier de rapport » : exécuter un run réel ou factice dans un répertoire temporaire de travail et vérifier qu'aucun nouveau fichier régulier n'apparaît dans la collection ni dans le répertoire courant
- [x] 6.3 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests dans `src/runner/`, et qu'aucun appel de log/`println!` n'imprime rapport, sortie de `bru` ou arguments
- [x] 6.4 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
