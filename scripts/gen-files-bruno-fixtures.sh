#!/usr/bin/env bash
# Régénère les contenus de référence des opérations de fichiers
# (`docs/edition-phase3.md`) avec le code officiel de Bruno.
#
# - `tests/fixtures/collections/writer-cases/files/*.bruno.bru` : fichiers
#   écrits par `stringifyRequest` / `stringifyFolder` (`@usebruno/filestore`) ;
# - `tests/fixtures/url-params/bruno-url-params.json` : paramètres que Bruno
#   extrait de chaque URL à la création d'une requête.
#
# Nécessite `node` et `@usebruno/cli` installé globalement, ou
# `BRUNO_LANG_DIR` pointant vers `@usebruno/lang` (`@usebruno/filestore` est
# cherché à côté, ou dans `BRUNO_FILESTORE_DIR`).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/tests/fixtures/collections/writer-cases/files"
mkdir -p "$OUT" "$ROOT/tests/fixtures/url-params"
node "$ROOT/scripts/bruno-new-items.js" "$OUT"
node "$ROOT/scripts/bruno-new-items.js" --url-params "$ROOT/tests/fixtures/url-params/bruno-url-params.json"
echo "bru $(bru --version 2>/dev/null || echo '?') : références écrites"
