#!/usr/bin/env bash
# Régénère les contenus de référence des opérations de fichiers
# (`docs/edition-phase3.md`) avec le sérialiseur officiel de Bruno.
#
# Écrit dans `tests/fixtures/collections/writer-cases/files/` les fichiers
# `*.bruno.bru` produits par `scripts/bruno-new-items.js`. Nécessite `node`
# et `@usebruno/cli` installé globalement, ou `BRUNO_LANG_DIR` pointant vers
# `@usebruno/lang`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/tests/fixtures/collections/writer-cases/files"
mkdir -p "$OUT"
node "$ROOT/scripts/bruno-new-items.js" "$OUT"
echo "bru $(bru --version 2>/dev/null || echo '?') : références écrites dans $OUT"
