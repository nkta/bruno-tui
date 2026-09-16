## Context

Voir `proposal.md` pour la motivation et `specs/search-and-yank/spec.md`
pour le contrat. Cette capacité se pose entièrement au-dessus de
`tui-shell` (archivé, `specs/tui-shell/spec.md`), sans le modifier.

Faits vérifiés dans le code actuel de `src/app/` :

- `message::to_message(event: AppEvent) -> Option<Message>` est une
  fonction pure, sans accès au modèle ; son commentaire de module affirme
  que « la correspondance ne dépend pas du modèle ». `key_message` court-
  circuite d'abord `Ctrl+C` (`ForceQuit`, quels que soient les autres
  modificateurs), puis rejette `Alt`, puis fait correspondre les touches
  connues.
- `mod::run` construit `model` avant la boucle et appelle
  `to_message(event)` juste avant `update(&mut model, message)` : le
  modèle est donc déjà en portée à cet endroit.
- Aucune touche existante n'utilise `/`, `n`, `N`, `v` ou `y` (vérifié par
  lecture exhaustive de `key_message` dans `message.rs`) : aucun conflit
  avec `tui-shell`.
- `update::navigate_tree` modifie `model.tree.selected` puis appelle
  `scroll_tree_into_view` ; changer `tree.selected` remet aussi
  `detail_scroll` à 0 (`if model.tree.selected != before { detail_scroll
  = 0 }`), ce qui vaut aussi pour un saut de recherche dans l'arbre sans
  code supplémentaire.
- `update::scroll_detail` incrémente `model.detail_scroll` (haut du
  panneau visible) d'une ligne, d'une page, ou le borne à `0` /
  `detail_max_scroll(model)`. `detail_max_scroll` calcule
  `Paragraph::new(detail_text(model)).wrap(Wrap { trim: false
  }).line_count(width) - height`, borné à 0, où `width`/`height` viennent
  de `inner(layout_for(model.size)?.detail)`. C'est un pager : les touches
  déplacent la fenêtre, il n'existe aucune notion de « ligne courante »
  distincte du sommet du panneau.
- `detail::detail_text(model) -> Text<'static>` construit une liste de
  `Line` stylées ; son module de test possède déjà une fonction `plain`
  qui aplati un `Text` en une chaîne, lignes jointes par `\n` — utile
  telle quelle pour la recherche et la copie, mais actuellement privée
  aux tests.
- `view::render_tree` construit ses `ListItem` depuis `model.tree.rows`
  (adresses déjà calculées par `visible_rows`, qui ignore les dossiers non
  dépliés) ; il n'existe pas de fonction qui énumère *tous* les nœuds,
  dossiers repliés compris.

Sonde hors dépôt (`arboard` 3.6.1, `/tmp/.../scratchpad/probe-clipboard`) :

- Avec les fonctions par défaut (`image-data`), l'arbre de dépendances
  compte 41 crates (`image`, `png`, `flate2`, deux copies de
  `miniz_oxide`…) pour un besoin qui se limite à du texte.
- `default-features = false` : 18 crates seulement, sur Linux limité au
  backend X11 (`x11rb`, protocole pur Rust, pas d'en-têtes système
  requis) ; `Clipboard::new()` et `set_text` compilent et s'exécutent.
- Sans serveur d'affichage (`DISPLAY` et `WAYLAND_DISPLAY` absents),
  `Clipboard::new()` rend `Err` en 25 ms, sans panique : un
  `Error::Unknown` dont le message mentionne un délai de connexion. Le
  texte de ce message n'est montré qu'à titre de diagnostic ; le contrat
  ne doit reposer sur aucune variante précise, seulement sur `Result`.
- Avec la fonction `wayland-data-control` en plus, l'arbre grimpe à 76
  crates (`wl-clipboard-rs`, `wayland-*`, `tree_magic_mini`, un
  dépendance de compilation `cc`) pour ne couvrir que les sessions
  Wayland sans XWayland.
- `arboard` n'a pas d'API asynchrone : `Clipboard::new()` et `set_text`
  sont bloquants tant que le serveur d'affichage répond (25 ms observés
  ici, mais un `DISPLAY` qui pointe vers un hôte injoignable peut aller
  jusqu'au délai TCP du système, potentiellement plusieurs secondes).

## Goals / Non-Goals

**Goals :**
- Recherche et copie qui n'imposent de comprendre aucun nouveau concept
  de navigation en dehors des touches déjà apprises pour `tui-shell`.
- Zéro régression observable sur le comportement déjà spécifié de
  `tui-shell` : chaque touche déjà liée garde exactement son effet en
  dehors d'une saisie de recherche.
- Le presse-papiers ne peut jamais bloquer ni faire paniquer la boucle
  d'événements, même sans serveur d'affichage.

**Non-Goals :**
- Recherche incrémentale (surbrillance pendant la frappe, avant
  validation) : ajoutée plus tard si le besoin se confirme.
- Filtre qui masque les nœuds non correspondants : cette capacité ne fait
  que sauter d'une correspondance à l'autre.
- Indicateur permanent de « ligne courante » dans le détail en dehors
  d'une recherche ou d'une sélection active (voir D6) : resterait à
  ajouter séparément si l'ergonomie s'avère insuffisante.
- Collage, historique, filtre jq, édition : hors périmètre de ce
  changement.
- Support Windows du presse-papiers : non vérifié ici : `arboard` le
  couvre nativement, mais aucune sonde n'a été faite sur cette
  plateforme ; à confirmer si le projet vise Windows.

## Decisions

### D1. Presse-papiers : `arboard`, sans les fonctions image, derrière un trait

```toml
arboard = { version = "3.6.2", default-features = false }
```

`default-features = false` retire `image-data` (donc `image`, `png`,
`objc2-core-graphics` sur macOS, `windows-sys` Gdi sur Windows) : ce
changement ne copie que du texte. `wayland-data-control` n'est pas
activée : sur une session Wayland avec XWayland (la configuration la plus
courante), le backend X11 par défaut d'`arboard` continue de fonctionner
sans elle ; l'activer tripliserait le nombre de crates (D9 ci-dessous)
pour ne couvrir que les sessions Wayland strictement sans XWayland, cas
non prioritaire pour un TUI de développement. À revoir si des utilisateurs
signalent ce cas.

Alternatives écartées :
- *`copypasta`* : encapsule aussi X11/Wayland/Windows/macOS, mais moins
  maintenue et sans les mêmes garanties sur les erreurs (`Result`
  générique `Box<dyn Error>`, plus difficile à intégrer à `thiserror`).
- *Lancer `xclip`/`wl-copy` en sous-processus* : ajouterait une
  dépendance à des binaires externes non garantis présents, contredirait
  l'esprit « aucun outil externe requis » déjà tenu pour l'UI (`bru` est
  la seule exception, déjà justifiée pour l'exécution).
- *`rdev`/`clipboard` (crate historique, non maintenue depuis 2020)* :
  écartée pour absence de maintenance.

`arboard` n'étant ni asynchrone ni garanti instantané (Context), l'accès
passe par un trait local, testable et substituable :

```rust
// src/app/clipboard.rs
pub trait Clipboard: Send + 'static {
    fn set_text(&mut self, text: String) -> Result<(), ClipboardError>;
}

pub struct SystemClipboard; // construit arboard::Clipboard à chaque appel

#[derive(Debug, Error)]
#[error("presse-papiers inaccessible : {0}")]
pub struct ClipboardError(String); // message d'arboard, jamais le texte copié
```

`SystemClipboard::set_text` construit un `arboard::Clipboard` neuf à
chaque appel plutôt que d'en garder un en champ : la connexion au serveur
X11 est ce qui peut être lent (Context), et la reconstruire à chaque appel
depuis une tâche `spawn_blocking` (D7) évite de garder une ressource
système ouverte entre deux copies.

### D2. `to_message` : capture de texte par énumération ouverte, pas par booléen

Le contrat actuel (« la correspondance ne dépend pas du modèle ») décrit
un mécanisme différent de la saisie de texte : `Up` reste `Up` quel que
soit le focus, c'est `update` qui l'interprète selon `Focus`. Mais pendant
la saisie d'un motif de recherche, la touche `j` doit produire le
caractère `j` et non le message `Down` : deux significations physiques
incompatibles pour la même touche, qu'aucune interprétation à
l'intérieur d'`update` ne peut démêler après coup.

**Ce changement n'est pas seul à avoir besoin de ce mécanisme.**
`add-field-editing` (mode Insert) et `add-response-filter` (saisie de
filtre jq) en ont chacun besoin, indépendamment, pour leur propre texte.
Un simple `bool` ne le permet pas : il indique qu'une saisie est active,
pas laquelle, alors que chaque source produit ses propres messages
(`SearchInput` contre `InsertChar` contre `FilterInput`). La signature
retenue ici est donc conçue dès le départ pour être étendue par ces
changements sœurs sans être redéfinie :

```rust
/// Ce que la boucle capture au clavier hors navigation normale. Au plus
/// une variante active à la fois (voir plus bas pourquoi c'est garanti
/// par construction). Cette énumération est délibérément ouverte :
/// `add-field-editing` y ajoute `Insert`, `add-response-filter` y ajoute
/// `Filter`, chacun dans son propre changement, sans toucher aux
/// variantes des autres.
pub enum TextCapture {
    Search,
}

pub fn to_message(event: AppEvent, capture: Option<TextCapture>) -> Option<Message>
```

`capture` n'est *pas* le modèle : c'est la valeur que `mod::run` calcule
juste avant l'appel, via une méthode dédiée du modèle
(`Model::text_capture() -> Option<TextCapture>`, D8), au même endroit où
`model` est déjà en portée (Context). Le reste de la fonction garde sa
forme actuelle. Le commentaire de module est mis à jour pour documenter
cette unique exception plutôt que de prétendre qu'elle n'existe pas.

```rust
fn capture_message(event: AppEvent, capture: TextCapture) -> Option<Message> {
    match capture {
        TextCapture::Search => search_capture_message(event),
        // TextCapture::Insert => ...  (ajouté par add-field-editing)
        // TextCapture::Filter => ...  (ajouté par add-response-filter)
    }
}
```

`to_message` appelle `capture_message(event, capture)` quand `capture`
vaut `Some(_)`, avant la table de correspondance actuelle (après le
traitement de `Ctrl+C`, qui reste prioritaire dans tous les cas et n'est
donc jamais délégué à `capture_message`). `search_capture_message`
produit :
- `KeyCode::Char(c)` sans `Ctrl` → `Message::SearchInput(c)` (`Alt` inclus
  : un caractère composé reste un caractère) ;
- `KeyCode::Backspace` → `Message::SearchBackspace` ;
- `KeyCode::Enter` → `Message::ConfirmSearch` ;
- `KeyCode::Esc` → `Message::CancelSearch` ;
- toute autre touche (flèches, `Tab`, pagination, `Home`/`End` en tant que
  touches dédiées) → ignorée (`None`), comme aujourd'hui pour les touches
  non liées.

**Pourquoi l'exclusion mutuelle entre saisies n'a pas besoin d'être
vérifiée explicitement** : tant qu'une capture est active, `capture_message`
intercepte tous les caractères avant que la table hors-saisie ne soit
jamais consultée — c'est cette même table qui porte les liaisons
d'ouverture (`/` pour `StartSearch`, et plus tard `i`/`e` pour
`add-field-editing`, `|` pour `add-response-filter`). Il est donc
impossible d'ouvrir une deuxième capture pendant qu'une première est
active : la touche qui l'ouvrirait est elle-même interceptée comme un
caractère de la capture en cours. Chaque changement sœur peut donc
ajouter sa branche à `Model::text_capture()` (D8) dans l'ordre qu'il
veut, sans coordination de priorité : au plus une branche peut être
vraie à la fois.

Ce découpage n'introduit aucune branche supplémentaire dans la table
existante : hors saisie, elle est inchangée à l'ajout des cinq nouvelles
liaisons (D3).

### D3. Nouveaux messages et liaisons

```rust
pub enum Message {
    // ... variantes existantes inchangées ...
    StartSearch,           // '/', hors saisie
    SearchInput(char),
    SearchBackspace,
    ConfirmSearch,         // Entrée, en saisie
    CancelSearch,          // Échap, en saisie
    NextMatch,             // 'n', hors saisie
    PreviousMatch,         // 'N', hors saisie
    ToggleVisual,          // 'v', hors saisie
    Yank,                  // 'y', hors saisie
}
```

Table de `key_message` hors saisie, ajouts uniquement :
`Char('/') → StartSearch`, `Char('n') → NextMatch`,
`Char('N') → PreviousMatch` (même style que `Char('G')` existant, sans
garde de modificateur explicite : le terminal rapporte déjà la majuscule),
`Char('v') → ToggleVisual`, `Char('y') → Yank`.

### D4. Recherche : état, portée, et parcours complet de l'arbre

```rust
// src/app/search.rs
pub enum SearchScope { Tree, Detail }

pub struct SearchState {
    pub pattern: String,        // motif validé (vide avant toute recherche)
    pub scope: SearchScope,     // panneau au moment de la validation
    pub editing: bool,          // ligne de saisie ouverte
    pub draft: String,          // texte en cours de frappe
    pub direction_forward: bool,// sens de la dernière recherche, pour N
    pub last_result: Option<bool>, // Some(false) = dernière recherche sans résultat
}
```

`Model.search: Option<SearchState>` : `None` tant que `/` n'a jamais été
pressé. `StartSearch` crée l'état (`editing: true`, `draft` vide,
`scope` = focus courant) s'il n'existe pas encore, ou rouvre la saisie sur
le motif déjà validé sinon. `SearchInput`/`SearchBackspace` modifient
`draft`. `CancelSearch` remet `editing` à `false` sans toucher `pattern`
ni `draft` restant affiché à la prochaine ouverture n'est pas nécessaire :
`draft` est réinitialisé à `pattern.clone()` à l'ouverture suivante.
`ConfirmSearch` avec `draft` vide se comporte comme `CancelSearch`
(exigence « motif vide »). Sinon : `pattern = draft.clone()`,
`editing = false`, puis lance la recherche vers l'avant (D4a/D4b) et met à
jour `last_result`.

**D4a. Recherche dans l'arbre — énumération complète.** `visible_rows`
(existant) filtre par `expanded` ; la recherche a besoin de l'équivalent
sans filtre :

```rust
// model.rs, à côté de visible_rows
pub fn all_rows(tree: &[TreeNode]) -> Vec<Row> // même parcours, sans le test `expanded.contains`
```

`search::find_tree_match(tree: &[TreeNode], rows: &[Row], from: usize,
pattern: &str, forward: bool) -> Option<usize>` compare
`view::tree::display_name(node)` en minuscules au motif en minuscules,
cherche à partir de `from + 1` (ou `from - 1` en arrière), avec
retour au début (ou à la fin) une fois la liste parcourue — une seule
passe circulaire, pas de retour arrière au-delà d'un tour complet. Sur
correspondance, `update` déplie chaque ancêtre du nœud trouvé
(`row.address` sans son dernier élément, à chaque profondeur, comme
`select` le fait déjà dans `test_support`) puis appelle `refresh_rows` et
fixe `tree.selected` à l'indice recalculé de ce nœud dans les lignes
maintenant visibles.

**D4b. Recherche dans le détail.** `detail::detail_text` reste inchangée
(utilisée pour le calcul de défilement, D-existant) ; une fonction
`detail::plain_lines(model: &Model) -> Vec<String>` (promotion de
l'actuel `plain` de test, désormais hors `#[cfg(test)]`) fournit le texte
brut ligne par ligne pour la recherche et pour la copie (D6/D7).
`search::find_detail_match(lines: &[String], from: u16, pattern: &str,
forward: bool) -> Option<(u16, Range<usize>)>` rend la ligne trouvée et la
position en octets du motif dans cette ligne (en minuscules des deux
côtés ; la position rendue s'applique à la ligne originale car la
comparaison ne change pas la longueur en ASCII/UTF-8 des motifs
usuels — un motif contenant des majuscules Unicode dont la capitalisation
change la longueur en octets resterait en dehors du périmètre couvert par
les tests). Sur correspondance, `update` fixe
`model.detail_scroll = line.min(detail_max_scroll(model))` et enregistre
`model.detail_match: Option<(u16, Range<usize>)>` pour la surbrillance
(D8), effacé par tout changement de nœud sélectionné (comme
`detail_scroll` l'est déjà) ou par une recherche sans résultat.

`NextMatch`/`PreviousMatch` relancent `find_tree_match`/
`find_detail_match` selon `search.scope`, indépendamment du focus courant
(exigence « n répète dans l'arbre depuis le détail »), à partir de la
position courante dans ce panneau-là (sélection de l'arbre, ou
`detail_scroll`), avec `forward` égal à `direction_forward` pour `n` et à
son inverse pour `N`.

### D5. Sélection visuelle : ancrage au sommet, borne mobile au bas

Nouvel état, actif seulement en focus Détail :

```rust
pub struct DetailSelection { pub anchor: u16 }
// Model.detail_selection: Option<DetailSelection>
```

`ToggleVisual` : si `None` et focus = Détail, pose
`Some(DetailSelection { anchor: model.detail_scroll })` ; si `Some`, le
remet à `None` (comme `Échap`, sans copier). Tant qu'une sélection est
active, chaque défilement (`Up`/`Down`/`PageUp`/`PageDown`/`Home`/`End` en
focus Détail) continue d'appeler exactement `scroll_detail` telle
qu'aujourd'hui (aucune duplication de sa logique). La plage sélectionnée
n'est pas stockée à part : elle se déduit à chaque rendu ou copie par

```rust
fn selection_range(model: &Model) -> Option<RangeInclusive<u16>> {
    let anchor = model.detail_selection.as_ref()?.anchor;
    let bottom = bottom_of_viewport(model); // scroll + hauteur_intérieure - 1, borné au nombre de lignes - 1
    Some(anchor.min(bottom)..=anchor.max(bottom))
}
```

`bottom_of_viewport` réutilise `layout_for`/`inner`/`detail_max_scroll`
déjà présents dans `update.rs`. Parce que `detail_scroll` est déjà borné à
`detail_max_scroll` (borne existante, inchangée), `bottom_of_viewport` au
défilement maximal vaut exactement `content_len - 1` : `Fin` porte donc
la borne mobile jusqu'à la vraie dernière ligne, quelle que soit la
hauteur du panneau — c'est ce qui permet à la sélection d'atteindre des
lignes qu'aucun sommet de panneau ne peut jamais atteindre seul (la
justification du scénario « Étendre jusqu'à la fin du contenu »).

Changer de nœud sélectionné dans l'arbre efface `detail_selection` (même
endroit que la remise à zéro de `detail_scroll` dans `navigate_tree`).

Alternative écartée : un curseur de ligne indépendant du défilement (à la
manière de `TreeState { selected, offset }`), qui aurait permis de viser
n'importe quelle ligne y compris hors sélection. Écartée parce qu'elle
changerait la sémantique de `Haut`/`Bas` hors sélection (aujourd'hui un
défilement pur d'une ligne par appui, pas un déplacement de curseur avec
auto-défilement) : `tui-shell` spécifie explicitement un défilement, et
changer cette sémantique casserait ses scénarios déjà couverts par des
tests. Conserver le défilement pur hors sélection, et ne faire apparaître
la notion de « borne mobile » que pendant une sélection active, garde
`tui-shell` intact à la lettre.

### D6. Copie : ligne du sommet, ou lignes de la sélection

`Yank`, en focus Détail hors saisie :
- sélection active → lignes de `plain_lines(model)[selection_range]`,
  jointes par `\n`, puis la sélection est levée (`detail_selection =
  None`) sans toucher `detail_scroll` ;
- sinon → une seule ligne, `plain_lines(model)[detail_scroll as usize]`.
- en focus Arbre, ou pendant une saisie de recherche : ignoré.

L'appel à `SystemClipboard::set_text` se fait dans une tâche
`tokio::task::spawn_blocking`, comme le chargement de la collection
(`mod::run`), pour ne jamais bloquer la boucle d'événements même dans le
cas D-contexte d'un `DISPLAY` injoignable qui répond lentement. Le
résultat revient par le même canal `mpsc::Sender<AppEvent>` que le
chargement et les touches :

```rust
AppEvent::ClipboardResult(Result<(), ClipboardError>)
```

`update` traduit ce résultat en un message affiché dans la barre d'état
(succès ou raison de l'échec), sans jamais faire dépendre l'état de
sélection ou de défilement de ce résultat asynchrone (qui peut arriver
après d'autres actions de l'utilisateur) : seul un compteur ou horodatage
simple distingue le dernier statut affiché, pour éviter qu'un résultat en
retard n'écrase une action plus récente déjà affichée par la barre
d'état — voir tâche dédiée pour le détail de cette garde.

### D7. Rendu : ligne de saisie, surbrillance de sélection et de motif

- Pendant une saisie (`search.editing`), la zone `areas.status` affiche
  `/` suivi de `draft` à la place des rappels de touches habituels
  (fonction `status_line` existante étendue d'un cas).
- Une sélection visuelle active teinte le fond des lignes de
  `selection_range` dans le `Paragraph` du détail, avec un style distinct
  du surlignage `REVERSED` déjà utilisé pour la sélection de l'arbre
  (éviter la confusion entre « nœud sélectionné » et « lignes
  sélectionnées »).
- Une correspondance de recherche dans le détail (`model.detail_match`)
  souligne la sous-chaîne trouvée sur sa ligne, dans un style encore
  distinct (ex. fond jaune sombre) — implémenté en découpant la `Line`
  concernée en trois `Span` (avant/motif/après) au moment du rendu,
  sans changer `detail_text` (qui reste la source utilisée pour le calcul
  de défilement, D-contexte).
- Aucune ligne n'est mise en évidence en dehors de ces deux cas (choix
  explicite, voir Non-Goals) : la barre d'état documente la règle
  (« y : copie la ligne du haut ») quand ni recherche ni sélection n'est
  active en focus Détail.

### D8. Modules touchés

```
src/app/
  clipboard.rs   nouveau : trait Clipboard, SystemClipboard, ClipboardError
  search.rs      nouveau : SearchScope, SearchState, find_tree_match, find_detail_match
  message.rs     + variantes D3, TextCapture, to_message(event, Option<TextCapture>),
                 capture_message, search_capture_message
  model.rs       + all_rows, Model.search / detail_selection / detail_match,
                 Model::text_capture()
  update.rs      + branches de recherche/visuel/copie, selection_range,
                 bottom_of_viewport, appel spawn_blocking pour la copie
  view/mod.rs    + ligne de saisie dans la barre d'état, appel des
                 fonctions de surbrillance
  view/detail.rs + plain_lines (promue hors test), fonction de découpage
                 en spans pour la surbrillance de sélection/motif
  mod.rs         run() : calcule model.text_capture() avant to_message, relaie
                 AppEvent::ClipboardResult
  event.rs       + variante AppEvent::ClipboardResult
```

### D9. Tests

- `message.rs` : chaque nouvelle liaison hors saisie (dans le style de
  `every_key_binding`) ; en saisie (`capture: Some(TextCapture::Search)`),
  un jeu de touches (lettres liées comme `j`/`q`, `Entrée`, `Échap`, `Retour
  arrière`, `Ctrl+C`) produit les messages de saisie plutôt que les
  messages de navigation, sauf `Ctrl+C` qui reste `ForceQuit`.
- `search.rs` : `find_tree_match`/`find_detail_match` sur des fixtures
  construites en mémoire (pas de dépendance à `parser-cases` pour les cas
  limites : liste vide, un seul élément, retour circulaire, insensibilité
  à la casse) et sur `parser-cases` chargée pour les scénarios de la
  spec (dossier replié, retour circulaire, aucun résultat).
- `update.rs` : recherche dans l'arbre déplie les ancêtres et sélectionne
  (scénarios de la spec, réutilisant `test_support::loaded_model`) ; `n`
  depuis un panneau différent de celui de la recherche ; sélection
  visuelle jusqu'à `Fin` couvre la dernière ligne réelle du contenu
  (vérifié en comparant `plain_lines(model).len() - 1` à la borne haute
  de `selection_range`) ; copie sans sélection contre une fausse
  implémentation de `Clipboard` enregistrant le texte reçu ; copie avec
  sélection sur plusieurs lignes ; échec du presse-papiers (fausse
  implémentation qui rend toujours `Err`) signalé sans modifier
  sélection ni défilement.
- `view/` : rendu `TestBackend` vérifiant la ligne de saisie affichée
  pendant l'édition, la teinte des lignes sélectionnées, et la
  surbrillance de la sous-chaîne trouvée sur son fond distinct.
- Aucun test ne dépend d'un serveur d'affichage réel : les tests de copie
  passent tous par une implémentation de `Clipboard` de test ; la seule
  vérification contre `arboard::Clipboard` réel reste la sonde manuelle
  déjà faite pour ce design (pas un test automatisé, à l'image de la
  vérification manuelle au pseudo-terminal déjà pratiquée pour
  `tui-shell`).

## Risks / Trade-offs

- [`arboard::Clipboard::new()` lent si `DISPLAY` pointe vers un hôte
  injoignable plutôt qu'absent] → isolé dans `spawn_blocking` (D7), la
  boucle reste réactive même si cet appel prend plusieurs secondes ;
  documenté dans le Contexte plutôt que supposé instantané.
- [Contenu copié perdu à la fermeture de l'application sur X11 pur, sans
  gestionnaire de presse-papiers qui reprenne la sélection] → limite
  connue du protocole X11 (non de ce projet) ; à documenter dans l'aide
  utilisateur si des retours le signalent.
- [Un motif contenant des caractères dont la casse change la longueur en
  octets (rare hors du latin) pourrait légèrement décaler la surbrillance
  du motif trouvé] → n'affecte que l'affichage de la surbrillance, jamais
  la sélection de la bonne ligne ni la copie ; non couvert par des tests
  dédiés, accepté pour ce changement.
- [Absence d'indicateur permanent de « ligne du haut » hors recherche ou
  sélection] → documenté dans la barre d'état ; ajout d'un indicateur
  visuel permanent laissé à un changement ultérieur si le besoin se
  confirme à l'usage.
- [`wayland-data-control` non activée] → dégradation vers l'échec propre
  déjà spécifié (« presse-papiers inaccessible ») sur les sessions
  Wayland strictement sans XWayland ; réversible sans changement de spec
  si le besoin apparaît (ajout de la fonction dans `Cargo.toml` seul).
