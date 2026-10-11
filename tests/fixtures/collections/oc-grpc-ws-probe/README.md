# Fixture oc-grpc-ws-probe

Collection OpenCollection (YAML) avec une requête gRPC, une WebSocket et une HTTP.

## Provenance

Fichiers écrits à la main selon les sources de Bruno 4.2.1
(`/snap/bruno/current/resources/app.asar`, extraites dans `.brief/ref/`) :

- `filestore/src/formats/yml/items/stringifyGrpcRequest.ts` et `parseGrpcRequest.ts` :
  `info.type: grpc`, bloc `grpc` (`url`, `method`, `methodType`, `protoFilePath`,
  `metadata`, `message`, `auth`), scripts `grpc:*` sous `runtime.scripts`.
- `filestore/src/formats/yml/items/stringifyWebsocketRequest.ts` et `parseWebsocketRequest.ts` :
  `info.type: websocket`, bloc `websocket` (`url`, `headers`, `message`, `auth`),
  `settings` (`timeout`, `keepAliveInterval`).

## Fichiers

- `say hello.yml` : gRPC unaire, deux métadonnées (dont une désactivée), un message,
  auth bearer (`{{token}}`, jamais résolu), script `grpc:before-call-start`, tests, docs.
- `echo.yml` : WebSocket, un en-tête, deux messages (le premier sélectionné), script
  `before-request`, settings, docs.
- `ping.yml` : requête HTTP servie par `scripts/gen-report-fixtures.sh`.

Rapport `bru run -r` capturé dans `tests/fixtures/reports/oc-grpc-ws.json`.
