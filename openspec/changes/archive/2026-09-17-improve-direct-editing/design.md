## Context

Motivation : voir `proposal.md`. Exigences : voir
`specs/field-editing/spec.md`.

État actuel observé dans le code (issu de `add-field-editing`) :

- `EditSession` (`src/app/model.rs`) porte `mode: EditMode { Normal,
  Insert { text_cursor, buffer } }`, `cursor` (indice de champ),
  `pending: Vec<FieldEdit>` et `dirty`. Le tampon n'est versé dans
  `pending` qu'à la sortie d'Insert (`leave_insert`), ce qui rend
  l'annulation triviale : il suffit de ne pas verser le tampon.
- La capture clavier passe par `Model::text_capture()` →
  `TextCapture::Insert` → `insert_capture_message` (`src/app/message.rs`),
  qui ne connaît que `Char`, `Backspace`, `←`, `→`, `Entrée`, `Échap`.
  `Ctrl+C`/`Ctrl+X` sont traités avant toute capture dans `key_message`.
- Hors capture, `Entrée` est traduite en `Message::Right`, comme `→` et
  `l` : `update` ne peut pas distinguer `Entrée` de `→` dans le détail.
- En session Normal, `update` détourne `Up`/`Down` vers
  `move_field_cursor` ; `i`, `w`, `Espace`, `e` ont leurs messages dédiés
  (`EnterInsert`, `SaveEdit`, `ToggleField`, `StartEdit`).
- `clear_detail_view_state` (appelée à tout changement de nœud :
  navigation, recherche, navigation croisée) fait `model.editing = None`
  **sans regarder `dirty`** : une session modifiée se perd en silence si
  l'utilisateur fait `Tab` puis `↓`.
- `cursor_position_in_detail` (`src/app/view/detail.rs`) recalcule la
  ligne du curseur avec des indices codés en dur (`3`, `7 + idx`, …) et
  une colonne en nombre de caractères, bornée à la largeur du panneau :
  le curseur disparaît au-delà de la largeur, et le détail ne défile pas
  pour le suivre.
- La confirmation (`PendingConfirm`) accepte `y`/`Entrée` et refuse
  `n`/`Échap` ; elle est hors périmètre et reste telle quelle.

## Goals / Non-Goals

**Goals:**

- Un modèle d'état à deux niveaux lisible (Sélection de champ / Saisie)
  qui remplace les modes vim sans ajouter de troisième état.
- Un éditeur de tampon pur (sans I/O, testable unitairement) couvrant
  curseur libre, `Suppr`, `Début`/`Fin`, déplacement vertical multi-ligne.
- Une règle d'arbitrage des touches qui se lit dans une seule table par
  état, sans exception cachée dans `update`.
- Aucune perte silencieuse de modifications non enregistrées.

**Non-Goals:**

- Ajouter, supprimer ou renommer des en-têtes/paramètres, ou éditer les
  clés : relève de `add-entry-management`.
- Collage entre crochets (`Event::Paste`), annuler/rétablir, sélection de
  texte, déplacement par mot : hors périmètre, voir Open Questions.
- Coloration syntaxique ou validation du JSON saisi dans le corps.
- Édition des corps de formulaire, scripts, tests, assertions.
- Toute modification de `bru-writer`, du format des `.bru` ou de la
  confirmation elle-même.

## Decisions

### D1. Remplacer les modes vim, ne pas les conserver en parallèle

Les modes Normal/Insert disparaissent ; `i` et `w` perdent leur sens
d'édition. Seul `e` est conservé comme **alias d'ouverture**, parce qu'il
ne crée aucun état supplémentaire et ne collisionne avec rien dans le
détail.

Alternatives écartées :
- *Deux jeux de touches coexistants (vim + direct)* : double la table de
  correspondance en Sélection de champ (`i` et `Entrée`, `w` et `Ctrl+S`)
  et surtout rend `Échap` ambigu en saisie (garder ou annuler ?), ce qui
  est exactement le défaut de clarté que la demande veut corriger.
- *Option de configuration `edit_keys = vim|direct`* : il n'existe pas de
  fichier de configuration utilisateur aujourd'hui ; en créer un pour ce
  seul besoin est disproportionné et double les tests.

`j`/`k` restent des alias de `↓`/`↑` en Sélection de champ : ce ne sont
pas des modes, ils existent déjà partout ailleurs dans l'application, et
les retirer leur rendrait leur sens de défilement du détail, ce qui
déplacerait l'affichage sans bouger le curseur de champ.

### D2. Modèle : `EditState` à deux variantes, tampon ligne/colonne dérivé

```rust
pub enum EditState {
    FieldSelect,
    Input(TextInput),
}

/// Éditeur de tampon pur, sans I/O (module `src/app/text_input.rs`).
pub struct TextInput {
    buffer: String,
    /// Position en caractères (jamais en octets).
    cursor: usize,
    multiline: bool,
    /// Valeur au début de la saisie, pour `is_modified()`.
    initial: String,
}
```

`TextInput` expose `insert(char)`, `backspace()`, `delete()`, `left()`,
`right()`, `home()`, `end()`, `up()`, `down()`, `newline()` (no-op si
`!multiline`), `text()`, `cursor_line_col()`, `is_modified()`. Les
positions ligne/colonne sont dérivées à la demande du tampon et de
`cursor` : pas de double représentation à synchroniser. `up`/`down`
visent la même colonne ou la fin de ligne ; aucune « colonne désirée »
mémorisée entre deux déplacements verticaux (voir Open Questions).

- **Annulation** : on remplace `Input` par `FieldSelect` sans verser le
  tampon. `pending` n'ayant jamais été touché, la valeur affichée
  redevient celle d'avant la saisie (chargée ou validée plus tôt), ce qui
  satisfait le scénario « Annulation après une validation précédente ».
- **Validation** : même logique que `leave_insert` aujourd'hui
  (`update_or_push_pending` si le tampon diffère de la valeur committée).
  `dirty` reste « au moins une modification validée ».
- **Modifications non enregistrées** (exigence de confirmation) :
  `session.has_unsaved() = dirty || matches!(state, Input(t) if
  t.is_modified())`. Utilisé par la sortie et par la garde de sélection
  (D6).

Alternative écartée : garder `EditMode::Insert { text_cursor, buffer }`
et ajouter les opérations dans `update.rs`. Le fichier fait déjà ~4600
lignes et les opérations de curseur sont de la logique de tampon pure,
plus simple à tester isolément.

### D3. Ouverture par `Entrée` : un message `Enter` distinct de `Right`

`key_message` traduit aujourd'hui `Entrée` en `Message::Right`. On
introduit `Message::Enter`, émis pour `KeyCode::Enter` hors capture, et
`update` le route :

- `Focus::Detail`, pas de session → `start_edit` ;
- `Focus::Detail`, session en `FieldSelect` → `begin_input` ;
- tout autre focus → traité comme `Message::Right` (comportement
  inchangé pour l'arbre, le sélecteur d'environnement, les diagnostics,
  l'historique, les secrets).

Le repli `Enter → Right` est fait en un seul point au début du bras de
navigation, pour que les spécifications existantes (`tui-shell`,
`environment-picker`, `secret-env-vars`) restent vraies sans relecture
de chaque panneau. La confirmation en attente accepte déjà `Right` ; elle
accepte aussi `Enter`.

Alternative écartée : décider dans `key_message` selon le focus. Le module
`message.rs` documente explicitement qu'il ignore le modèle hors capture ;
on garde cet invariant.

### D4. Table des touches par état

**Hors session** (focus Détail) : inchangé, plus `Entrée`/`e` → ouverture.

**Sélection de champ** (pas de capture de texte) :

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | curseur de champ |
| `Entrée` | commence la saisie |
| `Espace` | bascule activé/désactivé |
| `Ctrl+S` | enregistre |
| `Échap` | ferme la session (confirmation si modifiée) |
| autres | sens global inchangé (`Tab`, `q`, `r`, `/`, `v`, `y`, `PgPréc`/`PgSuiv`, `Début`/`Fin` de défilement, `D`/`H`/`E`/`S`…) |

**Saisie** (`TextCapture::Input`, remplace `TextCapture::Insert`) :

| Touche | Champ à une ligne | Corps |
|---|---|---|
| caractère (`Maj`/`Alt` inclus) | insère | insère |
| `←` `→` `Début` `Fin` | curseur | curseur (ligne courante pour `Début`/`Fin`) |
| `↑` `↓` | sans effet | ligne précédente/suivante |
| `Retour arrière` / `Suppr` | supprime avant / après | idem, joint les lignes |
| `Entrée` | valide | saut de ligne |
| `Tab` | valide | valide |
| `Échap` | annule | annule |
| `Ctrl+S` | valide puis enregistre | valide puis enregistre |
| `Ctrl+C` / `Ctrl+X` | sortie (confirmée) / annule l'exécution | idem |
| autre | ignorée | ignorée |

**Arbitrage des collisions.** Le mécanisme existant suffit : tant que
`text_capture()` renvoie `Some(Input)`, `capture_message` intercepte
toutes les touches sans `Ctrl` avant que la table hors-saisie (qui porte
`q`, `r`, `/`, `D`, `S`, …) ne soit consultée. `Tab` est capturé (donc ne
change pas le focus), ce qui garantit aussi qu'aucun panneau ne peut
prendre le focus au milieu d'une saisie. Les combinaisons `Ctrl` sont
traitées en amont, une seule fois, dans `key_message` : on y ajoute
`Ctrl+S → Message::SaveEdit` à côté de `Ctrl+C` et `Ctrl+X`. Hors session,
`SaveEdit` est sans effet.

En Sélection de champ, on ne capture pas le texte : c'est volontaire, car
ce n'est pas un état de saisie, et capturer ferait perdre `r`, `/`, `y`
sans bénéfice (aucune lettre n'y a de sens d'édition). La seule surcharge
de la table globale est faite dans `update`, comme aujourd'hui pour
`Up`/`Down`.

**Pourquoi `Ctrl+S` et `Tab`.** Le corps a besoin d'`Entrée` pour les
sauts de ligne, donc d'une autre touche de validation, identique sur
tous les champs pour rester prévisible. Alternatives écartées :
- `Ctrl+Entrée` : non transmise distinctement par la plupart des
  terminaux sans le protocole clavier kitty (crossterm ne l'active pas
  ici) ;
- `Alt+Entrée` : bascule plein écran dans Windows Terminal, utilisé par
  l'auteur sous WSL ;
- `Entrée` valide et `Ctrl+J` insère un saut de ligne (convention de
  certains formulaires) : inverse le geste le plus fréquent en édition
  de JSON ;
- `Échap` valide sur le corps : rend `Échap` ambigu d'un champ à l'autre.

`Tab` perd la possibilité d'insérer une tabulation dans le corps ;
l'indentation se fait en espaces (voir Risks). `Ctrl+S` est la touche
d'enregistrement la plus répandue ; en mode brut, crossterm désactive le
contrôle de flux `IXON`, elle arrive donc bien à l'application.

### D5. Sauvegarde

`Message::SaveEdit` (désormais `Ctrl+S`) : si la session est en `Input`,
on exécute d'abord la validation (D2), puis on repasse par `save_edit`
existant, qui produit `Command::SaveEdit` si `dirty`. `edit_saved` est
inchangé, à ceci près qu'il laisse la session en `FieldSelect` (c'est déjà
le cas puisqu'on y est revenu à la validation). La liaison `w` est
retirée de `key_message`, `i`/`EnterInsert` aussi.

### D6. Pas de perte silencieuse au changement de sélection

Plutôt qu'une confirmation avec action différée (il faudrait sérialiser
« navigation d'arbre `↓` », « résultat de recherche n° k », « cible de
navigation croisée »… dans `PendingConfirm`), on **refuse** le changement
de nœud tant que `has_unsaved()` :

- une fonction `selection_locked(model) -> bool` est consultée par les
  trois chemins qui modifient `model.tree.selected` vers un autre nœud :
  `navigate_tree`, `search_tree`, `cross_navigate_to_tree` ;
- `navigate_tree` calcule la nouvelle sélection sur une copie et ne
  l'applique que si le chemin du nœud ne change pas ou si la session
  n'est pas verrouillée ; replier/déplier le dossier déjà sélectionné
  reste donc permis ;
- en cas de refus : `last_status = StatusMessage::EditLocked` («
  Modifications non enregistrées : Ctrl+S pour enregistrer, Échap pour
  abandonner »), sélection inchangée ;
- `clear_detail_view_state` continue de fermer la session, mais n'est
  plus atteinte qu'avec une session propre.

Alternative écartée : confirmation « abandonner et changer de requête ».
Plus confortable, mais la mécanique d'action différée est trop coûteuse
pour ce changement et devra être revue de toute façon par
`add-mouse-support` (clic dans l'arbre) et `add-entry-management`
(suppression/renommage de la requête éditée). La garde unique
`selection_locked` leur offre un point d'accroche simple.

### D7. Rendu : correspondance champ → ligne, suivi vertical et horizontal

- `request_text_with_session` (détail) renvoie en plus une table
  `FieldLines { field → (première ligne, nombre de lignes, largeur du
  préfixe) }` construite au moment où les lignes sont poussées. Elle
  remplace les indices codés en dur de `cursor_position_in_detail`, qui
  devient une simple lecture de cette table + `TextInput::cursor_line_col`.
- **Suivi vertical** : `view` étant pur, le défilement est ajusté dans
  `update`, après chaque message de session (déplacement de champ,
  touche de saisie, validation), par une fonction
  `scroll_edit_into_view(model)` qui recalcule la table (le texte du
  détail est déjà recalculé par `detail_line_count` pour borner le
  défilement, on réutilise la même construction).
- **Suivi horizontal** : `EditSession` porte `hscroll: u16`, ajusté dans
  la même fonction pour que la colonne d'affichage du curseur tombe dans
  `[hscroll, hscroll + largeur)`. `view` applique ce décalage à la seule
  ligne (ou, pour le corps, aux seules lignes) du champ en saisie en
  tronquant ses spans à gauche ; les autres lignes restent non décalées.
- **Pas de retour à la ligne pendant une saisie** (précisé à
  l'implémentation) : le détail est rendu avec `Wrap`, et le défilement
  compte en lignes logiques ; une ligne longue repliée décalerait la
  ligne du curseur. Tant qu'une saisie est en cours sur la requête
  affichée, le détail est rendu sans `Wrap` : une ligne logique = une
  ligne affichée, le curseur tombe exactement, et c'est `hscroll` qui
  rend visible la partie utile de la ligne éditée. Les autres lignes
  longues sont tronquées le temps de la saisie, puis de nouveau repliées.
  Le décalage ne s'applique qu'à la valeur : le préfixe (`GET `,
  `  clé: `) reste visible.
- **Largeur d'affichage** : la colonne du curseur est la largeur du
  préfixe du texte avant le curseur, mesurée avec `Span::width()` de
  ratatui (qui s'appuie sur `unicode-width`, déjà dans l'arbre de
  dépendances) : aucune nouvelle dépendance.
- **Barre d'aide** : `status_line` remplace « -- NORMAL -- / -- INSERT --
  » par, par exemple, `Sélection · Url ● non enregistré — Entrée modifier
  · Espace activer · Ctrl+S enregistrer · Échap fermer` et `Saisie · Corps
  — Entrée nouvelle ligne · Tab valider · Échap annuler · Ctrl+S
  enregistrer`. Les messages de statut prioritaires existants gardent la
  priorité (ordre actuel de `status_line`).

### D8. Tests

- `text_input.rs` : tests unitaires de chaque opération (extrémités,
  multi-octet `é`, jointure/scission de lignes, `up`/`down` sur lignes de
  longueurs différentes, `newline` sans effet en une ligne).
- `message.rs` : `every_key_binding` mis à jour (`Entrée → Enter`,
  `Ctrl+S → SaveEdit`, retrait de `i`/`w`) ; nouveau test de capture
  `Input` couvrant `Tab`, `Suppr`, `Début`, `Fin`, `↑`, `↓`, et les
  lettres `q r / S` rendues en caractères.
- `update.rs` : réécriture des tests d'Insert existants
  (`insert_mode_transitions_and_value_preservation`, etc.) selon les
  scénarios de la delta spec ; nouveaux tests : annulation après
  validation, `Ctrl+S` depuis la saisie, `has_unsaved` avec saisie non
  validée + `ForceQuit`, refus de sélection (arbre, recherche, croisée),
  repli du dossier sélectionné permis, `Enter` routé comme `Right` hors
  détail.
- `view` : curseur visible en fin d'URL plus large que le panneau,
  dernier en-tête visible après déplacement, rendu de la barre d'aide.
- Toutes les requêtes de test proviennent des fixtures de
  `tests/fixtures/` ; aucune écriture réelle hors répertoire temporaire.

### D9. Points de contact avec les changements parallèles

- **`add-mouse-support`** : un clic dans l'arbre qui change de requête
  DOIT passer par `selection_locked` (D6), sinon il réintroduit la perte
  silencieuse. Un clic sur un champ du détail pourrait positionner le
  curseur de champ ou de texte : il DEVRAIT s'appuyer sur la table
  `FieldLines` (D7) et `hscroll` plutôt que sur des indices de ligne. Un
  clic hors du détail pendant une saisie ne doit pas changer le focus
  (équivalent de `Tab` capturé). Si les deux changements touchent
  `Message`/`key_message`, fusion attendue sur les nouvelles variantes
  (`Enter`, clics) sans conflit de sens.
- **`add-entry-management`** : ajouter/supprimer/renommer une entrée
  modifie la liste des champs (`EditableField::list_for`) et les indices
  de `FieldEdit` ; il doit définir son interaction avec une session
  ouverte (refuser si `has_unsaved`, ou rafraîchir `fields` et borner
  `cursor` comme `edit_saved`). Renommer ou supprimer la requête éditée
  doit être refusé tant que la session est modifiée (même garde). Les
  touches qu'il ajoutera en Sélection de champ (ex. ajout/suppression
  d'en-tête) doivent éviter `Entrée`, `Espace`, `Échap`, `Ctrl+S`, `↑↓jk`
  et ne rien ajouter en Saisie, où toute lettre est du texte.
- **`add-status-panel`** : insère un panneau au-dessus de la Réponse et
  retouche la disposition (`Areas`) ; aucun recouvrement de logique, mais
  le calcul de hauteur du détail utilisé par le suivi vertical (D7) doit
  lire `layout_for` après fusion, jamais une hauteur supposée. Les deux
  changements modifient `src/app/view/mod.rs` : conflit textuel probable
  dans `status_line`/`view`, sans conflit de comportement (la barre
  d'aide reste dans la barre d'état, pas dans le panneau Statut).

## Risks / Trade-offs

- [Rupture d'habitude : `Échap` annule au lieu de garder, `i`/`w` ne font
  plus rien] → la barre d'aide affiche en permanence les touches de
  l'état ; la validation par `Entrée`/`Tab` est rappelée ; `Échap` n'agit
  que sur la saisie en cours, jamais sur les modifications déjà validées.
- [Perte d'une longue saisie du corps par `Échap` réflexe] → accepté pour
  garder `Échap` uniforme ; la saisie est courte à refaire dans les cas
  visés (retouche d'un corps). À réévaluer à l'usage (Open Questions).
- [`Ctrl+S` intercepté par un multiplexeur ou un terminal avec
  `IXON` réactivé] → le mode brut de crossterm désactive `IXON` ; si un
  multiplexeur capture la touche, `Tab` puis `Ctrl+S` reste nécessaire.
  À vérifier manuellement sous herdr/tmux lors de l'implémentation.
- [`Tab` n'insère pas de tabulation dans le corps] → les corps JSON/XML
  des collections sont indentés en espaces ; une tabulation existante est
  conservée et éditable, seule la frappe d'une nouvelle est impossible.
- [Collage sans bracketed paste : un collage multi-ligne dans un champ à
  une ligne valide au premier saut de ligne et laisse le reste taper des
  raccourcis globaux] → c'est déjà le cas aujourd'hui avec `Entrée` en
  Insert ; documenté en Non-Goals, candidat à un changement dédié.
- [Refus du changement de sélection jugé plus rigide qu'une
  confirmation] → message explicite avec les deux issues ; évolution
  possible vers une confirmation une fois `add-mouse-support` et
  `add-entry-management` fusionnés.
- [`r` en Sélection de champ exécute la version sur disque, pas la
  version modifiée] → cohérent avec le principe « exécution déléguée à
  `bru` sur les fichiers » ; l'indicateur « non enregistré » est visible
  dans la barre d'aide au moment du lancement.

## Migration Plan

Pas de donnée ni de format à migrer. Déploiement en un seul changement ;
retour arrière par revert du commit. Les tests existants d'édition sont
réécrits dans le même changement (pas de période où les deux claviers
coexistent).

## Open Questions

- Faut-il mémoriser une « colonne désirée » lors de déplacements
  verticaux successifs dans le corps (comportement des éditeurs
  classiques) ? Sans effet sur les specs actuelles, ajoutable plus tard
  dans `TextInput`.
- Déplacement par mot (`Ctrl+←`/`Ctrl+→`) et `Ctrl+U`/`Ctrl+K` : utiles
  mais non demandés ; ajoutables dans la table de capture sans collision
  (les combinaisons `Ctrl` sont déjà filtrées en amont).
