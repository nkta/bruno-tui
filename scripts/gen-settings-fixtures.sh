#!/usr/bin/env bash
# Régénère `tests/fixtures/collections/settings-cases/` avec le vrai code de
# Bruno desktop (`transformBrunoConfigBeforeSave`, relevé dans `app.asar`) et
# `transformProxyConfig` de `@usebruno/requests` (livré avec `bru`).
# Nécessite `node`, l'`app.asar` de Bruno (`BRUNO_ASAR`, défaut :
# /snap/bruno/current/resources/app.asar) et `bru` installé
# (`BRUNO_REQUESTS_DIR` pour désigner `@usebruno/requests` à la main).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
node "$ROOT/scripts/bruno-settings-cases.js" "$ROOT/tests/fixtures/collections/settings-cases"
echo "bru $(bru --version 2>/dev/null || echo '?')"
