# Spécification — Édition, phase 2 : auth et corps

Statut : **approuvée par Nicolas le 2026-10-12**.
Décisions de Nicolas du 2026-10-12 :

1. Changement de mode d'auth : l'ancien bloc est supprimé, comme dans Bruno,
   avec confirmation s'il contient des valeurs.
2. Secret saisi en clair : popup avec choix (remplacer par `{{variable}}`,
   garder, revenir à la saisie).
3. Valeur sensible masquée pendant la saisie, `Ctrl+R` pour l'afficher.
Suite de `docs/edition.md` (phase 1, approuvée et livrée le 2026-10-12).
Les principes de la phase 1 (§ 2 : écriture chirurgicale, fraîcheur,
rien avant `Ctrl+S`, aucune interprétation, validation par `@usebruno/lang`)
s'appliquent sans changement.

Cette spécification, une fois approuvée, lève pour la phase 2 seulement la
règle de `CLAUDE.md` : « aucune écriture dans un fichier `.bru` tant que la
capacité d'édition n'a pas été spécifiée et approuvée ».

## 1. Périmètre

### Dans la phase 2

| Famille | Contenu |
|---|---|
| Mode d'auth | changer le mode d'une requête, d'un dossier ou de la collection |
| Paramètres d'auth | modifier les valeurs du bloc `auth:<mode>` actif |
| Mode du corps | changer le mode du corps d'une requête HTTP |
| Corps en formulaire | éditer `body:form-urlencoded` et les champs texte de `body:multipart-form` |
| Variables GraphQL | éditer `body:graphql:vars` |
| Secrets en clair | avertir à la saisie et proposer `{{variable}}` |

### Hors phase 2

- Fichiers joints : les champs `@file(...)` de `body:multipart-form` et le
  bloc `body:file` restent en lecture seule (on peut seulement les
  activer / désactiver ou les supprimer).
- OAuth 2 : pas de création ni de changement de `grant_type` ; on modifie
  seulement les valeurs des clés existantes du bloc `auth:oauth2`. Les
  sous-blocs `auth:oauth2:additional_params:*` restent en lecture seule.
- OAuth 1 et Akamai EdgeGrid : valeurs des clés existantes seulement.
- YAML, gRPC, WebSocket : toujours en lecture seule.
- Créer, renommer, supprimer des fichiers : phase 3.

## 2. Comportement de Bruno (référence)

Relevé dans Bruno desktop 4.2.1 (`app.asar`, réducteurs Redux) et dans
`jsonToBru.js` / `jsonToCollectionBru.js` de `@usebruno/lang` 4.2.1 :

- **Changer de mode d'auth** remplace l'objet `auth` par `{ mode }` :
  à l'enregistrement, l'ancien bloc `auth:<ancien>` disparaît du fichier.
- **Changer de mode du corps** ne change que le mode : les autres blocs
  `body:*` restent dans le fichier.
- Requête : le mode est la clé `auth:` (ou `body:`) du bloc de méthode.
  Dossier et collection : bloc `auth { mode: <mode> }`.
- Le bloc d'un mode est écrit avec toutes ses clés, valeur vide comprise
  (`username: `), dans un ordre fixe.

Le TUI fait la même chose, pour qu'un fichier enregistré par l'un soit
identique à celui enregistré par l'autre.

## 3. Mode d'auth

### 3.1 Opérations

- Nouveau champ « Mode d'auth » dans la session d'édition, en tête de
  l'onglet Auth.
- `Entrée` ouvre un sélecteur, comme pour la méthode (`↑↓`, `Entrée`, `Échap`).
- Modes proposés : `none`, `inherit`, `basic`, `bearer`, `digest`, `ntlm`,
  `wsse`, `apikey`, `awsv4`.
  - `inherit` n'est pas proposé pour la collection (rien à hériter).
  - `oauth1`, `oauth2`, `akamai-edgegrid` ne sont pas proposés (voir § 1),
    mais s'affichent normalement si le fichier les utilise déjà.

### 3.2 Écriture (au `Ctrl+S`)

1. La valeur du mode est remplacée (`auth: bearer`, ou `mode: bearer` dans
   le bloc `auth` du dossier / de la collection, créé s'il manque).
2. L'ancien bloc `auth:<ancien>` est retiré, comme dans Bruno.
   - S'il contenait au moins une valeur non vide, `Ctrl+S` demande d'abord
     confirmation : « L'ancien bloc auth:basic sera supprimé. Continuer ? ».
3. Le nouveau bloc `auth:<mode>` est créé avec toutes ses clés, valeurs vides,
   dans l'ordre de `jsonToBru.js`, à la place où Bruno l'écrit.
   - `none` et `inherit` n'ont pas de bloc.
   - Clés par mode :

| Mode | Clés |
|---|---|
| `basic`, `wsse`, `digest` | `username`, `password` |
| `bearer` | `token` |
| `ntlm` | `username`, `password`, `domain` |
| `apikey` | `key`, `value`, `placement` |
| `awsv4` | `accessKeyId`, `secretAccessKey`, `sessionToken`, `service`, `region`, `profileName` |

## 4. Paramètres d'auth

- Chaque clé du bloc `auth:<mode>` actif devient un champ ; `Entrée` modifie
  sa valeur. Pas d'ajout, de suppression, de renommage ni de `~` :
  les clés sont fixées par Bruno (comme `settings`).
- `apikey.placement` : sélecteur `header` / `queryparams`.
- Affichage : les valeurs sensibles restent masquées hors saisie
  (`src/collection/auth.rs`, déjà en place).

## 5. Secrets en clair

### 5.1 Détection

Une valeur saisie est « un secret en clair » quand :

- sa clé est sensible (`is_sensitive_auth_key` : `password`, `token`,
  `secret`, `apikey`, etc., et la clé `value` d'`apikey`) ;
- et la valeur n'est pas vide et ne contient aucune référence `{{…}}`.

### 5.2 Ce qui se passe

À la validation de la saisie (`Entrée`), avant d'enregistrer la modification
dans la session, un popup s'ouvre :

```
Secret en clair dans le fichier .bru (souvent partagé via git).
  Entrée  remplacer par {{bearerToken}}
  g       garder en clair
  Échap   revenir à la saisie
```

- Nom proposé : `<mode><Clé>` en camelCase (`basicPassword`, `bearerToken`,
  `apikeyValue`, `awsv4SecretAccessKey`).
- « Remplacer » écrit `{{bearerToken}}` à la place de la valeur, et le statut
  rappelle : « Définis bearerToken dans .env ou avec --secret bearerToken ».
  La valeur saisie est oubliée : le TUI ne l'écrit nulle part
  (ni `.env`, ni environnement, ni log).
- « Garder » enregistre la valeur telle quelle dans la session.

### 5.3 Pendant la saisie

Une valeur sensible est masquée pendant qu'on la tape (`•`), avec `Ctrl+R`
pour l'afficher le temps de vérifier.

## 6. Mode du corps

- Nouveau champ « Mode du corps » en tête de l'onglet Corps, avec sélecteur.
- Modes proposés pour une requête HTTP : `none`, `json`, `text`, `xml`,
  `sparql`, `formUrlEncoded`, `multipartForm`, `file`.
  - Requête GraphQL (`type: graphql`) : mode fixe, pas de sélecteur.
- Écriture : seule la clé `body:` du bloc de méthode change. Les blocs
  `body:*` existants restent, comme dans Bruno. Aucun bloc n'est créé tant
  que le contenu est vide (Bruno n'écrit pas un corps vide).

## 7. Contenu du corps

| Mode | Édition |
|---|---|
| `json`, `text`, `xml`, `sparql`, `graphql` | déjà en place (phase 1) |
| `formUrlEncoded` | clé / valeur, mêmes touches que les en-têtes (`a`, `d`, `c`, `Espace`, `Entrée`) |
| `multipartForm` | champs texte : mêmes touches ; champs `@file(...)` : `Espace` et `d` seulement |
| `file` | lecture seule |
| `body:graphql:vars` | texte, comme le corps JSON |

- Annotations (`@contentType(...)`, préfixes `@...`) conservées à l'octet près.
- Bloc absent : créé à la place où Bruno l'écrit ; dernière entrée supprimée :
  bloc retiré.

## 8. Tests et critères d'acceptation

1. Pour chaque opération : un test d'octets avant / après sur une fixture de
   `tests/fixtures/collections/writer-cases/`, et un test de relecture par
   `@usebruno/lang` (`tests/writer_bruno_lang.rs`).
2. Changement de mode d'auth : vérifier le fichier produit contre celui que
   produit `jsonToBru.js` pour le même JSON (script node du test existant).
3. Secrets : tests de la détection, du popup (3 issues), et qu'aucune valeur
   saisie n'apparaît dans un log, un statut ou un fichier après « Remplacer ».
4. Dossier et collection : au moins un cas d'auth pour chacun.
5. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
6. Essai réel dans le TUI, puis `bru run` sur le fichier modifié.

## 9. Découpage prévu

Deux lots en parallèle (fichiers distincts depuis la découpe de `update.rs`) :

- **Lot auth** : § 3, 4, 5 (`writer`, `update/edit.rs`, vue Auth).
- **Lot corps** : § 6, 7 (`writer`, `update/edit.rs`, vue Corps).

`update/edit.rs` reste commun : le lot qui finit second intègre `main`.
