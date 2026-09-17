#!/usr/bin/env node
// Relit un fichier `.bru` avec le parser officiel de Bruno
// (`@usebruno/lang`, livré avec `bru`) et imprime, en JSON stable, l'URL,
// les en-têtes et les paramètres qu'il en extrait.
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

const { bruToJsonV2 } = require(langDir());
const json = bruToJsonV2(fs.readFileSync(file, 'utf8'));
const entry = (item) => ({ name: item.name, value: item.value, enabled: item.enabled });
const dump = {
  url: json.http.url,
  headers: (json.headers || []).map(entry),
  params: (json.params || []).map((item) => ({ ...entry(item), type: item.type })),
};
process.stdout.write(JSON.stringify(dump, null, 2) + '\n');
