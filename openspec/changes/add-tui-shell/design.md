## Context

Voir `proposal.md` pour la motivation et `specs/tui-shell/spec.md` pour le
contrat.

État du dépôt :
- `src/main.rs` affiche « Hello, world » ; aucune dépendance UI.
- `collection::BruLoader` implémente `CollectionLoader` (synchrone,
  `Send + Sync`) et rend un `Collection` dont les nœuds sont déjà triés.
- `TreeNode::name()` rend le nom déclaré, sinon le nom de fichier **sans**
  extension, y compris pour un nœud en erreur.
- `runner::BruRunner` émet sur un `tokio::sync::mpsc` ; il n'est pas
  branché ici mais la boucle doit pouvoir l'accueillir ensuite.
- Toolchain Rust 1.96, édition 2024.

Faits vérifiés sur `ratatui` 0.30.2 (sonde compilée hors dépôt) :
- `ratatui::crossterm` réexporte crossterm 0.29 ;
  `ratatui::backend::TestBackend` permet un rendu en mémoire.
- `ratatui::try_init()` installe un hook de panique qui restaure le
  terminal, active le mode brut et l'écran alternatif ;
  `ratatui::try_restore()` défait le tout.
- `Terminal::draw` rend `Result<_, B::Error>` ; `B::Error` vaut
  `io::Error` pour crossterm et `Infallible` pour `TestBackend`.
- `ListState::with_selected(..).with_offset(..)` construit un état à la
  volée ; un rendu sur 1×1 ne panique pas.
- `Paragraph::line_count(width)` donne le nombre exact de lignes après
  retour à la ligne, derrière la feature `unstable-rendered-line-info`.
- Avec les features retenues (D1), l'arbre compte 77 crates contre 85 avec
  les features par défaut.

Fait vérifié sur tokio 1.53 : abandonner un `Runtime` **attend sans limite**
la fin des tâches `spawn_blocking`. Un `#[tokio::main]` bloquerait donc la
sortie tant qu'un chargement lent n'est pas terminé.

## Goals / Non-Goals

**Goals :**
- `update` et `view` testables sans terminal ni runtime.
- Une seule boucle, un seul canal d'entrée, prête à recevoir les
  événements du runner au changement suivant.
- Sortie immédiate, même pendant un chargement bloquant.

**Non-Goals :**
- Effets déclenchés par `update` (type `Command`) : aucun n'est
  nécessaire ici. Il sera introduit avec l'exécution de requêtes.
- Tests de bout en bout automatisés dans un vrai terminal : la
  restauration est vérifiée à la main dans un pseudo-terminal (D9).
- Unicode avancé (largeur des emojis dans les noms) au-delà de ce que
  ratatui gère.

## Decisions

### D1. Dépendance : ratatui seul, crossterm par réexportation

```toml
ratatui = { version = "0.30.2", default-features = false, features = ["crossterm", "layout-cache", "unstable-rendered-line-info"] }
```

- `crossterm` : backend imposé par la stack. Utilisé via
  `ratatui::crossterm`, ce qui garantit la même version que le backend.
  Une dépendance directe risquerait deux versions de crossterm.
- `layout-cache` : évite de recalculer les découpages à chaque rendu.
- `unstable-rendered-line-info` : borne exacte du défilement du détail
  (D6). Instable signifie « peut changer entre versions mineures » : la
  version est épinglée par `Cargo.lock` et l'usage est confiné à une
  fonction.
- Écartées : `all-widgets` (calendrier, tire la crate `time`), `macros`,
  `underline-color`.

Alternatives écartées :
- *clap* pour deux options et un argument : dépendance lourde pour un
  analyseur de dix lignes (D8).
- *feature `event-stream` de crossterm + crate `futures`* : lecture
  asynchrone des touches. Un thread dédié (D3) fait la même chose sans
  dépendance.

### D2. Modules

```
src/app/
  mod.rs       run() : boucle, chargement, rendu
  event.rs     AppEvent, thread de lecture du terminal
  message.rs   Message, correspondance AppEvent → Message
  model.rs     Model, CollectionState, TreeState, Row, Focus
  update.rs    update()
  view/
    mod.rs     view(), layout() partagé avec update
    tree.rs    lignes de l'arbre
    detail.rs  detail_text() : contenu du panneau de détail
  cli.rs       analyse des arguments
src/main.rs    arguments, runtime, init/restore du terminal
```

`app` est déclaré dans `src/lib.rs` pour que `tests/` y accède.

### D3. Entrées : `AppEvent`, thread de lecture, canal tokio

```rust
pub enum AppEvent {
    Terminal(crossterm::event::Event),
    TerminalClosed(io::Error),
    CollectionLoaded(Result<Collection, LoadError>),
}
```

- Un `std::thread` boucle sur `crossterm::event::read()` et envoie par
  `Sender::blocking_send`. Il s'arrête si l'envoi échoue (boucle
  terminée) ou si la lecture échoue (`TerminalClosed`). Il n'est pas
  joint : bloqué dans `read()`, il disparaît à la fin du processus.
- Canal `tokio::sync::mpsc` borné (256). Le runner émettra plus tard
  `AppEvent::Run(RunEvent)` sur le même canal.
- Seuls les `KeyEventKind::Press` sont retenus.

Alternative écartée : `crossterm::event::poll` avec délai dans la boucle
async. Bloquerait un worker tokio et ajouterait une latence arbitraire.

### D4. `Message` et correspondance des touches

```rust
pub enum Message {
    Up, Down, PageUp, PageDown, Home, End,
    Left, Right,           // Entrée est traduite en Right
    NextFocus, FocusTree,  // Tab, Échap
    Quit,                  // q
    ForceQuit,             // Ctrl+C
    Resize { width: u16, height: u16 },
    CollectionLoaded(Result<Collection, LoadError>),
    TerminalClosed,
}

pub fn to_message(event: AppEvent) -> Option<Message>;
```

La correspondance ne dépend pas du modèle : `Up` est interprété par
`update` selon le focus. `update` reste testable sans crossterm, et les
touches sont définies en un seul endroit. `q` et `Ctrl+C` sont distincts
pour qu'un futur champ de saisie puisse intercepter `q` sans perdre
`Ctrl+C`.

### D5. Modèle et arbre visible

```rust
pub struct Model {
    pub source: PathBuf,              // chemin demandé
    pub collection: CollectionState,  // Loading | Loaded(Collection) | Failed(LoadError)
    pub tree: TreeState,
    pub focus: Focus,                 // Tree | Detail
    pub detail_scroll: u16,
    pub size: (u16, u16),             // taille du terminal
    pub exit: Option<Exit>,           // Some => la boucle s'arrête
}

pub struct TreeState {
    pub expanded: HashSet<PathBuf>,   // chemins relatifs des dossiers dépliés
    pub rows: Vec<Row>,               // nœuds visibles, dans l'ordre d'affichage
    pub selected: usize,
    pub offset: usize,                // première ligne affichée
}

pub struct Row {
    pub address: Vec<usize>,          // indices depuis collection.tree
    pub depth: usize,
}
```

- `rows` est recalculé par `update` après chargement, dépliage ou repli.
  `view` ne fait que le parcourir.
- Un nœud est retrouvé par `address`, stable car la `Collection` n'est
  jamais modifiée. L'identité d'un dossier déplié est son chemin relatif.
- Après repli d'un dossier, la sélection est replacée sur la ligne qui
  porte son chemin.
- `offset` est ajusté par `update` pour que `selected` reste dans
  `[offset, offset + hauteur)`. La hauteur vient de `layout(size)` (D7),
  aussi utilisée par `view` : les deux ne peuvent pas diverger.
- `Exit` : `Normal` ou `TerminalError(io::Error)`. `main` en déduit le
  code de sortie.

Alternative écartée : `ListState` stocké dans le modèle et muté par le
rendu. Rendrait `view` impur. Recréer l'état à chaque rendu sans
`offset` coinçait la sélection en bas de l'écran pendant la remontée.

### D6. Détail : texte construit par une fonction pure

`detail_text(&Model) -> Text<'static>` construit le contenu du panneau.
`view` le rend dans un `Paragraph` avec retour à la ligne. `update` s'en
sert pour borner le défilement :
`max = line_count(largeur_intérieure) - hauteur_intérieure`, saturé à 0.
Le calcul n'a lieu que sur une touche de défilement ou un
redimensionnement.

Contenu, dans cet ordre :
- **requête** : nom, chemin ; `MÉTHODE URL` ; `Auth : <mode>` ; sections
  « En-têtes », « Paramètres de requête », « Paramètres de chemin » avec
  `clé: valeur` et ` (désactivé)` si besoin ; « Corps (<type>) » puis le
  contenu, ou la liste d'entrées, ou « bloc absent » ; enfin
  « Script pré-requête / Script post-réponse / Tests / Assertions » avec
  oui ou non. Les sections vides affichent « aucun ».
- **dossier** : nom, chemin, `seq` ou « aucun », nombre d'enfants,
  `folder.bru` absent, présent avec ses indicateurs, ou en erreur avec la
  raison.
- **erreur** : chemin et `Display` de l'erreur, qui ne cite jamais le
  contenu des lignes (garanti par `bru-parser`).

Libellés d'auth : la valeur écrite dans le fichier (`none`, `inherit`,
`bearer`, …), via une correspondance inverse de `AuthMode`.

### D7. Disposition et rendu

- `layout(size) -> Option<Areas>` : `None` sous 40×10 (message
  « Agrandir le terminal »). Sinon une ligne de titre, un corps découpé
  en arbre (35 %, au moins 24 colonnes) et détail (reste), une ligne de
  barre d'état.
- Titre : nom de la collection, ou `Chargement de <chemin>…`, ou
  `Erreur`.
- Lignes de l'arbre : indentation de deux espaces par niveau, puis
  - dossier : `▸`/`▾` et son nom ; suffixe ` ✗` si `folder.bru` est en
    erreur ;
  - requête : méthode alignée sur 6 caractères, puis le nom ;
  - erreur : `✗` et le **nom de fichier avec extension**, pris sur le
    chemin (et non `TreeNode::name()`, qui retire l'extension).
- Sélection en vidéo inverse. Bordure du panneau ayant le focus en gras
  et en couleur ; l'autre en style normal.
- Barre d'état : touches du focus courant, par exemple
  `↑↓ naviguer  → déplier  ← replier  Tab détail  q quitter`.
- Rendu après chaque message traité, pas de minuterie.

### D8. Ligne de commande

`cli::parse(args: impl Iterator<Item = OsString>) -> Result<Command, UsageError>`
avec `Command = Run(PathBuf) | Help | Version`. Pas d'`=`, pas de
regroupement d'options, `--` non géré (un chemin commençant par `-` se
passe en `./-x`). Chemins en `OsString` pour accepter les noms non UTF-8.

### D9. `main`, runtime et terminal

Ordre dans `main` :
1. Analyse des arguments ; `Help`/`Version` → sortie standard, code 0 ;
   erreur → usage sur la sortie d'erreur, code 2.
2. Si `stdout` n'est pas un terminal (`std::io::IsTerminal`) → message,
   code 1, avant toute modification du terminal.
3. Runtime tokio construit à la main (`Builder::new_multi_thread`).
4. `ratatui::try_init()` ; en cas d'échec, `try_restore()` puis code 1.
5. `runtime.block_on(app::run(&mut terminal, BruLoader, path, ...))`.
6. `ratatui::try_restore()`, puis `runtime.shutdown_background()` : le
   processus n'attend pas un chargement encore en cours.
7. Code 0 pour `Exit::Normal`, 1 avec message pour une erreur terminal.

`app::run` est générique sur le backend et reçoit le loader et le canal :

```rust
pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    loader: Arc<dyn CollectionLoader>,
    source: PathBuf,
    events: (mpsc::Sender<AppEvent>, mpsc::Receiver<AppEvent>),
) -> Result<Exit, B::Error>;
```

Il lance le chargement par `spawn_blocking`, dessine, puis traite les
événements jusqu'à `model.exit`. Le thread de lecture du terminal est
démarré par `main`, pas par `run`, ce qui permet de tester `run` avec
`TestBackend` en injectant des événements.

### D10. Tests

- `message` : correspondance de chaque touche, `Release` ignoré,
  `Ctrl+C` distinct de `c`.
- `update` : navigation complète sur `parser-cases` chargée par
  `BruLoader` (déplier, entrer, parent, replier, extrémités, `Fin` sur
  un terminal de 12 lignes), défilement borné du détail, retour en haut au
  changement de sélection, `Quit` pendant `Loading`.
- `view` : rendu `TestBackend` 100×30 comparé ligne à ligne sur
  l'arbre initial et le détail de `post-json`, `scripted`, `inherit`,
  `broken.bru`, `badmeta` ; 30×8 affiche le message ; 1×1 ne panique pas.
- `cli` : aide, version, chemin, défaut, erreurs.
- Intégration `tests/app_shell.rs` : `run` avec `TestBackend` et un
  loader bloqué sur une barrière ; l'envoi de `q` fait retourner `run`
  alors que le loader est toujours bloqué, puis la barrière est
  libérée.
- Manuel, dans un pseudo-terminal via `script` : lancement sur
  `parser-cases`, `q`, vérification du code 0 et de la séquence de sortie
  d'écran alternatif ; `bruno-tui --help | cat` ; `bruno-tui . > fichier`
  (code 1).

## Risks / Trade-offs

- [API `unstable-rendered-line-info` modifiée à une montée de version] →
  usage confiné à une fonction ; en repli, borne approximative par
  largeur de ligne.
- [Thread de lecture bloqué dans `read()` à la sortie] → sans effet :
  le processus se termine ; aucune ressource partagée n'est tenue.
- [`shutdown_background` abandonne un chargement en cours] → voulu :
  le chargement est en lecture seule, rien n'est laissé à moitié écrit.
- [Rendu synchrone dans une tâche async] → une écriture sur stdout par
  événement, négligeable ; `block_on` tient le fil principal, pas un
  worker du pool.
- [Collection très grande : milliers de lignes visibles] → `rows`
  recalculé seulement au dépliage ; le rendu ne construit que les lignes
  de la fenêtre visible.
- [Terminal sans glyphes `▸ ▾ ✗`] → rare dans les terminaux modernes ;
  remplaçables par `+ - x` sans changer la spec.
- [Détail d'un corps volumineux] → `line_count` parcourt tout le texte
  à chaque défilement ; acceptable pour des corps de requête usuels.
