## Context

Voir `proposal.md` pour la motivation et `specs/bru-parser/spec.md` pour le
contrat. État du dépôt : `src/lib.rs` n'expose que `runner`, aucune
dépendance de parsing, `bru` n'est pas installé sur la machine de
développement (les fixtures `.bru` doivent donc être écrites à la main, ce
que la demande impose de toute façon).

Faits sur le format `.bru` (grammaire Ohm de `@usebruno/lang` v2,
`bruToJson.js`, et fixtures de `runner-probe/`) qui dictent la
conception :

- Le format est **orienté lignes**. Un fichier est une suite de blocs
  séparés par des lignes vides ; rien d'autre n'est admis hors bloc.
- Un bloc s'ouvre par `nom {` ou `nom [` et se ferme par une ligne dont le
  premier caractère est `}` ou `]` (`tagend = nl "}"`). Le contenu des
  blocs texte est indenté de deux espaces à l'écriture et **désindenté de
  deux espaces** à la lecture ; c'est cette indentation, et non un
  comptage d'accolades, qui rend les `}` imbriqués inoffensifs.
- Blocs dictionnaire : `clé: valeur`, clé préfixée `~` = désactivée,
  valeur multi-ligne entre `'''` (lignes intérieures indentées de quatre
  espaces), annotations `@file(...)` / `@contentType(...)` possibles dans
  les valeurs multipart.
- Bloc liste : `vars:secret [` … `]`, un nom par ligne (environnements).
- Noms de blocs connus : `meta` ; méthodes `get post put delete patch
  options head connect trace` (dictionnaire `url`, `body`, `auth`) ;
  `params:query`, `params:path`, `headers`, `settings`, `assert`, `vars`,
  `vars:pre-request`, `vars:post-response`, `auth:*`,
  `body:form-urlencoded`, `body:multipart-form`, `body:file`
  (dictionnaires) ; `body`, `body:json`, `body:text`, `body:xml`,
  `body:sparql`, `body:graphql`, `body:graphql:vars`,
  `script:pre-request`, `script:post-response`, `tests`, `docs` (texte).
  Des blocs plus récents existent (`grpc`, `ws`, `metadata`, `example`,
  `query`) : ils seront conservés comme inconnus.
- `bruno.json` : `name`, `type`, `version`, `ignore` (liste de noms
  relatifs à la racine, défaut Bruno `["node_modules", ".git"]`), plus des
  champs libres.

## Goals / Non-Goals

**Goals :**
- Fidélité à l'octet garantie **par construction** : la reconstruction du
  fichier ne dépend d'aucune logique de sérialisation.
- Un seul point d'entrée synchrone, sans I/O caché, appelable depuis
  `spawn_blocking`.
- Aucune nouvelle dépendance.
- Messages d'erreur qui citent des numéros de ligne, jamais le contenu des
  lignes (les en-têtes et corps peuvent contenir des secrets).

**Non-Goals :**
- Validation sémantique au-delà du minimum (méthode présente) : une URL
  vide ou un `seq` non numérique n'est pas une erreur, la vue expose ce
  qu'elle peut.
- Écriture, même en mémoire, d'un fichier `.bru` depuis la vue typée.
- Surveillance du disque (rechargement à chaud) : un autre changement.
- Traitement particulier des `docs`, `settings`, `vars:*` autres que leur
  présence : conservés dans l'AST, non exposés dans la vue.

## Decisions

### D1. Parseur écrit à la main, orienté lignes

Le lexer découpe le source en lignes (avec leur fin de ligne brute), puis
un automate à trois états (hors bloc, dans un bloc `{`, dans un bloc `[`)
construit les nœuds. Aucune crate de parsing (`pest`, `nom`, `chumsky`).

Pourquoi : la grammaire est triviale au niveau des blocs ; l'exigence de
fidélité à l'octet demande de garder les tranches de source brutes, ce que
les générateurs de parseurs rendent laborieux ; et CLAUDE.md impose de
justifier toute dépendance. Un parseur maison d'environ 300 lignes est
plus lisible et plus facile à faire évoluer avec les versions de Bruno.

Alternative écartée : porter la grammaire Ohm avec `pest`. Rejeté pour la
dépendance, et parce que la grammaire officielle rejette silencieusement
certains fichiers que l'application Bruno accepte pourtant ; la tolérance
« bloc inconnu conservé » est plus simple à exprimer à la main.

### D2. AST par tranches de source

```rust
pub struct BruFile {
    source: String,          // fichier entier, tel que lu
    nodes: Vec<Node>,        // tranches contiguës couvrant 0..source.len()
}

pub struct Node {
    pub span: Range<usize>,  // tranche brute dans `source`
    pub kind: NodeKind,
}

pub enum NodeKind {
    Blank,                                   // lignes vides entre blocs
    Block { name: String, body: BlockBody },
}

pub enum BlockBody {
    Dictionary(Vec<Entry>),   // meta, méthodes, headers, params:*, auth:*, assert, vars*, settings, body:form-*, body:file
    Text { content: Range<usize> },          // tranche brute entre ouverture et fermeture
    List(Vec<String>),                       // vars:secret
    Unknown,                                 // nom non reconnu : span brut seulement
}

pub struct Entry {
    pub key: String,
    pub value: String,        // multi-ligne déjà rassemblée, sans les '''
    pub disabled: bool,       // préfixe ~
    pub span: Range<usize>,   // ligne(s) brute(s) de l'entrée
}
```

Chaque nœud est une tranche de `source` ; les tranches sont contiguës et
couvrent tout le fichier. `BruFile::raw()` retourne `&source` et le test
de round-trip vérifie l'égalité et l'invariant de contiguïté. Le contenu
désindenté d'un bloc texte est calculé à la demande (`dedented()` retire
au plus deux espaces par ligne, comme `outdentString` de Bruno).

Pourquoi des tranches plutôt que des `String` copiées par nœud : pas de
duplication du source, et la fidélité est un invariant structurel
vérifiable (couverture + contiguïté) plutôt qu'une propriété de chaque
copie. Le futur writer émettra la tranche brute des nœuds intacts et
régénérera seulement les nœuds modifiés.

Alternative écartée : AST « sémantique » (`HashMap<String, Value>`) à la
manière de `bruToJson`. Perd l'ordre, les blocs inconnus, les espaces et
les entrées désactivées : incompatible avec la contrainte de réécriture.

### D3. Règles de découpage précises

- Une ligne inclut sa fin `\n` ou `\r\n` ; le dernier segment peut n'avoir
  aucune fin de ligne. Rien n'est normalisé.
- Hors bloc : ligne vide (ou espaces seuls) → `Blank` ; ligne
  `nom<espaces>{` ou `nom<espaces>[` (espaces de fin tolérés) → ouverture ;
  toute autre ligne → `ParseError::TextOutsideBlock { line }`.
- Dans un bloc `{` : une ligne dont le premier octet est `}` ferme le bloc
  (reste de ligne : espaces seuls, sinon `BadClosing { line }`). Toute
  autre ligne appartient au contenu, y compris `  }` indentée et les
  lignes vides.
- Dans un bloc `[` : idem avec `]`.
- Fin de source dans un bloc → `UnclosedBlock { name, line }` (ligne
  d'ouverture).
- Le corps d'un bloc connu est ensuite interprété selon sa forme (table
  nom → forme) ; un échec d'interprétation d'un dictionnaire (ligne sans
  `:`) ne rejette pas le fichier : l'entrée est conservée avec sa tranche,
  clé égale à la ligne entière et valeur vide. La vue reste utilisable, le
  round-trip reste exact.
- Dictionnaire : `~` initial → `disabled` ; clé = texte avant le premier
  `:` (trim) ; valeur = texte après (trim). Valeur commençant par `'''`
  → accumulation jusqu'à la ligne dont le contenu trim se termine par
  `'''` ; lignes intérieures désindentées de quatre espaces au plus.
- Après le découpage, une requête sans bloc de méthode connu →
  `MissingMethod`. `collection.bru`, `folder.bru` et les environnements
  n'ont pas cette contrainte.

### D4. Vue typée dérivée, stockée à côté de l'AST

```rust
pub struct RequestView {
    pub name: Option<String>, pub kind: Option<String>, pub seq: Option<u32>,
    pub method: String,              // nom du bloc, en majuscules
    pub url: String,
    pub headers: Vec<KeyValue>,      // KeyValue { key, value, enabled }
    pub query_params: Vec<KeyValue>,
    pub path_params: Vec<KeyValue>,
    pub body: Option<BodyView>,      // BodyView { kind: BodyKind, content: BodyContent }
    pub auth: AuthMode,              // None | Inherit | Basic | Bearer | ... | Other(String)
    pub has_pre_request_script: bool, pub has_post_response_script: bool,
    pub has_tests: bool, pub has_assert: bool,
    pub assertions: Vec<KeyValue>,
}
```

`RequestView::from_ast(&BruFile) -> Result<RequestView, ParseError>` est
une fonction pure. Le nœud de requête porte `ast` et `view`. `BodyContent`
vaut soit le texte désindenté (blocs texte), soit la liste d'entrées
(`form-urlencoded`, `multipart-form`, `file`). Le type de corps vient de
la clé `body` du bloc de méthode ; si le bloc `body:<type>` correspondant
manque, `content` est vide. `seq` non numérique → `None` (pas d'erreur).

Pourquoi stocker la vue : la vue ratatui la lit à chaque rendu ; la
recalculer serait du travail dans `view`, interdit par l'architecture.

### D5. Modèle de collection et tri

```rust
pub struct Collection {
    pub root: PathBuf, pub name: String,
    pub settings: Option<FileMeta>,       // collection.bru
    pub tree: Vec<Node>,                  // enfants de la racine, triés
    pub environments: Vec<Environment>,   // triés par nom
}
pub enum Node { Request(RequestNode), Folder(FolderNode), Error(ErrorNode) }
pub struct FolderNode { pub path: PathBuf, pub name: String, pub seq: Option<u32>,
                        pub meta: Option<Result<FileMeta, ParseError>>, pub children: Vec<Node> }
pub struct ErrorNode  { pub path: PathBuf, pub error: ParseError }
pub struct FileMeta   { pub name: Option<String>, pub seq: Option<u32>, pub has_auth: bool, ... , pub ast: BruFile }
```

Clé de tri des enfants : `(seq.is_none(), seq, nom_de_fichier)` ; les
nœuds en erreur n'ont pas de `seq` et tombent donc en fin de liste. Le
`name` d'un dossier est celui de `folder.bru` s'il existe et se parse,
sinon le nom du répertoire. Les chemins sont relatifs à la racine, prêts
à être passés en `targets` de `RunRequest`.

### D6. Traversée et découverte de la racine

`std::fs` uniquement, pas de `walkdir`. Racine : remonter depuis le chemin
fourni (canonicalisé) jusqu'au premier répertoire contenant `bruno.json`.
Parcours : `read_dir` trié par nom, `symlink_metadata` pour ne pas suivre
les liens symboliques de répertoires, filtres `ignore` de `bruno.json`
plus `node_modules` et `.git`, `environments` traité à part à la racine
seulement. Profondeur bornée (64) pour éviter une récursion pathologique ;
au-delà, nœud en erreur `TooDeep`.

`bruno.json` : `struct BrunoConfig { name: Option<String>, ignore: Vec<String> }`
via `serde`, sans `deny_unknown_fields`. Absent → `LoadError::NotACollection`,
illisible ou invalide → `LoadError::InvalidConfig`.

### D7. Trait `CollectionLoader`

```rust
pub trait CollectionLoader {
    fn load(&self, path: &Path) -> Result<Collection, LoadError>;
}
pub struct BruLoader;
```

Synchrone : la boucle tokio l'enveloppe dans `spawn_blocking`. Un trait
`async` imposerait `async_trait` ou des `Box<dyn Future>` pour un gain nul.
Les types retournés (`Collection`, `Node`, `RequestView`) sont neutres :
`BruFile` n'apparaît qu'en champ `ast` optionnel, qu'un loader YAML pourra
laisser vide. Un `FakeLoader` de test valide l'interchangeabilité.

### D8. Erreurs

Deux types `thiserror` :
- `LoadError` (fatal, rend `load` en `Err`) : `NotACollection { path }`,
  `InvalidConfig { path, source }`, `Io { path, source }`.
- `ParseError` (par fichier, porté par les nœuds) : `Io`, `InvalidUtf8`,
  `TextOutsideBlock { line }`, `BadHeader { line }`, `BadClosing { line }`,
  `UnclosedBlock { name, line }`, `MissingMethod`, `TooDeep`.

Les messages `Display` sont en français, citent le nom de bloc et le
numéro de ligne, jamais le texte de la ligne. Test dédié : un fichier
malformé contenant `Authorization: Bearer s3cr3t` produit une erreur dont
`Display` et `Debug` ne contiennent pas `s3cr3t`.

### D9. Fixtures et tests

Une collection `tests/fixtures/collections/parser-cases/` écrite à la
main, couvrant tous les scénarios de la spec :

| Fichier | Rôle |
|---|---|
| `bruno.json` | `name`, `ignore: ["node_modules", "tmp"]`, champs inconnus |
| `collection.bru` | `meta`, `headers`, `auth:bearer` avec `{{tok}}` |
| `simple-get.bru` | GET, `{{host}}` non résolu, seq 7 |
| `post-json.bru` | POST, `body:json` avec `{ "a": { "b": [ { "c": 1 } ] }, "s": "}" }`, seq 10 |
| `scripted.bru` | scripts pré/post, `tests`, `assert`, `~` en-tête désactivé, seq 12 |
| `unknown-block.bru` | bloc `future:thing` entre `meta` et `get`, `tests` avant `headers` |
| `broken.bru` | bloc `headers` jamais fermé |
| `no-method.bru` | `meta` + `headers` seuls |
| `no-seq.bru` | requête sans `seq` |
| `multiline.bru` | `params:query` avec valeur `'''` |
| `grp/folder.bru` | `meta { name: Groupe, seq: 3 }`, `auth:bearer` |
| `grp/x.bru`, `grp/y.bru` | seq 2 et 20 |
| `grp/sub/folder.bru`, `grp/sub/deep.bru` | deuxième niveau, seq 5 |
| `grp/inherit.bru` | `auth: inherit` |
| `misc/z.bru` | dossier sans `folder.bru` |
| `badmeta/folder.bru`, `badmeta/ok.bru` | `folder.bru` malformé, requête valide |
| `tmp/ignored.bru`, `notes.md` | ignorés |
| `environments/local.bru` | `vars` + `vars:secret [ token ]` |

Tests :
- unitaires dans `src/collection/` : découpage sur chaînes inline (CRLF,
  sans saut final, `}` indenté, `'''`, `~`) ;
- intégration `tests/parser_fixtures.rs` : arbre attendu, vues, erreurs,
  environnements, round-trip sur chaque `.bru` valide de `parser-cases/`
  **et** de `runner-probe/` ; variante CRLF générée en mémoire à partir
  d'une fixture LF (évite les soucis de normalisation Git) ;
- lecture seule : instantané `(chemin, taille, mtime)` avant/après `load`
  sur une copie de la fixture dans un répertoire temporaire ;
- `FakeLoader` derrière `dyn CollectionLoader`.

## Risks / Trade-offs

- [Grammaire Bruno évolutive : nouveaux blocs, nouvelles annotations] →
  blocs inconnus conservés bruts ; table nom → forme centralisée et
  facile à étendre ; l'AST ne perd rien même quand la vue ignore.
- [Un `}` en première colonne dans un corps texte] → Bruno lui-même
  fermerait le bloc là ; comportement identique, fichier alors marqué en
  erreur ou tronqué comme dans Bruno. Documenté dans le commentaire du
  lexer.
- [Valeur `'''` mal rassemblée sur un cas exotique] → seule la vue est
  affectée, le round-trip reste exact car il repose sur les tranches.
- [Collection volumineuse : milliers de `.bru`] → parcours synchrone sous
  `spawn_blocking`, source de chaque fichier gardé en mémoire (quelques Ko
  par requête) ; acceptable. Chargement paresseux par dossier possible
  plus tard sans changer le trait.
- [Noms de fichiers non UTF-8] → `to_string_lossy` pour l'affichage, le
  `PathBuf` exact est conservé pour `RunRequest`.
- [Fichiers `.bru` très gros (corps binaire collé)] → lecture entière en
  mémoire, pas de limite ; on n'anticipe pas ce cas dans une collection
  Bruno réelle.
- [Le loader YAML futur n'a pas de `BruFile`] → `ast` optionnel dans les
  nœuds ; la vue typée est le contrat commun.
