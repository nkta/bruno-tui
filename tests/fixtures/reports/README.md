# Fixtures de rapport `bru run`

Rapports JSON produits par un vrai `bru run`, jamais écrits à la main.
Ils servent de référence aux types de `src/runner/report.rs`.

## `mixed.json`

- Version : `bru` 2.13.2 (Node 22.17.0, Linux/WSL)
- Date de capture : 2026-09-14
- Collection : `tests/fixtures/collections/runner-probe/`
- Commande : `scripts/gen-report-fixtures.sh`, soit
  `bru run -r --reporter-json /dev/fd/3 3>mixed.json` à la racine de la
  collection, avec `www/` servi sur `127.0.0.1:18765`
- Code de sortie : 1

Cas couverts :

| Requête | `status` | `response.status` | Particularité |
|---|---|---|---|
| `folder/down` | `error` | `"error"` | connexion refusée (port 18799 fermé) |
| `ok` | `pass` | `200` | assertion et test en échec, corps texte |
| `json` | `pass` | `200` | test pré-requête OK, test post-réponse KO, corps JSON |
| `green` | `pass` | `200` | tout en succès |
| `skip` | `skipped` | `"skipped"` | sautée par `bru.runner.skipRequest()` |

Aucun secret : requêtes sans auth ni en-têtes, serveur local.

Régénérer modifie les dates, `uid` et durées : mettre à jour les tests
qui comparent ces valeurs.

## `pre-request-error.json`

- Version : `bru` 2.13.2 (Node 22.17.0, Linux/WSL)
- Date de capture : 2026-09-17
- Collection : `tests/fixtures/collections/runner-probe-errored/`
- Commande : `scripts/gen-report-fixtures.sh`, soit
  `bru run -r --reporter-json /dev/fd/3 3>pre-request-error.json` à la
  racine de la collection (aucun serveur requis)
- Code de sortie : 1

Cas couvert :

| Requête | `status` | `response.status` | Particularité |
|---|---|---|---|
| `boom` | `error` | `"error"` | échec avant envoi : le script pré-requête lève une exception, `request.method`, `url`, `headers` et `data` valent `null` |

Même forme que pour des variables d'environnement requises manquantes
(`Missing required environment variables: …`). Aucun secret.

## `post-with-body.json`

- Version : `bru` 4.1.0 (Node 22.19.0, Linux)
- Date de capture : 2026-10-11
- Collection : `tests/fixtures/collections/runner-probe-post/`
- Commande : `scripts/gen-report-fixtures.sh`, soit
  `bru run -r --reporter-json /dev/fd/3 3>post-with-body.json` à la racine de la
  collection, avec `tests/fixtures/collections/runner-probe/www` servi sur `127.0.0.1:18765`
- Code de sortie : 0

Cas couvert :

| Requête | `status` | `response.status` | Particularité |
|---|---|---|---|
| `post` | `pass` | `501` | requête POST envoyée avec en-têtes et corps JSON (`request.data`) |


## `opencollection.json`

- Version : `bru` 4.1.0 (Node 22.19.0, Linux)
- Date de capture : 2026-10-11
- Collection : `tests/fixtures/collections/oc-probe/` (OpenCollection YAML)
- Commande : `scripts/gen-report-fixtures.sh`, soit
  `bru run -r --env local --env-var token=fixture-token --reporter-json /dev/fd/3 3>opencollection.json`
  à la racine de la collection, avec `tests/fixtures/collections/runner-probe/www`
  servi sur `127.0.0.1:18765`
- Code de sortie : 0

Cas couverts :

| Requête | `status` | `response.status` | Particularité |
|---|---|---|---|
| `data/nested/secure` | `pass` | `200` | auth bearer (masquée `********` par `bru`), 1 test et 2 assertions réussis |
| `data/data` | `pass` | `200` | |
| `data/create data` | `pass` | `501` | POST avec corps JSON, sans test |
| `pages/index` | `pass` | `200` | en-tête et paramètre de requête |

Différence avec une collection `.bru` : `path` garde l'extension
(`data/nested/secure.yml`), identique à `test.filename`. L'appariement
du TUI passe par `test.filename`, donc fonctionne sans conversion.

Aucun secret : jeton factice `fixture-token`, masqué dans le rapport.

## `csv-iterations.json`

- Version : `bru` 4.2.1 (Node 22.19.0, Linux)
- Date de capture : 2026-10-11
- Collection : `tests/fixtures/collections/csv-probe/`
- Commande : `scripts/gen-report-fixtures.sh`, soit
  `bru run --csv-file-path data.csv --reporter-json /dev/fd/3 3>csv-iterations.json`
  avec `tests/fixtures/collections/runner-probe/www` servi sur `127.0.0.1:18765`
- Code de sortie : 0

Cas couvert : exécution pilotée par les données d'un CSV (2 lignes).
Le rapport contient 2 itérations (`iterationIndex: 0` et `iterationIndex: 1`),
chacune contenant le résultat de `req.bru`. Permet de vérifier la gestion
des itérations multiples et l'affichage dans le TUI sans écrasement.


## `multi-env-up.json` et `multi-env-down.json`

- Version : `bru` 4.2.1 (Linux)
- Date de capture : 2026-10-12
- Collection : `tests/fixtures/collections/multi-env-probe/`
- Commande : `scripts/gen-report-fixtures.sh`, soit
  `bru run -r --env <up|down> --reporter-json /dev/fd/3 3>multi-env-<env>.json`
  à la racine de la collection, avec `tests/fixtures/collections/runner-probe/www`
  servi sur `127.0.0.1:18765`
- Codes de sortie : 0 (`up`), 1 (`down`)

Cas couvert : la même campagne lancée sur deux environnements dont les
verdicts divergent (tableau comparatif du panneau Campagne).

| Requête | `up` | `down` | Particularité |
|---|---|---|---|
| `data` | `pass` / `200` | `error` / `"error"` | `{{baseUrl}}` vers le port fermé 18799 sur `down` |
| `static` | `pass` / `200` | `pass` / `200` | URL fixe : même verdict partout |
| `skip` | `skipped` | `skipped` | sautée par `bru.runner.skipRequest()` |

Aucun secret.
