## Purpose

Charger une collection Bruno depuis le disque, en lecture seule, sous la
forme d'un arbre ordonné et d'un AST fidèle à l'octet de chaque fichier
`.bru`, avec une vue typée suffisante pour l'affichage, sans jamais
interpréter variables, scripts ni héritage d'auth.

## ADDED Requirements

### Requirement: Découverte de la racine de collection
Le système SHALL accepter un chemin de fichier ou de répertoire et
déterminer la racine de la collection comme le répertoire le plus proche,
en remontant depuis ce chemin, qui contient un fichier `bruno.json`. Il
MUST lire dans `bruno.json` le nom de la collection et la liste `ignore`
des entrées à ne pas parcourir, en ignorant tout champ inconnu. Si aucun
`bruno.json` n'est trouvé jusqu'à la racine du système de fichiers, le
chargement MUST échouer avec une erreur qui nomme le chemin de départ.

#### Scenario: Chemin d'une requête à l'intérieur de la collection
- **WHEN** l'appelant fournit le chemin d'un fichier `.bru` situé deux
  niveaux sous le répertoire contenant `bruno.json`
- **THEN** la racine retournée est le répertoire contenant `bruno.json`

#### Scenario: Répertoire hors de toute collection
- **WHEN** l'appelant fournit un répertoire dont aucun ancêtre ne contient
  `bruno.json`
- **THEN** le chargement échoue avec une erreur « pas une collection Bruno »
  portant le chemin fourni

#### Scenario: bruno.json avec champs inconnus
- **WHEN** `bruno.json` contient `name`, `ignore` et des champs absents du
  modèle (`scripts`, `presets`)
- **THEN** le nom et la liste `ignore` sont lus et les autres champs
  ignorés

### Requirement: Traversée de la collection
Le système SHALL parcourir récursivement la racine de la collection et
considérer comme requête tout fichier d'extension `.bru` autre que
`collection.bru` et `folder.bru`. Il MUST NOT descendre dans les entrées
listées dans `ignore` de `bruno.json`, ni dans `node_modules` et `.git`,
ni traiter le répertoire `environments` comme un dossier de requêtes. Il
MUST ignorer les fichiers sans extension `.bru` et MUST NOT suivre les
liens symboliques vers des répertoires.

#### Scenario: Dossier ignoré par bruno.json
- **WHEN** `bruno.json` déclare `"ignore": ["node_modules", "tmp"]` et le
  répertoire `tmp` contient des fichiers `.bru`
- **THEN** aucun nœud issu de `tmp` n'apparaît dans l'arbre

#### Scenario: Fichiers non .bru mêlés aux requêtes
- **WHEN** un dossier contient `a.bru`, `notes.md` et `data.json`
- **THEN** seul `a.bru` produit un nœud de requête

#### Scenario: Répertoire environments
- **WHEN** la racine contient `environments/local.bru`
- **THEN** aucun dossier `environments` ni requête `local` n'apparaît dans
  l'arbre, et `local` apparaît dans la liste des environnements

### Requirement: AST fidèle au fichier source
Le système SHALL représenter chaque fichier `.bru` comme une suite ordonnée
de nœuds : blocs et texte inter-blocs (lignes vides). Chaque nœud MUST
conserver le texte source exact qui l'a produit, fins de ligne comprises,
de sorte que la concaténation des textes de tous les nœuds reproduise le
fichier à l'octet près. L'ordre des blocs MUST être celui du fichier. Un
bloc dont le nom n'est pas reconnu MUST être conservé comme bloc inconnu
avec son nom et son contenu brut. Aucune information du fichier MUST NOT
être perdue au parsing.

#### Scenario: Round-trip à l'octet près
- **WHEN** un fichier de fixture valide est parsé
- **THEN** la concaténation du texte source de ses nœuds est identique,
  octet pour octet, au contenu du fichier

#### Scenario: Bloc inconnu conservé
- **WHEN** un fichier contient un bloc `future:thing { ... }` que le
  système ne connaît pas, placé entre `meta` et `get`
- **THEN** l'AST contient, à cette position, un bloc inconnu nommé
  `future:thing` dont le contenu brut est celui du fichier

#### Scenario: Ordre des blocs préservé
- **WHEN** un fichier déclare `tests` avant `headers`
- **THEN** l'AST liste `tests` avant `headers`

#### Scenario: Fichier sans saut de ligne final ou avec fins de ligne CRLF
- **WHEN** un fichier se termine sans saut de ligne, ou utilise `\r\n`
- **THEN** le round-trip reste identique à l'octet près

### Requirement: Formes de blocs
Le système SHALL reconnaître trois formes de blocs, toutes ouvertes par une
ligne `nom {` ou `nom [` et fermées par une ligne commençant par `}` ou
`]` en première colonne :
- **dictionnaire** : une entrée par ligne, `clé: valeur` ; une clé
  préfixée de `~` MUST être exposée comme entrée désactivée ; une valeur
  multi-ligne délimitée par `'''` MUST être exposée comme une seule
  valeur ;
- **texte** : le contenu brut entre l'ouverture et la fermeture, dont la
  vue désindentée retire au plus deux espaces en tête de chaque ligne ;
  une accolade fermante indentée ne ferme pas le bloc ;
- **liste** : un élément par ligne, entre `[` et `]`.

La forme d'un bloc MUST être déterminée par son nom pour les blocs connus.
Un bloc inconnu MUST être conservé brut sans être interprété.

#### Scenario: Corps JSON avec accolades imbriquées et dans une chaîne
- **WHEN** un bloc `body:json` contient `{ "a": { "b": [ { "c": 1 } ] },
  "s": "}" }` réparti sur plusieurs lignes indentées
- **THEN** le bloc se ferme à la ligne `}` en première colonne et le
  contenu désindenté est le JSON complet, chaîne `"}"` comprise

#### Scenario: Entrée désactivée dans un dictionnaire
- **WHEN** un bloc `headers` contient `~X-Debug: 1` et `Accept: application/json`
- **THEN** la vue expose deux en-têtes, dont `X-Debug` marqué désactivé

#### Scenario: Valeur multi-ligne
- **WHEN** un bloc `params:query` contient une valeur délimitée par `'''`
  sur trois lignes
- **THEN** la vue expose une seule entrée dont la valeur couvre les trois
  lignes

### Requirement: Arbre de collection ordonné par seq
Le système SHALL construire un arbre dont la racine porte le nom de la
collection et dont chaque nœud est une requête, un dossier ou une erreur.
Les enfants d'un même parent MUST être ordonnés par valeur croissante de
`seq` lue dans le bloc `meta` de la requête ou du `folder.bru` du dossier,
dossiers et requêtes confondus. Les nœuds sans `seq` MUST être placés
après ceux qui en ont, et les égalités MUST être départagées par le nom de
fichier, pour un ordre déterministe. Un dossier sans `folder.bru` MUST
apparaître avec le nom de son répertoire et sans `seq`.

#### Scenario: Seq non contigus sur deux niveaux
- **WHEN** la racine contient `b.bru` (seq 10), le dossier `grp`
  (`folder.bru` seq 3) et `a.bru` (seq 7), et `grp` contient `y.bru`
  (seq 20) et `x.bru` (seq 2)
- **THEN** l'ordre à la racine est `grp`, `a`, `b`, et dans `grp` l'ordre
  est `x`, `y`

#### Scenario: Requête sans seq
- **WHEN** un dossier contient `z.bru` sans `seq` et `a.bru` avec seq 5
- **THEN** l'ordre est `a`, `z`

#### Scenario: Dossier sans folder.bru
- **WHEN** un répertoire `misc` ne contient pas de `folder.bru`
- **THEN** un nœud dossier nommé `misc`, sans `seq`, est présent et trié
  parmi les nœuds sans `seq`

### Requirement: Vue typée d'une requête
Le système SHALL fournir, pour chaque requête parsée, une vue dérivée de
l'AST exposant : le nom et le type déclarés dans `meta`, la méthode HTTP
(nom du bloc de méthode), l'URL, les en-têtes avec leur état activé ou
désactivé, les paramètres de requête et de chemin avec leur état, le type
de corps déclaré et le contenu brut du bloc de corps correspondant s'il
existe, le mode d'auth déclaré dans le bloc de méthode, et la présence ou
l'absence des blocs `script:pre-request`, `script:post-response`, `tests`
et `assert`. Les valeurs MUST être exposées telles qu'écrites : les motifs
`{{variable}}` MUST NOT être résolus et le mode d'auth MUST NOT être
hérité du dossier ou de la collection.

#### Scenario: Requête GET simple
- **WHEN** une requête déclare `meta { name: ping, type: http, seq: 1 }` et
  `get { url: https://{{host}}/ping, body: none, auth: none }`
- **THEN** la vue expose la méthode `GET`, l'URL `https://{{host}}/ping`
  non résolue, aucun corps, l'auth `none` et aucun script, test ni
  assertion

#### Scenario: Requête POST à corps JSON
- **WHEN** une requête déclare `post { ..., body: json }` et un bloc
  `body:json`
- **THEN** la vue expose la méthode `POST`, le type de corps `json` et le
  contenu désindenté du bloc `body:json`

#### Scenario: Scripts, tests et assertions présents
- **WHEN** une requête contient les blocs `script:pre-request`,
  `script:post-response`, `tests` et `assert`
- **THEN** la vue indique la présence de chacun des quatre, et expose les
  assertions comme entrées de dictionnaire

#### Scenario: Auth héritée non résolue
- **WHEN** une requête déclare `auth: inherit` et son dossier déclare un
  `auth:bearer`
- **THEN** la vue de la requête expose le mode `inherit`, sans jeton ni
  mode emprunté au dossier

### Requirement: Fichiers de collection, de dossier et d'environnement
Le système SHALL parser `collection.bru` et `folder.bru` avec le même AST
que les requêtes, et exposer pour chacun le nom et le `seq` déclarés dans
`meta`, ainsi que la présence des blocs d'en-têtes, d'auth, de scripts,
de tests et de variables. Il SHALL parser chaque `environments/*.bru` et
exposer le nom de l'environnement (nom de fichier sans extension), ses
variables avec leur état activé ou désactivé, et la liste des noms de
variables déclarées secrètes, sans valeur.

#### Scenario: folder.bru avec auth
- **WHEN** `grp/folder.bru` contient `meta { name: Groupe, seq: 3 }` et
  `auth:bearer { token: {{tok}} }`
- **THEN** le nœud dossier expose le nom `Groupe`, le `seq` 3 et la
  présence d'un bloc d'auth

#### Scenario: Environnement avec variables secrètes
- **WHEN** `environments/local.bru` contient `vars { host: localhost }` et
  `vars:secret [ token ]`
- **THEN** l'environnement `local` expose la variable `host` de valeur
  `localhost` et le nom secret `token` sans valeur

### Requirement: Tolérance aux fichiers invalides
Le système SHALL continuer le chargement de la collection lorsqu'un fichier
`.bru` ne peut pas être lu ou parsé. Le fichier fautif MUST apparaître
dans l'arbre comme un nœud en erreur portant son chemin et une raison
lisible qui nomme le problème et, quand elle existe, la ligne concernée.
Sont au moins considérés invalides : un bloc non fermé en fin de fichier,
une ligne non vide hors de tout bloc, un contenu qui n'est pas de l'UTF-8
valide, un fichier illisible, et une requête sans bloc de méthode HTTP. Un
`folder.bru` ou `collection.bru` invalide MUST NOT empêcher le chargement
des requêtes du dossier concerné.

#### Scenario: Fichier malformé au milieu d'un dossier
- **WHEN** un dossier contient `a.bru` valide, `broken.bru` dont un bloc
  n'est jamais fermé, et `c.bru` valide
- **THEN** l'arbre contient les requêtes `a` et `c` et un nœud en erreur
  pour `broken.bru` dont la raison mentionne un bloc non fermé et la ligne
  d'ouverture

#### Scenario: Requête sans méthode
- **WHEN** un fichier `.bru` ne contient que `meta` et `headers`
- **THEN** le nœud est en erreur avec une raison mentionnant l'absence de
  bloc de méthode

#### Scenario: folder.bru invalide
- **WHEN** `grp/folder.bru` est malformé et `grp/x.bru` est valide
- **THEN** le dossier `grp` est présent, marqué en erreur de méta-données,
  et contient la requête `x`

### Requirement: Lecture seule
Le système MUST NOT créer, modifier ni supprimer aucun fichier ou
répertoire pendant le chargement, quelle qu'en soit l'issue.

#### Scenario: Aucune écriture pendant le chargement
- **WHEN** une collection contenant des fichiers valides et invalides est
  chargée
- **THEN** l'ensemble des fichiers de la collection, leurs contenus et
  leurs dates de modification sont identiques avant et après le
  chargement

### Requirement: Chargement derrière une abstraction interchangeable
Le système SHALL exposer le chargement derrière une interface unique
acceptant un chemin et retournant la collection chargée ou une erreur, de
sorte qu'un autre format de collection puisse être pris en charge par une
implémentation distincte sans modifier les appelants. Le chargement MUST
NOT bloquer la boucle d'événements de l'application : l'appelant doit
pouvoir l'exécuter hors du fil d'interface.

#### Scenario: Appelant indépendant du format
- **WHEN** un appelant charge une collection à travers l'interface en
  tenant une implémentation Bruno `.bru`
- **THEN** il obtient l'arbre, les environnements et les erreurs sans
  dépendre d'un type propre au format `.bru`

#### Scenario: Implémentation de substitution
- **WHEN** une implémentation de test retourne un arbre fixe
- **THEN** l'appelant fonctionne à l'identique avec cette implémentation
