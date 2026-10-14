# Spécification — Édition, phase 4 : déplacer, et créer depuis `curl`

Statut : **brouillon, à valider par Nicolas**.

Suite de `docs/edition-phase3.md`. Ses principes s'appliquent :
fraîcheur (`FileStamp`), écriture atomique, corbeille, écritures confinées
à la collection, aucune valeur devinée.

Relevé dans Bruno desktop 4.2.1 (`app.asar`). Le code de Bruno a été
**exécuté** (pas seulement lu) pour la partie 2 : voir § 6.

## 0. Décisions à prendre (Nicolas)

Option recommandée en premier.

| # | Question | Recommandation |
|---|---|---|
| 1 | Que faire de la partie 2 (`curl`) ? | **La sortir dans une spec à part** (phase 5) |
| 2 | Interaction pour déplacer | **Popup « Déplacer vers » (`m`)** |
| 3 | `seq` de l'élément déplacé dans le dossier cible | **Comme Bruno : inchangé** |
| 4 | Déplacer entre collections d'un workspace | **Exclure** |
| 5 | Si la partie 2 est faite : secrets collés | **Popup `{{variable}}` / garder / annuler** |
| 6 | Si la partie 2 est faite : option `curl` inconnue | **Refuser** |
| 7 | Si la partie 2 est faite : comment saisir la commande | **Éditeur externe (`$EDITOR`)** |

Raisons :

1. **Partie 2 à part.** Le parseur de Bruno n'est pas un petit parseur de
   `curl`. C'est un assemblage de 6 bibliothèques, avec beaucoup de
   comportements surprenants (§ 6). Le refaire « à l'identique » en Rust
   est un chantier à lui seul. Le mélanger au déplacement retarderait
   les deux.
2. **Popup plutôt que couper/coller.** Pas d'état caché (une « coupe » qui
   survit au rechargement de l'arbre). Destination visible avant de
   valider. Même modèle que les popups de la phase 3.
3. **`seq` inchangé.** C'est ce que fait Bruno pour un dépôt « dans » un
   dossier. Le fichier déplacé reste identique à l'octet près. Voir § 2.3
   pour l'alternative (« à la fin »), qui n'est **pas** le comportement de
   Bruno.
4. **Pas de déplacement entre collections.** Fraîcheur et confinement sont
   définis par collection. Bruno ajoute le cas `bru` ↔ `yml` (§ 2.5).
5. à 7. Voir § 5 et § 6.

## 1. Périmètre

| Opération | Requête | Dossier |
|---|---|---|
| Déplacer vers un autre dossier de la même collection | oui | oui |
| Déplacer vers la racine de la collection | oui | oui |
| Créer une requête depuis `curl` | partie 2 (décision 1) | — |

Hors phase 4 :

- Déplacer vers une autre collection (§ 2.5).
- Choisir la position exacte dans le dossier cible (« au-dessus de », « en
  dessous de ») : se fait ensuite avec `Maj+↑` / `Maj+↓` (phase 3).
- Glisser-déposer à la souris.
- Collections YAML : lecture seule, comme avant.
- `bruno.json`, `collection.bru`, `folder.bru` seuls : jamais déplacés seuls.
- Coller une commande `curl` dans l'URL d'une requête existante (§ 6.6).

## 2. Partie 1 — Comportement de Bruno : déplacer

### 2.1 Chaîne d'appel

| Étape | Source |
|---|---|
| Glisser-déposer dans la barre latérale | renderer, action `moveItem` (fonction `eP`, `index.c9f1eb9b.js`) |
| Chemin cible | `getNewPathname` (`ex`) |
| Déplacement sur disque | canal IPC `renderer:move-item` (`src/ipc/collection.js`) |
| `seq` des autres éléments | canal `renderer:resequence-items` |

Les canaux `renderer:move-file-item` et `renderer:move-folder-item`
existent dans `src/ipc/collection.js` mais le renderer ne les appelle
plus (aucune occurrence dans `index.c9f1eb9b.js`). Ils sont ignorés ici.

### 2.2 Sur disque (`renderer:move-item`)

Entrée : `{ targetDirname, sourcePathname }`.

1. Les deux chemins doivent être dans la collection
   (`validatePathIsInsideCollection`).
2. `bruno.json` ne peut pas être déplacé.
3. Le dossier cible doit exister. Sinon : erreur.
4. Même dossier que la source : **rien ne se passe**.
5. Collision : Bruno choisit un nom libre avec suffixe ` 1`, ` 2`…
   (`getUniqueTargetPath` → `getUniqueRenamePath`, `src/utils/filesystem.js`).
   Sans erreur. Pour un dossier, le suffixe va sur le nom du répertoire
   (`Posts 1`) ; pour un fichier, avant l'extension (`get-post 1.bru`).
   Le nom affiché (`meta.name`) ne change pas.
6. Copie récursive (`copyPathTo`), puis **suppression de la source**
   (`removePath`). Refus si la cible est la source ou dans la source.
   Les liens symboliques sont suivis (copiés comme fichiers).

Le contenu des fichiers n'est jamais réécrit par le déplacement.

### 2.3 `seq` (renderer, `moveItem`)

| Cas | Ce que fait Bruno |
|---|---|
| Dossier source | frères restants renumérotés (`getItemsToResequence`, `eb`), comme après une suppression |
| Dépôt « dans » un dossier | **aucun `seq` écrit dans la cible** : l'élément garde son ancien `seq` |
| Dépôt au-dessus / en dessous (autre dossier) | `getReorderedItemsInSourceDirectory` (`eg`) : place l'élément à côté de la cible, renumérote la cible |
| Même dossier (réordonner) | même fonction `eg`, sans déplacement de fichier |

Conséquence du cas « dans » : le `seq` peut déjà exister dans la cible.
`sortItems` (`zz`, déjà reproduit dans `src/writer/sequence.rs`) départage
alors les doublons par nom.

Variante possible, **qui n'est pas Bruno** : placer l'élément en dernier
(`seq` = nombre de frères + 1). Elle écrit un fichier de plus. À n'adopter
que si Nicolas préfère un ordre prévisible à la fidélité (décision 3).

### 2.4 Cas refusés ou ignorés par Bruno

| Cas | Comportement de Bruno |
|---|---|
| Déposer sur soi-même | refusé (`canDrop`, `ek`) |
| Dossier déposé dans un de ses descendants | ignoré sans message (`isDescendant`, `ev`) |
| Déposer sur la racine alors que l'élément y est déjà | ignoré |
| `bruno.json` | message « bruno.json cannot be moved » |
| Nom pris dans la cible | suffixe ` 1`, ` 2`… |

### 2.5 Entre collections (hors périmètre)

Bruno accepte un dépôt dans une autre collection. Si les formats diffèrent
(`bru` / `yml`), il passe par `renderer:move-item-cross-format` : relecture,
conversion, écriture, suppression. Un **dossier** entre formats différents
est refusé (« Moving folders between collections with different formats is
not supported »).

## 3. Partie 1 — Ce que ferait le TUI

### 3.1 Différences voulues avec Bruno

| Bruno | TUI |
|---|---|
| copie puis suppression définitive | **un seul `rename`**, jamais de copie ni de suppression |
| autre disque : copie possible | **refus** : « Impossible de déplacer : autre disque » |
| liens symboliques suivis | **refus** si l'élément contient un lien symbolique (comme le clonage, phase 3) |
| aucun contrôle de fraîcheur | `FileStamp` de chaque fichier concerné ; refus si modifié |

Le mécanisme de nom libre existe déjà : `move_to_free_name`
(`src/writer/files.rs`) — dossier réservé par `create_dir` puis `rename` ;
fichier par `hard_link` puis `remove_file`. Il ne couvre aujourd'hui que le
même répertoire. La phase 4 l'étend à un autre répertoire. Le nom suffixé
suit `suffixed_name` / `truncate_base_for_suffix`, déjà alignés sur Bruno.

### 3.2 Renumérotation

Après le déplacement :

1. Frères restants du dossier source : `items_to_resequence` (déjà écrit,
   utilisé pour la corbeille). Si l'ordre de Bruno n'est pas reproductible
   (noms non comparables), les `seq` restent tels quels et le statut le dit.
2. Dossier cible : rien (décision 3, option recommandée).

Seuls les `seq` qui changent sont écrits, comme en phase 3.

## 4. Touches et popup

### 4.1 Touches (dans l'arbre)

| Touche | Effet |
|---|---|
| `m` | ouvrir « Déplacer vers » pour la ligne sélectionnée |

Vérification des conflits (`src/app/message.rs`, `src/app/update/mod.rs`) :

- `m` est déjà `OpenRequestEdit`, mais seulement si `focus == Detail`
  (`update/mod.rs`, `Message::OpenRequestEdit`). Dans l'arbre, il est libre.
  Même schéma que `a`, `d`, `p` en phase 3 : garde `focus == Tree`.
- Il faudra ajouter `m` à l'aide (`src/app/view/help.rs`, section arbre) et
  au `README`.
- Sur l'en-tête d'une collection : refusé avec un message.

**Variante (décision 2, option B)** : `x` couper, `v` coller.
`x` n'agit que dans la Réponse (`Message::ExportResponseBody`) et `v` que
dans le Détail et la Réponse (`toggle_visual`) : libres dans l'arbre aussi.
Mais il faut garder une « coupe » en mémoire, la marquer dans l'arbre, et
la rendre invalide quand l'arbre se recharge. C'est pourquoi elle n'est pas
recommandée.

### 4.2 Popup « Déplacer vers »

```
╭ Déplacer — get-post.bru ─────────────────────╮
│ Vers  ‹filtre›                                │
│ ▸ (racine de la collection)                   │
│   Posts                                       │
│     Commentaires                              │
│   Utilisateurs                                │
│                                               │
│ Sera nommé : get-post 1.bru                   │
│ Entrée déplacer   Échap annuler               │
╰───────────────────────────────────────────────╯
```

- Liste : racine de la collection, puis tous les dossiers, en arbre.
- Saisie = filtre (sous-chaîne, sans casse). `↑↓` choisissent. `Entrée` valide.
- Masqués : le dossier actuel de l'élément (Bruno ne fait rien), l'élément
  lui-même et ses descendants si c'est un dossier.
- La ligne « Sera nommé » n'apparaît qu'en cas de collision ; le statut
  donne ensuite le nom final.
- Après le déplacement : l'arbre est rechargé, l'élément reste sélectionné
  (déplié dans son nouveau dossier).
- Erreur (fichier modifié, autre disque, lien symbolique) : message dans le
  popup, rien n'est déplacé.

## 5. Règles communes

- Collection YAML : refusé (lecture seule).
- Pendant un `bru run` de la collection : refusé, avec un message.
- Tout reste dans la collection (`inside`, pas de `..`, pas de lien qui sort).
- Une erreur laisse le disque propre : un seul `rename`, donc tout ou rien.
- Un dossier déplacé garde `folder.bru` et son contenu à l'identique.
- Rien n'est effacé (pas de repli en copie + suppression).

## 6. Partie 2 — Créer une requête depuis `curl`

**Recommandation : ne pas la faire dans cette phase.** Voir § 6.4.

### 6.1 Comportement de Bruno

| Élément | Source |
|---|---|
| Popup « New Request », choix « From cURL », champ `curlCommand` | renderer, composant du popup (`index.c9f1eb9b.js`, ~221000) |
| Validation « Invalid cURL Command » | schéma Yup, `lp(t) !== null` |
| Conversion | fonction `lp(commande, type)` → `{url, method, headers, body, auth}` |
| Création | action `newHttpRequest` (`e_`) avec `settings: {encodeUrl: false, forwardAuthorizationHeader: false}` |
| Collage dans le champ URL | gestionnaire `onPaste` : si le texte commence par `curl` (`/^\s*curl\s/i`), bascule sur « From cURL » |
| Type GraphQL | si l'URL finit par `/graphql` ou si `Content-Type` contient `application/graphql` |

Fichier produit (exemple réel, `stringifyRequest` de `@usebruno/filestore`
0.13.0, commande
`curl -X POST 'https://a.b/c?x=1' -H 'Content-Type: application/json' -H 'Authorization: Bearer abc' -u bob:secret -d '{"a":1}'`) :

```
meta {
  name: Depuis curl
  type: http
  seq: 1
}

post {
  url: https://a.b/c?x=1
  body: json
  auth: basic
}

params:query {
  x: 1
}

headers {
  Content-Type: application/json
  Authorization: Bearer abc
}

auth:basic {
  username: bob
  password: secret
}

body:json {
  {
    "a": 1
  }
}

settings {
  encodeUrl: false
  forwardAuthorizationHeader: false
}
```

Remarque : `encodeUrl: false` ici, alors que la requête par défaut du TUI
écrit `encodeUrl: true` (`new_request_content`).

### 6.2 Options `curl` gérées par Bruno

Table `i$` dans le renderer :

| Option | Effet |
|---|---|
| `-X`, `--request` | méthode (majuscule forcée) |
| `-H`, `--header` | en-tête |
| `-A`, `--user-agent` | en-tête `User-Agent` |
| `-d`, `--data`, `--data-ascii`, `--data-urlencode` | corps |
| `--data-raw`, `--data-binary` | corps (binaire : fichier si `@…`) |
| `--json` | corps JSON + `Content-Type`, méthode POST |
| `-F`, `--form` | corps `multipartForm`, méthode POST |
| `-u`, `--user` | auth `basic` (ou `digest` / `ntlm` avec `--digest` / `--ntlm`) |
| `-b`, `--cookie` | en-tête `Cookie` |
| `-I`, `--head` | méthode HEAD |
| `-G`, `--get` | corps ajouté à l'URL |
| `--compressed` | en-tête `Accept-Encoding: deflate, gzip` |
| `-k`, `--insecure` | lu, puis ignoré dans le fichier |

Toute autre option est **ignorée sans message** (voir § 6.3).

### 6.3 Comportements surprenants (relevés par exécution)

Le vrai `lp` a été extrait du bundle et exécuté sur 100 commandes.
Quelques résultats, tous reproductibles :

| Commande | Résultat de Bruno |
|---|---|
| `-d '{"a":1}'` sans `Content-Type` | corps `formUrlEncoded`, champ nommé `{"a":1}`, valeur vide |
| `--data-binary @f.json` sans `Content-Type` | **plante** (`null`, « Invalid cURL Command ») |
| `-o out.txt https://a.b/c` | URL = `https://out.txthttps//a.b/c` (l'argument de `-o` est pris pour l'URL) |
| deux URL | collées bout à bout |
| `-H 'Accept:'` | en-tête de valeur `undefined` |
| `-H'A: b'` (sans espace) | ignoré |
| `-H 'A: b\nC: d'` | seul `A: b` est gardé |
| `-d 'x'` avec `-X GET` | corps gardé, méthode GET |
| `-X foo` | méthode `foo` acceptée |
| `ftp://a.b/c` | URL `http://ftp://a.b/c` |
| `localhost` (sans point) | refusé (`null`) |
| `file:///etc/passwd` | refusé (`null`) |
| `{{var}}` dans l'URL | encodé en `%7B%7Bvar%7D%7D` |
| `$HOME` dans une valeur | remplacé par le vide (pas d'accès à l'environnement) |
| `-u a:b:c` | mot de passe `b` (le reste est perdu) |
| `-F 'a=b;type=text/plain'` | le `;type=…` reste dans la valeur |
| `-d '{"a":1'` (JSON invalide, type JSON) | accepté, mis en forme à moitié |
| `Content-Type: application/octet-stream` + `-d` | corps `none` : le corps est perdu |
| `\|`, `>`, `;` (redirection, tube) | ignorés |

Le message d'erreur de Bruno est unique : « Invalid cURL Command ».

### 6.4 Taille du parseur : trop gros pour cette phase

`lp` s'appuie sur (identifiants de modules du bundle) :

| Bibliothèque | Rôle | Module |
|---|---|---|
| `shell-quote` | découpe de la ligne de commande | 43784 / 18331 |
| `url` (Node, `url.parse` ancien) | nettoyage de l'URL, encodage `%XX` | 43427 |
| `qs` (utilitaires Bruno) | requête de l'URL | 2338 |
| `query-string` | corps `-d` en paires clé/valeur | 6343 |
| `cookie` | `-b` | 20556 |
| `jsonc-parser` + beautifier | mise en forme du corps JSON, y compris `{{var}}` | 7295 (`W`) |

Estimation du travail en Rust :

- ~800 à 1500 lignes de code, plus une table de cas générée par le vrai code
  (`scripts/bruno-curl-cases.js`, à écrire, sur le modèle de
  `scripts/bruno-new-items.js`).
- Les deux plus risqués : `url.parse` (règles d'échappement, port, hôte en
  minuscules, `#`) et la mise en forme JSON (jetons préservés, commentaires,
  `{{var}}`).
- Chaque comportement du § 6.3 doit être reproduit **ou refusé**. Refuser
  coûte moins que reproduire, mais la liste des refus doit être juste.

Deux voies :

| Voie | Contenu |
|---|---|
| **A (recommandée)** : spec à part | Phase 5 : relevé complet, table de 100+ cas, parseur reproduit, ou sous-ensemble strict avec refus de tout le reste |
| B : sous-ensemble ici | Seulement `-X`, `-H`, `--json`, `-d` avec `Content-Type: application/json` valide, `-u` ; tout le reste refusé |

La voie B reste un vrai parseur : `shell-quote` et la mise en forme JSON y
sont déjà nécessaires. Elle n'économise que `url.parse` (en refusant toute
URL qui n'est pas déjà propre).

### 6.5 Secrets (cas où la partie 2 est faite)

Bruno écrit tout en clair : `-u user:pass`, `Authorization: Bearer …`,
`Cookie`, `user:pass@` dans l'URL. Il n'avertit pas.

Règle proposée pour le TUI (même modèle que `docs/edition-phase2.md` § 5) :

- Détection : mot de passe de `-u`, valeur de l'en-tête `Authorization`
  (et `Proxy-Authorization`), en-têtes dont le nom contient `token`, `key`,
  `secret` ; la partie `user:pass@` d'une URL.
- Avant l'écriture, un popup :

```
Secret en clair dans la commande curl.
  Entrée  remplacer par {{basicPassword}}
  g       garder en clair
  Échap   annuler
```

- « Remplacer » écrit `{{nom}}` ; la valeur est oubliée. Jamais écrite
  ailleurs, jamais loguée (ni la commande entière).
- La commande collée n'est **jamais** gardée : ni historique, ni log, ni
  fichier temporaire. Si `$EDITOR` sert à la saisir (décision 7), le
  fichier temporaire est créé en mode `0600` et supprimé après lecture
  (c'est un fichier du TUI, pas un fichier de la collection).

### 6.6 Saisie de la commande (cas où la partie 2 est faite)

- Le TUI n'active pas le collage « bracketed » (`Event::Paste` absent de
  `src/app/event.rs`). Une commande collée sur plusieurs lignes arriverait
  comme des touches `Entrée`, qui feraient avancer le popup.
- Le presse-papiers du TUI est en écriture seule (`Clipboard::set_text`).
- Options : activer le collage « bracketed » (changement du terminal),
  ou saisir la commande dans `$EDITOR` (le TUI sait déjà ouvrir un éditeur
  externe pour le corps). Recommandation : `$EDITOR`.
- Coller du `curl` dans l'URL d'une requête **existante** : Bruno le fait
  (autre gestionnaire `onPaste`, mêmes `lp`), mais c'est de l'édition de
  brouillon, hors de cette spec.
  Remarque sur Bruno : dans ce gestionnaire, les corps `formUrlEncoded` et
  `multipartForm` ne sont pas appliqués (branche sans effet).

## 7. Tests et critères d'acceptation

### Partie 1

1. Nom libre : mêmes suffixes que Bruno (` 1`, ` 2`…), fichier et dossier.
2. Déplacer une requête et un dossier (avec sous-dossiers) : octets
   identiques avant / après, `folder.bru` intact.
3. Renumérotation de la source : table de référence de
   `tests/fixtures/sequence/bruno-sort-items.json` (déjà en place).
4. Refus : dossier dans lui-même ou un descendant, même dossier, racine
   de collection (en-tête), YAML, `bru run` en cours, fichier modifié
   (`FileStamp`), autre disque, lien symbolique.
5. Jamais d'effacement : aucun chemin de code ne supprime autre chose que
   le nom d'origine après `hard_link`.
6. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
7. Essai réel en release sur une copie de `examples/`, puis `bru run`.

### Partie 2 (spec à part)

- Table de cas générée par le vrai `lp` (`scripts/bruno-curl-cases.js`),
  comparée octet pour octet avec le `.bru` de `stringifyRequest`.
- Chaque ligne du § 6.3 : reproduite, ou refusée avec un test.
- Fichiers attendus relus par `@usebruno/lang`.
- Aucun secret dans les logs, l'historique, ni les fichiers temporaires.

## 8. Hors périmètre

- Déplacement entre collections ; conversion `bru` ↔ `yml`.
- Choix de la position (au-dessus / en dessous) en un seul geste.
- Glisser-déposer à la souris.
- Édition d'une requête existante depuis `curl`.
- Export vers `curl` (déjà fait : touches `c` et `u` dans la Réponse).
