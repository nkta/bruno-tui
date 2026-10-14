#!/usr/bin/env node
// Table de référence de la renumérotation des `seq` par Bruno desktop
// 4.2.1 : `sortItems` (module 88008, export `zz`), `ef` et
// `getItemsToResequence` (`ky` = `eb`), recopiés tels quels du bundle
// minifié (`/snap/bruno/current/resources/app.asar`), et `localeCompare`
// sur les noms que le TUI sait comparer comme Node.
//
// Usage : node scripts/bruno-sort-items.js <fichier.json>
'use strict';

const fs = require('fs');

const cloneDeep = (value) => JSON.parse(JSON.stringify(value));
// eslint-disable-next-line
const a8=t=>{let r=t=>Number.isFinite(t)&&Number.isInteger(t)&&t>0,a=[...t].sort((t,r)=>t.name&&r.name&&t.name.localeCompare(r.name)||0),n=a.filter(t=>!r(t.seq));return a.filter(t=>r(t.seq)).sort((t,r)=>t.seq-r.seq).forEach(t=>{var r;let a=t.seq-1,o=n[a];if(Array.isArray(o)?(null==(r=null==o?void 0:o[0])?void 0:r.seq)===t.seq:(null==o?void 0:o.seq)===t.seq){let r=Array.isArray(o)?[...o,t]:[o,t];n.splice(a,1,r)}else n.splice(a,0,t)}),n.flat()};
const ef = (t) => a8(t).map((t, r) => ((t.seq = r + 1), t));
// eslint-disable-next-line
const eb=t=>{let{items:r}=t;return ef(cloneDeep(r)).filter(t=>r.find(r=>r?.uid===t?.uid)?.seq!==t?.seq)};

// Frères restants après une suppression : `uid` = indice, `seq` absent = null.
const SORT_CASES = [
  [['A', 1], ['B', 2], ['C', 3]],
  [['A', 1], ['C', 3], ['D', 4]],
  [['B', 2], ['C', 5], ['A', 9]],
  [['X', 3], ['Y', 3], ['Z', 1]],
  [['b', null], ['a', null], ['c', 2]],
  [['zeta', null], ['Alpha', 1], ['beta', null], ['gamma', 1]],
  [['A', 0], ['B', 2], ['C', -1]],
  [['dossier', null], ['req 2', 2], ['req 10', 10], ['req 1', null]],
  [['m', 1], ['n', 1], ['o', 1], ['p', 2]],
  [['un', 5], ['deux', null], ['trois', 1]],
  [['Liste', 1], ['liste', null], ['LISTE', null], ['list-e', null], ['list_e', null], ['list.e', null]],
  [],
];

const LOCALE_NAMES = [
  'a', 'A', 'b', 'B', 'ab', 'aB', 'Ab', 'a b', 'a-b', 'a_b', 'a.b', 'a1', 'a10', 'a2',
  '1', '10', '9', ' a', '-a', '_a', '.a', 'Z', 'z', 'zz', 'a  b', 'a-', 'a_', 'A1b', 'a1B',
  'req 1', 'req 10', 'req 2', 'Requête',
];

const sortCases = SORT_CASES.map((items) => {
  const input = items.map(([name, seq], uid) => ({ uid, name, seq: seq === null ? undefined : seq }));
  return {
    items: items.map(([name, seq]) => ({ name, seq })),
    order: ef(cloneDeep(input)).map((t) => t.uid),
    changes: eb({ items: input }).map((t) => [t.uid, t.seq]),
  };
});
const locale = [];
for (const a of LOCALE_NAMES) {
  for (const b of LOCALE_NAMES) {
    locale.push([a, b, Math.sign(a.localeCompare(b))]);
  }
}
fs.writeFileSync(process.argv[2], JSON.stringify({ sortCases, locale }, null, 1) + '\n');
process.stdout.write(`${sortCases.length} cas, ${locale.length} comparaisons\n`);
