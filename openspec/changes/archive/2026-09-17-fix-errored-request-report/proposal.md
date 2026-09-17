## Why

Quand une requête échoue avant d'être envoyée (script pré-requête qui
lève une exception, variables d'environnement requises manquantes…),
`bru run --reporter-json` (2.13.2) émet un résultat dont
`request.method`, `request.url` et `request.headers` valent `null`. Le
modèle de rapport exige une méthode et une URL non nulles : la
désérialisation de **tout le rapport** échoue, l'exécution tombe en
« Rapport invalide » et l'utilisateur ne voit ni le résultat de cette
requête, ni ceux des autres requêtes de la campagne, ni surtout le
message d'erreur qui explique la panne (ex. `Missing required
environment variables: oktaClientSecret`).

Forme vérifiée sur un vrai `bru run` 2.13.2 avec un script pré-requête
`throw new Error(...)` : `"request": {"method": null, "url": null,
"headers": null, "data": null}`, `"response": {"status": "error", …,
"responseTime": 0}`, `"status": "error"`, `"error": "<message>"`, listes
de tests vides.

## What Changes

- Le modèle de rapport accepte une requête non envoyée : méthode, URL et
  en-têtes de la requête deviennent optionnels (`null` ou absents).
- Le bandeau de statut du panneau Réponse affiche le message `error`
  rapporté par `bru` dès qu'il est présent, pour que la cause d'une
  requête en erreur avant envoi soit visible.
- Nouvelle fixture de rapport issue d'un vrai `bru run`, produite par une
  collection de fixture dédiée dont le script pré-requête lève une
  exception, et régénérable par `scripts/gen-report-fixtures.sh`.
- Tests de désérialisation et de vue couvrant ce cas.

## Capabilities

### New Capabilities

_Aucune._

### Modified Capabilities

- `bru-runner` : le modèle de rapport typé tolère une requête sans
  méthode, URL ni en-têtes (échec avant envoi) ; la couverture par
  fixtures réelles inclut ce cas.
- `response-tabs` : le bandeau de statut affiche le message d'erreur
  rapporté par `bru` quand il existe.

## Impact

- `src/runner/report.rs` : `RequestInfo` (champs optionnels).
- `src/app/view/detail.rs` : `status_band`, et les résultats construits
  à la main dans les tests (`base_result`), ainsi que
  `sample_request_result` dans `src/app/update.rs`.
- `tests/report_fixtures.rs`, `tests/fixtures/reports/` (nouvelle
  fixture + README), `tests/fixtures/collections/` (nouvelle collection
  de fixture), `scripts/gen-report-fixtures.sh`.
- Aucune nouvelle dépendance. Aucun changement d'API publique hors du
  type `RequestInfo`, qui n'est lu par aucune vue aujourd'hui.
