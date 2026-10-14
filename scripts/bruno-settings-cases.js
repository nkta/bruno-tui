#!/usr/bin/env node
// Génère les fichiers attendus de `tests/fixtures/collections/settings-cases/`
// avec le VRAI code de Bruno desktop 4.2.1 (`docs/reglages-collection.md` § 7).
//
// Ce qui vient de Bruno, sans réécriture :
//   - `transformBrunoConfigBeforeSave` (`/src/utils/transformBrunoConfig.js`
//     de `app.asar`, chargé tel quel, seul `./filesystem` est bouchonné) ;
//   - `transformProxyConfig` de `@usebruno/requests` ;
//   - `JSON.stringify(valeur, null, 2)` (`stringifyJson`, `/src/utils/common.js`).
// Ce qui est recopié du renderer (`index.c9f1eb9b.js`) : la forme des
// changements `{...proxy, ...changement}` de chaque réglage (fonction `op*`).
//
// Pour chaque cas, `<cas>.before.bruno.json` est le fichier de départ et
// `<cas>.after.bruno.json` le fichier attendu du TUI : le départ où seule
// la valeur de la clé modifiée est remplacée par celle que Bruno écrit
// (décalée au niveau de la clé), le reste à l'octet près.
// `<cas>.bruno-full.json` est ce que Bruno écrirait pour tout le fichier.
//
// Usage : node scripts/bruno-settings-cases.js <dossier de sortie>
// Variables : BRUNO_ASAR (défaut /snap/bruno/current/resources/app.asar),
// BRUNO_REQUESTS_DIR (défaut : installation de `bru`).
'use strict';

const fs = require('fs');
const os = require('os');
const path = require('path');

// --- Chargement du code de Bruno --------------------------------------------

function readAsarFile(asar, wanted) {
  const fd = fs.openSync(asar, 'r');
  const head = Buffer.alloc(16);
  fs.readSync(fd, head, 0, 16, 0);
  const size = head.readUInt32LE(12);
  const headerBuf = Buffer.alloc(size);
  fs.readSync(fd, headerBuf, 0, size, 16);
  const header = JSON.parse(headerBuf.toString('utf8'));
  const base = 8 + head.readUInt32LE(4);
  let node = header;
  for (const part of wanted.split('/').filter(Boolean)) {
    node = node.files[part];
  }
  const out = Buffer.alloc(node.size);
  fs.readSync(fd, out, 0, node.size, base + Number(node.offset));
  fs.closeSync(fd);
  return out.toString('utf8');
}

function requestsDir() {
  if (process.env.BRUNO_REQUESTS_DIR) return process.env.BRUNO_REQUESTS_DIR;
  const home = path.join(os.homedir(), '.local/lib/bru-cli/node_modules/@usebruno/requests');
  if (fs.existsSync(home)) return home;
  const { execSync } = require('child_process');
  const root = execSync('npm root -g', { encoding: 'utf8' }).trim();
  return path.join(root, '@usebruno', 'cli', 'node_modules', '@usebruno', 'requests');
}

function loadBruno() {
  const asar = process.env.BRUNO_ASAR || '/snap/bruno/current/resources/app.asar';
  const source = readAsarFile(asar, '/src/utils/transformBrunoConfig.js');
  const requests = require(requestsDir());
  const mod = { exports: {} };
  const fakeRequire = (id) => {
    if (id === './filesystem') return { isFile: () => false, isDirectory: () => false };
    if (id === '@usebruno/requests') return requests;
    return require(id);
  };
  new Function('require', 'module', 'exports', source)(fakeRequire, mod, mod.exports);
  return { transform: mod.exports.transformBrunoConfigBeforeSave, requests };
}

const { transform, requests } = loadBruno();
const stringifyJson = (value) => JSON.stringify(value, null, 2);

// --- Changements du renderer (`index.c9f1eb9b.js`) --------------------------

const DEFAULT_PROXY = {
  inherit: true,
  config: { protocol: 'http', hostname: '', port: '', auth: { username: '', password: '' }, bypassProxy: '' },
};
const DEFAULT_PRESETS = { requestType: 'http', requestUrl: '' };

// Gestionnaire du composant « Proxy » : `r = {...Z, ...t}`.
const opProxy = (cfg, change) => {
  cfg.proxy = { ...(cfg.proxy ?? structuredClone(DEFAULT_PROXY)), ...change };
};
const opProxyMode = (cfg, mode) =>
  opProxy(
    cfg,
    mode === 'inherit'
      ? { disabled: false, inherit: true }
      : mode === 'enabled'
        ? { disabled: false, inherit: false }
        : { disabled: true, inherit: false },
  );
const opProxyConfig = (cfg, change) => {
  const base = cfg.proxy ?? structuredClone(DEFAULT_PROXY);
  opProxy(cfg, { config: { ...base.config, ...change } });
};
const opProxyAuth = (cfg, change) => {
  const base = cfg.proxy ?? structuredClone(DEFAULT_PROXY);
  opProxyConfig(cfg, { auth: { ...base.config.auth, ...change } });
};
// Composant « Presets » : `{...z, ...t}`, « None » retire la clé.
const opPresets = (cfg, change) => {
  cfg.presets = { ...(cfg.presets ?? structuredClone(DEFAULT_PRESETS)), ...change };
};
const opPresetsNoEnv = (cfg) => {
  const { defaultEnvironment, ...rest } = cfg.presets ?? structuredClone(DEFAULT_PRESETS);
  cfg.presets = rest;
};
// Composant « Client Certificates » : `{enabled: true, certs}`.
const certs = (cfg) => cfg.clientCertificates?.certs ?? [];
const opCertAdd = (cfg, t) => {
  const entry =
    t.type === 'cert'
      ? { domain: t.domain.trim(), type: t.type, certFilePath: t.certFilePath, keyFilePath: t.keyFilePath, passphrase: t.passphrase }
      : { domain: t.domain.trim(), type: t.type, pfxFilePath: t.pfxFilePath, passphrase: t.passphrase };
  cfg.clientCertificates = { enabled: true, certs: [...certs(cfg), entry] };
};
const opCertRemove = (cfg, index) => {
  cfg.clientCertificates = { enabled: true, certs: certs(cfg).filter((_, i) => i !== index) };
};
const opCertToggle = (cfg, index) => {
  const list = certs(cfg).map((c, i) => {
    if (i !== index) return c;
    const n = { ...c };
    if (n.disabled) delete n.disabled;
    else n.disabled = true;
    return n;
  });
  cfg.clientCertificates = { enabled: true, certs: list };
};

// --- Cas --------------------------------------------------------------------

const BASE = '{\n  "version": "1",\n  "name": "demo",\n  "type": "collection",\n  "ignore": ["node_modules", ".git"]\n}';
const PROXY_FILE =
  '{\n  "version": "1",\n  "name": "demo",\n  "proxy": {\n    "inherit": false,\n    "config": {\n      "protocol": "http",\n      "hostname": "old.example",\n      "port": 3128,\n      "auth": {\n        "username": "alice",\n        "password": "{{proxy_password}}",\n        "disabled": true\n      },\n      "bypassProxy": "localhost"\n    }\n  },\n  "type": "collection",\n  "ignore": ["node_modules", ".git"]\n}';
const CERTS_FILE =
  '{\n  "version": "1",\n  "name": "demo",\n  "clientCertificates": {\n    "enabled": true,\n    "certs": [\n      {\n        "domain": "api.exemple.fr",\n        "type": "cert",\n        "certFilePath": "certs/client.crt",\n        "keyFilePath": "certs/client.key",\n        "passphrase": ""\n      },\n      {\n        "domain": "vieux.exemple.fr",\n        "type": "cert",\n        "certFilePath": "certs/old.crt",\n        "keyFilePath": "certs/old.key",\n        "passphrase": "{{oldPass}}",\n        "disabled": true\n      }\n    ]\n  },\n  "ignore": ["node_modules", ".git"]\n}';
const PRESETS_FILE =
  '{\n  "version": "1",\n  "name": "demo",\n  "presets": {\n    "requestType": "graphql",\n    "requestUrl": "{{base_url}}",\n    "defaultEnvironment": "local"\n  },\n  "ignore": ["node_modules", ".git"]\n}';
const FOUR = '{\n    "version": "1",\n    "name": "demo",\n    "ignore": ["node_modules", ".git"]\n}';

const ENABLED_PROXY = (cfg) => {
  opProxyMode(cfg, 'enabled');
  opProxyConfig(cfg, { protocol: 'http' });
  opProxyConfig(cfg, { hostname: '{{proxy_host}}' });
  opProxyConfig(cfg, { port: 8080 });
};

const CASES = [
  { name: 'proxy-add', key: 'proxy', before: BASE, op: ENABLED_PROXY },
  { name: 'proxy-add-final-newline', key: 'proxy', before: `${BASE}\n`, op: ENABLED_PROXY },
  { name: 'proxy-add-four-spaces', key: 'proxy', before: FOUR, op: ENABLED_PROXY },
  { name: 'proxy-modify', key: 'proxy', before: PROXY_FILE, op: (c) => opProxyConfig(c, { hostname: '{{proxy_host}}' }) },
  { name: 'proxy-modify-final-newline', key: 'proxy', before: `${PROXY_FILE}\n`, op: (c) => opProxyConfig(c, { hostname: '{{proxy_host}}' }) },
  { name: 'proxy-auth-enabled', key: 'proxy', before: PROXY_FILE, op: (c) => opProxyAuth(c, { disabled: false }) },
  { name: 'proxy-auth-disabled', key: 'proxy', before: BASE, op: (c) => opProxyAuth(c, { disabled: true }) },
  { name: 'proxy-mode-inherit', key: 'proxy', before: PROXY_FILE, op: (c) => opProxyMode(c, 'inherit') },
  { name: 'proxy-mode-enabled', key: 'proxy', before: BASE, op: (c) => opProxyMode(c, 'enabled') },
  { name: 'proxy-mode-disabled', key: 'proxy', before: BASE, op: (c) => opProxyMode(c, 'disabled') },
  { name: 'proxy-port-empty', key: 'proxy', before: PROXY_FILE, op: (c) => opProxyConfig(c, { port: '' }) },
  { name: 'presets-add-env', key: 'presets', before: BASE, op: (c) => opPresets(c, { defaultEnvironment: 'local' }) },
  { name: 'presets-remove-env', key: 'presets', before: PRESETS_FILE, op: opPresetsNoEnv },
  { name: 'presets-change-type', key: 'presets', before: PRESETS_FILE, op: (c) => opPresets(c, { requestType: 'http' }) },
  {
    name: 'certs-add-cert',
    key: 'clientCertificates',
    before: BASE,
    op: (c) => opCertAdd(c, { domain: ' api.exemple.fr ', type: 'cert', certFilePath: 'certs/client.crt', keyFilePath: 'certs/client.key', passphrase: '{{certPass}}' }),
  },
  {
    name: 'certs-add-pfx',
    key: 'clientCertificates',
    before: CERTS_FILE,
    op: (c) => opCertAdd(c, { domain: '*.exemple.fr', type: 'pfx', pfxFilePath: 'certs/client.pfx', passphrase: '' }),
  },
  { name: 'certs-remove', key: 'clientCertificates', before: CERTS_FILE, op: (c) => opCertRemove(c, 0) },
  { name: 'certs-disable', key: 'clientCertificates', before: CERTS_FILE, op: (c) => opCertToggle(c, 0) },
  { name: 'certs-enable', key: 'clientCertificates', before: CERTS_FILE, op: (c) => opCertToggle(c, 1) },
];

// --- Remplacement de la valeur d'une clé dans le texte d'origine ------------

// Parcours minimal : position de la valeur de `key` au premier niveau, ou
// fin de la dernière valeur du premier niveau si la clé est absente.
function endOfValue(text, start) {
  const first = text[start];
  let i = start;
  if (first === '"') {
    i += 1;
    while (text[i] !== '"') i += text[i] === '\\' ? 2 : 1;
    return i + 1;
  }
  if (first === '{' || first === '[') {
    let depth = 0;
    do {
      const c = text[i];
      if (c === '"') {
        i += 1;
        while (text[i] !== '"') i += text[i] === '\\' ? 2 : 1;
      } else if (c === '{' || c === '[') depth += 1;
      else if (c === '}' || c === ']') depth -= 1;
      i += 1;
    } while (depth > 0);
    return i;
  }
  while (i < text.length && !/[,}\]\s]/.test(text[i])) i += 1;
  return i;
}

function locate(text, key) {
  let i = text.indexOf('{') + 1;
  let lastEnd = -1;
  for (;;) {
    while (/\s/.test(text[i])) i += 1;
    if (text[i] === '}') return { lastEnd };
    const keyStart = i;
    const keyEnd = endOfValue(text, i);
    const name = JSON.parse(text.slice(keyStart, keyEnd));
    i = text.indexOf(':', keyEnd) + 1;
    while (/\s/.test(text[i])) i += 1;
    const valueStart = i;
    const valueEnd = endOfValue(text, valueStart);
    if (name === key) return { keyStart, valueStart, valueEnd };
    lastEnd = valueEnd;
    i = valueEnd;
    while (/\s/.test(text[i])) i += 1;
    if (text[i] === ',') i += 1;
  }
}

function splice(before, key, value) {
  const found = locate(before, key);
  const lineStart = found.keyStart === undefined ? 0 : before.lastIndexOf('\n', found.keyStart) + 1;
  const firstKey = before.indexOf('"');
  const keyLineIndent = found.keyStart === undefined
    ? before.slice(before.lastIndexOf('\n', firstKey) + 1, firstKey)
    : before.slice(lineStart, found.keyStart);
  const text = stringifyJson(value).replace(/\n/g, `\n${keyLineIndent}`);
  if (found.keyStart !== undefined) {
    return before.slice(0, found.valueStart) + text + before.slice(found.valueEnd);
  }
  return `${before.slice(0, found.lastEnd)},\n${keyLineIndent}${JSON.stringify(key)}: ${text}${before.slice(found.lastEnd)}`;
}

// --- Génération -------------------------------------------------------------

const out = process.argv[2];
if (!out) {
  process.stderr.write('usage : bruno-settings-cases.js <dossier de sortie>\n');
  process.exit(2);
}
fs.mkdirSync(out, { recursive: true });

for (const c of CASES) {
  const config = JSON.parse(c.before);
  c.op(config);
  const saved = transform(structuredClone(config));
  const after = splice(c.before, c.key, saved[c.key]);
  fs.writeFileSync(path.join(out, `${c.name}.before.bruno.json`), c.before);
  fs.writeFileSync(path.join(out, `${c.name}.after.bruno.json`), after);
  fs.writeFileSync(path.join(out, `${c.name}.bruno-full.json`), stringifyJson(saved));
  // Relecture par Node : le fichier attendu est du JSON valide.
  JSON.parse(after);
}

// Ancien format de proxy : `transformProxyConfig` pour les trois valeurs de `enabled`.
const legacy = {};
for (const enabled of [true, false, 'global']) {
  const input = {
    enabled,
    protocol: 'https',
    hostname: 'proxy.example',
    port: 8080,
    auth: { enabled: false, username: 'alice', password: '{{proxy_password}}' },
    bypassProxy: 'localhost',
  };
  legacy[String(enabled)] = { input, output: requests.transformProxyConfig(structuredClone(input)) };
}
legacy.minimal = { input: { enabled: true }, output: requests.transformProxyConfig({ enabled: true }) };
fs.writeFileSync(path.join(out, 'legacy-proxy.json'), `${stringifyJson(legacy)}\n`);

process.stdout.write(`${CASES.length} cas écrits dans ${out}\n`);
