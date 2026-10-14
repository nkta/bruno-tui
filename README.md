# bruno-tui

TUI en Rust pour explorer et éditer des collections [Bruno](https://www.usebruno.com/)
(fichiers `.bru`, ou OpenCollection YAML en lecture seule) dans le terminal. L'exécution d'une requête est toujours
déléguée au vrai CLI `bru` : ce projet n'interprète ni les variables, ni les
scripts, ni les tests — seulement l'affichage et l'édition des fichiers.

## Lancer l'application

```
bruno-tui [OPTIONS] [CHEMIN...]
```

`CHEMIN` : une collection, ou un fichier/dossier qu'elle contient (par
défaut, le répertoire courant). Un dossier contenant `workspace.yml`, ou
plusieurs chemins, ouvrent plusieurs collections ensemble : voir
[Workspace](#workspace-plusieurs-collections).

Options :

| Option | Effet |
|---|---|
| `--secret NOM[=CLÉ]` | variable secrète transmise à `bru` (répétable). Cherchée dans le `.env` de la collection puis le shell, sous `CLÉ` ou `NOM`/`NOM_EN_MAJUSCULES` ; une saisie dans le panneau Secrets prime. Jamais de valeur en clair sur la ligne de commande. |
| `--editor COMMANDE` | commande d'éditeur externe pour l'édition de texte (scripts, tests, docs) et l'ouverture du corps (priorité sur `VISUAL` et `EDITOR`, repli sur `nano`) |
| `--no-mouse` | n'active pas la capture souris (la sélection native du terminal reste disponible) |
| `--no-watch` | désactive la surveillance et le rechargement automatique de la collection |
| `--no-history` | désactive la persistance de l'historique des exécutions dans le fichier d'état |
| `-h`, `--help` | aide |
| `-V`, `--version` | version |

## Formats de collection

Le format est détecté comme dans Bruno 4, sur le premier répertoire
parent qui marque une collection :

| Fichier à la racine | Format | Édition |
|---|---|---|
| `opencollection.yml` | OpenCollection YAML (`*.yml`, `folder.yml`, `environments/*.yml`) | lecture seule |
| `bruno.json` (sans `opencollection.yml`) | bru-lang (`*.bru`) | oui |

Si les deux fichiers sont présents, `opencollection.yml` l'emporte.

Pour une collection YAML :

- l'arbre, le détail et les environnements s'affichent comme pour une
  collection `.bru` ; le titre indique « OpenCollection YAML · lecture
  seule » ;
- l'exécution reste déléguée à `bru run` (chemins `.yml` relatifs à la
  racine) et les résultats du rapport sont rattachés aux requêtes ;
- l'édition est désactivée : `Entrée`/`e`/`m` dans le détail, `e` dans le
  panneau Environnement et `Ctrl+S` affichent « Collection OpenCollection
  (YAML) en lecture seule » ;
- les requêtes `http`, `graphql`, `grpc` et `websocket` sont affichées
  (voir [Requêtes gRPC et WebSocket](#requêtes-grpc-et-websocket)) ; les
  autres types (`script`, `app`, ...) et les fichiers invalides
  apparaissent en erreur dans l'arbre ;
- les clés que le TUI ne sait pas afficher sont ignorées et listées comme
  avis dans le panneau Diagnostics (`D`), sans jamais citer de valeur ;
- le nom d'un environnement est son nom de fichier sans extension, celui
  qu'attend `bru run --env`.

## Workspace (plusieurs collections)

| Commande | Résultat |
|---|---|
| `bruno-tui <dossier avec workspace.yml>` | toutes les collections du workspace |
| `bruno-tui A B C` | workspace sans nom ; chaque chemin est la racine d'une collection |
| `bruno-tui <collection>` ou `bruno-tui` | une seule collection, comme avant |

`workspace.yml` (format de Bruno 4.2.1) est lu, jamais écrit. Seuls
`info.name` et `collections[].name` / `path` sont lus ; `path` est
relatif au dossier du workspace ou absolu. Un `workspace.yml` illisible
ou invalide arrête le lancement avec un message clair.

Exemple prêt à l'emploi : `bruno-tui examples/workspace` ouvre les
collections `examples/jsonplaceholder` et `examples/httpbin`. Pour
retrouver ses propres collections à chaque lancement, créer un
`workspace.yml` qui les liste, puis un alias
(`alias bt='bruno-tui ~/api/mon-workspace'`).

**Arbre**

- Une ligne d'en-tête par collection : `▸ Clients`.
- `→` / `←` (ou un clic sur l'en-tête sélectionné) la déplient ou la
  replient. Repliées au démarrage, sauf s'il n'y en a qu'une.
- Un chemin absent ou qui n'est pas une collection donne une ligne
  d'erreur `✗ Nom — raison` ; les autres collections se chargent.
- Titre du panneau : `Workspace : <nom>`, ou `Collections` sans nom.

**Collection active**

- C'est celle de la ligne sélectionnée (en-tête compris) ; son nom est
  souligné.
- Elle détermine : Environnement (son nom dans le titre du panneau),
  Secrets, Historique, Campagne, Diagnostics, `Ctrl+K`, `f`, `/`, `c`
  et `r`.
- Chaque collection garde son état : environnement choisi, secrets
  saisis, résultats, historique, dossiers dépliés.
- `r` sur un en-tête lance la collection entière.

**Exécution**

- `bru run` part de la racine de la collection active.
- Avec un `workspace.yml`, `--workspace-path` vaut par défaut le dossier
  du workspace (modifiable avec `O`). Il est transmis aux campagnes et à
  l'exécution d'une requête seule (`r`), seule option du runner qui
  s'applique à une requête seule.
- Pendant une exécution, la sélection ne peut pas quitter la
  collection : un message le rappelle dans la barre d'état.

**Surveillance** : un watcher par collection ; un rechargement ne touche
que la sienne.

**Hors périmètre** : modifier `workspace.yml`, `.env` et mocks du
workspace, recherche, filtre et campagne sur plusieurs collections à la
fois.

## Vue d'ensemble

L'écran est divisé en panneaux : **Collection** (arbre), **Détail** (requête
sélectionnée), **Environnement** (toujours visible), **Statut** et
**Réponse**. `Tab` fait tourner le focus entre Collection, Détail et
Réponse ; `Échap` y ramène directement depuis n'importe où. `q` quitte
(Collection/Détail), `Ctrl+C` quitte en toutes circonstances — un popup
d'édition non enregistré demande confirmation.

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | naviguer |
| `→`/`l`, `←`/`h` | déplier/replier un dossier (Collection) |
| `Début`/`g`, `Fin`/`G` | début, fin de la liste |
| `PageUp`/`PageDown` | page précédente/suivante |
| `r` | lancer la requête (ou récursivement un dossier / la collection) ; `Ctrl+X` annule ; refusé pour gRPC/WebSocket |
| `c` | sélectionner la collection racine (affiche `collection.bru` dans le panneau Détail) ; clic en-tête Collection |
| `f` | filtrer l'arbre de la collection (en direct ; Entrée valide, Échap annule ou efface) |
| `Ctrl+K` | recherche globale par nom, méthode, chemin et URL (popup centré en direct ; ↑↓ choisir, Entrée valide, Échap ferme) |
| `M` | activer/désactiver la capture souris |
| `D` | panneau Diagnostics (erreurs de chargement, avis sur les champs YAML ignorés) |
| `H` | panneau Historique des exécutions |
| `S` | panneau Variables secrètes |
| `?` | aide des raccourcis (popup centré) |
| `C` | panneau Campagne (bilan TNR de la dernière exécution récursive) |
| `O` | options de campagne (environnements, tags, TLS, données, sandbox, verbose, rapports JUnit/HTML) |
| `t` | panneau Campagne : liste / tableau comparatif multi-environnements |
| `]` / `[` | requête en échec suivante / précédente (Collection, Détail, Réponse) |
| `z` | basculer le mode plein écran du panneau focalisé (Collection, Détail, Réponse) |
| `a` / `A` | nouvelle requête / nouveau dossier (Collection ; voir « Fichiers et dossiers ») |
| `F2` | renommer la ligne (Collection) |
| `p` | cloner la ligne (Collection) |
| `Maj+↑` / `Maj+↓` | monter / descendre la ligne parmi ses frères (Collection) |
| `m` | déplacer la ligne vers un autre dossier (Collection ; dans le Détail, `m` ouvre le popup Requête) |
| `d` | mettre la ligne à la corbeille, après confirmation (Collection) |

La racine de la collection est surveillée en continu : dès qu'un fichier
`.bru` ou `.yml` (OpenCollection), `bruno.json`, `collection.bru`, `folder.bru` ou un fichier du dossier
`environments/` est créé, modifié, supprimé ou renommé, la collection est
automatiquement rechargée en préservant la sélection (par chemin), les dossiers
dépliés, le focus et l'environnement actif (l'option `--no-watch` permet de
désactiver cette surveillance). Si un popup d'édition comporte des
modifications non enregistrées, le rechargement est différé après sa fermeture
et un avertissement signale la modification sur disque.

## Requêtes gRPC et WebSocket

Les requêtes gRPC (`type: grpc`) et WebSocket (`type: ws` en `.bru`,
`websocket` en YAML) de Bruno 4 sont affichées **en lecture seule** :

- l'arbre et la barre d'URL montrent `GRPC` ou `WS` à la place de la
  méthode HTTP ;
- le Détail réutilise les onglets existants :
  - **Params** : boîte « Appel gRPC » (méthode RPC, type, fichier proto) ;
  - **En-têtes** : « Métadonnées » pour gRPC (bloc `metadata`) ;
  - **Corps** : « Messages », chaque message précédé de `▸ nom (type,
    sélectionné)` ;
  - **Script** : les quatre scripts `grpc:*` pour gRPC
    (`before-call-start`, `before-message-send`, `after-message-receive`,
    `after-call-end`) ;
  - **Auth**, **Tests**, **Docs**, **Vars**, **Settings** : comme en HTTP ;
- l'édition est refusée (`Entrée`, `e`, `m`, clic) : « Requête gRPC en
  lecture seule : l'édition ne gère que les requêtes HTTP » ;
- `r` sur une telle requête n'appelle pas `bru` et affiche « Requête gRPC :
  type non exécutable par bru run ».

`bru run` 4.2.1 ne sait pas les exécuter : il les envoie comme des requêtes
HTTP et échoue aussitôt (`Unsupported protocol grpc:` ou `ws:`), sans
lancer leurs tests. Une campagne récursive (dossier, `C`, popup `O`) reste
déléguée à `bru run` : ces requêtes y apparaissent en erreur avec ce
message, et les autres requêtes s'exécutent normalement (fixtures
`tests/fixtures/reports/grpc-ws.json` et `oc-grpc-ws.json`).

## Panneau Détail

Affiche la requête sélectionnée selon dix onglets, comme Bruno bureau :
**Params**, **Corps**, **En-têtes**, **Auth**, **Vars**, **Script**,
**Assert**, **Tests**, **Docs** et **Settings**. Les onglets vides sont
atténués ; une barre d'onglets adaptative fait défiler les onglets sur les
écrans étroits.

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler dans le détail |
| `Début`/`g`, `Fin`/`G` | début, fin du détail |
| `←`/`h`, `→`/`l` | onglet précédent/suivant (requête, dossier ou collection) |
| `Entrée` / `e` | ouvrir le popup d'édition de l'onglet affiché (requêtes HTTP, dossiers, collection ; refusé pour gRPC/WebSocket) |
| `m` | ouvrir le popup Requête (méthode et URL), depuis n'importe quel onglet |
| `/` | chercher dans l'onglet affiché ; `n`/`N` : occurrence suivante/précédente |
| `v` | sélection visuelle de lignes ; `y` copie dans le presse-papiers |
| `Clic` | cliquer sur un onglet pour l'afficher ; sur un champ, ouvrir son popup d'édition |

Le détail est en lecture seule : l'édition passe par les popups (voir
[Édition par popups](#édition-par-popups)). Les onglets Script et Tests
affichent le code brut avec numérotation de lignes, aussi bien pour les
fichiers `.bru` que pour les collections OpenCollection YAML.

### Contenu des dossiers et de la collection racine

- **Dossiers (`folder.bru` / `folder.yml`)** : quand un dossier est sélectionné dans l'arbre,
  le panneau Détail affiche ses métadonnées dans six onglets dédiés (**En-têtes**, **Auth**,
  **Vars**, **Script**, **Tests**, **Docs**).
- **Collection racine (`collection.bru` / `opencollection.yml`)** : la touche `c` dans l'arbre
  (ou un clic sur la bordure d'en-tête du panneau Collection) sélectionne la racine de la
  collection pour afficher ses déclarations dans ces mêmes onglets. Appuyer sur `r` lorsque la
  collection est sélectionnée lance toute la collection de manière récursive.
- **Aucune résolution d'héritage** : le TUI n'affiche jamais d'authentification effective ni
  d'en-têtes fusionnés. Chaque niveau montre strictement ce qu'il déclare lui-même (signalé par
  la mention « sans héritage » dans l'en-tête du panneau).
- **Masquage d'authentification** : les valeurs littérales sensibles (mots de passe, tokens,
  secrets, clés d'API) sont masquées (`********`) dans l'onglet Auth, tandis que les références
  de variables (`{{variable}}`) restent visibles en clair.
- **Édition** : pour les collections Bruno (`.bru`), les dossiers (`folder.bru`) et la
  racine (`collection.bru`) s'éditent par les popups de leurs onglets (`Entrée` / `e`) :
  en-têtes, authentification, variables, scripts, tests, documentation. Ils n'ont ni popup
  Requête, ni Params, ni Corps. Les collections OpenCollection (YAML), gRPC et WebSocket
  restent strictement en lecture seule.


## Fichiers et dossiers

Depuis l'arbre (focus Collection), on crée, renomme, clone, réordonne,
déplace et met à la corbeille les requêtes et les dossiers, comme dans
Bruno 4.2.1 (spécifications : `docs/edition-phase3.md`,
`docs/edition-phase4.md` pour le déplacement).

| Touche | Effet |
|---|---|
| `a` | nouvelle requête, dans le dossier sélectionné (ou celui de la requête) ; sur l'en-tête de la collection, à la racine |
| `A` | nouveau dossier, au même endroit |
| `F2` | renommer la ligne : nom affiché (`meta.name`) et, case cochée, le fichier ou le dossier |
| `p` | cloner la ligne, pré-remplie avec « <nom> copie » ; un dossier est copié en entier |
| `Maj+↑` / `Maj+↓` | monter / descendre la ligne parmi ses frères |
| `m` | déplacer la ligne vers un autre dossier de la même collection (popup « Déplacer vers ») |
| `d` | mettre à la corbeille, après confirmation `y`/`n` (le nombre de requêtes d'un dossier est indiqué) |

Les popups suivent le modèle des popups d'édition :

| Touche | Effet |
|---|---|
| `↑`/`↓`, `Tab`/`Maj+Tab` | ligne précédente, suivante ; la frappe va au champ sous le curseur |
| `←`/`→`, `Espace` | changer le type (‹ http › / ‹ graphql ›), la méthode ; cocher ou décocher la case |
| `Entrée` | ligne suivante ; sur la dernière ligne, créer ou appliquer |
| `Ctrl+S` | créer ou appliquer |
| `Échap` | fermer sans rien écrire |

Règles, reprises de Bruno :

- Le nom de fichier suit le nom (`sanitizeName`) tant qu'il n'a pas été
  saisi. Un nom refusé par `validateName` (caractères `< > : " / \ | ? *`,
  espace ou `-` en tête, espace ou `.` en fin, noms réservés Windows) est
  signalé dans le popup avec le message de Bruno ; `collection` et `folder`
  sont réservés pour une requête.
- Collision : suffixe ` 1`, ` 2`… ; le statut donne le nom final.
- Nouvelle requête : contenu identique à celui que Bruno écrit
  (`jsonToBru`), `seq` = nombre d'éléments du dossier + 1. Les paramètres
  de l'URL (`?…`, `:id`, segments OData `Items(id=:key)`) sont écrits dans
  `params:query` et `params:path`, comme Bruno. Une URL dont Bruno tirerait
  un résultat incertain (identifiants `user@`, IPv6, hôte non ASCII,
  `\`, segments `.`/`..`) ou un nom de paramètre à guillemets (`:`, `"`,
  `{`, `}`, espace) est refusée : la créer sans, puis compléter l'URL avec
  le popup Requête (`m`).
- Renommer, cloner, réordonner : seules les lignes `name:` et `seq:` de
  `meta` changent, le reste du fichier est conservé à l'octet près. Un
  dossier sans `folder.bru` en reçoit un, avec le contenu que Bruno écrit.
- Corbeille freedesktop (`$XDG_DATA_HOME/Trash`, par défaut
  `~/.local/share/Trash`) : jamais d'effacement définitif. Un élément situé
  sur un autre disque que la corbeille est refusé. Après la mise à la
  corbeille, les `seq` des frères restants sont renumérotés 1, 2, 3… comme
  Bruno ; seuls les fichiers dont le `seq` change sont écrits.
- Ordre pour `Maj+↑`/`Maj+↓` et la renumérotation : celui de Bruno
  (`sortItems` : `seq` d'abord, noms pour les `seq` absents ou en double).
  Si cet ordre dépend de noms que le TUI ne compare pas comme Bruno (hors
  lettres et chiffres ASCII, espace, `_`, `-`, `.`), ou de deux noms
  identiques, rien n'est renuméroté et le statut le dit.
- Refusé : collection YAML (lecture seule), exécution en cours, et, sur
  l'en-tête de la collection, tout sauf `a` et `A`. Aucune écriture hors de
  la collection, aucun fichier modifié sur disque depuis son chargement
  n'est réécrit.
- Après l'opération, l'arbre est rechargé et la sélection suit l'élément
  créé, renommé, cloné ou déplacé (en workspace, dans la collection active).

### Déplacer (`m`)

`m` dans l'arbre ouvre « Déplacer vers » : la racine de la collection, puis
tous ses dossiers en arbre. Dans le Détail, `m` reste le popup Requête.

| Touche | Effet |
|---|---|
| saisie | filtrer les dossiers (sous-chaîne, sans casse, sur le nom ou le chemin) |
| `↑`/`↓`, `Tab`/`Maj+Tab` | destination précédente, suivante |
| `Entrée` | déplacer |
| `Échap` | fermer sans rien déplacer |

- Masqués : le dossier actuel de l'élément (Bruno ne fait rien), l'élément
  lui-même et, pour un dossier, ses descendants.
- Un seul `rename`, jamais de copie suivie d'une suppression. Contenu,
  `folder.bru` et `seq` de l'élément restent identiques à l'octet (comme
  Bruno : le `seq` n'est pas réécrit dans le dossier cible ; `Maj+↑`/`Maj+↓`
  règle ensuite la position).
- Nom déjà pris dans le dossier cible : suffixe ` 1`, ` 2`… comme Bruno
  (`get-post 1.bru`, `Posts 1`). Le popup l'annonce (« Sera nommé : … ») et
  le statut donne le nom final ; `meta.name` ne change pas. Rien n'est écrasé.
- Les frères restants du dossier source sont renumérotés comme après une
  corbeille (mêmes limites que ci-dessus, le statut le dit).
- Refusé, avec un message dans le popup et sans rien déplacer : fichier
  modifié sur disque depuis son chargement, autre disque, lien symbolique
  dans l'élément ou dossier cible. Refusé avant l'ouverture : collection
  YAML, exécution en cours, en-tête de la collection, fichier illisible.
- Pas de déplacement entre collections d'un workspace : la collection
  active seulement.

## Édition par popups

Le panneau Détail est en lecture seule. Chaque onglet s'édite dans un
popup centré, par-dessus l'écran, comme le popup d'environnement :

- `Entrée` (ou `e`) dans le détail ouvre le popup de l'**onglet affiché** ;
- `m` ouvre le popup **Requête** (méthode et URL), depuis n'importe quel onglet ;
- un clic sur un champ du détail ou de la barre d'URL ouvre son popup.

Un popup édite un seul fichier : la requête, le `folder.bru` ou le
`collection.bru` sélectionné. Ses modifications restent dans le popup :
rien n'est écrit sur disque avant `Ctrl+S`.

| Popup | Contenu |
|---|---|
| Requête (`m`) | Méthode ‹ GET › ; URL |
| Params | Paramètres de requête ; Paramètres de chemin |
| En-têtes | tableau Clé / Valeur |
| Auth | Mode ‹ bearer › ; paramètres du mode |
| Corps | Mode ‹ json › ; contenu du mode |
| Vars | Pré-requête ; Post-réponse |
| Assert | tableau (clé = expression, valeur = `eq 200`) |
| Settings | valeurs seulement |
| Script | Pré-requête ; Post-réponse |
| Tests, Docs | aperçu du bloc |

Touches communes :

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | ligne précédente, suivante |
| `Tab`, `Maj+Tab` | partie suivante, précédente (ex. pré-requête / post-réponse) |
| `Début`/`g`, `Fin`/`G` | première, dernière ligne du popup |
| `Entrée` | modifier la valeur ; sur un choix, ouvrir sa liste (`↑↓`, `Entrée`, `Échap`) ; sur un script, des tests ou la doc, ouvrir l'éditeur externe |
| `←`/`h`, `→`/`l` | sur un choix (méthode, mode, placement) : valeur précédente, suivante |
| `Ctrl+S` | enregistrer, puis fermer le popup |
| `Échap` | annuler la saisie en cours ; sinon fermer le popup |

Tableaux clé / valeur (en-têtes, paramètres, vars, assert, formulaires
du corps) :

| Touche | Effet |
|---|---|
| `Entrée` | modifier la valeur |
| `c` | renommer la clé (hors `@file(...)`) |
| `a` | ajouter une ligne (clé, puis valeur) ; une partie vide affiche « aucune ligne · a ajouter » |
| `d` | supprimer la ligne, sans confirmation |
| `Espace` | activer / désactiver la ligne |

- Une ligne désactivée s'affiche grisée, avec « (désactivé) ».
- Fermer avec des changements non enregistrés demande « Abandonner les
  modifications ? (y/n) » ; le titre du popup affiche alors
  « ● non enregistré ».
- La dernière ligne du popup rappelle les touches utiles à l'endroit du
  curseur ; la barre de statut les répète.
- Un clic sur une ligne la sélectionne (une saisie en cours est d'abord
  validée) ; un clic hors du popup ne fait rien. La molette fait défiler
  le popup.
- Tant que le popup est ouvert, il garde le clavier : ni l'arbre, ni les
  autres panneaux ne réagissent.

Saisie : `Entrée` ou `Tab` valide, `Échap` annule, `Ctrl+S` valide puis
enregistre ; pendant la saisie de la clé d'une nouvelle entrée, `Ctrl+S`
l'ajoute avec une valeur vide. Dans une zone de texte (corps, variables
GraphQL), `Entrée` passe à la ligne et `Tab` sort de la zone. Toute
lettre tapée en saisie est du texte.

### Édition de l'authentification et protection des secrets

- **Popup Auth** : ligne Mode ‹ bearer ›, puis les paramètres du mode. `←→` sur le mode le change et remplace aussitôt les lignes par celles du nouveau mode (clés vides) ; revenir au mode d'origine avant `Ctrl+S` retrouve les anciennes valeurs.
- **Auth non déclarée** : sur une requête, un dossier ou la collection sans mode d'auth, le popup affiche ‹ none › (non déclarée), comme le détail ; rien n'est écrit tant qu'un autre mode n'est pas choisi.
- **Modes proposés** : `none`, `inherit` (pas pour la collection), `basic`, `bearer`, `digest`, `ntlm`, `apikey`, `awsv4`, `wsse`. Pour `apikey`, le placement est un choix ‹ header › / ‹ queryparams ›. OAuth 1, OAuth 2 et Akamai : seules les valeurs des clés existantes se modifient ; si le fichier utilise l'un d'eux, il est proposé en tête de la liste et du cycle `←→`, pour y revenir avant `Ctrl+S` et retrouver ses valeurs.
- **Masquage et `Ctrl+R`** : les clés sensibles (tokens, mots de passe, clés secrètes) sont masquées par des puces `•` lors de la saisie (les références `{{variable}}` restent visibles). `Ctrl+R` bascule l'affichage en clair ou masqué.
- **Détection des secrets en clair** : valider une valeur littérale sur une clé sensible sans variable `{{...}}` ouvre la question dans le bas du popup Auth :
  - `[Entrée]` **Remplacer** par la variable suggérée en camelCase (ex. `{{bearerToken}}`, `{{basicPassword}}`) : le secret en clair est immédiatement oublié (jamais écrit sur disque, jamais loggué), et la barre de statut rappelle la configuration `.env` ou `--secret`.
  - `[g]` **Garder** la valeur en clair.
  - `[Échap]` **Revenir** au mode de saisie.
- **Confirmation au Ctrl+S** : changer de mode d'authentification retire l'ancien bloc `auth:<ancien>`. Si celui-ci contenait des valeurs non vides, `Ctrl+S` demande confirmation (`y`/`n`) avant d'écrire sur le disque.

### Édition externe des blocs de texte

Dans les popups Script, Tests et Docs, `Entrée` sur un bloc suspend le TUI et ouvre le
texte dans un éditeur externe. À sa fermeture, on revient au popup, qui montre un aperçu
des premières lignes ; `Ctrl+S` enregistre.

- **Choix de l'éditeur** : `--editor COMMANDE` prioritaire, puis variable `VISUAL`, puis `EDITOR`, avec repli sur `nano`.
- **Fichier temporaire sécurisé** : créé dans `$XDG_RUNTIME_DIR` (ou le dossier temporaire système) avec des droits stricts `0600` et une extension adaptée (`.js` pour les scripts/tests, `.md` pour la documentation). Le fichier est systématiquement supprimé dès la fermeture de l'éditeur.
- **Prise en compte des modifications** : le texte relu devient une modification en attente dans le popup. Si le texte est vidé, le bloc correspondant est retiré à l'enregistrement.
- **Formatage Bruno** : lors de la sauvegarde (`Ctrl+S`), le contenu est réindenté à 2 espaces conformément au format Bruno.

### Mode et contenu du corps

- **Mode du corps** : ligne Mode ‹ json › en tête du popup Corps (`none`, `json`, `text`, `xml`, `sparql`, `formUrlEncoded`, `multipartForm`, `file`), changée par `←→` ou par sa liste (`Entrée`). Pour les requêtes GraphQL (`type: graphql`), le mode est fixe. Seule la clé `body:` du bloc méthode change au `Ctrl+S` ; les blocs de corps existants sont préservés à l'octet près.
- **Formulaires** : `body:form-urlencoded` et `body:multipart-form` s'éditent clé/valeur (`a`, `d`, `c`, `Espace`, `Entrée`). Les champs `@file(...)` de multipart sont en lecture seule (seuls `Espace` et `d` suppression sont autorisés). Les annotations `@contentType(...)` et préfixes `@...` sont préservés à l'octet près.
- **Texte** (`json`, `text`, `xml`, `sparql`, `graphql`) : zone multiligne dans le popup. `Entrée` passe à la ligne ; `Tab` sort de la zone.
- **GraphQL** : deux parties, Requête et Variables (`body:graphql:vars`), éditées dans le popup.
- **Création et suppression** : un bloc absent est créé à sa position canonique définie par Bruno (`jsonToBru.js`), et un bloc vidé est automatiquement retiré à la sauvegarde.
- **Corps fichier** : `body:file` est en lecture seule.

Comme dans Bruno, les paramètres de requête et l'URL restent cohérents :
modifier un paramètre reconstruit la chaîne de requête de l'URL à partir
des paramètres activés, et modifier l'URL recalcule les paramètres activés
(les paramètres désactivés restent à leur place). Une clé vide, contenant
un espace ou `:`, ou commençant par `~` ou `"` est refusée (les clés
d'assertion acceptent les espaces, ex. `res.status`) ; les réglages
(`settings`) n'acceptent que la modification de valeur ; dans un
paramètre de requête, `&`, `#` et le saut de ligne sont refusés (ainsi que
`=` dans la clé).

À la souris (sauf `--no-mouse`) : cliquer sur l'URL, un en-tête ou un
paramètre du détail ouvre son popup et démarre sa saisie (un script, des
tests ou la doc sont seulement sélectionnés). Glisser dans le détail ou
la réponse sélectionne des lignes (`v` bascule aussi une sélection au
clavier), `y` copie la sélection dans le presse-papiers.

## Panneau Environnement

Toujours visible, en haut à droite (au-dessus du panneau Statut) — comme
les autres panneaux, il n'a besoin d'aucune touche pour apparaître.

| Touche | Effet |
|---|---|
| `E` | donner le focus au panneau |
| `↑`/`k`, `↓`/`j` | naviguer dans la liste des environnements |
| `Entrée`/`→` | activer l'environnement sous le curseur (« Aucun » désactive) |
| `e` | ouvrir le popup d'édition de ses variables |

Un double-clic sur une entrée du panneau fait les deux à la fois : active
l'environnement et ouvre le popup.

Dans le popup (tableau Clé/Valeur, centré) :

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | variable précédente, suivante |
| `Entrée` | modifier la valeur (une variable désactivée — `~` en tête de clé dans le fichier — n'est pas éditable) |
| `a` | ajouter une variable (clé, puis valeur) |
| `d` | supprimer la variable sous le curseur, sans confirmation |
| `Ctrl+S` | enregistrer |
| `Échap` | annuler la saisie en cours, sinon fermer le popup |

Un clic sur une ligne démarre directement sa saisie, comme pour un
en-tête de requête. Seules les valeurs sont éditées ici : les variables
secrètes (`vars:secret`) n'apparaissent jamais dans ce popup, elles
restent gérées par le panneau Secrets (`S`).

Environnements Bruno 4 (`extends`, `color`) : la liste affiche le ou les
parents à côté du nom (`staging ← base`) et une pastille `■` de la couleur
déclarée, si c'est une couleur hexadécimale (`#rgb` ou `#rrggbb`). Le TUI
ne résout jamais l'héritage : le popup ne montre que les variables du
fichier lui-même, et c'est `bru` qui fusionne les parents à l'exécution.
Enregistrer depuis le popup conserve `extends` et `color` tels quels.
Attention : `extends` demande `bru` 4.2 ou plus récent ; `bru` 4.1.0 refuse
l'environnement (erreur de lecture).

## Panneau Réponse

Affiche le résultat de la dernière exécution de la requête sélectionnée,
en cinq onglets (Corps, En-têtes, Tests, Timeline, Console). Le corps JSON (brut ou issu d'un
filtre `jq`) bénéficie d'une coloration syntaxique (clés, chaînes, nombres,
booléens, null, ponctuation), tandis qu'un corps textuel ou HTML reste
affiché sans coloration. L'onglet Timeline récapitule la requête envoyée
(méthode, URL résolue, en-têtes, corps) et la réponse reçue (statut, durée,
URL finale si redirigée). L'onglet Console restitue la sortie standard et
d'erreur de `bru` (débarrassée des codes ANSI) ; lors d'une exécution
récursive, la console globale de toute la campagne est affichée. SÉCURITÉ :
la sortie console reste en mémoire vive uniquement — elle n'est jamais écrite
sur le disque, jamais enregistrée dans l'historique persistant, ni logguée.

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler |
| `Début`/`g`, `Fin`/`G` | début, fin |
| `←`/`→` (ou `Entrée`) | onglet précédent/suivant |
| `/` | chercher dans l'onglet courant ; `n`/`N` : occurrence suivante/précédente |
| `\|` | filtrer le corps avec une expression `jq` |
| `v` | sélection visuelle ; `y` copie |
| `c` | copier en commande `curl` (en-têtes d'auth masqués) |
| `u` | copier en commande `curl` complète (confirmation si des secrets sont présents) |
| `Ctrl+E` | ouvrir le corps brut de la réponse (non filtré) dans `$VISUAL`, `$EDITOR`, ou `nano` — consultation seule, rien n'est relu |
| `x` | exporter le corps brut dans un fichier (chemin proposé hors collection, confirmation si existant) |

`Ctrl+E` fonctionne quel que soit le focus courant, tant qu'une requête
sélectionnée a un résultat avec un corps non vide.

## Autres panneaux

- **Diagnostics** (`D`) : erreurs de chargement de la collection ; `→`
  saute au nœud concerné dans l'arbre.
- **Historique** (`H`) : journal persistant des exécutions (sauvegardé dans
  `$XDG_STATE_HOME/bruno-tui/history.json` ou `~/.local/state/bruno-tui/history.json`,
  borné à 200 entrées par collection, limité strictement aux métadonnées sans secrets
  ni corps de réponse ; désactivable via `--no-history`) ; `r` relance l'entrée
  sélectionnée.
- **Secrets** (`S`) : variables `vars:secret` de l'environnement actif et
  celles demandées par `--secret` ; `a` ajoute un nom, `Entrée` saisit sa
  valeur, `d` l'oublie. Une valeur secrète n'est jamais écrite sur disque
  ni affichée en clair.
- **Campagne** (`C`) : bilan de la dernière exécution récursive (campagne de
  TNR) ; en-tête avec cible, options actives, taux de succès, durée, filtre
  actif et compteurs (`Tous`, `Échecs`, `Réussis`, `Ignorés`) ; liste des
  requêtes filtrée (par défaut les `Échecs`) avec code HTTP, statut ou raison
  d'échec ; `f` fait tourner le filtre (`Échecs` / `Réussis` / `Ignorés` / `Tous`) ;
  `Entrée` ou `→` sélectionne la requête dans l'arbre et donne le focus au Détail ;
  `R` relance uniquement les requêtes en échec avec les options courantes ;
  `r` relance l'ensemble de la campagne.
  Après une campagne sur plusieurs environnements (voir **Environnements** dans les
  options `O`), le panneau s'ouvre sur la vue **Comparaison** ; `t` bascule entre la
  liste (dernier environnement exécuté) et ce tableau. Une ligne par requête, une
  colonne par environnement : `✓` réussi, `✗` échec, `–` ignoré, `·` absente du rapport
  de cet environnement. Avec des itérations CSV/JSON, une cellule résume toutes les
  itérations (`✗` dès qu'une échoue). Les requêtes dont le verdict diffère entre
  environnements sont marquées `≠` et mises en évidence ; `f` fait tourner le filtre
  (`Tous` / `Divergences` / `Échecs` / `Réussis` / `Ignorés`). `↑`/`↓` choisit la requête,
  `←`/`→` l'environnement (défilement horizontal quand les colonnes ne tiennent pas,
  signalé par `‹` / `›` ; noms d'environnement tronqués à 8 caractères), `Entrée`
  sélectionne la requête et affiche le résultat de cet environnement dans le panneau
  Réponse, dont la bordure rappelle l'environnement (`[staging]`). Les résultats par
  environnement restent en mémoire uniquement ; une nouvelle campagne sur un seul
  environnement efface le tableau.
- **Options du runner** (`O`) : popup de configuration des options d'exécution
  récursive mémorisées en session :
  - **Tags inclus** (`--tags`) et **tags exclus** (`--exclude-tags`) sous forme
    de liste séparée par des virgules (espaces interdits).
  - **Délai** inter-requêtes en millisecondes (`--delay`).
  - **Bail** (`--bail`) : arrêt dès le premier échec.
  - **Tests-only** (`--tests-only`) : exécution des requêtes comportant des tests uniquement.
  - **Réseau et TLS** : connexions non sécurisées (`--insecure`), certificat CA personnalisé
    (`--cacert`), ignorance du truststore système (`--ignore-truststore`, requiert `--cacert`),
    configuration du certificat client (`--client-cert-config`), désactivation des proxys
    (`--noproxy`) et désactivation des cookies automatiques (`--disable-cookies`). Les chemins
    sont vérifiés à la saisie et transmis sous forme absolue à `bru`.
  - **Données et itérations** : exécution pilotée par des jeux de données CSV (`--csv-file-path`)
    ou JSON (`--json-file-path`) mutuellement exclusifs, nombre d'itérations (`--iteration-count`),
    et exécution concurrente (`--parallel`, requiert un fichier de données). En présence de plusieurs
    itérations, le bilan de campagne totalise chaque exécution avec son numéro d'itération (`#N`),
    et le panneau Réponse affiche la dernière itération (`itération N/M`).
  - **Divers** : mode de bac à sable JavaScript (`--sandbox`, choix entre `safe` par défaut
    et `developer`), et sortie détaillée (`--verbose`) pour le débogage.
  - **Exports de rapports** JUnit XML (`--reporter-junit`) et HTML (`--reporter-html`)
    délégués à `bru` : écrits hors collection (`$XDG_STATE_HOME/bruno-tui/reports/` ou
    `~/.local/state/bruno-tui/reports/`, repli vers `./reports`), horodatés avec le nom
    du dossier cible. Le masquage des en-têtes sensibles (`--reporter-skip-all-headers`)
    est activé par défaut et peut être basculé.
  - **Environnements** : une case par environnement de la collection (`Espace` ou
    `Entrée` coche/décoche). Aucun coché : la campagne tourne sur l'environnement actif,
    comme avant. Un seul coché : il remplace l'environnement actif pour la campagne.
    Deux ou plus : la campagne lance un `bru run` par environnement coché, l'un après
    l'autre (jamais en parallèle), dans l'ordre de la collection et avec les mêmes
    options. La barre d'état suit la progression (`env 2/3 : staging`). `Ctrl+X` arrête
    l'exécution en cours et n'enchaîne pas les suivantes. Avant de démarrer la série,
    le panneau Secrets propose, environnement par environnement, les `vars:secret`
    manquants (`r` passe à l'environnement suivant puis lance, `Échap` abandonne toute
    la série). Une requête seule (`r` sur une requête) reste sur l'environnement actif.
    La même section porte les options d'environnement de `bru` :
    - **Fichier d'environnement** (`--env-file`) : fichier `.bru` ou `.json`, vérifié à la
      saisie et transmis en chemin absolu. Exclusif avec les cases cochées ; il remplace
      l'environnement actif (`--env` n'est plus transmis à la campagne).
    - **Environnement global** (`--global-env <nom>`) : exige que la collection soit dans
      un workspace Bruno, détecté automatiquement ou indiqué par le champ suivant.
    - **Workspace** (`--workspace-path`) : dossier contenant `workspace.yml`, vérifié à la
      saisie et transmis en chemin absolu. En mode workspace, il s'applique aussi à une
      requête seule.
  - `Entrée` édite ou bascule un champ, `r` (hors saisie) lance directement la
    campagne avec les options configurées, `Échap` enregistre et referme le popup.
- **Recherche globale** (`Ctrl+K`) : popup centré de recherche en direct dans l'ensemble
  des requêtes de la collection chargée (nom affiché, méthode HTTP, chemin relatif et URL brute
  telle qu'écrite avec ses variables non résolues). `↑`/`↓` choisit dans la liste ordonnée
  par pertinence (correspondance mise en évidence, URL tronquée proprement), `Entrée` sélectionne
  la requête dans l'arbre en dépliant automatiquement ses dossiers parents et donne le focus
  au panneau Détail, `Échap` referme.

Aucun de ces panneaux ne prend le clic pour l'instant — seul le
clavier y navigue.

## Aide des raccourcis

À tout moment hors saisie de texte, la touche `?` ouvre un popup centré
rappelant l'ensemble des raccourcis clavier de l'application, organisés par
section : Global, Collection, Détail, Popups d'édition, Réponse,
Environnement (panneau et popup), Diagnostics / Historique / Secrets, et
Campagne (TNR).

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler dans l'aide |
| `Début`/`g`, `Fin`/`G` | début, fin de l'aide |
| `?`, `Échap`, `q` | fermer le popup d'aide |

Toutes les autres touches sont ignorées tant que le popup est ouvert, à
l'exception de `Ctrl+C` qui quitte l'application. À la souris, cliquer en
dehors du popup le referme également.

