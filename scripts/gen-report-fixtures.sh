#!/usr/bin/env bash
# Régénère les fixtures de rapport à partir d'un vrai `bru run`.
#
# Sert `tests/fixtures/collections/runner-probe/www` sur 127.0.0.1:18765,
# lance la collection de fixture en récursif et capture le rapport JSON
# via le fd 3 (même transport que le runner), puis arrête le serveur.
# Le port 18799 doit rester fermé : il simule une erreur de connexion.
#
# Produit aussi `pre-request-error.json` depuis
# `tests/fixtures/collections/runner-probe-errored` : requête en échec avant
# envoi (script pré-requête qui lève une exception), sans serveur requis.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
COLLECTION="$ROOT/tests/fixtures/collections/runner-probe"
OUT="$ROOT/tests/fixtures/reports/mixed.json"
ERRORED_COLLECTION="$ROOT/tests/fixtures/collections/runner-probe-errored"
ERRORED_OUT="$ROOT/tests/fixtures/reports/pre-request-error.json"
PORT=18765

python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$COLLECTION/www" \
  >/dev/null 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

# Attente du serveur
for _ in $(seq 1 50); do
  if python3 -c "import socket; socket.create_connection(('127.0.0.1', $PORT), 0.2)" 2>/dev/null; then
    break
  fi
  sleep 0.1
done

cd "$COLLECTION"
set +e
bru run -r --reporter-json /dev/fd/3 3>"$OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $OUT"

cd "$ERRORED_COLLECTION"
set +e
bru run -r --reporter-json /dev/fd/3 3>"$ERRORED_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $ERRORED_OUT"
