## Context

Motivation : voir `proposal.md`. Contrats : `specs/bru-writer/spec.md` et
`specs/field-editing/spec.md` (deltas). État du code lu pour cette
proposition :

- `src/writer/edit.rs` : `FieldEdit` a huit variantes (`Url`,
  `{Header,QueryParam,PathParam}{Value,Enabled}`, `BodyText`). `resolve`
  regroupe les éditions par cible (`TargetKey`), part de l'`Entry` d'origine
  et produit un `Replacement { span, bytes }` par entrée touchée ; les
  indices désignent toujours l'`Entry` du fichier chargé. Aucune insertion
  ni suppression de tranche n'est possible.
- `src/writer/format.rs` : `format_entry` (deux espaces, `~`, forme `'''`),
  `detect_eol`, `serialize` (concaténation des remplacements triés sur
  `ast.raw()`). `src/writer/mod.rs` : `FileStamp`, `RequestWriter`,
  `BruWriter` (contrôle de fraîcheur, temporaire, permissions, renommage).
  Ces deux derniers points ne changent pas.
- `src/collection/ast.rs` : `Entry.span` couvre exactement les lignes d'une
  entrée (toutes celles d'une valeur `'''`) ; `Node.span` couvre un bloc de
  sa ligne d'ouverture à sa ligne de fermeture incluse ; les lignes vides
  entre blocs sont des nœuds `Blank`. La clé est lue jusqu'au premier `:`,
  sans prise en charge des clés entre guillemets.
- `src/app/model.rs` (après `improve-direct-editing`) : `EditSession {
  path, stamp, fields, cursor, state: EditState, pending: Vec<FieldEdit>,
  dirty, hscroll }`, `EditState::{FieldSelect, Input(TextInput)}`,
  `has_unsaved()` ; `EditableField::{Url, HeaderValue(i),
  QueryParamValue(i), PathParamValue(i), BodyText}`.
  `field_value_committed`/`field_enabled` relisent `pending` à rebours par
  indice, ce qui suppose des indices stables ;
  `src/app/update.rs::update_or_push_pending` fusionne par indice.
- `src/app/text_input.rs` : tampon de saisie pur (`TextInput::new(value,
  multiline)`, curseur en caractères, `is_modified`), utilisé par
  `begin_input`/`validate_input`/`cancel_input`/`input_key`.
- `src/app/message.rs` : `Ctrl+S` → `SaveEdit`, `Entrée` → `Enter`
  (ouverture de session ou début de saisie dans le détail), `Tab` et
  `Échap` capturés en saisie (`ValidateInput`, `CancelInput`) ; hors
  saisie, `a` → `AddSecret`, `d` → `ForgetSecret` (traités seulement si le
  focus est sur Secrets) ; `c` sans modificateur est libre. En saisie,
  `TextCapture::Input` intercepte toute touche sans `Ctrl`.
- `src/app/update.rs::selection_locked` refuse tout changement de requête
  tant que `has_unsaved()` ; `src/app/view/detail.rs::request_text_and_fields`
  construit le détail et la table `FieldLine { field, line, count,
  prefix_width }` utilisée pour le curseur et le défilement.
- `status-panel` (`src/app/view/status.rs`) n'affiche ni l'URL ni les
  en-têtes de la requête : aucune donnée de la session n'y transite.
- Comportement de Bruno, vérifié dans l'installation locale de `bru`
  2.13.2 : `bru run` n'envoie que `request.url` (les `params:query` ne sont
  pas réinjectés, `prepare-request.js`) et ne substitue les `params:path`
  que pour les segments `:nom` présents (`interpolate-vars.js`) ;
  `@usebruno/common` `buildQueryString` / `parseQueryParams` (sans
  encodage, découpe sur `&` puis premier `=`, noms vides ignorés) ;
  `@usebruno/lang` `jsonToBru` écrit les blocs dans l'ordre méthode,
  `params:query`, `params:path`, `headers`, n'écrit aucun bloc vide, et
  `bruToJsonV2` accepte les clés dupliquées et le préfixe `~` (essai
  direct sur une source contenant `q: 1`, `q: 2`, `~z: 3`).

## Goals / Non-Goals

**Goals :**
- Une seule logique d'application des modifications, partagée par
  l'écriture et l'aperçu affiché par la session : ce qui est affiché avant
  `Ctrl+S` est, par construction, ce qui est relu après enregistrement.
- Réécriture limitée aux tranches touchées, y compris pour l'insertion et
  le retrait d'un bloc.
- Aucune rupture pour les appelants actuels : une liste sans modification
  structurelle se comporte comme aujourd'hui, à l'exception assumée de la
  synchronisation de l'URL sur une modification de `params:query`.

**Non-Goals :**
- Synchroniser `params:path` avec les segments `:nom` de l'URL, dans un
  sens comme dans l'autre.
- Encoder ou décoder les paramètres dans l'URL (Bruno ne le fait pas non
  plus à l'édition).
- Clés entre guillemets, réordonnancement, undo.
- Nouveau panneau, nouvel état de session au-delà de Sélection de champ /
  Saisie, ou interaction souris.

## Decisions

### D1. Nouvelles variantes de `FieldEdit`, section explicite

```rust
pub enum EntrySection { Headers, QueryParams, PathParams }

pub enum FieldEdit {
    // variantes existantes inchangées
    AddEntry { section: EntrySection, key: String, value: String, enabled: bool },
    RemoveEntry { section: EntrySection, index: usize },
    RenameKey { section: EntrySection, index: usize, key: String },
}
```

Les variantes existantes restent en place : `field-editing`, ses tests et
le changement parallèle `improve-direct-editing` les utilisent. Les
nouvelles portent une `EntrySection` plutôt que de tripler les variantes
(neuf de plus) : l'opération est identique pour les trois blocs, seules la
validation des paramètres de requête et la synchronisation d'URL
diffèrent, et un `match` sur `EntrySection` les isole.

Alternative écartée : migrer toutes les variantes indexées vers
`EntryValue { section, index, .. }`. Plus uniforme, mais réécrit l'API au
moment où un autre changement la consomme ; peut être fait plus tard sans
effet sur les specs.

### D2. Application séquentielle sur un brouillon, puis diff vers des tranches

`edit::resolve` est remplacé par deux étapes pures :

1. `Draft::from_ast(ast)` construit, pour chaque section, un
   `Vec<DraftEntry { origin: Option<usize>, key, value, disabled }>`
   (`origin` = indice de l'`Entry` d'origine, `None` pour une entrée
   ajoutée), plus l'URL et le corps (valeur d'origine et valeur courante).
2. Chaque `FieldEdit` est appliqué **dans l'ordre** au brouillon ; l'indice
   désigne la position courante dans le `Vec` (exigence « Application
   ordonnée »). Une erreur arrête tout, sans effet de bord.
3. Le brouillon final est comparé à l'AST pour produire les
   remplacements :
   - entrée d'origine absente du brouillon → remplacement de son `span` par
     une chaîne vide ;
   - entrée d'origine dont la clé, la valeur ou l'état a changé →
     remplacement de son `span` par `format_entry` ;
   - entrées ajoutées (toujours en fin de section, puisqu'aucune opération
     ne réordonne) → une insertion (`span` vide) au point d'ancrage : fin du
     `span` de la dernière `Entry` d'origine, sinon début du contenu du
     bloc ;
   - bloc absent et section non vide → insertion de
     `{eol}{nom} {{eol}{entrées}}}{eol}` à la fin du `span` du dernier bloc
     présent parmi ses prédécesseurs dans l'ordre Bruno (méthode,
     `params:query`, `params:path`, `headers`), précédée d'un `eol` si ce
     bloc est le dernier du fichier sans saut de ligne final ;
   - bloc présent dont toutes les entrées ont disparu et qui en avait au
     chargement ou en a reçu → un seul remplacement couvrant la ligne vide
     qui le précède immédiatement (si elle existe) et le `span` du bloc ; les
     remplacements d'entrée de ce bloc ne sont pas émis ;
   - URL et corps : comme aujourd'hui, seulement si la valeur a changé.
4. Tri par `(span.start, ordre d'émission)` puis contrôle de chevauchement
   existant (`EditError::Overlap`), en autorisant plusieurs insertions à la
   même position.

`Replacement` garde sa forme (`span` + `bytes`) ; une insertion est un
`span` vide, une suppression des `bytes` vides : `serialize` n'a pas à
changer.

Pourquoi un brouillon plutôt que l'extension de la fusion par cible
actuelle : avec ajout et suppression, les indices ne sont plus stables et
la fusion par `TargetKey` n'a plus de sens. Le brouillon fait de l'ordre
la seule règle, et le diff garde la minimalité : une entrée non touchée
n'est jamais réémise, même si ses voisines bougent.

Symétrie assumée : insérer un bloc ajoute « ligne vide + bloc », en retirer
un enlève « ligne vide précédente + bloc ». Ajouter puis supprimer une
entrée dans une section absente retrouve le fichier d'origine à l'octet
près (testé).

### D3. Synchronisation `params:query` ↔ URL dans le brouillon

Nouveau module pur `src/writer/query.rs` :

```rust
struct SplitUrl<'a> { base: &'a str, query: Option<&'a str>, fragment: Option<&'a str> }
fn split_url(url: &str) -> SplitUrl<'_>;            // premier '#', puis premier '?'
fn parse_query(query: &str) -> Vec<(String, String)>; // '&', premier '=', noms vides ignorés
fn build_url(split: &SplitUrl, enabled: &[(&str, &str)]) -> String;
```

- Après toute opération sur `EntrySection::QueryParams` ou
  `QueryParam{Value,Enabled}`, le brouillon recalcule `url.current` par
  `build_url` avec les entrées activées. Si `url.current` redevient égal à
  l'original, rien n'est réécrit.
- Après `FieldEdit::Url`, le brouillon remplace positionnellement les
  entrées activées de la section (la i-ème prend la i-ème paire), ajoute
  les paires excédentaires en fin (`origin: None`), retire les activées
  excédentaires ; les désactivées ne bougent pas.

Écart volontaire avec Bruno : son `requestUrlChanged` concatène
`activées + désactivées` et déplace donc les désactivées en fin de liste.
On les laisse en place pour ne pas réécrire des lignes que l'utilisateur
n'a pas touchées ; `bru run` ne lisant que l'URL, l'effet à l'exécution est
identique.

Les paires issues d'une URL modifiée passent par la même validation que
les clés et valeurs saisies (D4) : une URL dont la chaîne de requête
produirait une clé invalide (par exemple `?~a=1`, qui s'écrirait comme une
entrée désactivée) est refusée comme le serait l'ajout de cette clé.

Pas de synchronisation sur les fichiers chargés : un fichier incohérent le
reste tant qu'aucune modification ne touche l'URL ou `params:query`
(scénario « En-tête sans effet sur l'URL »).

### D4. Validation et erreurs

```rust
pub enum EntryProblem { EmptyKey, KeyWhitespace, KeyColon, KeyLeadingMarker,
                        QueryKeyReserved, QueryValueReserved }
EditError::InvalidEntry { block: &'static str, index: Option<usize>, problem: EntryProblem }
```

Les caractères refusés dans une clé suivent la grammaire de
`@usebruno/lang` (`keychar = ~(tagend | st | nl | ":") any`) plus `~` et
`"` en tête, qui changeraient le sens de la ligne. Pour `params:query`,
`&`, `#` (clé et valeur), `=` (clé) et le saut de ligne (valeur) sont
refusés parce que `build_url` → `parse_query` ne les restituerait pas : le
fichier écrit ne se relirait pas avec les mêmes paramètres. Le `Display`
d'`EntryProblem` décrit la nature du problème sans jamais citer la clé ou
la valeur, conformément à « Aucun secret journalisé ».

Conséquence sur l'existant : le test `multiline_target_edit_reloads_with_
expected_value` édite aujourd'hui un paramètre de requête avec une valeur
multi-ligne, désormais refusée. La fixture `multiline-target.bru` bascule
sur un en-tête (le scénario de spec correspondant a été modifié en ce
sens).

### D5. Aperçu = sérialisation en mémoire puis relecture par `bru-parser`

```rust
pub fn preview(ast: &BruFile, edits: &[FieldEdit]) -> Result<RequestView, EditError>;
```

Implémentation : `format::serialize(ast, edits)` → `BruFile::parse` →
`RequestView::from_ast`. Aucune I/O. L'égalité « aperçu = vue relue après
écriture » est ainsi vraie par construction et non par une seconde
implémentation à maintenir en phase.

Alternative écartée : dériver la vue directement du `Draft`. Moins de
travail CPU, mais deux chemins de calcul de la même vue, dont
l'équivalence devrait être démontrée par des tests. Coût de l'option
retenue : un parse de fichier `.bru` (quelques ko) par action validée
dans la session, pas par frappe — négligeable. Un échec de relecture
(qui signalerait un bogue du writer) est retourné comme
`EditError::Unreadable` sans citer le contenu.

`BruWriter::write_request` n'appelle pas `preview` : il sérialise une
seule fois ; les tests vérifient l'égalité des deux sur chaque fixture.

### D6. Session : journal ordonné et aperçu mis en cache

- `EditSession.pending` devient un journal ordonné, poussé tel quel. Seule
  coalescence conservée : une modification de valeur (ou d'URL, de corps)
  qui suit immédiatement une modification de valeur du même champ la
  remplace, pour qu'une suite de saisies validées sur un même champ
  n'allonge pas le journal. `update_or_push_pending`, qui fusionne par
  indice, disparaît : les indices ne sont plus stables.
- Nouveau champ `EditSession.preview: RequestView`, initialisé depuis la
  vue chargée par `start_edit` et recalculé par `update` après chaque
  ajout au journal. `view` le lit sans calcul (`view` reste pur).
- `field_value_committed`, `field_value` et `field_enabled` lisent
  l'aperçu (et le tampon de `TextInput` en saisie) au lieu de parcourir
  `pending` à rebours ; `display_name` utilise les clés de l'aperçu.
- `EditableField` gagne `AddRow(EntrySection)`, et `list_for` insère une
  ligne d'ajout après chaque section (exigence « Champs éditables et
  curseur de champ »). `fields` est recalculé depuis l'aperçu à chaque
  changement du journal, et `cursor` borné.
- Validation (« Refus immédiat ») : un helper `try_commit(model, edit)`
  appelle `writer::preview(ast, pending + [edit])` ; `Ok(view)` → push,
  cache de l'aperçu, `dirty = true` ; `Err(e)` →
  `StatusMessage::EditRefused(problème)`, saisie laissée ouverte, rien
  n'est poussé. `Espace` (bascule) et `d` passent par le même helper, qui
  ne peut pas échouer pour eux.
- `has_unsaved` est inchangé dans son principe (`dirty ||` saisie
  modifiée) ; pour une saisie de valeur de nouvelle entrée, la saisie est
  considérée modifiée dès lors que la clé a été validée (D7), puisque
  l'abandonner perdrait cette clé. `selection_locked` s'applique donc
  aussi pendant un ajout commencé.
- `edit_saved` : en succès, `pending` vidé, `preview` remplacé par la vue
  enregistrée, `fields` recalculé ; en échec, inchangé (exigence
  existante).

### D7. États de saisie et routage des touches

Les deux états de `improve-direct-editing` sont conservés :
`EditState::{FieldSelect, Input(TextInput)}`. La cible d'une saisie est
portée à côté, par un champ `EditSession.target: InputTarget`, significatif
seulement en `Input` :

```rust
pub enum InputTarget {
    /// Valeur du champ sous le curseur (URL, entrée, corps) : comportement actuel.
    Field,
    NewKey { section: EntrySection, return_cursor: usize },
    NewValue { section: EntrySection, key: String, return_cursor: usize },
    RenameKey { section: EntrySection, index: usize },
}
```

Pourquoi un champ à côté plutôt que des variantes supplémentaires
d'`EditState` : toutes ces saisies partagent exactement la capture
(`TextCapture::Input`), le `TextInput` à une ligne, l'annulation et le
suivi de défilement ; les dizaines de `matches!(state, EditState::Input(_))`
existants (capture, `has_unsaved`, rendu du curseur) restent justes sans
modification, et la spec garde deux états.

Transitions (toutes dans `update`, via `TextInput` pour le texte) :

| Depuis | Touche | Effet |
|---|---|---|
| `FieldSelect` sur `AddRow(s)` | `Entrée` ou `a` | `Input(TextInput::new("", false))`, `NewKey { s, cursor }` |
| `FieldSelect` sur entrée de `s` | `a` | idem, `return_cursor` = position courante |
| `FieldSelect` sur entrée | `c` | `Input(TextInput::new(clé, false))`, `RenameKey` |
| `FieldSelect` sur entrée | `d` | `try_commit(RemoveEntry)`, curseur borné |
| `NewKey` | `Entrée`/`Tab` | clé vérifiée par `preview(pending + [AddEntry{clé, ""}])` ; `Ok` → `NewValue`, nouveau `TextInput` vide ; `Err` → message, saisie ouverte |
| `NewKey` | `Ctrl+S` | `try_commit(AddEntry{clé, ""})` puis `save_edit` si `Ok` |
| `NewValue` | `Entrée`/`Tab`/`Ctrl+S` | `try_commit(AddEntry{clé, valeur})`, curseur sur la nouvelle entrée ; `Ctrl+S` enregistre ensuite |
| `NewKey`/`NewValue` | `Échap` | `FieldSelect`, `cursor = return_cursor`, rien de poussé |
| `RenameKey` | `Entrée`/`Tab`/`Ctrl+S` | `try_commit(RenameKey)` si la clé a changé |
| toute saisie | `Échap` | comportement actuel (`cancel_input`) |

- Touches : `a` et `d` produisent aujourd'hui `AddSecret`/`ForgetSecret`,
  traités seulement si le focus est sur Secrets. Ils sont renommés en
  `Message::Add`/`Message::Delete` et aiguillés par `update` vers le
  panneau Secrets (focus Secrets) ou la session (`FieldSelect`). `c`
  produit un nouveau `Message::Rename`, sans effet hors session. En
  `Input`, la capture existante intercepte ces lettres avant la table
  globale : rien à ajouter pour satisfaire « rien en Saisie ».
- `Ctrl+S` (`Message::SaveEdit`) : `validate_input` devient
  `commit_input(model) -> bool` qui traite la cible ; `SaveEdit` n'appelle
  `save_edit` que si le commit a réussi (ou s'il n'y avait pas de saisie).
  La seule différence entre `Tab` et `Ctrl+S` est en `NewKey` (passage à
  la valeur contre ajout à valeur vide).
- `Espace` sur `AddRow` : sans effet (le `match` de `toggle_field` ignore
  cette variante), comme `d` et `c`.

### D8. Rendu

- `request_text_and_fields` lit `session.preview` pour l'URL, les
  en-têtes et les paramètres (clés comprises). Hors session, rendu
  inchangé (« aucun » pour une section vide, pas de ligne d'ajout).
- En session, chaque section se termine par sa ligne « + Ajouter … »,
  enregistrée dans `FieldLines` comme les autres champs ; une section vide
  n'affiche que cette ligne. Pendant `NewKey`/`NewValue`, une ligne
  provisoire `  clé: valeur` est rendue juste avant la ligne d'ajout, et
  `FieldLines` y associe la position de saisie pour que
  `cursor_position_in_detail` et `scroll_edit_into_view` placent le
  curseur de texte sur la clé ou la valeur. Pendant `RenameKey`, la clé de
  l'entrée affiche le tampon et le curseur est placé dans la clé
  (préfixe de deux espaces).
- `session_help_line` : en `FieldSelect` sur une entrée, ajoute
  `a ajouter · d supprimer · c renommer` ; sur une ligne d'ajout,
  `Entrée ajouter` ; en `Input`, le libellé nomme la cible (`Ajout ·
  En-têtes · clé`, `Renommage · En-tête Accept`) avant les touches
  habituelles. `StatusMessage::EditRefused` s'insère parmi les messages
  prioritaires existants. Aucune nouvelle couleur : jetons existants de
  `visual-theme`.

### D9. Fixtures et vérification par Bruno

Nouveaux cas dans `tests/fixtures/collections/writer-cases/`, chacun avec
son `*.after.bru` versionné :

| Fixture | Opérations |
|---|---|
| `add-header.bru` | ajout à un `headers` existant de 3 entrées, dont une `~` |
| `add-block.bru` | GET sans `params`/`headers` : ajout d'un en-tête (bloc créé après la méthode) |
| `add-path-between.bru` | `params:query` + `headers` : ajout d'un paramètre de chemin (bloc inséré entre) |
| `remove-entry.bru` | suppression au milieu, suppression de la dernière entrée d'un bloc (bloc retiré) |
| `rename-disabled.bru` | renommage d'une entrée `~` |
| `duplicates.bru` | ajout d'un `Accept` déjà présent, `?tag=a&tag=b` |
| `query-sync.bru` | ajout, désactivation, suppression d'un paramètre de requête avec fragment `#` |
| `url-sync.bru` | modification d'URL recalculant `params:query` avec entrée `~` intercalée |
| `crlf-add.bru` | ajout et suppression en `\r\n` |

Vérification Bruno :

- `scripts/bruno-lang-dump.js` : charge `@usebruno/lang` depuis
  `$BRUNO_LANG_DIR`, sinon depuis `$(npm root -g)/@usebruno/cli/
  node_modules/@usebruno/lang`, applique `bruToJsonV2` à un fichier et
  imprime `{ url, headers, params }` (JSON trié, stable).
- `scripts/gen-writer-bruno-fixtures.sh` : exécute ce script sur le
  `*.after.bru` de chaque fixture du tableau ci-dessus et écrit
  `*.after.bruno.json` à côté (même patron que `gen-report-fixtures.sh`).
  Les fixtures antérieures sont exclues : `json-body` porte volontairement
  un bloc inconnu que le parser de Bruno refuse.
- `tests/writer_bruno_lang.rs` : test `#[ignore]` qui relance le script et
  compare à `*.after.bruno.json` (vérifie que le fichier est toujours relu
  ainsi par Bruno) ; test **non ignoré** qui compare `*.after.bruno.json` à
  la vue de `bru-parser` sur le même `*.after.bru` (notre relecture doit
  s'accorder avec ce que Bruno a réellement produit — pas de structure
  devinée, et sans Node dans la suite par défaut).

Aucune dépendance Rust ajoutée ; `serde_json` est déjà présent pour lire
les JSON de référence.

### D10. Points de contact avec les autres changements

- **`improve-direct-editing`** (archivé, dans `main`) : ce changement se
  construit dessus. Réutilisés tels quels : les deux états, `TextInput`,
  la capture `Input`, `Ctrl+S`, l'annulation par `Échap`, `FieldLines`,
  `scroll_edit_into_view`, `selection_locked`. Modifiés : `pending` n'est
  plus fusionné par indice et les valeurs affichées viennent de l'aperçu
  (D6), `EditableField` gagne `AddRow` (D6), `validate_input` devient
  `commit_input` (D7). Les tests de `update.rs` et `model.rs` qui comptent
  les positions de curseur ou lisent `pending` devront être adaptés aux
  lignes d'ajout.
- **`add-status-panel`** (archivé, dans `main`) : aucun recouvrement de
  comportement. Le panneau Statut n'affiche ni URL ni en-tête ; les refus
  et échecs de sauvegarde restent dans la barre d'aide de la session, pas
  dans le panneau. Le calcul de hauteur du détail continue de passer par
  `layout_for`, qui intègre déjà le panneau.
- **`add-mouse-support`** (implémenté en parallèle, non touché ici) :
  un clic sur une ligne du détail en session doit viser les
  `EditableField` de `FieldLines`, désormais construits depuis l'aperçu
  et incluant `AddRow` ; les lignes bougent après un ajout ou une
  suppression, aucune correspondance ligne → champ ne doit être
  mémorisée. La ligne provisoire d'ajout ne doit pas être cliquable. Un
  clic qui changerait de requête pendant un ajout commencé est refusé par
  `selection_locked` (D6). Conflit textuel probable sur `Message`
  (`AddSecret`/`ForgetSecret` renommés en `Add`/`Delete`) et
  `EditableField` à la fusion.

## Risks / Trade-offs

- [Un fichier dont l'URL et `params:query` divergent est réaligné à la
  première modification de paramètre de requête] → comportement de Bruno,
  visible dans l'aperçu avant `Ctrl+S` ; documenté dans la spec.
- [Valeur avec espaces en tête ou en fin] → `bru-parser` les retire à la
  lecture ; l'aperçu (D5) les montre donc déjà retirés, pas de surprise
  après sauvegarde, mais la saisie est silencieusement normalisée.
- [Paramètres de chemin dupliqués ou sans segment `:nom` dans l'URL] →
  acceptés ; `bru run` n'utilisera que le premier de même nom et ignorera
  les autres. Pas d'avertissement dans ce changement.
- [Suppression sans confirmation ni undo, choix validé] → la suppression
  n'est effective qu'à `Ctrl+S` ; `Échap` en Sélection de champ, avec
  confirmation, abandonne toute la session.
- [Lignes « + Ajouter » qui allongent le détail en session] → trois lignes
  au plus, visibles seulement en session ; le suivi de défilement existant
  garde le curseur visible.
- [Parse complet à chaque action validée] → fichiers de quelques ko,
  hors du chemin de frappe ; à mesurer si des corps volumineux posent
  problème.
- [Chemin de `@usebruno/lang` dépendant de l'installation de `bru`] →
  surcharge par `BRUNO_LANG_DIR` ; test ignoré par défaut ; la comparaison
  non ignorée repose sur les JSON versionnés.
- [Conflit de fusion avec `add-mouse-support` sur `Message` et
  `EditableField`] → voir D10 ; à résoudre au merge, sans changement de
  comportement.

## Migration Plan

Aucune migration de données : les fichiers `.bru` existants ne sont
touchés que par une sauvegarde explicite. Retour arrière : revert du
changement ; les fichiers écrits restent lisibles par Bruno (vérifié par
D9).
