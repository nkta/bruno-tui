## Context

Voir `proposal.md` (Why). État actuel observé :

- `RequestInfo` (`src/runner/report.rs`) déclare `method: String`,
  `url: String` et `headers: BTreeMap<String, Value>` avec
  `#[serde(default)]`. `default` ne couvre que l'absence du champ, pas
  un `null` explicite : les trois champs font échouer serde sur le
  rapport d'une requête non envoyée. `process::classify` classe alors
  toute l'exécution en `RunError::InvalidReport`.
- `RequestInfo` n'est lu par aucune vue : seuls les tests
  (`base_result` dans `src/app/view/detail.rs`, `sample_request_result`
  dans `src/app/update.rs`) le construisent.
- `status_band` affiche déjà `result.error`, mais uniquement dans la
  branche `ResponseStatus::Error`.
- La forme exacte du JSON a été vérifiée sur un vrai `bru run` 2.13.2
  (collection jetable avec `throw new Error(...)` en pré-requête) : code
  de sortie 1, `request` entièrement à `null` (y compris `data`),
  `response.status = "error"`, `responseTime = 0`, `status = "error"`,
  `error` = message de l'exception, listes de tests vides, pas de champ
  `skipped` ni `shouldStopRunnerExecution`, résumé avec
  `errorRequests: 1`.
- `mixed.json` est référencé par `tests/report_fixtures.rs`,
  `tests/runner_fake_bru.rs` et `app::test_support::runner_probe_model`,
  qui vérifient noms, résumé, dates et en-têtes exacts.

## Goals / Non-Goals

**Goals:**

- Désérialiser sans erreur un rapport contenant une requête non envoyée.
- Rendre visible le message `error` dans le panneau Réponse.
- Fixer la forme par une fixture réelle régénérable.

**Non-Goals:**

- Afficher la méthode ou l'URL résolues de la requête envoyée (aucune vue
  ne les utilise aujourd'hui).
- Modéliser `request.data`, toujours ignoré.
- Reproduire en fixture le cas « Missing required environment
  variables » : il nécessite une configuration d'environnement ; la
  forme émise est la même que pour une exception pré-requête, qui est
  déterministe et sans secret.
- Changer la classification « Rapport invalide » pour d'autres formes
  inattendues.

## Decisions

### D1. Champs de `RequestInfo` en `Option`

`method: Option<String>`, `url: Option<String>`,
`headers: Option<BTreeMap<String, Value>>`, chacun avec
`#[serde(default)]` (serde mappe `null` et l'absence sur `None`).

Cohérent avec `ResponseInfo`, qui modélise déjà `headers`, `url` et
`status_text` en `Option` pour la même raison.

Alternatives écartées :
- Garder `String` avec un désérialiseur `null → ""` : invente une valeur
  qui n'a pas été émise et rend indistinguables « URL vide » et « requête
  non envoyée ».
- Rendre `request` entier optionnel : ne correspond pas au JSON, où
  l'objet est présent avec des champs `null`.

### D2. Ligne « Erreur » affichée dès que `result.error` existe

Dans `status_band`, sortir l'affichage de `error` de la branche
`ResponseStatus::Error` : après la ligne « Statut », ajouter
`field("Erreur", …)` si `result.error.is_some()`, quel que soit le
statut. La branche `Error` conserve « Statut : aucune réponse ».

Un résultat qui porte une erreur est en échec (`is_failure`) ; masquer
le message selon le statut de réponse cacherait la cause d'une forme que
`bru` pourrait émettre à l'avenir. Pas de doublon : le message n'est
émis qu'à un seul endroit.

### D3. Collection de fixture dédiée plutôt qu'ajout à `runner-probe`

Nouvelle collection `tests/fixtures/collections/runner-probe-errored/`
(`bruno.json` + `boom.bru`, `seq: 1`, URL `http://127.0.0.1:18799/nope`
jamais atteinte, `script:pre-request` faisant
`throw new Error("pre-request failure (fixture)")`), et nouvelle fixture
`tests/fixtures/reports/pre-request-error.json`.

Ajouter la requête à `runner-probe` changerait `mixed.json` (noms,
résumé, dates, `uid`) et forcerait la mise à jour de trois suites de
tests sans rapport avec ce bug. Message ASCII pour éviter toute
ambiguïté d'échappement dans les assertions.

### D4. Génération dans `scripts/gen-report-fixtures.sh`

Le script produit les deux fixtures : après `mixed.json` (serveur HTTP
requis), un second `bru run -r --reporter-json /dev/fd/3` à la racine de
`runner-probe-errored` (aucun serveur requis), même transport fd 3. Le
README de `tests/fixtures/reports/` documente version, date, commande,
code de sortie et cas couvert, comme pour `mixed.json`.

### D5. Tests

- `tests/report_fixtures.rs` : test sur `pre-request-error.json`
  vérifiant tous les champs du résultat (requête à `None`, réponse
  `Error`, statut `Error`, message, listes vides, verdict en échec) et le
  résumé.
- `src/app/view/detail.rs` : test du bandeau construit depuis la fixture
  réelle (via `include_str!`) vérifiant « Verdict : échec », « Statut :
  aucune réponse » et le message ; test qu'un résultat HTTP 200 sans
  erreur n'affiche pas de ligne « Erreur ».
- `tests/runner_real_bru.rs` (ignoré par défaut) : exécution réelle de
  `runner-probe-errored` aboutissant à une issue Terminée, pour détecter
  un changement de forme lors d'une montée de version de `bru`.

## Risks / Trade-offs

- [Une future version de `bru` rend d'autres champs `null`
  (ex. `response.responseTime`, `name`)] → hors périmètre ; le test
  ignoré sur vrai `bru` et la régénération des fixtures le détecteront.
- [Changement de type public de `RequestInfo`] → seuls des tests le
  construisent ; mise à jour mécanique en `Some(...)`.
- [La fixture ne reproduit pas littéralement le cas « Missing required
  environment variables »] → même forme vérifiée ; le scénario de spec
  du bandeau utilise ce message sur un résultat construit, la fixture
  garantit la structure.
