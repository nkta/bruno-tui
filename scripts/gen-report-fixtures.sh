#!/usr/bin/env bash
# Régénère les fixtures de rapport à partir d'un vrai `bru run`.
#
# Sert `tests/fixtures/collections/runner-probe/www` sur 127.0.0.1:18765,
# lance la collection de fixture en récursif et capture le rapport JSON
# via le fd 3 (même transport que le runner), puis arrête le serveur.
# Le port 18799 doit rester fermé : il simule une erreur de connexion.
#
# Produit aussi `opencollection.json` depuis
# `tests/fixtures/collections/oc-probe` (collection OpenCollection YAML).
#
# Produit aussi `multi-env-up.json` et `multi-env-down.json` depuis
# `tests/fixtures/collections/multi-env-probe` : même collection lancée sur
# deux environnements, `down` pointant vers le port fermé 18799.
#
# Produit aussi `grpc-ws.json` et `oc-grpc-ws.json` depuis
# `tests/fixtures/collections/grpc-ws-probe` (`.bru`) et `oc-grpc-ws-probe`
# (YAML) : requêtes gRPC et WebSocket, que `bru run` 4.2.1 n'exécute pas.
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
POST_COLLECTION="$ROOT/tests/fixtures/collections/runner-probe-post"
POST_OUT="$ROOT/tests/fixtures/reports/post-with-body.json"
YML_COLLECTION="$ROOT/tests/fixtures/collections/oc-probe"
YML_OUT="$ROOT/tests/fixtures/reports/opencollection.json"
CSV_COLLECTION="$ROOT/tests/fixtures/collections/csv-probe"
CSV_OUT="$ROOT/tests/fixtures/reports/csv-iterations.json"
MULTI_ENV_COLLECTION="$ROOT/tests/fixtures/collections/multi-env-probe"
MULTI_ENV_UP_OUT="$ROOT/tests/fixtures/reports/multi-env-up.json"
MULTI_ENV_DOWN_OUT="$ROOT/tests/fixtures/reports/multi-env-down.json"
GRPC_WS_COLLECTION="$ROOT/tests/fixtures/collections/grpc-ws-probe"
GRPC_WS_OUT="$ROOT/tests/fixtures/reports/grpc-ws.json"
OC_GRPC_WS_COLLECTION="$ROOT/tests/fixtures/collections/oc-grpc-ws-probe"
OC_GRPC_WS_OUT="$ROOT/tests/fixtures/reports/oc-grpc-ws.json"
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

cd "$POST_COLLECTION"
set +e
bru run -r --reporter-json /dev/fd/3 3>"$POST_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $POST_OUT"

# Collection OpenCollection (YAML) : même serveur, environnement `local`,
# jeton factice transmis en variable (aucun secret réel).
cd "$YML_COLLECTION"
set +e
bru run -r --env local --env-var token=fixture-token \
  --reporter-json /dev/fd/3 3>"$YML_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $YML_OUT"

cd "$GRPC_WS_COLLECTION"
set +e
bru run -r --reporter-json /dev/fd/3 3>"$GRPC_WS_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $GRPC_WS_OUT"

cd "$OC_GRPC_WS_COLLECTION"
set +e
bru run -r --reporter-json /dev/fd/3 3>"$OC_GRPC_WS_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $OC_GRPC_WS_OUT"

cd "$ERRORED_COLLECTION"
set +e
bru run -r --reporter-json /dev/fd/3 3>"$ERRORED_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $ERRORED_OUT"

cd "$CSV_COLLECTION"
set +e
bru run --csv-file-path data.csv --reporter-json /dev/fd/3 3>"$CSV_OUT" >/dev/null 2>&1
CODE=$?
set -e

echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $CSV_OUT"


# Campagne multi-environnements : une exécution par environnement.
cd "$MULTI_ENV_COLLECTION"
for ENV_NAME in up down; do
  if [ "$ENV_NAME" = up ]; then ENV_OUT="$MULTI_ENV_UP_OUT"; else ENV_OUT="$MULTI_ENV_DOWN_OUT"; fi
  set +e
  bru run -r --env "$ENV_NAME" --reporter-json /dev/fd/3 3>"$ENV_OUT" >/dev/null 2>&1
  CODE=$?
  set -e
  echo "bru $(bru --version) : code de sortie $CODE, rapport écrit dans $ENV_OUT"
done
