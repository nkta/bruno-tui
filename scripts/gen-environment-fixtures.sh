#!/usr/bin/env bash
# Régénère la fixture de nouveau fichier d'environnement avec @usebruno/filestore.
#
# Fichier produit :
# - tests/fixtures/collections/writer-cases/files/new-environment.bruno.bru
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/tests/fixtures/collections/writer-cases/files"
mkdir -p "$OUT"

FILESTORE_DIR="${BRUNO_FILESTORE_DIR:-$HOME/.local/lib/bru-cli/node_modules/@usebruno/filestore}"

node -e '
const fs = require("fs");
const path = require("path");
const filestorePath = process.argv[1];
const outPath = process.argv[2];
const { stringifyEnvironment } = require(filestorePath);

const content = stringifyEnvironment({ name: "empty", variables: [] }, { format: "bru" });
fs.writeFileSync(outPath, content);
' "$FILESTORE_DIR" "$OUT/new-environment.bruno.bru"

echo "Fixture d'environnement générée : $OUT/new-environment.bruno.bru"
