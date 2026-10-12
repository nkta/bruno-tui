#!/usr/bin/env node
// Relit un fichier `.bru` avec le parser officiel de Bruno
// (`@usebruno/lang`, livré avec `bru`) et imprime, en JSON stable, l'URL,
// les en-têtes, les paramètres, les variables, les assertions et les settings
// qu'il en extrait.
//
// Usage : node scripts/bruno-lang-dump.js <fichier.bru>
// Le module est cherché dans `$BRUNO_LANG_DIR`, sinon dans l'installation
// globale de `@usebruno/cli` (`npm root -g`).
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

const file = process.argv[2];
if (!file) {
  process.stderr.write('usage : bruno-lang-dump.js <fichier.bru>\n');
  process.exit(2);
}

const lang = require(langDir());
const content = fs.readFileSync(file, 'utf8');

let json;
try {
  json = lang.bruToJsonV2(content);
} catch (e) {
  json = lang.collectionBruToJson(content);
}

const entry = (item) => ({ name: item.name, value: item.value, enabled: item.enabled });
const dump = {
  url: json.http ? json.http.url : '',
  headers: (json.headers || []).map(entry),
  params: (json.params || []).map((item) => ({ ...entry(item), type: item.type })),
  vars_req: (json.vars && json.vars.req || []).map(entry),
  vars_res: (json.vars && json.vars.res || []).map(entry),
  assertions: (json.assertions || []).map(entry),
  settings: json.settings || {},
};
process.stdout.write(JSON.stringify(dump, null, 2) + '\n');
