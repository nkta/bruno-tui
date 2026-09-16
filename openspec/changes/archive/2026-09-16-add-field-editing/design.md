## Context

Voir `proposal.md` pour la motivation et `specs/field-editing/spec.md` +
`specs/tui-shell/spec.md` (delta) pour le contrat. État du code et des
changements sœurs, vérifié en lisant leurs fichiers :

- `add-bru-writer` (`openspec/changes/add-bru-writer/design.md`) expose
  `trait RequestWriter { fn write_request(&self, path: &Path, ast:
  &BruFile, stamp: &FileStamp, edits: &[FieldEdit]) -> Result<FileStamp,
  WriteError>; }`, implémenté par `BruWriter`, dans un futur module
  `src/writer/`. `FieldEdit` a sept variantes (`Url`, `HeaderValue{index,
  value}`, `HeaderEnabled{index, enabled}`, `QueryParamValue{index,
  value}`, `QueryParamEnabled{index, enabled}`, `PathParamValue{index,
  value}`, `PathParamEnabled{index, enabled}`, `BodyText(String)`) ;
  plusieurs éditions sur le même indice sont fusionnées par le writer, pas
  besoin de les dédupliquer avant l'appel. `FileStamp::capture(path) ->
  io::Result<Self>` est une fonction de lecture de métadonnées seule.
  `WriteError::Stale` signale un fichier changé depuis l'instantané.
- `add-request-run` (`openspec/changes/add-request-run/design.md`)
  introduit `pub enum Command { None, StartRun { .. } }` et change la
  signature d'`update` en `fn update(model: &mut Model, message: Message)
  -> Command`, exécutée uniquement par la boucle (`src/app/mod.rs::run`),
  qui seule fait de l'I/O. C'est le patron que ce changement réutilise
  pour l'écriture.
- `add-search-and-yank` (`openspec/changes/add-search-and-yank/design.md`,
  D2) modifie `to_message` en `fn to_message(event: AppEvent, capture:
  Option<TextCapture>) -> Option<Message>`, où `TextCapture` est une
  énumération délibérément ouverte et `capture` est lu par `run()` sur le
  modèle (`Model::text_capture()`) juste avant l'appel, pour dérouter la
  saisie de caractères vers des messages dédiés plutôt que vers la table
  de raccourcis. Ce changement ajoute la variante `Insert` à cette
  énumération pour la saisie en mode Insert (D4), plutôt que d'introduire
  un mécanisme concurrent.
- `src/app/view/detail.rs::request_text` (actuel, `tui-shell`) construit
  le détail d'une requête dans cet ordre : titre, chemin, ligne
  `MÉTHODE URL`, `Auth`, section « En-têtes » (une ligne par entrée,
  `entries()`), section « Paramètres de requête », section « Paramètres
  de chemin », `Corps` (type puis contenu ou « bloc absent »), puis les
  indicateurs de scripts/tests/assertions. `entries()` affiche `aucun` si
  la liste est vide, sinon une ligne par `KeyValue` avec `(désactivé)` en
  suffixe si `!enabled`.
- `RequestView.headers`/`.query_params`/`.path_params` sont des
  `Vec<KeyValue>` dans l'ordre exact des `Entry` de l'AST (garanti par
  `bru-parser`, réutilisé tel quel par `bru-writer` pour ses indices) :
  la position `i` dans ces vecteurs est la même que l'`index` attendu par
  `FieldEdit::HeaderValue`, etc.
- `message.rs::key_message` traite `Ctrl+C` avant tout, puis un `match`
  sur `KeyCode` sans modificateur (Alt exclu). `e`, `i`, `w`, `Espace` ne
  sont utilisées par aucune liaison existante ni par les deux autres
  changements sœurs (`r`, `Ctrl+X` pour `add-request-run` ; `/`, `n`, `N`,
  `v`, `y` pour `add-search-and-yank`).
- `TreeNode::path()` existe déjà (`src/collection/tree.rs`) ; il n'existe
  en revanche aucun moyen de retrouver, ni de remplacer, un nœud de
  `Collection.tree` par son chemin — seul `Model::node_at` le fait par
  adresse (`Vec<usize>`), pour la lecture.

## Goals / Non-Goals

**Goals :**
- Réutiliser tels quels les contrats déjà fixés par `add-bru-writer`
  (`FieldEdit`, `FileStamp`, `RequestWriter`) et `add-request-run`
  (`Command`, invariant « `update` sans I/O ») plutôt que d'en inventer
  des équivalents.
- Une session d'édition simple : un curseur de champ, un curseur de
  texte, un indicateur modifié — pas de undo/redo, pas d'édition
  multi-requête simultanée.
- Cohérence avec `add-search-and-yank` sur le mécanisme de capture de
  saisie : extension de son énumération `TextCapture` par une nouvelle
  variante (D4), pas un mécanisme concurrent.

**Non-Goals :**
- Résoudre l'ordre d'implémentation entre les trois changements sœurs :
  ce document suppose leurs contrats tels que rédigés à la date de cette
  proposition ; un écart sera visible et corrigible à l'implémentation
  (le mécanisme `/opsx:apply` permet de revoir les artefacts).
- Undo/redo, historique des éditions, édition simultanée de deux champs.
- Toute forme de validation sémantique de la valeur saisie (une URL
  vide ou un JSON invalide restent acceptés, comme `bru-parser`/
  `bru-writer` les acceptent).

## Decisions

### D1. Une session d'édition, superposée au focus Détail

```rust
pub struct EditSession {
    /// Chemin de la requête éditée, relatif à la racine ; la session se
    /// ferme sans confirmation si la sélection change de nœud.
    pub path: PathBuf,
    pub stamp: writer::FileStamp,
    /// Champs éditables, dans l'ordre d'affichage (D2).
    pub fields: Vec<EditableField>,
    /// Indice du champ sous le curseur, dans `fields`.
    pub cursor: usize,
    pub mode: EditMode,
    /// Modifications en attente, une par cible touchée (fusionnées par
    /// écrasement, dans l'ordre de `FieldEdit` attendu par bru-writer :
    /// pas besoin de dédupliquer davantage, le writer le fait déjà).
    pub pending: Vec<writer::FieldEdit>,
    pub dirty: bool,
}

pub enum EditMode {
    Normal,
    /// Curseur de texte en indice de caractère (pas d'octet) dans
    /// `EditableField::value`.
    Insert { text_cursor: usize },
}

pub enum EditableField {
    Url,
    HeaderValue(usize),
    QueryParamValue(usize),
    PathParamValue(usize),
    BodyText,
}
```

`Model` gagne `pub editing: Option<EditSession>` et
`pub confirm: Option<PendingConfirm>` (D6). Une session n'existe que si
`Focus == Detail` et que le nœud sélectionné est une requête ; changer de
nœud pendant qu'une session est ouverte referme la session sans
confirmation (une modification perdue par changement de sélection sans
confirmation serait surprenante ; c'est traité par la Requirement
« Confirmation avant de quitter... » qui ne couvre que la fermeture de
l'application et de la session par `Échap`, pas le changement de
sélection — documenté ici comme limite volontaire pour garder `update`
simple, cf. Risks).

`EditableField` ne porte que l'identité du champ (pas sa valeur) : la
valeur affichée est recalculée à la demande depuis
`RequestView`/`EditSession.pending` (D3), pour n'avoir qu'une seule
source de vérité par champ plutôt que deux copies à synchroniser.

Pourquoi une session plutôt qu'un état d'édition par requête dans
`Model.run` ou ailleurs : une seule session à la fois (comme une seule
exécution à la fois dans `add-request-run`) simplifie le rendu et la
confirmation de fermeture, et correspond à l'usage réel (éditer un champ,
sauvegarder, passer à la requête suivante).

### D2. Construction de la liste des champs éditables

`EditableField::list_for(view: &RequestView) -> Vec<EditableField>` :
`Url`, puis `HeaderValue(0..headers.len())`, puis
`QueryParamValue(0..query_params.len())`, puis
`PathParamValue(0..path_params.len())`, puis `BodyText` seulement si
`view.body` est `Some` et que son `BodyKind` est l'une de `Json | Text |
Xml | Sparql | Graphql` (les seules formes que `bru-writer` accepte en
écriture — `FormUrlEncoded | MultipartForm | File | Other(_)` sont
exclues de la liste, pas seulement refusées à la sauvegarde : le curseur
ne s'y arrête jamais). Cette fonction est appelée une fois à l'ouverture
de la session ; elle ne se recalcule pas pendant la session (un champ
désactivé reste éditable, seul son état d'activation change).

Pourquoi exclure les corps non textuels de la liste plutôt que de laisser
le curseur s'y arrêter et échouer à la sauvegarde : un champ qu'on ne
peut jamais modifier ne devrait pas apparaître comme un arrêt de
navigation — cohérent avec le principe de moindre surprise déjà appliqué
par `tui-shell` (« sans effet » documenté explicitement pour chaque cas
limite de navigation).

### D3. Valeur affichée d'un champ : vue + éditions en attente

`fn field_value(session: &EditSession, view: &RequestView, field: &EditableField) -> &str`
lit d'abord `session.pending` pour une édition déjà faite sur ce champ
(dernière valeur de type `*Value`/`BodyText` trouvée, en parcourant
`pending` dans l'ordre, la plus récente gagnant), sinon retombe sur la
valeur d'origine dans `view`. Idem pour l'état activé/désactivé d'un
en-tête ou paramètre (`*Enabled`). Cette fonction est pure et utilisée à
la fois par `view/detail.rs` (rendu) et par `update.rs` (valeur de départ
en entrant en mode Insert).

Pourquoi ne pas maintenir un `RequestView` modifié en parallèle : `view`
reste la source de vérité chargée, `pending` la liste, courte en
pratique, des écarts — évite une resynchronisation à chaque frappe et
rend `field_value` trivialement testable sans reconstruire un `RequestView`
complet.

### D4. `TextCapture::Insert`, une variante ajoutée à un type conçu pour ça

`add-search-and-yank` (D2) introduit `to_message(event, capture:
Option<TextCapture>)` avec une énumération `TextCapture` explicitement
ouverte à d'autres sources de saisie, précisément pour ce cas. Ce
changement ajoute la variante `Insert` à cette énumération et le bras
correspondant à `capture_message`, dans les mêmes fichiers
(`src/app/message.rs`) :

```rust
pub enum TextCapture {
    Search,   // add-search-and-yank
    Insert,   // ajouté ici
}

fn capture_message(event: AppEvent, capture: TextCapture) -> Option<Message> {
    match capture {
        TextCapture::Search => search_capture_message(event),
        TextCapture::Insert => insert_capture_message(event),
    }
}
```

Si `add-search-and-yank` est déjà appliqué, ce sont des ajouts à un fichier
existant (nouvelle variante d'enum, nouveau bras de `match`, nouvelle
fonction), pas une redéfinition. Si `add-field-editing` est implémenté
seul ou en premier, il crée lui-même `TextCapture` avec uniquement la
variante `Insert` ; `add-search-and-yank` ajoutera alors `Search` à son
tour, symétriquement.

`Model::text_capture()` (définie par `add-search-and-yank`, D8 de son
design) gagne une branche :

```rust
pub fn text_capture(&self) -> Option<TextCapture> {
    if self.editing.as_ref().is_some_and(|s| matches!(s.mode, EditMode::Insert { .. })) {
        return Some(TextCapture::Insert);
    }
    self.search.as_ref()
        .is_some_and(SearchState::is_editing)
        .then_some(TextCapture::Search) // add-search-and-yank
}
```

L'ordre des deux branches n'a pas besoin d'être arbitré : par
construction (cf. `add-search-and-yank`/D2, « pourquoi l'exclusion
mutuelle n'a pas besoin d'être vérifiée explicitement »), une session
d'édition et une recherche ne peuvent pas être ouvertes en même temps —
tant que l'une est active, ses propres touches interceptent tous les
caractères avant que `/` (ouverture de la recherche) ou `i`/`e`
(ouverture de l'édition) ne soient jamais atteintes par la table
hors-saisie.

`insert_capture_message` produit (avant la table existante, `Ctrl+C`
restant prioritaire, géré une seule fois en amont par `to_message` et
jamais dupliqué dans les fonctions de capture) :
- `KeyCode::Char(c)` sans `Ctrl` → `Message::InsertChar(c)` (`Alt` inclus,
  comme la recherche) ;
- `KeyCode::Backspace` → `Message::InsertBackspace` ;
- `KeyCode::Left` → `Message::InsertCursorLeft` ;
- `KeyCode::Right` → `Message::InsertCursorRight` ;
- `KeyCode::Enter` → `Message::InsertEnter` (interprété par `update` selon
  le champ courant : nouvelle ligne sur `BodyText`, fin d'Insert sinon) ;
- `KeyCode::Esc` → `Message::LeaveInsert` ;
- toute autre touche → ignorée, comme le fait déjà la recherche pour les
  touches non liées.

Parce que `capture_message` distribue explicitement vers
`insert_capture_message` ou `search_capture_message` selon la variante
reçue, `to_message` n'a plus besoin de deviner : c'est la variante de
`TextCapture`, pas seulement sa présence, qui détermine le jeu de
messages produit. `update` reçoit donc toujours le bon message dès la
sortie de `to_message`, sans avoir à distinguer les sources après coup.

### D5. Nouveaux messages et liaisons hors saisie

```rust
pub enum Message {
    // ... variantes existantes et celles d'add-request-run inchangées ...
    StartEdit,              // 'e', hors saisie
    MoveFieldCursor(i8),    // '↓'/'j' (+1), '↑'/'k' (-1), en session Normal
    ToggleField,            // 'Espace', en session Normal
    EnterInsert,            // 'i', en session Normal
    SaveEdit,               // 'w', en session Normal
    LeaveInsert,            // Échap, en saisie (D4)
    InsertChar(char),
    InsertBackspace,
    InsertCursorLeft,
    InsertCursorRight,
    InsertEnter,
    /// Renvoyé par la boucle après `Command::SaveEdit`.
    EditSaved { path: PathBuf, result: Result<SavedEdit, writer::WriteError> },
    ConfirmYes,             // réponse à un PendingConfirm
    ConfirmNo,
}
```

Liaisons hors saisie, dans `key_message` (aucune ne collisionne avec
l'existant ni avec les deux changements sœurs, vérifié par lecture de
leurs designs) : `e` → `StartEdit` (seulement pertinent si
`Focus::Detail` et pas de session — sinon `update` l'ignore,
`Command::None`) ; `j`/`↓` et `k`/`↑`, quand une session est ouverte en
mode Normal, → `MoveFieldCursor(±1)` au lieu du défilement habituel du
détail (branché dans `update`, pas dans `key_message`, qui reste neutre
vis-à-vis du modèle — cf. commentaire de module de `message.rs`) ;
`Espace` → `ToggleField` ; `i` → `EnterInsert` ; `w` → `SaveEdit` ; `y`/`n`
(ou `Entrée`/`Échap`, à trancher en implémentation selon ce que retient
`add-diagnostics-and-history`/`add-response-filter` pour leurs propres
confirmations éventuelles — aucune connue à ce jour) → `ConfirmYes` /
`ConfirmNo` quand `model.confirm.is_some()`, prioritaire sur toute autre
interprétation de la touche.

### D6. Confirmation, unifiée pour l'édition et la fermeture

```rust
pub enum PendingConfirm {
    /// Fermer la session en cours (Échap) malgré des modifications.
    DiscardEdit,
    /// Fermer l'application (`q` ou `Ctrl+C`) malgré une session modifiée.
    QuitWithUnsavedEdit,
}
```

`Model.confirm: Option<PendingConfirm>`. `Quit`/`ForceQuit` : si
`model.editing.as_ref().is_some_and(|s| s.dirty)`,
`model.confirm = Some(QuitWithUnsavedEdit)` au lieu de fixer `model.exit`
directement. `LeaveInsert`... non, fermeture de session : c'est
`Message` dédié — en mode Normal de session, `Échap` (traduit par
`to_message` comme aujourd'hui, hors saisie car on est en mode Normal) →
si `session.dirty`, `model.confirm = Some(DiscardEdit)`, sinon fermeture
immédiate de la session (`model.editing = None`). `ConfirmYes` avec
`confirm == Some(DiscardEdit)` → ferme la session ; avec
`Some(QuitWithUnsavedEdit)` → fixe `model.exit = Some(Exit::Normal)`.
`ConfirmNo` dans les deux cas → `model.confirm = None`, aucun autre effet.
Tant que `model.confirm.is_some()`, la barre d'état/mode affiche la
question à la place de son contenu habituel (D8).

Pourquoi un type unique pour les deux confirmations plutôt que deux
booléens séparés : une seule confirmation peut être active à la fois (on
ne peut pas fermer l'application et fermer la session en même temps), un
`Option<enum>` rend cet invariant visible dans le type plutôt qu'à
vérifier par convention.

### D7. Sauvegarde par `Command`, symétrique à `Command::StartRun`

```rust
pub enum Command {
    None,
    StartRun { .. },  // add-request-run, inchangé
    SaveEdit {
        path: PathBuf,
        ast: collection::BruFile,  // clone, BruFile: Clone (bru-parser)
        stamp: writer::FileStamp,
        edits: Vec<writer::FieldEdit>,
    },
}
```

`update` sur `SaveEdit` (D5), avec une session modifiée : clone
`session.pending` et l'`ast` du `RequestNode` courant (retrouvé par
`model.selected_node()`, garanti être la requête de la session puisque
changer de sélection ferme la session — D1), retourne `Command::SaveEdit`
sans modifier `session.dirty` ni `session.pending` (l'écriture n'a pas
encore eu lieu). La boucle (`run()`, `src/app/mod.rs`) exécute :

```rust
Command::SaveEdit { path, ast, stamp, edits } => {
    let root = /* racine de la collection courante */;
    let writer: Arc<dyn writer::RequestWriter> = /* injecté, BruWriter en usage réel */;
    let full_path = root.join(&path);
    tokio::task::spawn_blocking(move || {
        let result = writer.write_request(&full_path, &ast, &stamp, &edits)
            .and_then(|new_stamp| {
                let bytes = std::fs::read(&full_path).map_err(|source| {
                    writer::WriteError::Io { path: full_path.clone(), source }
                })?;
                let text = String::from_utf8(bytes)
                    .map_err(|_| writer::WriteError::Io { /* .. */ })?;
                let ast = collection::BruFile::parse(text)
                    .map_err(|_| writer::WriteError::Io { /* .. */ })?; // voir Risks
                let view = collection::RequestView::from_ast(&ast)
                    .map_err(|_| writer::WriteError::Io { /* .. */ })?;
                Ok(SavedEdit { stamp: new_stamp, ast, view })
            });
        let _ = sender.send(AppEvent::EditSaved { path, result }).await;
    });
}
```

Sur `Message::EditSaved { path, result }` dans `update` : si
`model.editing.as_ref().is_some_and(|s| s.path == path)` (garde contre un
événement obsolète, même principe que `RunFinished`/`id` dans
`add-request-run`) : succès → remplace `RequestNode.ast`/`.view` du nœud à
ce chemin (nouvelle fonction `Collection`-adjacente, pas dans
`bru-parser`, cf. Risks) par ceux reçus, vide `session.pending`, `dirty =
false`, `session.stamp = nouveau`, recalcule `session.fields` (D2) sur la
nouvelle vue (les indices peuvent différer si... non, `bru-writer` ne
change ni le nombre ni l'ordre des entrées, donc `fields` reste
identique en pratique — recalculé par simplicité et robustesse plutôt que
supposé stable) ; échec → message d'erreur affiché (D8), session
inchangée, modifications conservées (Requirement « Échec de sauvegarde
sans perte »).

Pourquoi relire et reparser après écriture plutôt que reconstruire l'AST
en mémoire à partir des `edits` : `bru-writer` ne garantit la fidélité
qu'à l'octet du fichier réellement écrit ; reconstruire « à la main » un
`BruFile` équivalent dupliquerait la logique de formatage du writer
(D3/D3 de `bru-writer`) pour un gain de performance négligeable (un
fichier `.bru` fait quelques kilo-octets). Le rechargement se fait dans le
même `spawn_blocking` que l'écriture, donc sans aller-retour
supplémentaire sur le canal d'événements.

### D8. Rendu : curseur de champ, mode Insert, confirmation

- `view/detail.rs` : une nouvelle fonction, prenant `Option<&EditSession>`
  en plus de `RequestView`, décore la ligne du champ sous le curseur
  (fond distinct, cohérent avec la sélection déjà utilisée par l'arbre) et
  substitue la valeur affichée par `field_value` (D3) plutôt que la valeur
  brute de `view` pour les champs qui ont une édition en attente. En mode
  Insert, insère un curseur de texte visible (par exemple un caractère
  souligné ou un curseur de terminal positionné, à trancher en
  implémentation selon ce que permet `ratatui::Frame::set_cursor_position`)
  à `text_cursor`.
- `view/mod.rs::status_line` (ou une ligne dédiée juste en dessous du
  panneau détail, à trancher en implémentation selon l'espace disponible)
  affiche, par priorité : la question de confirmation si
  `model.confirm.is_some()` ; sinon, si une session est ouverte, le mode
  (`-- NORMAL --` / `-- INSERT --`) et le nom du champ sous le curseur ;
  sinon le contenu habituel de `tui-shell`.
- Le nom d'un champ pour l'affichage (`Url`, `En-tête <clé>`, `Paramètre
  de requête <clé>`, `Paramètre de chemin <clé>`, `Corps`) est dérivé de
  `EditableField` et de la `RequestView`, dans une fonction pure
  réutilisée par les tests.

### D9. Fixtures et tests

Réutilise `tests/fixtures/collections/writer-cases/` telles que définies
par `add-bru-writer` (`simple.bru`, `headers.bru`,
`multiline-target.bru`, `json-body.bru`, `crlf.bru`, `form-body.bru`) :
ce changement n'a pas besoin de fixtures supplémentaires, seulement
d'exercer `bru-writer` à travers `update`/`Command` plutôt que
directement.

Tests :
- unitaires `model.rs`/`update.rs` : construction de `EditableField::list_for`
  (URL+2 en-têtes+corps json → 4 champs ; corps `formUrlEncoded` → corps
  exclu) ; parcours du curseur avec extrémités ; bascule `Espace` sur
  en-tête, sans effet sur URL/corps ; entrée/sortie Insert, saisie de
  caractères avec `field_value`, `Entrée` sur champ simple vs corps,
  `dirty` inchangé sur un aller-retour Insert sans frappe ; `w` sans
  modification ne retourne pas `Command::SaveEdit` ; `w` avec modification
  retourne `Command::SaveEdit` avec les `pending` attendus ; `EditSaved`
  réussi vide `pending`, échec conserve tout ; confirmation : `Échap`
  avec session modifiée passe par `PendingConfirm::DiscardEdit`, `q` avec
  session modifiée par `QuitWithUnsavedEdit`, `ConfirmNo` annule sans
  effet, `ConfirmYes` applique l'effet différé ;
- `message.rs` : chaque nouvelle liaison hors saisie ; en saisie
  (`capture: Some(TextCapture::Insert)`), le jeu `InsertChar`/
  `InsertBackspace`/curseur/`InsertEnter`/`LeaveInsert`, dans le style de
  `every_key_binding` ;
- intégration `tests/app_shell.rs` (ou un fichier dédié
  `tests/field_editing.rs`) : ouverture d'une session sur une fixture de
  `writer-cases/`, édition de l'URL, `w`, vérification que le fichier sur
  disque a changé exactement comme attendu (à l'octet près hors la zone
  éditée, même méthode de comparaison que les tests de `bru-writer`), et
  que le détail affiché après `EditSaved` montre la nouvelle valeur ;
  un test de refus (`Stale` simulé en modifiant le fichier entre
  l'ouverture et la sauvegarde) vérifiant que rien n'est perdu à l'écran ;
  un test de confirmation de sortie avec session modifiée.

## Risks / Trade-offs

- [Trois changements sœurs modifient `update`/`message.rs`/`Command` en
  parallèle] → la capture de texte est réglée : `TextCapture` (défini par
  `add-search-and-yank`/D2) est étendu par une variante par changement
  sœur (`Insert` ici, `Filter` par `add-response-filter`), jamais
  redéfini. Les points d'intégration restants (`Command`, touches
  réservées) sont documentés précisément dans chaque design plutôt que
  de supposer un ordre d'implémentation ; un écart sera détecté à la
  compilation (`Command`/`Message`/`TextCapture` sont des enums
  exhaustifs) et corrigé via une mise à jour d'artefact (`/opsx:update`)
  avant `/opsx:apply`.
- [Rechargement complet du fichier après écriture plutôt que
  construction en mémoire] → une écriture réussie suivie d'un fichier
  externe corrompu entre le `rename` et la relecture (fenêtre très
  étroite) ferait échouer le rechargement avec une erreur `Io` généraliste
  peu informative ; acceptable, documenté, à affiner si constaté en
  pratique.
- [Remplacer `RequestNode.ast`/`.view` dans `Collection.tree` après une
  sauvegarde nécessite une recherche par chemin, absente aujourd'hui] →
  fonction ajoutée à `src/app/model.rs` (pas à `bru-parser`, qui n'a pas
  besoin de mutation), coût négligeable pour une collection de taille
  raisonnable, recherche linéaire par chemin comme le fait déjà
  `node_at` par adresse.
- [Changer de sélection avec une session ouverte la ferme sans
  confirmation] → limite assumée pour ne pas complexifier
  `navigate_tree`/`scroll_detail` avec une vérification de confirmation à
  chaque déplacement ; documentée dans D1, à revoir si des utilisateurs
  perdent réellement des modifications de cette façon.
- [Curseur de texte visuel dépend d'une API `ratatui::Frame` non encore
  vérifiée dans ce dépôt] → à sonder en implémentation (comme
  `unstable-rendered-line-info` l'a été pour `add-tui-shell`) ; repli
  possible sur un simple surlignage du caractère sous le curseur si
  `set_cursor_position` s'avère malcommode avec le `Terminal` actuel.
- [Valeur de champ contenant déjà `'''` littéral] → limite héritée de
  `bru-writer` (non échappée à l'écriture), non aggravée ni corrigée ici.
