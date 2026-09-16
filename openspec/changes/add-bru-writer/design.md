## Context

Voir `proposal.md` pour la motivation et `specs/bru-writer/spec.md` pour le
contrat. État du dépôt pertinent (lu dans `src/collection/`) :

- `BruFile` (`src/collection/ast.rs`) porte `source: String` et
  `nodes: Vec<Node>`, chaque `Node` couvrant une tranche contiguë de
  `source` via `span: Range<usize>`. `BruFile::raw()` retourne `&source`,
  `BruFile::slice(&range)` retourne le texte d'une tranche.
- Un bloc dictionnaire (`BlockBody::Dictionary(Vec<Entry>)`) porte des
  `Entry { key, value, disabled, span }` où `span` couvre exactement la ou
  les lignes brutes de **cette entrée seule**, fins de ligne comprises
  (vérifié par le test `disabled_entry` : `slice(&entries[1].span) ==
  "  ~X-Debug: 1\n"`).
- Un bloc texte (`BlockBody::Text { content: Range<usize> }`) porte la
  tranche brute entre l'ouverture et la fermeture du bloc ; `dedent()`
  retire la fin de ligne finale et au plus deux espaces par ligne à la
  lecture.
- `RequestView::from_ast` (`src/collection/view.rs`) dérive `headers`,
  `query_params`, `path_params` comme `Vec<KeyValue>` en itérant
  `dictionary(block)` **dans l'ordre**, sans filtrage : l'indice `i` dans
  `RequestView.headers` correspond exactement à l'entrée `i` du bloc
  `headers` de l'AST. Le type de corps vient de la clé `body` du bloc de
  méthode ; son bloc porteur est nommé par `BodyKind::block_name()`.
- `CollectionLoader`/`BruLoader` (`src/collection/loader.rs`) sont
  synchrones et neutres vis-à-vis du format ; l'appelant les enveloppe
  dans `spawn_blocking`. Ce changement suit le même patron pour l'écriture.
- Aucune fonction d'écriture n'existe dans `src/collection/` : sa spec
  porte l'exigence « Lecture seule ». Ce changement n'y touche pas.

## Goals / Non-Goals

**Goals :**
- Fidélité par construction : ce qui n'est pas édité est réémis depuis les
  tranches déjà conservées par l'AST, jamais reformaté.
- Résolution des modifications séparée de l'écriture disque : une fonction
  pure produit les octets, une fonction d'I/O les écrit atomiquement.
- Refus sûr plutôt qu'écrasement silencieux face à une modification
  concurrente du fichier.

**Non-Goals :**
- Ajout, suppression ou renommage d'entrées : chaque champ édité doit déjà
  exister dans le fichier (voir proposal.md, hors périmètre).
- Résolution ou re-validation sémantique de la valeur écrite (une URL
  vide ou un JSON invalide dans `body:json` sont acceptés tels quels,
  comme `bru-parser` les accepte à la lecture).
- Verrouillage de fichier au niveau du système d'exploitation : la
  détection de modification concurrente se limite à taille + date de
  modification, capturées avant l'écriture (voir Risks).
- Écriture de `collection.bru`, `folder.bru`, `environments/*.bru`, ou de
  tout corps de forme formulaire.

## Decisions

### D1. Modifications ciblées par champ, pas par bloc entier

```rust
pub enum FieldEdit {
    Url(String),
    HeaderValue { index: usize, value: String },
    HeaderEnabled { index: usize, enabled: bool },
    QueryParamValue { index: usize, value: String },
    QueryParamEnabled { index: usize, enabled: bool },
    PathParamValue { index: usize, value: String },
    PathParamEnabled { index: usize, enabled: bool },
    BodyText(String),
}
```

`index` désigne la position dans `RequestView.headers` /
`.query_params` / `.path_params`, qui correspond 1:1 à l'ordre des
`Entry` du bloc AST correspondant (constaté dans `view.rs::key_values`,
qui n'exclut ni ne réordonne rien). Pas de variante générique
« bloc + indice » : les sept variantes couvrent exactement les champs du
périmètre et rendent chaque test explicite. `Url` et `BodyText` sont
singletons (un seul par requête), les autres sont indexés.

Alternative écartée : cibler un bloc par son nom (`&str`) et un indice
d'entrée générique. Plus court à écrire, mais confond des champs de
nature différente (l'URL est une clé unique du bloc de méthode, pas une
liste) et perd la vérification à la compilation du nombre de variantes
gérées par un `match`.

### D2. Résolution en deux temps : fusion des éditions, puis remplacement de tranches

`resolve(ast: &BruFile, edits: &[FieldEdit]) -> Result<Vec<Replacement>, EditError>`
où `Replacement { span: Range<usize>, bytes: String }`.

1. Chaque `FieldEdit` est d'abord résolu vers sa cible dans l'AST : l'
   `Entry` du bloc concerné (par indice) pour les variantes de liste, ou
   l'entrée `url` du bloc de méthode, ou le `content` du bloc de corps
   texte porteur du type de corps courant (`BodyKind::block_name()`).
   Une cible introuvable (bloc absent, indice hors limite, entrée `url`
   absente, corps non textuel) produit `EditError` sans effet de bord.
2. Les résolutions ciblant la **même** entrée (par exemple `HeaderValue`
   puis `HeaderEnabled` sur le même indice, dans un seul appel) sont
   fusionnées : l'état final (valeur, désactivé) part de l'entrée
   d'origine et applique les éditions dans l'ordre de la liste, pour ne
   produire qu'un seul remplacement par entrée touchée.
3. Chaque cible fusionnée produit un `Replacement` dont `span` est celui
   de l'`Entry` (ou du `content` pour un corps texte) et `bytes` le texte
   formaté (D3).
4. Les `Replacement` sont triés par `span.start` ; comme chaque bloc est
   disjoint des autres et chaque entrée disjointe des autres entrées de
   son bloc, ils ne se chevauchent jamais par construction. Un
   chevauchement constaté serait un bogue interne, retourné comme
   `EditError::Overlap` plutôt qu'un panic.

Pourquoi fusionner avant de remplacer : une entrée ne peut être remplacée
qu'une fois (les tranches sont disjointes et consommées une seule fois) ;
fusionner d'abord évite d'avoir à gérer des remplacements imbriqués ou
successifs sur la même tranche.

### D3. Sérialisation d'une entrée et d'un corps texte

Détection des fins de ligne : `eol = "\r\n"` si `ast.raw()` contient
`"\r\n"`, sinon `"\n"` — une seule détection par fichier, cohérente avec
le fait qu'un `.bru` réel n'a pas de raison de mélanger les deux
(limitation documentée en Risks).

Entrée dictionnaire (indentation de deux espaces, celle de tous les blocs
dictionnaire existants) :
- valeur sans saut de ligne : `"  {~}{key}: {value}{eol}"` ;
- valeur avec au moins un saut de ligne : forme `'''`, symétrique de la
  lecture (D3 de `bru-parser`, `dedent_by(_, 4)`) —
  ```
    {key}: '''{eol}
      {ligne 1}{eol}
      {ligne 2}{eol}
    '''{eol}
  ```
  ouverture et fermeture indentées de deux espaces (l'indentation du
  bloc), lignes intérieures indentées de quatre espaces. Un rechargement
  par `bru-parser` doit retrouver la valeur fournie à l'octet près :
  vérifié par un test d'aller-retour édition → lecture.

Corps texte (`BodyText`) : la nouvelle valeur est réindentée de deux
espaces par ligne (l'inverse de `dedent`), chaque ligne suivie de `eol`,
et remplace tout le `content` du bloc ; les lignes d'ouverture et de
fermeture du bloc (`body:json {` et `}`) restent des tranches brutes
inchangées.

Non traité, documenté comme limite : une valeur contenant littéralement
`'''` n'est pas échappée. Rare dans un en-tête ou un paramètre ; un futur
changement pourra le traiter si le besoin apparaît (voir Risks).

### D4. `EditError` et `WriteError`, sur le modèle de `LoadError`/`ParseError`

```rust
pub enum EditError {
    NoSuchBlock { block: &'static str },
    IndexOutOfRange { block: &'static str, index: usize },
    MissingField { field: &'static str },
    WrongBodyForm { expected: &'static str },
    Overlap,
}

pub enum WriteError {
    Stale { path: PathBuf },
    Edit(EditError),
    Io { path: PathBuf, source: io::Error },
}
```

Même découpage que `bru-parser` (`LoadError` fatal / `ParseError` par
fichier) : `EditError` est pur et testable sans toucher au disque,
`WriteError` l'enveloppe pour les échecs propres à l'écriture. Aucun
message `Display` ne cite une valeur de champ, seulement des noms de bloc,
des indices et des chemins — même contrainte et même style de test que
`ParseError::messages_never_quote_line_content`.

### D5. Instantané de fraîcheur

```rust
pub struct FileStamp { len: u64, modified: SystemTime }
impl FileStamp {
    pub fn capture(path: &Path) -> io::Result<Self>;
}
```

Capturé via `fs::metadata(path)` par l'appelant, typiquement juste après
le chargement de la requête par `bru-parser`. Comparaison par égalité
stricte de `len` et `modified` avant d'écrire. `SystemTime` a une
résolution dépendant de l'OS (souvent inférieure à la milliseconde sur
Linux ext4) ; c'est la même détection que celle décrite dans la demande
utilisateur (taille + date), sans hash de contenu pour rester bon marché,
avec le compromis documenté en Risks.

`FileStamp` est un type de `bru-writer`, pas un champ ajouté à
`RequestNode` ou `BruFile` : `bru-parser` reste inchangé, et un appelant
capture l'instantané explicitement au moment qui lui convient (au
chargement, ou à l'ouverture d'un formulaire d'édition).

### D6. Écriture atomique

```rust
pub trait RequestWriter: Send + Sync {
    fn write_request(
        &self,
        path: &Path,
        ast: &BruFile,
        stamp: &FileStamp,
        edits: &[FieldEdit],
    ) -> Result<FileStamp, WriteError>;
}
pub struct BruWriter;
```

Séquence de `BruWriter::write_request` :
1. `FileStamp::capture(path)` et comparaison à `stamp` fourni → `Stale`
   si différent (D5).
2. `resolve(ast, edits)` (D2) → `Edit` si erreur, sans I/O.
3. Application des `Replacement` sur `ast.raw()` par concaténation
   (préfixe avant le premier remplacement, chaque `bytes`, suffixe entre
   remplacements successifs, suffixe final) → `Vec<u8>`.
4. Écriture dans un fichier temporaire du même répertoire (nom dérivé,
   par exemple `.<nom>.bru.tmp-<pid>-<compteur>` pour éviter une collision
   entre écritures concurrentes du même processus).
5. `fs::set_permissions` sur le temporaire avec les permissions lues sur
   le fichier cible avant écriture (`fs::metadata(path)?.permissions()`),
   avant le renommage.
6. `fs::rename(temp, path)` — atomique sur un même système de fichiers,
   remplace la cible aussi bien sur Unix que sur Windows d'après la
   documentation de `std::fs::rename`. Pas de `cfg(unix)` nécessaire ici :
   contrairement à `bru-runner` (passage de descripteur de fichier),
   aucune primitive de cette étape n'est spécifique à une plateforme.
7. En cas d'échec aux étapes 4 ou 5, le fichier temporaire est supprimé
   au mieux (`let _ = fs::remove_file(&temp);`) et l'erreur `Io` est
   retournée ; le fichier cible n'a pas été touché.
8. Succès → nouvel instantané via `FileStamp::capture(path)`, retourné à
   l'appelant pour qu'il n'ait pas à le recapturer.

`BruWriter` ne recharge pas la requête après écriture : l'appelant décide
s'il relit via `CollectionLoader` (source de vérité unique) ou met à jour
son état en mémoire à partir des `edits` qu'il vient d'appliquer.

### D7. Emplacement du module

`src/writer/{mod.rs, edit.rs, format.rs, error.rs}`, déclaré
`pub mod writer;` dans `src/lib.rs`, au même niveau que `collection` et
`runner`. Pas dans `src/collection/` : la spec de `bru-parser` porte
l'exigence « Lecture seule » sur ce module, et y ajouter du code
d'écriture, même correct, obscurcirait cette limite pour quiconque relit
`src/collection/` en s'appuyant sur sa spec. `writer` dépend de
`collection` (types `BruFile`, `Entry`, `BlockBody`) mais l'inverse
n'existe pas.

### D8. Fixtures et tests

`tests/fixtures/collections/writer-cases/` — requêtes minimales dédiées à
l'écriture, plutôt que de réutiliser `parser-cases/` (dont les fichiers
sont la référence des tests de `bru-parser` et ne doivent pas devenir
aussi la référence, plus fragile, des octets exacts attendus après
écriture) :

| Fichier | Rôle |
|---|---|
| `simple.bru` | GET, une URL, aucun en-tête : édition de l'URL seule |
| `headers.bru` | trois en-têtes dont un désactivé : édition d'un seul, préservation des deux autres à l'octet près |
| `multiline-target.bru` | un paramètre de requête à valeur courte : cible d'une édition vers une valeur multi-ligne |
| `json-body.bru` | `body:json` à réindenter, bloc inconnu avant `meta` à préserver |
| `crlf.bru` | mêmes cas que `headers.bru`, fins de ligne `\r\n` |
| `form-body.bru` | `body: formUrlEncoded` avec `body:form-urlencoded` : cible du refus `WrongBodyForm` |

Tests :
- unitaires dans `src/writer/` : formatage d'une entrée (ligne simple,
  valeur multi-ligne, `~`), détection de `eol`, fusion de deux éditions
  sur le même indice ;
- intégration `tests/writer_fixtures.rs` : pour chaque fixture, une
  édition attendue est appliquée, le résultat est comparé octet à octet
  à un fichier `*.after.bru` versionné à côté de la fixture ; les octets
  hors de la zone modifiée sont en plus vérifiés égaux à l'original par
  une comparaison de préfixe et de suffixe communs ;
  round-trip édition → relecture par `BruLoader`/`RequestView::from_ast`
  pour la valeur multi-ligne et pour l'URL modifiée ;
  aucune édition fournie → fichier produit égal à l'original ;
  indice hors limite, bloc absent, corps de forme formulaire → erreurs
  attendues, fichier cible inchangé ;
  fichier modifié entre capture de l'instantané et écriture → `Stale`,
  fichier cible resté celui du tiers ;
  permissions Unix (`0640` par exemple) préservées après écriture ;
  aucun fichier temporaire résiduel après un succès, ni après un échec
  simulé (permission d'écriture retirée sur le répertoire, `#[cfg(unix)]`) ;
  message d'erreur d'un échec sur un en-tête portant un jeton secret ne
  contenant pas ce jeton ;
- `FakeWriter` de test implémentant `RequestWriter` sans toucher au disque,
  pour valider l'interchangeabilité derrière `&dyn RequestWriter`.

## Risks / Trade-offs

- [Instantané taille + date sans hash] → une modification qui préserve
  exactement taille et date à la résolution de l'horloge du système de
  fichiers ne serait pas détectée ; jugé acceptable pour un usage
  interactif mono-utilisateur, un hash de contenu pourrait être ajouté
  plus tard sans changer le trait (le type `FileStamp` resterait opaque).
- [Fenêtre entre la vérification de fraîcheur et le renommage] → aucun
  verrou de fichier ; une modification concurrente dans cette fenêtre
  étroite n'est pas détectée. Risque résiduel jugé faible pour une
  collection éditée localement, documenté plutôt que traité.
- [Valeur contenant littéralement `'''`] → non échappée à l'écriture ;
  produirait un fichier qui ne se relirait pas fidèlement. Situation rare
  (jamais vue dans les fixtures existantes), traitement différé.
- [Mélange de fins de ligne dans un même fichier] → la détection est
  globale au fichier ; un fichier réellement mixte verrait ses lignes
  éditées alignées sur la fin de ligne dominante, pas sur celle de la
  ligne remplacée. Non rencontré dans les fixtures de `bru-parser`.
- [`fs::rename` inter-systèmes de fichiers] → échouerait si le répertoire
  temporaire différait du répertoire cible ; évité en construisant
  toujours le temporaire dans le même répertoire que la cible.
- [Écritures concurrentes du même processus sur le même fichier] → hors
  périmètre d'un TUI mono-thread d'édition ; le nom du fichier temporaire
  inclut néanmoins un compteur pour ne pas collisionner entre deux
  écritures rapprochées.
