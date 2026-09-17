## 1. Ligne de commande

- [x] 1.1 Ajouter `SecretMapping { name, key: Option<String> }` (nouveau
      module `src/secrets/mod.rs`, exporté par `src/lib.rs`) avec une
      fonction de validation de nom/clé (non vide, sans `=` ni espace
      blanc) et `upper_snake` ; tests unitaires sur `token`, `a b`, `=x`,
      `a=b`, et le scénario « Formes UPPER_SNAKE_CASE »
- [x] 1.2 Étendre `cli::parse` (`src/app/cli.rs`) : `Command::Run` porte
      `Vec<SecretMapping>` ; accepter `--secret X`, `--secret X=Y`,
      `--secret=X`, `--secret=X=Y`, répétables et placés avant ou après le
      chemin ; nouvelles `UsageError::MissingSecretArgument` et
      `InvalidSecret`. Tests : scénarios « Déclarations de secrets » et
      « Déclaration de secret invalide » de `specs/tui-shell`, plus
      non-régression des tests CLI existants
- [x] 1.3 Documenter `--secret NOM[=CLÉ]` dans `USAGE` (recherche
      automatique des `vars:secret`, clés candidates, ordre saisie >
      `.env` > shell, jamais de valeur en argument, visibilité dans `ps`
      pendant l'exécution de `bru`) et transmettre les mappings de
      `src/main.rs` à `app::run` ; vérifier `bruno-tui --help`

## 2. Analyse `.env` et résolution

- [x] 2.1 Écrire `src/secrets/dotenv.rs` reproduisant `dotenv.parse`
      16.6.1 (voir design.md, « Analyse `.env` écrite à la main ») ; les
      erreurs et `Debug` ne contiennent aucune valeur
- [x] 2.2 Créer la fixture `tests/fixtures/dotenv/cases.env` (export,
      `:`, quotes simples/doubles/backtick, multiligne, `\n` entre `"`,
      commentaire en fin de ligne, doublon, ligne invalide) et relever les
      valeurs attendues avec `node -e` sur le `dotenv` embarqué par `bru`
      2.13.2 ; test de fixture comparant clé par clé, version de `dotenv`
      notée dans le test
- [x] 2.3 Implémenter `SecretLookup` (clés candidates : clé explicite,
      sinon nom exact puis `UPPER_SNAKE_CASE`) et
      `resolve(root, lookups, lookup_env)` → `Vec<Resolved>` avec
      `SecretSource` (`DotEnv`, `Shell`, `Missing { keys }`, `Invalid`) ;
      tests unitaires avec `lookup_env` injecté : recherche automatique en
      `UPPER_SNAKE_CASE`, nom exact prioritaire, clé explicite exclusive,
      repli shell, `.env` prioritaire sur l'ordre des clés, aucune clé
      trouvée, `.env` absent, `.env` illisible, valeur multiligne et shell
      non UTF-8 refusés, `Debug` de `Resolved` sans valeur

## 3. Modèle et messages

- [x] 3.1 Ajouter à `SecretString` (`src/runner/request.rs`) les
      opérations de tampon nécessaires à la saisie (`push`, `pop`,
      `is_empty`) sans exposer la valeur ; tests unitaires
- [x] 3.2 Ajouter `SecretsState` et `Focus::Secrets` au `Model`
      (`src/app/model.rs`), mappings placés par `app::run` ; écrire
      les fonctions pures `secret_rows(model)` (union ordonnée
      mappings → `vars:secret` courants → ajouts, état par ligne) et
      `secret_env_vars(model)` ; tests : scénarios « Union des sources de
      noms », « Aucun environnement », « Changement d'environnement »,
      « Saisie prioritaire », et `format!("{model:?}")` sans valeur
- [x] 3.3 Ajouter les messages `ToggleSecrets` (`S`), `AddSecret` (`a`),
      `ForgetSecret` (`d`), `SecretsResolved`, et les captures
      `TextCapture::SecretName` / `SecretValue` avec `SecretInput`,
      `SecretBackspace`, `ConfirmSecretInput`, `CancelSecretInput`
      (`src/app/message.rs`) ; `Debug` de `SecretInput` masqué ; tests de
      table de touches : `S`/`a`/`d` hors saisie, `q`/`r`/`j` capturés en
      saisie, `Ctrl+C` toujours `ForceQuit`, aucune collision avec les
      touches existantes
- [x] 3.4 Ajouter `AppEvent::SecretsResolved` (`src/app/event.rs`) et sa
      traduction dans `to_message`

## 4. Logique `update`

- [x] 4.1 Sur `CollectionLoaded(Ok)`, renvoyer
      `Command::ResolveSecrets { root, lookups }` pour tous les noms
      connus (mappings et `vars:secret` de tous les environnements
      valides, `Command::None` s'il n'y en a aucun), réinitialiser
      `acknowledged`, `pending_run` et fermer le panneau ; sur `SecretsResolved`, stocker
      le résultat ; ajuster les tests existants concernés et vérifier
      `cargo test`
- [x] 4.2 Ouverture/fermeture/navigation du panneau (`S`, `Échap`,
      `↑`/`↓`, sans collection = sans effet) ; tests des scénarios
      « Ouverture et états affichés », « Liste vide », « S sans
      collection chargée »
- [x] 4.3 Saisie masquée : `Entrée` → saisie de valeur, validation,
      abandon, valeur vide = oubli ; `a` → nom puis valeur, refus d'un
      nom invalide ou en double ; `d` → oubli de la saisie et retrait
      d'un nom ajouté ; tests des scénarios « Saisie d'une valeur »,
      « Abandon de la saisie », « Ajout d'une variable non déclarée »,
      « Oubli d'une valeur saisie »
- [x] 4.4 Remplacer `env_vars: Vec::new()` par `secret_env_vars(model)`
      dans `run_selected` (arbre et rejeu) ; tests : scénarios « Cas
      oktaClientSecret », « Aucune variable secrète », « Rejeu après
      modification », et `HistoryEntry` inchangé
- [x] 4.5 Proposition au lancement : `pending_run`, ouverture sur la
      première `vars:secret` non fournie, `r` en focus `Secrets` lance
      et acquitte, `Échap` abandonne et acquitte, exécution en cours
      prioritaire, noms `--secret`/ajoutés non déclencheurs ; tests des
      scénarios de l'exigence « Proposition de saisie au lancement »

## 5. Boucle et vue

- [x] 5.1 Exécuter `Command::ResolveSecrets` dans `spawn_blocking`
      (`src/app/mod.rs`) avec `std::env::var_os`, résultat renvoyé par
      `AppEvent::SecretsResolved` ; test d'intégration dans
      `tests/app_run.rs` (ou nouveau fichier) sur une fixture avec `.env`
- [x] 5.2 Rendre le panneau « Variables secrètes » (`src/app/view/`) :
      nom + état (saisie, `.env (CLÉ)`, `shell (CLÉ)`, non fournie,
      erreur), mention d'exécution en attente, aide des touches, saisie
      affichée en `•` ; tests `TestBackend` vérifiant qu'aucune valeur ni
      longueur n'apparaît hors saisie et que la valeur tapée n'apparaît
      jamais dans le buffer rendu
- [x] 5.3 Ajouter `S` à l'aide/barre d'état existante, sur le modèle de
      `E` ; vérifier le rendu par test

## 6. Fixtures et vérification de bout en bout

- [x] 6.1 Créer `tests/fixtures/collections/secret-probe/` : `bruno.json`,
      `.env` factice (`OKTA_CLIENT_SECRET=fixture-value`),
      `environments/CI.bru` avec `vars:secret [ oktaClientSecret ]`, une
      requête dont le script pre-request lit `bru.getEnvVar("oktaClientSecret")` et un
      test qui vérifie sa valeur ; vérifier en CLI que
      `bru run --env CI --env-var oktaClientSecret=fixture-value` passe
      et échoue sans `--env-var`
- [x] 6.2 Test ignoré par défaut dans `tests/runner_real_bru.rs` : la
      `RunRequest` produite par `update` sur `secret-probe` avec
      l'environnement `CI`, sans `--secret` (recherche automatique),
      donne un rapport dont le test est en succès
- [x] 6.3 Vérifier qu'aucun fichier n'est créé ni modifié pendant une
      session de test (sauf fixtures lues) et qu'aucune valeur n'est
      écrite sur stdout/stderr ; puis `cargo fmt`, `cargo clippy -- -D
      warnings`, `cargo test` et `cargo test -- --ignored` passent
