#!/usr/bin/env node
// Écrit, avec le sérialiseur officiel de Bruno (`@usebruno/lang`), les
// fichiers que Bruno desktop 4.2.1 crée pour une nouvelle requête, un
// nouveau dossier, et un `folder.bru` absent au renommage, au clonage et au
// réordonnancement d'un dossier.
//
// Les objets JSON ci-dessous reprennent, sans rien deviner, ce que Bruno
// desktop 4.2.1 passe à `jsonToBru` / `jsonToCollectionBru` (relevé dans
// `/snap/bruno/current/resources/app.asar`) :
// - requête : action `newHttpRequest` du renderer (objet `L` : headers [],
//   body { mode: 'none', … }, vars, assertions [], auth { mode: 'inherit' },
//   settings = `DEFAULT_HTTP_ITEM_SETTINGS` { encodeUrl: true,
//   forwardAuthorizationHeader: false }), transformé par
//   `stringifyBruRequest` de `@usebruno/filestore` (meta { name, type, seq,
//   tags: [] }, http { method, url, auth, body }, …) ;
// - dossier : action `newFolder` (`{ meta: { name, seq }, request: { auth:
//   { mode: 'inherit' } } }`) transformée par `stringifyBruCollection(…,
//   true)` ;
// - `folder.bru` absent : `renderer:rename-item-name` (`{ meta: { name } }`),
//   `renderer:resequence-items` (`{ meta: { name: basename, seq } }`),
//   clonage (`root.meta = { name, seq }`).
//
// Usage : node scripts/bruno-new-items.js <dossier de sortie>
'use strict';

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

function langDir() {
  if (process.env.BRUNO_LANG_DIR) {
    return process.env.BRUNO_LANG_DIR;
  }
  const globalRoot = execSync('npm root -g', { encoding: 'utf8' }).trim();
  return path.join(globalRoot, '@usebruno', 'cli', 'node_modules', '@usebruno', 'lang');
}

const out = process.argv[2];
if (!out) {
  process.stderr.write('usage : bruno-new-items.js <dossier>\n');
  process.exit(2);
}
const { jsonToBruV2, jsonToCollectionBru } = require(langDir());

// `stringifyBruRequest` appliqué à la requête par défaut de `newHttpRequest`.
const request = ({ name, type, seq, method, url }) => ({
  meta: { name, type, seq, tags: [] },
  http: { method: method.toLowerCase(), url, auth: 'inherit', body: 'none' },
  params: [],
  body: {
    mode: 'none', json: null, text: null, xml: null, sparql: null,
    multipartForm: [], formUrlEncoded: [], file: []
  },
  headers: [],
  auth: { mode: 'inherit' },
  script: {},
  vars: { req: [], res: [] },
  assertions: [],
  tests: '',
  settings: { encodeUrl: true, forwardAuthorizationHeader: false },
  docs: '',
  examples: []
});

// `stringifyBruCollection(json, true)`.
const folder = (json) => {
  const out = {
    headers: [],
    script: { req: '', res: '' },
    vars: { req: [], res: [] },
    tests: '',
    auth: (json.request && json.request.auth) || {},
    docs: ''
  };
  out.meta = { name: json.meta.name };
  if (json.meta.seq !== undefined) out.meta.seq = Number(json.meta.seq);
  return jsonToCollectionBru(out);
};

const files = {
  'new-request-http.bruno.bru': jsonToBruV2(request({
    name: 'Lire un article', type: 'http', seq: 3, method: 'GET', url: '{{base_url}}/posts/1'
  })),
  'new-request-graphql.bruno.bru': jsonToBruV2(request({
    name: 'Lister', type: 'graphql', seq: 1, method: 'POST', url: ''
  })),
  'new-request-delete.bruno.bru': jsonToBruV2(request({
    name: 'Supprimer: tout', type: 'http', seq: 12, method: 'DELETE', url: 'https://api.example.com/items'
  })),
  'new-folder.bruno.bru': folder({ meta: { name: 'Posts', seq: 2 }, request: { auth: { mode: 'inherit' } } }),
  'folder-renamed.bruno.bru': folder({ meta: { name: 'Articles' } }),
  'folder-resequenced.bruno.bru': folder({ meta: { name: 'NoMeta', seq: 1 } }),
  'folder-cloned.bruno.bru': folder({ meta: { name: 'NoMeta copie', seq: 5 } })
};

for (const [name, content] of Object.entries(files)) {
  fs.writeFileSync(path.join(out, name), content);
  process.stdout.write(name + '\n');
}
