## Context

Voir `proposal.md` — Why. État observé du code (`main` après fusion de
`add-status-panel` et `improve-direct-editing`) :

- `src/main.rs` initialise le terminal par `ratatui::try_init()` (mode
  brut, écran alternatif, hook de panique qui appelle `restore`) et le
  restaure par `ratatui::try_restore()`. Aucune capture souris n'est
  activée ; le hook de panique de ratatui ne la désactive pas.
- `src/app/event.rs` publie déjà tout `crossterm::Event` en
  `AppEvent::Terminal(event)` ; `to_message` (`src/app/message.rs`) ignore
  tout sauf `Key` et `Resize`. `to_message` ne dépend du modèle que par
  `TextCapture` (`Search`, `Input`, `Filter`, `SecretName`, `SecretValue`).
  Pendant `TextCapture::Input` (état Saisie), toute touche imprimable,
  dont `M` et `y`, est du texte ; hors capture, `y` → `Yank` et `M` n'est
  pas affecté.
- La boucle (`src/app/mod.rs`) redessine après **chaque** événement
  traduit et exécute les `Command` hors de `update` (`StartRun`,
  `CopyToClipboard`, `SaveEdit`, `ResolveSecrets`), avec des dépendances
  injectables (`Clipboard`, `RequestWriter`) pour les tests.
- `view::layout(area) -> Option<Areas>` est pure ; `Areas` porte `tree`,
  `detail`, `response_status` (panneau Statut, 3 ou 5 lignes) et
  `response` (décalé sous le Statut). `update` l'utilise via
  `layout_for(model.size)`.
- Le détail est rendu **avec** retour à la ligne hors saisie et **sans**
  pendant une saisie (`input_in_progress`), la réponse toujours avec.
  `update` compte le défilement en lignes logiques, alors que
  `Paragraph::scroll` saute des lignes rendues.
- `request_text_and_fields(request, session) -> (Text, Vec<FieldLine>)`
  (`view/detail.rs`) donne déjà, pour chaque champ éditable, sa première
  ligne logique et son nombre de lignes ; `cursor_position_in_detail` et
  `scroll_edit_into_view` s'appuient dessus.
- Édition (`update.rs`) : `EditSession { state: FieldSelect | Input(TextInput),
  cursor, .. }`, `start_edit` (garde `focus == Detail`), `begin_input`,
  `validate_input` (= `Tab`), `cancel_input` (= `Échap`),
  `selection_locked` (`has_unsaved`) consulté par `navigate_tree`, la
  recherche et la navigation croisée, qui posent
  `StatusMessage::EditLocked` et laissent la sélection intacte.
  `clear_detail_view_state` ferme la session lors d'un changement de nœud.
- `DetailSelection { anchor }` : borne mobile dérivée du bas du viewport
  (`selection_range`, `response_selection_range`) ; `yank` copie à partir
  de ces plages via `Command::CopyToClipboard` avec jeton.
- `ratatui` 0.30 avec `unstable-rendered-line-info` (`Paragraph::line_count`) ;
  crossterm expose `EnableMouseCapture`/`DisableMouseCapture` et
  `MouseEventKind`.

## Goals / Non-Goals

**Goals:**
- Garder `view` pur et `update` sans I/O : toute géométrie cliquable est
  recalculée depuis le `Model`, jamais mémorisée par le rendu.
- Une seule source de vérité pour « quelle ligne/quel champ est à cette
  position », partagée par le hit-testing et le rendu.
- Réutiliser les chemins existants : `navigate_tree` et son verrou,
  `start_edit`/`begin_input`/`validate_input`, défilement, sélection
  visuelle, `yank`.
- Testable sans vrai terminal ni vraie souris.

**Non-Goals:**
- Sélection au caractère près (lignes entières, validé par l'utilisateur).
- Copie automatique au relâchement ; copie via OSC 52 (copie par `y`
  uniquement, validé par l'utilisateur).
- Placer le curseur de texte à la colonne cliquée (validé par
  l'utilisateur : hors périmètre).
- Clic sur les onglets de la réponse, dans les panneaux superposés, sur
  le titre, la barre d'état ou le panneau Statut ; double-clic ; menu
  contextuel ; défilement horizontal.
- Corriger l'écart préexistant entre défilement logique et défilement
  rendu des lignes retournées à la ligne (le hit-testing s'y adapte, D3).

## Decisions

### D1. Traduction brute, interprétation dans `update`

`to_message` traduit `Event::Mouse` en `Message::Mouse(MouseInput)` avec
`MouseInput { kind: MouseKind, column, row }`, `MouseKind` ∈ `Press`,
`Drag`, `Release` (bouton gauche), `WheelUp`, `WheelDown`. `Moved`, les
autres boutons et le défilement horizontal donnent `None` dès
`to_message` : crossterm active le suivi de tous les mouvements et chaque
message provoque un redessin. La traduction ne dépend pas de `capture` :
les règles d'état sont appliquées dans `update`.

*Alternative écartée* : résoudre la cible dans `to_message` — il faudrait
lui passer le modèle entier.

### D2. Hit-testing pur dans `src/app/view/hit.rs`

`pub fn hit_test(model: &Model, column: u16, row: u16) -> Option<Hit>`,
`Hit::Tree { row: Option<usize> }`, `Hit::Detail { line: Option<u16> }`,
`Hit::Response { line: Option<u16> }` (`None` = bordure ou au-delà du
contenu). Calculé depuis `layout_for(model.size)`, `inner`, les
défilements et `detail_text`/`response_text`. `Areas::response_status`
n'a **pas** de variante : le panneau Statut est traité comme le titre et
la barre d'état (retour `None`).

*Pourquoi le panneau Statut est sans effet plutôt que « focus Réponse »* :
`status-panel` le déclare non focalisable et sans touche dédiée ; son
contenu (1 à 3 lignes) ne défile pas et n'a rien à sélectionner. Rediriger
le clic vers la Réponse donnerait le focus à un panneau autre que celui
cliqué, contrairement à la règle générale « un clic focalise le panneau
cliqué », et un clic sur le Statut pendant une Saisie validerait la
saisie sans raison visible. Le cas « je veux la réponse » reste à un clic,
juste en dessous.

La boucle redessine après chaque message : l'état du modèle au moment
d'un événement souris est celui du dernier écran dessiné.

*Alternative écartée* : enregistrer les `Rect` pendant `view` — rend
`view` impur.

### D3. Ligne écran → ligne logique

Réponse, et détail hors saisie : hauteur rendue de chaque ligne logique
par `Paragraph::new(line).wrap(Wrap { trim: false }).line_count(width)`,
on saute `scroll` lignes rendues comme `Paragraph`, puis chaque ligne
écran est attribuée à sa ligne logique. Détail pendant une saisie
(`input_in_progress`) : pas de retour à la ligne, donc ligne logique =
`scroll + y`. La fonction qui décide « avec ou sans retour » est la même
que celle de `view` (exposée en `pub(crate)`), pour ne jamais diverger.

Test sur rendu `TestBackend` : pour des contenus avec et sans lignes
longues, plusieurs défilements, et pendant une saisie, le texte à la
ligne écran `y` appartient à la ligne logique renvoyée.

### D4. Lignes des champs : `request_text_and_fields`

Le hit-testing d'un champ réutilise `request_text_and_fields(request,
model.editing.as_ref())` : une ligne logique `l` appartient au champ dont
`line <= l < line + count`. Aucune nouvelle fonction de géométrie des
champs ; la liste reflète l'état courant de la session (valeurs en
attente, corps en cours de saisie avec ses lignes ajoutées).

### D5. État souris et traitement des événements

```text
Model.mouse: MouseState {
    capture: bool,             // état effectivement appliqué au terminal
    drag: Option<Drag>,        // appui gauche en cours dans détail/réponse
}
Drag { panel: DragPanel /* Detail | Response */, anchor: u16, moved: bool }
```

Fonction `mouse(model, input)` dans `update.rs` :

- **Garde** (D9) puis `hit_test` ; `None` → rien.
- **Press** :
  1. Si l'état Saisie est actif : si la cible est une ligne du champ en
     saisie (`Hit::Detail` dans sa plage `FieldLine`), **arrêt** (ni
     focus, ni `drag`) ; sinon `validate_input(model)`.
  2. Arbre : `focus = Tree` ; ligne de nœud ≠ sélection → poser
     `tree.selected` puis appliquer la même logique que `navigate_tree`
     (extraite en `select_row(model, index)` partagée avec le clavier :
     verrou `selection_locked` → restauration + `EditLocked`, sinon
     `clear_detail_view_state`, filtre, `scroll_tree_into_view`) ; ligne
     = sélection et dossier → `expand_or_enter`/repli via le même chemin
     que `→`/`←` sur un dossier (le nœud ne change pas, donc pas de
     verrou). Pas de `drag`.
  3. Détail/réponse : `focus` sur ce panneau ; si `line` existe,
     `drag = Some(Drag { anchor: line, moved: false })`.
- **Drag** : si `drag` existe et que la ligne visée diffère de l'ancrage,
  `moved = true` et la sélection du panneau devient
  `DetailSelection { anchor, head: Some(line) }` ; pointeur au-dessus ou
  au-dessous de l'intérieur du panneau : défilement d'une ligne borné
  (`detail_max_scroll`/`response_max_scroll`), `head` = première/dernière
  ligne visible.
- **Release** : `drag.take()` ; si `moved`, rien de plus. Sinon, c'est un
  clic : lever la sélection du panneau ; dans le détail d'une requête,
  si la ligne est dans un `FieldLine` : ouvrir la session si besoin
  (`start_edit`, focus déjà sur le détail), placer `cursor` sur l'indice
  du champ dans `session.fields`, `begin_input`. Hors champ : rien de
  plus (une saisie éventuelle a déjà été validée à l'appui).
- **Wheel** : panneau sous le pointeur, sans toucher au focus, ni au
  `drag`, ni à la session ; détail/réponse → trois appels à
  `scroll_detail`/`scroll_response` (`Up`/`Down`), jamais
  `move_field_cursor` ni `scroll_edit_into_view` ; arbre → `offset` ± 3
  borné par `rows.len().saturating_sub(inner.height)`.

Un appui ignoré par la garde ne crée pas de `drag`, donc un `Release`
orphelin est sans effet.

### D6. Borne explicite de sélection

`DetailSelection` gagne `head: Option<u16>`. `None` : comportement actuel
(sélection clavier, borne = bas du viewport). `Some(h)` : borne fixe
(souris). `selection_range`/`response_selection_range` prennent `head`
s'il existe ; le rendu, `v`, `Échap` et `clear_detail_view_state` sont
inchangés.

Constat à l'implémentation : `yank` levait la sélection dès l'émission de
`Command::CopyToClipboard`, contrairement à `search-and-yank` (« levée
après une copie réussie », « échec … sans modifier la sélection ») et à
`mouse-support`. Le panneau dont la sélection est copiée est désormais
mémorisé avec le jeton (`pending_clipboard_selection`) et sa sélection
n'est levée qu'à la réception d'un `ClipboardResult` réussi, pour les
sélections clavier comme souris.

### D7. Capture : injectable, pilotée par commande

- Trait `MouseCapture: Send + Sync { fn set(&self, enabled: bool) ->
  io::Result<()> }` ; `TerminalMouseCapture` (`execute!(stdout(),
  Enable/DisableMouseCapture)`) en usage réel, faux en test — même patron
  que `Clipboard` et `RequestWriter`.
- `M` → `Message::ToggleMouseCapture` → `Command::SetMouseCapture(bool)`.
  La boucle l'exécute de façon synchrone (quelques octets, comme
  `terminal.draw`) et renvoie `Message::MouseCaptureChanged { enabled,
  result }` via `update`, comme `RunStarted`. `mouse.capture` n'est mis à
  jour qu'en cas de succès ; `StatusMessage::MouseCapture(bool)` ou
  `MouseCaptureError(String)` ; désactiver annule `drag`.
- Démarrage : `main` active la capture après `try_init` sauf `--no-mouse`
  et passe l'état effectif à `run`, qui l'inscrit dans le modèle ; un
  échec au démarrage laisse la capture inactive et pose le message
  d'erreur.
- Écrire ces séquences est du pilotage du terminal, au même titre que le
  mode brut, pas un effet de bord au sens de `tui-shell`.

*Alternatives écartées* : touche seule, option seule, variable
d'environnement ou fichier de configuration. `Shift`+glisser est
mentionné dans l'usage mais n'est pas garanti partout (tmux, certains
terminaux Windows), d'où la bascule explicite.

### D8. Restauration, y compris sur panique

Après `ratatui::try_init()`, `main` enveloppe le hook de panique courant :
le nouveau hook envoie `DisableMouseCapture` sur `stdout` en ignorant
l'erreur, puis appelle le hook précédent. À la sortie normale,
`DisableMouseCapture` est envoyé inconditionnellement avant
`try_restore`.

### D9. Liaisons, CLI et garde

- `M` est libre hors saisie (clés prises : `j k h l g G q r D H E S a d /
  | n N v y e`, `Espace`, `Entrée`, `Tab`, `Échap`, `Ctrl+C/X/S`). Il
  est ajouté à la table hors capture de `key_message` seulement : en
  `TextCapture::Input` comme dans les autres captures il reste du texte,
  sans changement de `capture_message`. `y` est déjà dans ce cas.
- `cli.rs` : `--no-mouse` → champ `mouse: bool` de `Command::Run` ;
  `USAGE` le documente avec `M` et `Shift`+glisser.
- Garde unique en tête de `mouse` : `!mouse.capture`, `confirm.is_some()`,
  `text_capture()` ∉ {`None`, `Some(Input)`}, `focus` ∉ {`Tree`, `Detail`,
  `Response`}, collection non chargée, `layout_for` à `None`.

## Points de contact avec les changements parallèles

`add-status-panel` et `improve-direct-editing` sont fusionnés sur `main`
et intégrés ci-dessus (D2, D3, D4, D5, D9).

| Changement | Contact | Conduite à tenir |
|---|---|---|
| `add-entry-management` (en cours, autre worktree — non modifié ici) | Nouvelles saisies (nom d'entrée) et confirmations (suppression) ; lignes de l'arbre qui changent ; nouvelles touches possibles ; `cli.rs`/`USAGE`, `message.rs`, `update.rs` touchés des deux côtés. | Si ses saisies passent par `TextCapture` et ses confirmations par `PendingConfirm`, la garde D9 les couvre sans modification ; sinon, les y ajouter à la fusion. Le clic d'arbre passe par `select_row`, extrait de `navigate_tree` : si ce changement modifie `navigate_tree`, reporter dans `select_row`. S'assurer que `M` n'y est pas repris. Tout panneau superposé qu'il ajoute doit rester hors des focus acceptés par la garde. |

## Risks / Trade-offs

- [Coût du recalcul à chaque événement de glisser : `detail_text` et
  `line_count` par ligne] → mêmes coûts qu'une touche aujourd'hui ;
  `Moved` filtré dès `to_message`.
- [Écart défilement logique / rendu préexistant] → le hit-testing suit
  le rendu (D3), donc ce que voit l'utilisateur.
- [Clic dans l'arbre pendant une saisie modifiée : validation puis
  refus du changement de requête] → comportement voulu (aucune perte de
  texte) ; le message `EditLocked` explique l'étape suivante.
- [Terminal ou multiplexeur sans rapport souris] → aucun événement reçu,
  usage clavier intact.
- [Capture perdue par un processus externe] → `M` deux fois la
  réapplique.
- [Clic sur un champ ouvre une session involontairement] → rien n'est
  écrit ; `Échap` puis `Échap` referme sans confirmation tant que le
  texte est inchangé.

## Migration Plan

Aucune donnée ni format persistant. La capture est active par défaut :
`--no-mouse` ou `M` pour l'ancien comportement. Retour arrière = revert.
