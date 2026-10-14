# Spécification — Édition, phase 3 : fichiers

Statut : **approuvée par Nicolas le 2026-10-14**.
Décisions :

1. Suppression vers la corbeille, jamais d'effacement définitif
   (décision du 2026-10-12, `docs/edition.md`).
2. L'édition passe par des popups (`docs/edition-popups.md`).
3. Choix par défaut acceptés le 2026-10-14 : touches du § 3, case
   « Renommer aussi le fichier » cochée par défaut, déplacement vers un
   autre dossier remis à une phase suivante.

Suite de `docs/edition.md` et `docs/edition-phase2.md`. Leurs principes
s'appliquent : fraîcheur (`FileStamp`), aucune interprétation, validation par
`@usebruno/lang` dans les tests. Une fois approuvée, cette spec lève pour la
phase 3 seulement la règle de `CLAUDE.md` sur l'écriture des fichiers `.bru`.

## 1. Périmètre

| Opération | Requête | Dossier |
|---|---|---|
| Créer | oui | oui |
| Renommer | oui | oui |
| Cloner | oui | oui |
| Réordonner (`seq`) | oui | oui |
| Mettre à la corbeille | oui | oui |

Hors phase 3 :

- Déplacer vers un autre dossier (glisser-déposer de Bruno) : phase suivante.
- Collections YAML (OpenCollection) : toujours en lecture seule.
- `bruno.json`, `collection.bru`, `folder.bru` seuls, environnements :
  ni renommés, ni clonés, ni supprimés par ces opérations.
- Créer une requête gRPC ou WebSocket (lecture seule dans le TUI).
- Créer, renommer ou supprimer une collection, ou modifier `workspace.yml`.

## 2. Comportement de Bruno (référence)

Relevé dans Bruno desktop 4.2.1 (`app.asar`, gestionnaires `ipcMain`) :

- **Noms de fichier** (`validateName`) : 1 à 255 caractères ; ni
  `< > : " / \ | ? *` ni caractère de contrôle ; pas d'espace ni de `-`
  en tête ; pas d'espace ni de `.` en fin ; pas de nom réservé Windows
  (`CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9`).
- **Nom de fichier proposé** (`sanitizeName`) : à partir du nom affiché,
  caractères interdits remplacés par `-`, espaces et `-` de tête retirés,
  points et espaces de fin retirés.
- **Collision** : Bruno ajoute ` 1`, ` 2`… au nom (`writeFileUnique`,
  `mkdirUnique`, `getUniqueRenamePath`), sans erreur.
- **Créer une requête** : fichier écrit par `jsonToBru` depuis la requête
  par défaut de Bruno.
- **Créer un dossier** : dossier + `folder.bru` avec `meta { name, seq }`.
- **Renommer** :
  - le nom affiché (`meta.name` de la requête, ou de `folder.bru`, créé
    s'il manque) ;
  - et, si demandé, le nom du fichier ou du dossier.
- **Cloner** : copie avec un nouveau nom ; un dossier est copié récursivement.
- **Réordonner** (`resequence-items`) : nouvelles valeurs de `seq` pour les
  frères concernés ; un fichier dont le `seq` ne change pas n'est pas écrit.
- **Supprimer** : Bruno efface définitivement (`fs.rmSync`, `unlinkSync`).
  Le TUI, lui, met à la corbeille (décision 1).

Les valeurs exactes non relevées ici (contenu de la requête par défaut,
`seq` d'un clone) sont à relever dans `app.asar` au moment de coder, et à
figer dans une fixture. Rien n'est deviné.

## 3. Touches (dans l'arbre)

| Touche | Effet |
|---|---|
| `a` | nouvelle requête, dans le dossier sélectionné (ou celui de la requête) |
| `A` | nouveau dossier, au même endroit |
| `F2` | renommer la ligne sélectionnée |
| `p` | cloner la ligne sélectionnée |
| `Maj+↑` / `Maj+↓` | monter / descendre la ligne parmi ses frères |
| `d` | mettre à la corbeille, après confirmation |

- Ces touches sont libres dans l'arbre aujourd'hui (`a` et `d` ne servent que
  dans le détail et les panneaux). Vérifier les conflits avant de coder ;
  si `Maj+↑` / `Maj+↓` sont pris, utiliser `K` / `J`.
- Sur l'en-tête d'une collection : `a` et `A` créent à la racine ; les autres
  touches sont refusées avec un message.

## 4. Popups

Même modèle que les popups d'édition : `↑↓`, `Tab`, `Entrée`, `Échap`.
Ici, `Entrée` sur la dernière ligne (ou `Ctrl+S`) **crée** ou **applique**.

### 4.1 Nouvelle requête

```
╭ Nouvelle requête — dans Posts ───────────────╮
│ Nom        Lire un article                    │
│ Fichier    Lire un article.bru                │
│ Type       ‹ http ›                           │
│ Méthode    ‹ GET ›                            │
│ URL        {{base_url}}/posts/1               │
│                                               │
│ Ctrl+S créer   Échap annuler                  │
╰───────────────────────────────────────────────╯
```

- `Fichier` suit le `Nom` (`sanitizeName`) tant qu'on ne l'a pas modifié.
- `Type` : `http` ou `graphql`.
- Nom ou fichier invalide : refus avec le message de Bruno, en français.
- Collision : suffixe ` 1`, ` 2`… comme Bruno ; le statut indique le nom final.
- `seq` : celui que Bruno donnerait (relevé dans `app.asar`).
- Après création : la nouvelle requête est sélectionnée.

### 4.2 Nouveau dossier

Popup avec `Nom` et `Dossier` (nom du répertoire, suit le nom).

### 4.3 Renommer

```
╭ Renommer — get-post.bru ─────────────────────╮
│ Nom        Lire un article                    │
│ Fichier    get-post.bru                       │
│ [x] Renommer aussi le fichier                 │
╰───────────────────────────────────────────────╯
```

- La case est cochée par défaut ; décochée, seul `meta.name` change.
- `Espace` coche ou décoche.
- Pour un dossier : `meta.name` de `folder.bru` (créé s'il manque, avec le
  contenu que Bruno écrit) et, si coché, le nom du répertoire.
- `meta.name` est réécrit de façon chirurgicale (seule la ligne `name:` change).

### 4.4 Cloner

Popup comme 4.3, pré-rempli avec « <nom> copie ». La copie est identique à
l'octet près, sauf `meta.name` (et `seq` si Bruno le change).

### 4.5 Mettre à la corbeille

Confirmation dans la barre de statut :
« Mettre « Lire un article » à la corbeille ? (y/n) ».
Pour un dossier, elle donne le nombre de requêtes qu'il contient.

## 5. Réordonner

- `Maj+↑` / `Maj+↓` échange la ligne avec sa voisine, parmi ses frères.
- Les `seq` sont réécrits comme Bruno : seuls les fichiers dont le `seq`
  change sont écrits (pour un dossier : `folder.bru`, créé s'il manque).
- La sélection suit la ligne déplacée.

## 6. Corbeille

Spécification freedesktop.org (« Trash ») sous Linux :

- corbeille de l'utilisateur : `$XDG_DATA_HOME/Trash` (par défaut
  `~/.local/share/Trash`), avec `files/` et `info/<nom>.trashinfo`
  (`Path=` encodé en URL, `DeletionDate=` en heure locale) ;
- nom déjà pris dans la corbeille : suffixe `.2`, `.3`… ;
- fichier sur un autre système de fichiers que la corbeille : **refus**, avec
  le message « Impossible de mettre à la corbeille : autre disque ». Jamais
  d'effacement définitif en repli.
- Écrit dans le TUI, sans nouvelle dépendance.

## 7. Règles communes

- Collection YAML : toutes ces opérations sont refusées (lecture seule).
- Pendant une exécution `bru run` de la collection : refusées, avec un message.
- Toutes les écritures restent dans la collection (pas de `..`, pas de lien
  symbolique qui sort de la racine), comme `validatePathIsInsideCollection`.
- Après chaque opération, l'arbre est rechargé et la sélection suit l'élément.
- Une erreur d'écriture laisse le disque dans un état propre : rien de créé à
  moitié (écriture dans un fichier temporaire, puis renommage).

## 8. Tests et critères d'acceptation

1. Validation et nettoyage des noms : mêmes cas que Bruno.
2. Créer, renommer, cloner, réordonner : octets avant / après sur des
   fixtures de `tests/fixtures/collections/writer-cases/` ; contenu de la
   nouvelle requête identique à celui de `jsonToBru` pour la requête par
   défaut de Bruno ; relecture par `@usebruno/lang`.
3. Collisions : suffixes ` 1`, ` 2`.
4. Corbeille : dans un `XDG_DATA_HOME` temporaire ; fichier `.trashinfo`
   valide ; collision `.2` ; refus sur un autre système de fichiers ;
   jamais d'effacement.
5. Refus : collection YAML, exécution en cours, en-tête de collection.
6. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
7. Essai réel en release sur une copie de `examples/`, puis `bru run`.
