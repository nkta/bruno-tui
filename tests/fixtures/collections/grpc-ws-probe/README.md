# Fixture grpc-ws-probe

Petite collection de test contenant des requêtes gRPC, WebSocket et HTTP.

## Provenance

Les requêtes sont écrites strictement selon la grammaire bru v2 et la logique de sérialisation extraites des sources de référence de Bruno Desktop 4.2.1 (`/snap/bruno/current/resources/app.asar`) :
- Grammaire ohm : `.brief/ref/lang/v2/src/bruToJson.js` (règles `grpc`, `ws`, `metadata`, `bodygrpc`, `bodyws`, `script:grpc:*`).
- Sérialiseur : `.brief/ref/lang/v2/src/jsonToBru.js` (génération des blocs `grpc`, `ws`, `metadata`, `body:grpc`, `body:ws`, `script:grpc:*`).

## Fichiers

- `say-hello.bru` : requête de type `grpc` avec URL, méthode RPC, methodType unary, metadata, corps multi-lignes, script `before-call-start`, tests et docs.
- `echo.bru` : requête de type `ws` avec URL, auth, en-têtes, corps message JSON sélectionné, script pre-request, tests et docs.
- `ping.bru` : requête de type `http` standard pour vérifier le comportement d'une campagne récursive mélangeant requêtes HTTP et requêtes non-HTTP.

La valeur `type` de `meta` (`grpc`, `ws`) suit la conversion de Bruno 4.2.1
(`grpc-request` ↔ `grpc`, `ws-request` ↔ `ws`), relevée dans `app.asar`.

Rapport `bru run -r` capturé dans `tests/fixtures/reports/grpc-ws.json`.
