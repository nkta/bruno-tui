#!/usr/bin/env bash
# Régénère les relectures Bruno des fixtures d'écriture.
#
# Pour chaque fixture d'ajout, de suppression ou de renommage de
# `tests/fixtures/collections/writer-cases/`, relit `<nom>.after.bru` avec le
# parser officiel de Bruno (`scripts/bruno-lang-dump.js`) et écrit
# `<nom>.after.bruno.json` à côté. Les fixtures plus anciennes ne sont pas
# concernées : `json-body` porte volontairement un bloc que Bruno refuse. Nécessite `node` et `@usebruno/cli`
# installé globalement, ou `BRUNO_LANG_DIR` pointant vers `@usebruno/lang`.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CASES="$ROOT/tests/fixtures/collections/writer-cases"

# Même liste que `ENTRY_CASES` dans `tests/writer_bruno_lang.rs`.
CASES_NAMES=(add-header add-block add-path-between remove-entry rename-disabled
  duplicates query-sync url-sync crlf-add)

for name in "${CASES_NAMES[@]}"; do
  out="$CASES/$name.after.bruno.json"
  node "$ROOT/scripts/bruno-lang-dump.js" "$CASES/$name.after.bru" >"$out"
  echo "$(basename "$out")"
done
echo "bru $(bru --version 2>/dev/null || echo '?') : relectures écrites dans $CASES"
