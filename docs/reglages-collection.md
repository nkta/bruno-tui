# Spécification — Réglages de la collection (`bruno.json`)

Statut : **brouillon, à valider par Nicolas**.

Aujourd'hui le TUI lit `bruno.json` pour détecter une collection
(`name`, `ignore`). Il ne l'écrit jamais.
Cette spec demande un **nouveau droit d'écriture** : trois clés de
`bruno.json`, et rien d'autre. Tant qu'elle n'est pas approuvée,
la règle de `CLAUDE.md` (lecture seule) reste en vigueur.

Suite de `docs/edition-popups.md` et `docs/edition-phase3.md`.
Leurs principes s'appliquent : fraîcheur (`FileStamp`), écriture
atomique, jamais d'effacement, écritures confinées à la collection.

## 0. Décisions à prendre (Nicolas)

L'option recommandée est la première de chaque ligne.

| # | Question | Options |
|---|---|---|
| D1 | Comment écrire `bruno.json` ? | **A** : chirurgicale, seule la valeur de la clé change. **B** : réécrire tout le fichier comme Bruno. |
| D2 | Quelles clés ? | **A** : `proxy`, `clientCertificates`, `presets`. **B** : A + `ignore`, `name`. |
| D3 | Secret en clair (mot de passe de proxy, passphrase) ? | **A** : refusé, seul `{{variable}}` est accepté. **B** : comme le popup Auth (`g` pour garder). |
| D4 | Touche d'ouverture ? | **A** : `B` (comme **B**runo.json). **B** : `P`. **C** : `Ctrl+,`. |
| D5 | Proxy à l'ancien format (`enabled`) ? | **A** : conversion au nouveau format, avec message. **B** : lecture seule, édition refusée. |
| D6 | `presets.requestType` / `requestUrl` : le TUI les applique-t-il ? | **A** : oui, ils préremplissent le popup `a`. **B** : non, on les écrit sans les utiliser. |
| D7 | `presets.defaultEnvironment` : préselection dans le TUI ? | **A** : non (hors périmètre), on l'écrit seulement. **B** : oui, au chargement. |

Raisons :

- **D1-A** : Bruno réécrit tout le fichier avec `JSON.stringify(…, null, 2)`
  (§ 1.2). Cela éclate les tableaux sur une ligne. Les `bruno.json`
  du dépôt (`examples/`) ont `"ignore": ["node_modules", ".git"]` sur une
  ligne. Avec B, un simple changement de proxy réécrirait cette ligne.
  A garde tout le reste à l'octet près.
- **D2-A** : ce sont les trois clés demandées. `ignore` et `name`
  s'ajouteront plus tard sans changer le mécanisme.
- **D3-A** : `CLAUDE.md` interdit d'écrire un secret sur disque. Bruno,
  lui, écrit tout en clair (§ 1.6). Le TUI ne doit pas le suivre.
- **D4-A** : `B` est libre partout (§ 3). Une majuscule évite les
  frappes accidentelles. `P` est trop proche de `p` (cloner).
- **D5-A** : c'est ce que fait Bruno (`transformProxyConfig`). Le message
  prévient avant l'écriture. `bru` lit déjà les deux formats.
- **D6-A** : sinon ces deux valeurs n'ont aucun effet pour le TUI,
  car `bru run` les ignore (§ 1.7).
- **D7-A** : `bru` applique déjà ce défaut quand aucun `--env` n'est
  passé (§ 1.7). Le TUI ne passe `--env` que si un environnement est choisi.
  Rien à ajouter ici.

## 1. Comportement de Bruno (référence)

Relevé dans Bruno desktop 4.2.1 (`app.asar`) et dans `bru` 4.2.1
(`~/.local/lib/bru-cli/node_modules/@usebruno/cli`).
Rien n'est deviné. Chaque ligne cite sa source.

### 1.1 Onglets de « Collection Settings »

Renderer (`/web/static/js/index.c9f1eb9b.js`, attributs
`collection-settings-tab-*`).

| Onglet | Écrit dans | Déjà dans le TUI ? |
|---|---|---|
| Overview (version, docs) | `bruno.json` (version), `collection.bru` (docs) | docs : oui |
| Headers | `collection.bru` | oui |
| Vars | `collection.bru` | oui |
| Auth | `collection.bru` | oui |
| Script | `collection.bru` | oui |
| Tests | `collection.bru` | oui |
| **Presets** | **`bruno.json`** | **non** |
| **Proxy** | **`bruno.json`** | **non** |
| **Client Certificates** | **`bruno.json`** | **non** |
| Protobuf | `bruno.json` (`protobuf`) | non, hors périmètre |
| Secrets (externes) | fichiers d'environnement, pas `bruno.json` | non, hors périmètre |

- Les cinq premiers sont éditables dans le TUI (README, « Édition par
  popups » : en-têtes, auth, vars, scripts, tests, docs du `collection.bru`).
- Reste donc : Presets, Proxy, Client Certificates. C'est le sujet de cette spec.
- Protobuf ne sert qu'aux requêtes gRPC, que `bru run` 4.2.1 ne sait pas
  exécuter (README, « Requêtes gRPC et WebSocket »).

### 1.2 Canal d'écriture

| Élément | Valeur | Source |
|---|---|---|
| Canal IPC | `renderer:update-bruno-config` | `/src/ipc/collection.js`, `ipcMain.handle` |
| Fonction | `writeBrunoConfig(brunoConfig, collectionPath, collectionRoot)` | idem |
| Avant écriture | `transformBrunoConfigBeforeSave(_.cloneDeep(config))` | `/src/utils/transformBrunoConfig.js` |
| Sérialisation | `stringifyJson` = `JSON.stringify(valeur, null, 2)` | `/src/utils/common.js` |
| Écriture | `writeFile` → `safeWriteFile` → `fs-extra.outputFileSync` | `/src/utils/filesystem.js` |

Conséquences :

- indentation : **2 espaces** ;
- ordre des clés : celui de l'objet JavaScript. Une clé existante garde sa
  place ; une clé nouvelle est **ajoutée à la fin** ;
- **aucun retour à la ligne final** (`JSON.stringify` n'en ajoute pas) ;
- tout tableau est éclaté, un élément par ligne ;
- l'écriture de Bruno n'est **pas atomique** (`outputFileSync`) ;
- depuis les Réglages, Bruno écrit **deux fichiers** d'un coup :
  `collection.bru` (`renderer:save-collection-root`) puis `bruno.json`
  (`ty`, `eb` dans le renderer). Le TUI n'écrira que `bruno.json`.

`transformBrunoConfigBeforeSave` fait, dans l'ordre :

1. `version` devient `"1"` ; l'ancienne valeur passe dans `collectionVersion`
   (sauf format `opencollection`) ;
2. retire `protobuf.importPaths[].exists` et `protobuf.protoFiles[].exists` ;
3. si `proxy.disabled === false`, supprime `proxy.disabled` ;
4. si `proxy.config.auth.disabled === false`, supprime `auth.disabled`.

Seuls 3 et 4 concernent cette spec. Le TUI ne touche pas à `version`.

### 1.3 Schéma de `proxy`

Source : composant du renderer « Proxy » (collection), reducer
`updateCollectionProxy`, `transformProxyConfig`
(`@usebruno/requests`, `src/utils/proxy-util.ts`).

Valeur par défaut affichée quand la clé est absente :

```json
{ "inherit": true,
  "config": { "protocol": "http", "hostname": "", "port": "",
              "auth": { "username": "", "password": "" },
              "bypassProxy": "" } }
```

| Champ | Type, règle | Source |
|---|---|---|
| mode | 3 boutons : `inherit`, `enabled`, `disabled` | renderer |
| `disabled` | booléen, absent si `false` | `transformBrunoConfigBeforeSave` |
| `inherit` | booléen | renderer |
| `config.protocol` | `http`, `https`, `socks4`, `socks5` | renderer (boutons radio) |
| `config.hostname` | texte, 1024 caractères au plus | renderer |
| `config.port` | entier 1 à 65535, ou `""` si vide | renderer |
| `config.auth.disabled` | booléen, absent si `false` | idem |
| `config.auth.username` | texte, 1024 au plus | renderer |
| `config.auth.password` | texte, 1024 au plus | renderer |
| `config.bypassProxy` | texte, 1024 au plus | renderer |

Correspondance mode ↔ champs (gestionnaire du renderer) :

| Mode | `disabled` | `inherit` |
|---|---|---|
| inherit | `false` (retiré à l'écriture) | `true` |
| enabled | `false` (retiré à l'écriture) | `false` |
| disabled | `true` | `false` |

- Les champs de `config` ne s'affichent que pour le mode `enabled`.
  Changer de mode **ne vide pas** `config`.
- La case d'authentification écrit `auth.disabled` = l'inverse de la case.
- Chaque modification refait un objet `{...proxy, ...changement}` : une
  première modification, clé absente, écrit donc l'objet par défaut complet.
- Ancien format (`enabled: true | false | 'global'`, `auth.enabled`) :
  converti à la lecture par `transformProxyConfig`. Bruno l'écrit au
  nouveau format dès qu'on enregistre.

### 1.4 Schéma de `clientCertificates`

Source : composant « Client Certificates » du renderer, reducer
`updateCollectionClientCertificates`.

```json
"clientCertificates": {
  "enabled": true,
  "certs": [
    { "domain": "api.exemple.fr", "type": "cert",
      "certFilePath": "certs/client.crt", "keyFilePath": "certs/client.key",
      "passphrase": "" },
    { "domain": "*.exemple.fr", "type": "pfx",
      "pfxFilePath": "certs/client.pfx", "passphrase": "{{pfxPass}}" }
  ]
}
```

| Champ | Règle (Yup du renderer) |
|---|---|
| `domain` | obligatoire, `trim()` appliqué à l'écriture |
| `type` | `cert` ou `pfx` |
| `certFilePath`, `keyFilePath` | obligatoires si `type` = `cert` |
| `pfxFilePath` | obligatoire si `type` = `pfx` |
| `passphrase` | texte libre, peut être vide |
| `disabled` | `true` si l'entrée est désactivée ; clé **retirée** si réactivée |

- Ordre des clés d'une entrée `cert` : `domain`, `type`, `certFilePath`,
  `keyFilePath`, `passphrase`. Entrée `pfx` : `domain`, `type`,
  `pfxFilePath`, `passphrase`. `disabled` vient après.
- Bruno propose : ajouter, supprimer, activer / désactiver. **Pas de
  modification d'une entrée** : on supprime puis on ajoute.
- Les chemins choisis sont convertis en chemin **relatif à la collection**
  (`path.relative(collection.pathname, fichier)`). Il peut contenir `..`.
- À chaque écriture, `enabled: true` est réécrit.

### 1.5 Schéma de `presets`

Source : composant « Presets » du renderer, reducer `updateCollectionPresets`.

| Champ | Valeurs | Défaut affiché |
|---|---|---|
| `requestType` | `http`, `graphql`, `grpc`, `ws` | `http` |
| `requestUrl` | texte (« Default Base URL ») | `""` |
| `defaultEnvironment` | nom d'un environnement de la collection | absent |

- Clé `presets` absente : première modification, Bruno écrit
  `{ "requestType": "http", "requestUrl": "", … }` avec le champ changé.
- `defaultEnvironment` choisi « None » : la clé est **retirée**, le reste
  de `presets` est conservé.
- Le nouveau dialogue de requête de Bruno lit `presets.requestType` et
  `presets.requestUrl` pour se préremplir.

### 1.6 Secrets : où Bruno les met

| Valeur | Où | Protection |
|---|---|---|
| `proxy.config.auth.password` | `bruno.json`, **en clair** | champ masqué à l'écran ; aucun avertissement relevé |
| `clientCertificates.certs[].passphrase` | `bruno.json`, **en clair** | champ masqué ; avertissement « Store sensitive info as a secret variable or in a .env file » si la valeur n'est pas entièrement `{{…}}` (`/^\s*\{\{.*\}\}\s*$/`) |

- Bruno ne propose rien d'autre : pas de coffre, pas de chiffrement.
- `{{variable}}` et `{{process.env.NOM}}` sont acceptés par `bru`
  (§ 1.7, essai réel).

### 1.7 Ce que `bru run` 4.2.1 lit réellement

Lecture : `JSON.parse` brut de `bruno.json`
(`cli/src/utils/collection.js`, `getCollectionConfig`). Aucune des
transformations du desktop n'est appliquée, sauf `transformProxyConfig`.

| Clé | Lue par `bru` ? | Source |
|---|---|---|
| `proxy` | **oui** | `runner/run-single-request.js`, `utils/proxy-util.js` |
| `clientCertificates` | **oui** | `runner/run-single-request.js` |
| `presets.defaultEnvironment` | **oui**, si ni `--env` ni `--env-file` | `commands/run.js` |
| `presets.requestType`, `presets.requestUrl` | **non** | aucune occurrence dans `cli/src` |
| `protobuf` | non | idem |
| `scripts` | oui (modules, accès fichiers) | hors périmètre |

Points relevés dans le code :

- tous les champs du proxy et des certificats passent par
  `interpolateString` : `{{variable}}` et `{{process.env.X}}` y marchent ;
- `--noproxy` (option du runner `O`) force le proxy à `off` ;
- `--client-cert-config` ajoute des certificats à ceux de `bruno.json` ;
- certificat : le **premier** domaine qui correspond gagne ;
- chemin relatif : joint à la racine de la collection ;
- `defaultEnvironment` : seul un nom de fichier simple est accepté
  (`isSafeFileName`), sinon avertissement et valeur ignorée.

**Essais réels** (collection jetable, `bru` 4.2.1, faux proxy local) :

| Essai | Résultat |
|---|---|
| `proxy` activé, hôte `{{phost}}`, `presets.defaultEnvironment: "loc"` qui définit `phost`, aucun `--env` | la requête passe par le proxy ; l'environnement par défaut a été appliqué |
| même collection, hôte `{{process.env.PHOST}}` avec `.env` | la requête passe par le proxy |
| `proxy.disabled: true` | la requête ne passe plus par le proxy (erreur DNS directe) |

Conclusion : `proxy`, `clientCertificates` et `presets.defaultEnvironment`
ont un effet sur `bru`. `requestType` et `requestUrl` n'en ont qu'à travers
le TUI lui-même (D6).

## 2. Périmètre

| Réglage | Lire | Écrire |
|---|---|---|
| `proxy` | oui | oui |
| `clientCertificates` | oui | oui (ajouter, supprimer, activer) |
| `presets` | oui | oui |
| autres clés (`name`, `version`, `ignore`, `scripts`, `protobuf`, `openapi`, clés inconnues…) | non | **jamais** |

Hors périmètre :

- Collections OpenCollection YAML : la config vit dans `opencollection.yml`.
  Le popup s'ouvre en lecture seule, comme le reste (README).
- `collection.bru` : déjà éditable par ses propres popups.
- Créer `bruno.json`, le supprimer, le renommer.
- Lire le contenu d'un certificat ou d'une clé, les copier, les déplacer.
- Valider le certificat, tester le proxy.
- Proxy global de Bruno (Préférences) et proxy système.
- Préselectionner `defaultEnvironment` dans le TUI (D7).

## 3. Touches

| Touche | Où | Effet |
|---|---|---|
| `B` | panneau Collection, sur n'importe quelle ligne | ouvre le popup **Réglages** de la collection active |

Vérification des conflits (D4) :

- `src/app/message.rs`, `key_message` : lettres déjà prises =
  `q r D H E S C s a d c u f n N v y e m M z O R t x A p k j h l g G`,
  plus `/ | ? [ ]` et `F2`. **`B` n'y figure pas.**
- `src/app/view/help.rs` : aucune entrée `B`.
- README, tableaux de touches : aucune entrée `B`.
- `Ctrl+B` n'est pas utilisé non plus, mais on évite les `Ctrl`.
- En workspace, la collection active est celle de la ligne sélectionnée
  (`docs/workspace.md`, § 4). `B` ouvre donc ses réglages.
- Refusé avec un message : collection YAML (« Collection OpenCollection
  (YAML) en lecture seule »), exécution `bru run` en cours.
- À ajouter : ligne `B` dans l'aide `?` et dans le README (tableau de la
  vue d'ensemble).

## 4. Popup « Réglages »

Même modèle que `docs/edition-popups.md` : centré, trois parties,
`Tab` / `Maj+Tab` pour changer de partie, rien d'écrit avant `Ctrl+S`.

Touches communes (identiques aux autres popups) :

| Touche | Effet |
|---|---|
| `↑` `↓` | ligne précédente, suivante |
| `Tab`, `Maj+Tab` | partie suivante, précédente |
| `Entrée` | modifier la valeur ; sur un choix, ouvrir sa liste |
| `←` `→` | sur un choix : valeur précédente, suivante |
| `Espace` | cocher / décocher ; activer / désactiver un certificat |
| `Ctrl+S` | enregistrer, puis fermer |
| `Échap` | annuler la saisie ; sinon fermer (confirmation si modifié) |

### 4.1 Partie « Presets »

```
╭ Réglages — httpbin ─ Presets │ Proxy │ Certificats ─╮
│ Type de requête   ‹ http ›                           │
│ URL par défaut    {{base_url}}                       │
│ Environnement     ‹ aucun ›                          │
│                                                      │
│ ←→ choisir   Entrée modifier   Ctrl+S enregistrer    │
╰──────────────────────────────────────────────────────╯
```

- Type : `http`, `graphql`, `grpc`, `ws` (les quatre valeurs de Bruno).
- Environnement : « aucun » + les environnements de la collection.
  « aucun » retire la clé `defaultEnvironment`.
- Si D6-A : le popup `a` (nouvelle requête) préremplit `Type` et `URL`.
  `grpc` et `ws` ne sont pas créables dans le TUI : repli sur `http`,
  avec une mention dans le statut.

### 4.2 Partie « Proxy »

```
╭ Réglages — httpbin ─ Presets │ Proxy │ Certificats ─╮
│ Mode              ‹ enabled ›                        │
│ Protocole         ‹ http ›                           │
│ Hôte              {{proxy_host}}                     │
│ Port              8080                               │
│ [x] Authentification                                 │
│ Utilisateur       alice                              │
│ Mot de passe      {{proxy_password}}                 │
│ Contournement     localhost,*.interne                │
│                                                      │
│ Ctrl+S enregistrer   Échap fermer                    │
╰──────────────────────────────────────────────────────╯
```

- Mode : `inherit`, `enabled`, `disabled` (§ 1.3). Les lignes sous Mode
  sont grisées et non éditables hors du mode `enabled`, comme Bruno.
  Leurs valeurs sont conservées dans le fichier.
- Protocole : `http`, `https`, `socks4`, `socks5`.
- Port : entier 1 à 65535, ou vide (écrit `""`, comme Bruno).
  Le port ne peut pas être `{{variable}}` : Bruno impose un nombre.
- Utilisateur et mot de passe : grisés si la case est décochée.
- Mot de passe : masqué par des puces `•` ; `Ctrl+R` bascule l'affichage,
  comme le popup Auth.
- Les textes sont limités à 1024 caractères, avec le message de Bruno
  (« Hostname must be less than 1024 characters », etc.), en français.
- Fichier à l'ancien format : voir D5 et § 6.

### 4.3 Partie « Certificats »

```
╭ Réglages — httpbin ─ Presets │ Proxy │ Certificats ─╮
│ ▸ api.exemple.fr     cert   certs/client.crt         │
│   *.exemple.fr       pfx    certs/client.pfx         │
│   vieux.exemple.fr   cert   certs/old.crt (désactivé)│
│                                                      │
│ a ajouter   d supprimer   Espace activer/désactiver  │
╰──────────────────────────────────────────────────────╯
```

Ajouter (`a`), sous-popup :

```
╭ Nouveau certificat ──────────────────────────────────╮
│ Domaine       api.exemple.fr                         │
│ Type          ‹ cert ›                               │
│ Certificat    certs/client.crt                       │
│ Clé           certs/client.key                       │
│ Passphrase    {{certPass}}                           │
│                                                      │
│ Ctrl+S ajouter   Échap annuler                       │
╰──────────────────────────────────────────────────────╯
```

- Type `pfx` : les lignes Certificat et Clé sont remplacées par `Fichier PFX`.
  Changer de type vide les champs de l'autre type, comme Bruno.
- Validation : domaine non vide après `trim` ; fichiers requis selon le
  type (messages de Bruno : « certFilePath is a required field », etc.).
- Chemin : saisi à la main (pas de sélecteur de fichier en terminal).
  Un chemin absolu est converti en chemin relatif à la racine de la
  collection, comme Bruno (`path.relative`). Un chemin relatif est gardé tel quel.
- Fichier introuvable : **avertissement** dans le statut, pas de refus
  (Bruno ne vérifie rien à la saisie ; `bru` l'affiche à l'exécution).
  Le TUI ne lit jamais le contenu du fichier.
- Supprimer (`d`) : confirmation `y`/`n` dans la barre de statut.
  Bruno ne demande rien. Ici la confirmation évite de perdre une entrée
  qu'il faudrait ressaisir en entier, secret compris.
- Espace : ajoute `disabled: true`, ou retire la clé.
- Pas de modification d'une entrée (comme Bruno) : supprimer, puis ajouter.

### 4.4 Secrets (D3-A)

- Champs concernés : `proxy.config.auth.password`, `passphrase`.
- Une valeur littérale est **refusée** à la validation. Le statut propose
  un nom de variable (`{{proxyPassword}}`, `{{certPassphrase}}`) et
  rappelle la configuration `.env` ou `--secret`.
- Sont acceptées : toute valeur entièrement `{{…}}` (même règle que Bruno,
  `/^\s*\{\{.*\}\}\s*$/`) et la valeur vide.
- La valeur littérale tapée est oubliée aussitôt : jamais écrite, jamais
  loggée, jamais mise dans l'historique ni dans un message d'erreur.
- Secret **déjà présent** dans le fichier : affiché masqué (`********`),
  jamais loggé. Le TUI ne le récrit pas sauf si la clé entière est
  réécrite (§ 5, ajout ou suppression d'un certificat) : il recopie alors
  ce qui était dans le fichier. Il n'invente rien.
- Variante D3-B (si choisie) : même question que dans le popup Auth,
  avec `[Entrée]` remplacer, `[g]` garder, `[Échap]` revenir.

## 5. Écriture

Règles (D1-A) :

1. **Lecture au moment d'ouvrir le popup** : octets de `bruno.json` +
   `FileStamp::capture`. Fichier illisible ou JSON invalide : popup refusé,
   message clair (le chargement le refuse déjà).
2. **Un seul écrit, au `Ctrl+S`**, et seulement si quelque chose a changé.
3. **Contrôle de fraîcheur** : `FileStamp` comparé avant l'écriture.
   Fichier modifié sur disque : refus (« bruno.json a changé sur le disque »),
   rien n'est écrit. Le popup reste ouvert.
4. **Écriture atomique** : fichier temporaire dans la collection, puis
   renommage (`write_atomic`, `src/writer/mod.rs`). Droits du fichier conservés.
5. **Jamais d'effacement** : on ne supprime pas `bruno.json`, on ne retire
   aucune clé de premier niveau.
6. **Confinement** : le seul fichier écrit est `<racine>/bruno.json`.
   Pas de lien symbolique qui sort de la racine (`validatePathIsInsideCollection`).
   Les chemins de certificats sont des chaînes : jamais ouverts en écriture.
7. Après l'écriture : `Message` de rechargement ; le watcher voit déjà
   `bruno.json` (`src/app/watch.rs`).

Méthode chirurgicale :

| Cas | Ce que fait le TUI |
|---|---|
| clé présente (`"proxy": …`) | remplace uniquement le texte de la valeur ; le reste du fichier est copié tel quel |
| clé absente | ajoute `,` + saut de ligne + `"clé": valeur` avant le `}` final (Bruno ajoute aussi à la fin) |
| plusieurs clés modifiées | chaque clé est traitée séparément, dans l'ordre du fichier |
| clé en double au premier niveau | refus (ambiguïté) |
| racine qui n'est pas un objet | refus |

- La valeur insérée est celle que Bruno écrirait : `JSON.stringify(valeur, null, 2)`,
  décalée au niveau d'indentation de la clé (2 espaces par niveau).
- Dans la valeur remplacée, la mise en forme devient celle de Bruno
  (tableaux éclatés). C'est la seule différence d'octets, limitée à la clé modifiée.
- Fin de ligne et retour final : conservés comme dans le fichier ;
  fichier à tabulations ou fins de ligne mixtes : refus avec un message
  (à réétudier si le cas se présente).
- Les deux nettoyages de § 1.2 (3 et 4) sont appliqués : `disabled: false`
  n'est jamais écrit.
- Ne sont **pas** appliqués : le basculement de `version` (on ne
  réécrit pas le fichier), la suppression de `protobuf.*.exists`.

Contenu écrit (D5-A : ancien proxy) :

- Ancien format (`enabled`) : le popup affiche la conversion de
  `transformProxyConfig` et le statut dit « Proxy converti au nouveau format
  de Bruno ». L'écriture remplace tout l'objet `proxy`.
- Ancien format et popup fermé sans modifier le proxy : rien n'est converti.

## 6. Règles communes

- Collection YAML : popup en lecture seule (§ 2).
- `bru run` en cours : refus avec un message (comme `docs/edition-phase3.md`, § 7).
- Un popup d'édition de requête ou un autre popup ouvert : `B` est ignoré
  (le popup garde le clavier).
- Rechargement différé tant que le popup a des modifications non
  enregistrées (comportement existant), avec avertissement.
- Le chemin de la collection et les noms d'environnement s'affichent ;
  **aucune valeur de secret ne s'affiche ni ne se logge**.
- Les valeurs `{{variable}}` ne sont jamais résolues par le TUI (principe de `CLAUDE.md`).
  Elles restent des chaînes, `bru` les résoudra.

## 7. Tests et critères d'acceptation

1. **Fichiers attendus générés par le vrai code de Bruno.**
   Script Node (hors dépôt Rust, dans `tests/tools/`) qui charge
   `transformBrunoConfigBeforeSave` et `JSON.stringify(…, null, 2)`, et
   produit la valeur attendue de chaque clé. Les sorties sont figées dans
   `tests/fixtures/collections/settings-cases/` (`*.before.bruno.json`,
   `*.after.bruno.json`), comme `writer-cases/`.
2. **Cas à l'octet près** :
   - ajout de `proxy` à un fichier sans `proxy` (clé ajoutée à la fin) ;
   - modification de `proxy` quand la clé existe (le reste du fichier,
     dont `"ignore": ["node_modules", ".git"]` sur une ligne, est identique) ;
   - `proxy.disabled: false` jamais écrit ; `auth.disabled: false` idem ;
   - passage de mode `inherit` → `enabled` → `disabled` (§ 1.3) ;
   - `presets` absent puis `defaultEnvironment` ajouté (objet par défaut complet) ;
   - `defaultEnvironment` retiré (le reste de `presets` conservé) ;
   - ajout d'un certificat `cert` puis `pfx` ; ordre des clés ;
   - suppression ; désactivation (`disabled: true`) ; réactivation (clé retirée) ;
   - fichier sans retour final, puis avec retour final ;
   - indentation à 2 espaces et à 4 espaces.
3. **Relecture** : chaque fichier écrit est relu par `JSON.parse` de Node
   et par `bru run` (la collection de test se charge).
4. **Secrets** : valeur littérale refusée pour mot de passe et passphrase ;
   `{{x}}` et vide acceptés ; vérifier par test qu'aucune valeur de secret
   n'apparaît dans les messages d'erreur, le statut ni l'historique ;
   fichier contenant déjà un secret : jamais affiché en clair.
5. **Fraîcheur** : fichier modifié entre l'ouverture et `Ctrl+S` → refus,
   aucun octet écrit.
6. **Atomicité** : échec simulé au renommage → `bruno.json` inchangé,
   pas de fichier temporaire restant.
7. **Refus** : YAML, exécution en cours, JSON invalide, clé en double,
   racine non objet, port hors de 1 à 65535, texte de plus de 1024 caractères,
   domaine vide, type `cert` sans clé.
8. **Ancien proxy** (`enabled: true`, etc.) : conversion identique à
   `transformProxyConfig` pour les trois valeurs de `enabled`.
9. **Touche `B`** : ouvre le popup ; aucun conflit (test sur `key_message`) ;
   ligne dans l'aide `?`.
10. **Essai réel** en release sur une copie de `examples/`, puis un
    `bru run` qui passe par un proxy local (comme l'essai du § 1.7).
11. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

## 8. Points ouverts

- **Générateur des fichiers attendus** : `transformBrunoConfigBeforeSave`
  importe `@usebruno/requests`. Il faut vérifier qu'il se charge hors
  d'Electron avec `~/.local/lib/bru-cli/node_modules`. Sinon, recopier
  ses quatre étapes (§ 1.2) dans le script et le dire dans la fixture.
- **Fichier temporaire** : `write_atomic` nomme le temporaire
  `.bruno.json.tmp-<pid>-<n>`. Vérifier que le watcher l'ignore.
- **Port `""`** : Bruno écrit `""` quand le port est vide ; `bru` en fait
  `http://hôte:`. À confirmer par un essai avant de figer le comportement.
- **Fichiers à tabulations** ou fins de ligne mixtes : refusés pour l'instant.
