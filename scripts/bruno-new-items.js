#!/usr/bin/env node
// Écrit, avec le code officiel de Bruno, les fichiers que Bruno desktop
// 4.2.1 crée pour une nouvelle requête, un nouveau dossier, et un
// `folder.bru` absent au renommage, au clonage et au réordonnancement d'un
// dossier ; et la table des paramètres que Bruno extrait d'une URL.
//
// Rien n'est réécrit à la main :
// - sérialisation : `stringifyRequest` et `stringifyFolder` de
//   `@usebruno/filestore` (même version, 0.13.0, que Bruno desktop 4.2.1 et
//   `bru` 4.2.1) ;
// - requête par défaut : objet de l'action `newHttpRequest` du renderer
//   (relevé dans `/snap/bruno/current/resources/app.asar`) : headers [],
//   body { mode: 'none', … }, vars, assertions [], auth { mode: 'inherit' },
//   settings = `DEFAULT_HTTP_ITEM_SETTINGS` ;
// - paramètres de l'URL : `splitOnFirst`, `parseQueryParams` et
//   `parsePathParams` recopiés tels quels du bundle minifié (y compris le
//   bogue `B.enabled = !0`, qui laisse `enabled` indéfini sur les
//   paramètres de chemin) ;
// - dossiers : objets des actions `newFolder`, `renderer:rename-item-name`,
//   `renderer:resequence-items` et du clonage.
//
// Usage :
//   node scripts/bruno-new-items.js <dossier>          fichiers *.bruno.bru
//   node scripts/bruno-new-items.js --url-params <f>   table JSON des URL
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

function filestoreDir() {
  return process.env.BRUNO_FILESTORE_DIR || path.join(path.dirname(langDir()), 'filestore');
}

// --- Code de Bruno desktop 4.2.1, recopié du bundle minifié. ---
// eslint-disable-next-line
const splitOnFirst=(t,r)=>{if(!t||!t.length)return[t];let a=t.replace(/\{\{.*?\}\}/g,t=>"_".repeat(t.length)).indexOf(r);return -1===a?[t]:[t.slice(0,a),t.slice(a+1)]};
// eslint-disable-next-line
function parseQueryParams(t){let{decode:r=!1,stripFragment:a=!0}=arguments.length>1&&void 0!==arguments[1]?arguments[1]:{};if(!t||!t.length)return[];try{return(a?t.split("#")[0]:t).split("&").map(t=>{let[a,...n]=t.split("=");if(!a)return null;let o=t.includes("=")?r?decodeURIComponent(n.join("=")):n.join("="):void 0;return{name:r?decodeURIComponent(a):a,value:o}}).filter(t=>null!==t)}catch(t){return console.error("Error parsing query params:",t),[]}}
// eslint-disable-next-line
const parsePathParams=t=>{let r,a=t.slice();if(!a||!a.length)return[];a.startsWith("http://")||a.startsWith("https://")||(a=`http://${a}`);try{r=(a=new URL(a)).pathname.split("/")}catch(t){r=a.split("/")}let n=new Set;return r.forEach(t=>{let r;if(t.startsWith(":")){let r=t.slice(1);r&&!n.has(r)&&n.add(r);return}if(!/^[A-Za-z0-9_.-]+\([^)]*\)$/.test(t))return;let a=/[:]([a-zA-Z_]\w*)/g;for(;r=a.exec(t);){if(!r[1])continue;let t=r[1].replace(/[')"`]+$/,"");(t=t.replace(/^[('"`]+/,""))&&!n.has(t)&&n.add(t)}}),Array.from(n).map(t=>({name:t,value:""}))};

// Paramètres de `newHttpRequest` : requête, puis chemin.
function urlParams(url) {
  const R = splitOnFirst(url, '?');
  const M = parseQueryParams(R[1]);
  M.forEach((t) => { t.enabled = true; t.type = 'query'; });
  const B = parsePathParams(url);
  B.forEach((t) => { B.enabled = true; t.type = 'path'; });
  return [...M, ...B];
}

// Requête créée par `newHttpRequest`.
function newHttpRequest({ name, type, method, url, seq }) {
  return {
    type,
    name,
    filename: name,
    seq,
    request: {
      method,
      url,
      headers: [],
      params: urlParams(url),
      body: {
        mode: 'none', json: null, text: null, xml: null, sparql: null,
        multipartForm: [], formUrlEncoded: [], file: []
      },
      vars: { req: [], res: [] },
      assertions: [],
      auth: { mode: 'inherit' }
    },
    settings: { encodeUrl: true, forwardAuthorizationHeader: false }
  };
}

// URL de la table des paramètres (`tests/fixtures/url-params/`).
const URL_CASES = [
  '{{base_url}}/posts/:id?full=1',
  '{{base_url}}/posts?tag=a&tag=b',
  'http://h/x?a=1=2',
  'http://h/x?flag',
  'http://h/x?=z&b=2',
  'http://h/x?a=1#frag',
  'http://h/x?a&&b=',
  '{{base_url}}/posts/:id?',
  '{{base_url?x}}/posts/:id',
  '{{base_url}}/x?{{q?}}=1&y=2',
  'http://h/x?a=%20b&c=d+e',
  '{{base_url}}/a b/:id',
  '{{base_url}}/café/:élève',
  'http://h/{x}/:a',
  'https://api.example.com/odata/Items(id=:key,b=:other)',
  'api.example.com/users/:userId/posts/:postId',
  'https://api.example.com/users/:id',
  '',
  '{{base_url}}/users/:id/:id',
  '{{host}}:{{port}}/users/:id',
  'http://h:8080/a/:b',
  'http://h/a/:b#/:c',
  'http://h/a?x=1/:y',
  'http://h/a/:',
  'http://h|x/:a',
  'HTTP://h/:a',
  'http://user@h/:a',
  'http://h/./:a',
  'http://h/a\\:b',
  'http://127.1/:a',
  'http://exämple.com/:a',
];

const args = process.argv.slice(2);
if (args[0] === '--url-params') {
  const table = URL_CASES.map((url) => {
    const params = urlParams(url);
    return {
      url,
      query: params.filter((p) => p.type === 'query')
        .map((p) => ({ name: p.name, value: p.value === undefined ? null : p.value })),
      path: params.filter((p) => p.type === 'path').map((p) => p.name)
    };
  });
  fs.writeFileSync(args[1], JSON.stringify(table, null, 2) + '\n');
  process.stdout.write(`${table.length} URL\n`);
  process.exit(0);
}

const out = args[0];
if (!out) {
  process.stderr.write('usage : bruno-new-items.js <dossier> | --url-params <fichier.json>\n');
  process.exit(2);
}
const { stringifyRequest, stringifyFolder } = require(filestoreDir());
const bru = { format: 'bru' };
const request = (json) => stringifyRequest(newHttpRequest(json), bru);
const folder = (json) => stringifyFolder(json, bru);

const files = {
  'new-request-http.bruno.bru': request({
    name: 'Lire un article', type: 'http-request', seq: 3, method: 'GET', url: '{{base_url}}/posts/1'
  }),
  'new-request-graphql.bruno.bru': request({
    name: 'Lister', type: 'graphql-request', seq: 1, method: 'POST', url: ''
  }),
  'new-request-delete.bruno.bru': request({
    name: 'Supprimer: tout', type: 'http-request', seq: 12, method: 'DELETE', url: 'https://api.example.com/items'
  }),
  'new-request-params.bruno.bru': request({
    name: 'Commentaires', type: 'http-request', seq: 2, method: 'GET',
    url: '{{base_url}}/posts/:id/comments?full=1&flag&a=1=2&full=2#haut'
  }),
  'new-request-odata.bruno.bru': request({
    name: 'OData', type: 'http-request', seq: 4, method: 'GET',
    url: 'https://api.example.com/odata/Items(id=:key)/:sub?$top=5'
  }),
  'new-request-encoded.bruno.bru': request({
    name: 'Encodé', type: 'graphql-request', seq: 1, method: 'POST',
    url: '{{base_url}}/café/:nom élève'
  }),
  'new-folder.bruno.bru': folder({ meta: { name: 'Posts', seq: 2 }, request: { auth: { mode: 'inherit' } } }),
  'folder-renamed.bruno.bru': folder({ meta: { name: 'Articles' } }),
  'folder-resequenced.bruno.bru': folder({ meta: { name: 'NoMeta', seq: 1 } }),
  'folder-cloned.bruno.bru': folder({ meta: { name: 'NoMeta copie', seq: 5 } })
};

for (const [name, content] of Object.entries(files)) {
  fs.writeFileSync(path.join(out, name), content);
  process.stdout.write(name + '\n');
}
