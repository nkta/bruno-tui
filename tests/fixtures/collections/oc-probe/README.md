# Fixture `oc-probe` — collection OpenCollection (YAML)

Collection de test du loader OpenCollection (`src/collection/opencollection.rs`).
Ne jamais l'éditer depuis le TUI : le format YAML est en lecture seule.

## Génération

- Outil : `bru` 4.1.0 (Node 22.19.0, Linux)
- Date : 2026-10-11
- Spec source : `tests/fixtures/openapi/oc-probe.yaml`
- Commande, à la racine du dépôt :

```
bru import openapi -s tests/fixtures/openapi/oc-probe.yaml \
  -o tests/fixtures/collections/oc-probe --collection-format opencollection
```

Fichiers produits tels quels par cette commande :

- `pages/folder.yml`, `pages/index.yml` (en-tête et paramètre de requête)
- `data/folder.yml`, `data/data.yml`, `data/create data.yml` (corps JSON)
- `environments/Environment 1.yml`

## Ajouts à la main

Écrits en suivant exactement les `stringify*.ts` de Bruno 4.2.1
(`@usebruno/filestore`, `src/formats/yml/`) :

| Fichier | Contenu ajouté |
|---|---|
| `opencollection.yml` | en-têtes par défaut (`request.headers`) et `docs` (`stringifyCollection.ts`) ; reste du fichier généré |
| `data/nested/folder.yml` | dossier imbriqué : `seq`, en-tête, auth bearer, variable, script, docs (`stringifyFolder.ts`) |
| `data/nested/secure.yml` | en-tête et paramètre désactivés, auth bearer, variables, scripts pré/post, tests, assertions, action `set-variable`, docs (`stringifyHttpRequest.ts`) |
| `environments/local.yml` | `color`, variable désactivée, variable secrète `token` (`stringifyEnvironment.ts`) |
| `environments/staging.yml` | `extends: local` et `color` (`stringifyEnvironment.ts`) |

## Exécution réelle

`bru run` exécute cette collection, à sa racine, avec
`tests/fixtures/collections/runner-probe/www` servi sur `127.0.0.1:18765` :

```
bru run -r --env local --env-var token=fixture-token --reporter-json /dev/fd/3 3>rapport.json
```

Résultat (2026-10-11, `bru` 4.1.0) : 4 requêtes, code de sortie 0,
1 test et 2 assertions réussis. Le rapport capturé est
`tests/fixtures/reports/opencollection.json` (voir son README), régénéré
par `scripts/gen-report-fixtures.sh`.

Remarques constatées avec `bru` 4.1.0 :

- `bru run` ne tient pas compte de `extensions.bruno.ignore` (le TUI, lui,
  l'applique comme Bruno bureau) ;
- `extends` d'un environnement YAML est ignoré par ce CLI (il est lu par
  Bruno 4.2.1) ;
- `auth: none` n'est pas une forme valide (avertissement
  `Unsupported auth type`) : une requête sans auth n'a pas de clé `auth`.
