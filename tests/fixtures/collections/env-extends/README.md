# Fixture collection : env-extends

Cette collection de fixtures reproduit fidèlement les fichiers d'environnement générés par Bruno bureau 4.x.

## Provenance

Les fichiers sous `environments/` ont été écrits à la main en suivant ligne à ligne le
sérialiseur de Bruno : `@usebruno/lang/v2/src/jsonToEnv.js`, lu dans l'archive de Bruno
bureau 4.2.1 (`resources/app.asar`). Forme reproduite : `extends` en tête, puis `vars`,
puis `color` en fin de fichier, sans ligne vide entre les blocs.

Les cas couverts :
- `base.bru` : environnement de base avec variables `base_url` et `timeout`, couleur hexadécimale `#336699`.
- `custom-parent.bru` : second environnement parent avec variable `custom_key`, couleur `#663399`.
- `staging.bru` : héritage simple `extends: base`, variables propres `app_env` et `debug`, couleur `#ffaa00`.
- `multi.bru` : héritage multiple `extends [\n  base,\n  custom-parent\n]`, variable propre `app_env`, couleur `#00ff00`.
- `quoted.bru` : héritage avec nom contenant une virgule nécessitant des guillemets `extends [\n  base,\n  "parent, with comma"\n]`, sans variables, couleur courte `#f0a`.
- `invalid-color.bru` : environnement avec valeur de couleur non hexadécimale `color: not-a-color` (qui doit être ignorée sans erreur par le TUI).

## Requête

`ping.bru` est une requête GET simple utilisant `{{base_url}}` (définie dans `base.bru`) pour valider l'exécution avec `bru run --env staging`.
