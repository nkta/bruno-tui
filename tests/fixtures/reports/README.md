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

