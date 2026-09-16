## 1. Modèle de résultat

- [ ] 1.1 Ajouter dans `src/app/model.rs` `RunState`, `ActiveRun`, `RequestOutcome`, `RunFailure` (D2), champ `pub run: RunState` sur `Model`, initialisé par défaut dans `Model::new` ; vérifier `cargo build`
- [ ] 1.2 Ajouter `Command` (D1) dans `src/app/update.rs` ou un fichier dédié, et changer la signature de `update` en `-> Command` en retournant `Command::None` sur toutes les branches existantes ; vérifier `cargo build` et que `cargo test --lib app` compile sans modification des tests existants

## 2. Messages et touches

- [ ] 2.1 Ajouter à `AppEvent` la variante `Run(runner::RunEvent)` dans `src/app/event.rs` ; vérifier `cargo build`
- [ ] 2.2 Ajouter à `Message` `RunSelected`, `CancelRun`, `RunStarted { id, target, recursive, handle }`, `RunFinished(RunEvent)` (D3) ; étendre `to_message` pour `AppEvent::Run` → `Message::RunFinished` ; vérifier par test unitaire que `to_message` traduit un `RunEvent` fabriqué (mode `report`/`none` du faux `bru` non nécessaire ici : un `RunOutcome::Cancelled` suffit à construire l'événement sans I/O)
- [ ] 2.3 Ajouter `KeyCode::Char('r') => Message::RunSelected` dans `key_message`, et remplacer le test d'égalité `Ctrl+C` par un `match` couvrant aussi `Ctrl+X → Message::CancelRun` (D3) ; vérifier par tests unitaires que `r` sans modificateur donne `RunSelected`, que `Ctrl+X` donne `CancelRun`, que `Ctrl+C` continue de donner `ForceQuit`, et qu'un autre `Ctrl+<lettre>` reste ignoré

## 3. Transitions dans update

- [ ] 3.1 Implémenter `RunSelected` (D4) : sans effet si `model.run.active.is_some()` ; sinon détermine la cible depuis `model.selected_node()` (`Request` → chemin, non récursif ; `Folder` → chemin, récursif ; `Error` ou absence de sélection → sans effet), efface `model.run.last_failure`, retourne `Command::StartRun` ; vérifier par tests unitaires sur `parser-cases` : requête sélectionnée → `StartRun` non récursif avec le bon chemin, dossier `grp` sélectionné → `StartRun` récursif, nœud en erreur (`broken.bru`) sélectionné → `Command::None` sans changement d'état
- [ ] 3.2 Implémenter la règle « une exécution à la fois » : `RunSelected` avec `model.run.active` déjà renseigné retourne `Command::None` sans construire de requête ; vérifier par test unitaire en posant `model.run.active` à une valeur construite pour le test (sans `RunHandle` réel, `handle: None` suffit pour ce test précis)
- [ ] 3.3 Implémenter `RunStarted` (D4) : renseigne `model.run.active` ; vérifier par test d'intégration (section 6, nécessite un vrai `RunHandle`)
- [ ] 3.4 Implémenter `CancelRun` (D4) : prend le `handle` de `active` s'il existe et l'annule, sans effacer `active` ; sans effet si aucune exécution n'est en cours ou si `handle` est déjà `None` (annulation déjà demandée) ; vérifier par test unitaire que `CancelRun` sans `active` ne modifie rien, et par test d'intégration (section 6) l'appel réel à `RunHandle::cancel`
- [ ] 3.5 Implémenter `RunFinished` (D4) : ignore un `id` différent de celui de `active` ou l'absence d'exécution active ; sur `Completed`, distribue chaque `RequestResult` du rapport dans `model.run.outcomes` par `test.filename`, efface `last_failure` ; sur `Failed`/`Cancelled`, renseigne `model.run.last_failure` avec la cible de l'exécution prise dans `active` ; efface toujours `active` ; vérifier par tests unitaires construits sur un `Report` et un `RunOutcome` fabriqués à la main (pas besoin de vrai `bru` pour ce test) : un rapport à deux résultats peuple deux entrées de `outcomes` avec les bonnes clés, un `Failed(RunError::BruNotFound)` peuple `last_failure` et vide `active`, un `id` non concordant est ignoré et laisse `active` intact

## 4. Boucle et binaire

- [ ] 4.1 Ajouter le paramètre `bru_program: OsString` à `app::run`, construire `BruRunner::with_program` et la tâche d'adaptation `RunEvent → AppEvent::Run` (D5) ; vérifier `cargo build`
- [ ] 4.2 Dans la boucle de `run()`, après `update`, exécuter `Command::StartRun` en appelant `runner.start(request)` puis en réinjectant `Message::RunStarted` dans `update` (D5) ; vérifier par test d'intégration `tests/app_run.rs` (nouveau fichier, style `tests/app_shell.rs`) avec le faux `bru` en mode `report` sur une fixture de `runner-probe` : après avoir sélectionné une requête, l'envoi de la touche de lancement fait apparaître le résultat correspondant dans le rendu du détail
- [ ] 4.3 Mettre à jour `src/main.rs` pour passer `"bru".into()` à `app::run` ; vérifier `cargo build` et `cargo run -- --help`

## 5. Rendu

- [ ] 5.1 Étendre `tree::row_line` (ou sa signature) pour recevoir le statut de la requête (résultat connu, en cours, ou aucun) et afficher le marqueur ou l'indicateur correspondant (D6) ; vérifier par test unitaire sur des cas construits que le marqueur de succès, le marqueur d'échec et l'indicateur de progression sont chacun rendus, et distincts textuellement du marqueur `✗` de parsing déjà utilisé par les nœuds `Folder`/`Error`
- [ ] 5.2 Étendre `status_line` pour afficher la cible en cours d'exécution avec le rappel de la touche d'annulation quand `model.run.active` est renseigné, sinon le message de `model.run.last_failure` s'il existe, sinon le rappel habituel (D6) ; vérifier par tests unitaires les trois cas
- [ ] 5.3 Étendre `detail::detail_text` pour ajouter la section « Résultat » d'une requête ayant une entrée dans `model.run.outcomes` (D6) ; vérifier par tests unitaires construits sur des `RequestResult` fabriqués à la main (aucun `bru` nécessaire) couvrant chaque scénario du spec : requête réussie, assertion en échec, test post-réponse en échec, aucune réponse (erreur de connexion), requête ignorée
- [ ] 5.4 Vérifier par relecture que `view` continue de ne recevoir le modèle qu'en référence immuable et de ne faire aucune I/O ; par `grep`, qu'aucun appel `std::fs`, `std::process`, `println!` ou `eprintln!` n'apparaît dans les fichiers modifiés de `src/app/view/`

## 6. Tests d'intégration avec le faux bru

- [ ] 6.1 Créer `tests/app_run.rs` sur le modèle de `tests/app_shell.rs`, utilisant `tests/fixtures/fake-bru/fake-bru.sh` et la fixture `tests/fixtures/collections/runner-probe/` ; test « lancement puis résultat » : sélectionner `ok`, envoyer la touche de lancement, injecter l'événement produit par le faux `bru` en mode `report` avec une fixture de rapport contenant une assertion en échec, vérifier que le rendu du détail affiche le verdict d'échec et le message de l'assertion, et que la ligne de l'arbre porte le marqueur d'échec
- [ ] 6.2 Ajouter à `tests/app_run.rs` le test de non-blocage : lancer une exécution avec le mode `slow-report` du faux `bru`, envoyer une touche de navigation puis la touche de sortie avant la fin de l'exécution, vérifier que l'application se ferme sans attendre (même style de mesure de délai que `quit_does_not_wait_for_a_blocked_load` dans `tests/app_shell.rs`)
- [ ] 6.3 Ajouter le test d'annulation : lancer une exécution avec le mode `sleep` du faux `bru`, envoyer la touche d'annulation, vérifier que l'issue reçue est `Cancelled`, que `model.run.last_failure` (ou son reflet dans le rendu) l'indique, et que le processus du faux `bru` n'existe plus après l'événement (même vérification de PID que les tests existants du runner)
- [ ] 6.4 Ajouter un test par variante d'échec du faux `bru` (`none` → aucun rapport, `invalid` → rapport non conforme, programme inexistant → `bru` introuvable) vérifiant que le message affiché distingue chaque cas ; vérifier `cargo test --test app_run`
- [ ] 6.5 Ajouter le test « une exécution à la fois » de bout en bout : lancer une exécution avec le mode `sleep`, changer la sélection puis envoyer de nouveau la touche de lancement, vérifier qu'aucune deuxième exécution n'a démarré (le faux `bru` n'est invoqué qu'une fois, vérifiable par le fichier de PID unique déjà utilisé par le mode `sleep`)

## 7. Vérifications transverses

- [ ] 7.1 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests et hors `main` dans les fichiers modifiés ou ajoutés sous `src/app/`
- [ ] 7.2 Vérifier par relecture qu'aucune valeur de requête, de réponse ou de résultat n'est journalisée (`println!`/`eprintln!`/log) dans `src/app/`, et que `RunRequest.env`/`env_vars` restent toujours vides dans le code produit par ce changement
- [ ] 7.3 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
