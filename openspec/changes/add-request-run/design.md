## Context

Voir `proposal.md` pour la motivation et
`specs/request-execution/spec.md` pour le contrat. État du code, vérifié
en lisant `src/runner/` et `src/app/` :

- `BruRunner::start(&self, RunRequest) -> RunHandle` (dans
  `src/runner/process.rs`) fait un `tokio::spawn` interne et retourne
  immédiatement. L'issue arrive plus tard comme un unique `RunEvent { id:
  RunId, outcome: RunOutcome }` sur le `mpsc::Sender<RunEvent>` fourni à
  la construction du `BruRunner`. `RunId` et `RunEvent` n'ont pas d'autre
  constructeur public ; un test qui a besoin d'un vrai `RunHandle` doit
  passer par un `BruRunner` réel (faux `bru`), pas par une valeur
  fabriquée à la main.
- `RunHandle::cancel(self)` consomme la poignée et envoie sur un
  `oneshot::Sender<()>` interne : c'est un appel synchrone, sans I/O
  bloquante, utilisable en dehors d'un contexte tokio.
- `RunOutcome` a trois variantes : `Completed { report: Report, exit_code
  }`, `Failed(RunError)`, `Cancelled`. `RunError` a six variantes
  (`BruNotFound`, `Spawn`, `NoReport`, `InvalidReport`, `Io`,
  `UnsupportedPlatform`) ; son `Display` ne cite jamais le contenu d'un
  rapport (garanti et testé dans `bru-runner`).
- Un `RequestResult` du rapport porte `test.filename` : le chemin relatif
  à la racine **avec** l'extension `.bru` (vérifié dans
  `tests/fixtures/reports/mixed.json`, ex. `"filename": "folder/down.bru"`
  pour la requête de `path: "folder/down"`). C'est exactement la forme de
  `RequestNode::path` dans `src/collection/tree.rs` : aucune conversion
  n'est nécessaire pour relier un résultat à son nœud dans l'arbre.
  `RequestResult::is_failure()` donne déjà le verdict exigé par le spec.
- `src/app/mod.rs::run()` est la seule fonction qui fait de l'I/O
  (chargement par `spawn_blocking`, lecture du terminal déléguée à
  `main`). `src/app/update.rs` documente explicitement : « `update` est
  la seule fonction qui modifie le modèle. Elle ne fait aucune I/O ». Les
  tests existants de `update` sont des `#[test]` synchrones, sans runtime
  tokio actif : un appel à `tokio::spawn` (donc à `BruRunner::start`)
  depuis `update` y paniquerait.
- Le module `app::view` a une signature `view(&Model, &mut Frame)`, pure,
  et `detail::detail_text(&Model) -> Text<'static>` construit le contenu
  du panneau à partir du nœud sélectionné. `tree::row_line` n'ajoute
  aujourd'hui un marqueur d'erreur que pour `TreeNode::Folder` (méta-
  données invalides) et `TreeNode::Error` ; `TreeNode::Request` n'a
  aucune décoration au-delà de la méthode.
- `message.rs::key_message` retourne tôt sur toute touche `Ctrl+<x>` :
  seul `Ctrl+C` produit un message (`ForceQuit`), toute autre combinaison
  `Ctrl+*` est ignorée. Aucune touche `r` ni `Ctrl+X` n'est utilisée
  ailleurs.
- `tui-shell`'s design.md anticipait ce changement : « Effets déclenchés
  par `update` (type `Command`) : aucun n'est nécessaire ici. Il sera
  introduit avec l'exécution de requêtes. » Ce changement introduit donc
  ce type, prévu mais absent jusqu'ici.

## Goals / Non-Goals

**Goals :**
- Préserver l'invariant « `update` ne fait aucune I/O » en introduisant un
  type d'effet que `update` retourne et que la boucle exécute.
- Donner aux résultats une forme de donnée simple, indexée par chemin de
  requête, exploitable telle quelle par `add-diagnostics-and-history`
  (qui veut la liste des échecs) et par `add-response-filter` (qui veut
  filtrer le corps de la dernière réponse d'une requête) sans qu'ils
  aient à relire le rapport brut.
- Garder `BruRunner` injectable par chemin de programme, comme le fait
  déjà `tests/runner_fake_bru.rs`, pour tester sans Node ni réseau.

**Non-Goals :**
- Agrégat de résultats affiché sur un nœud dossier (nombre de
  réussites/échecs d'un sous-arbre) : seul le nœud requête porte un
  marqueur. Laissé à `add-diagnostics-and-history`, qui a besoin d'un
  parcours récursif de toute façon.
- Historique de plusieurs exécutions par requête : seul le dernier
  résultat est conservé.
- Choix de l'environnement ou surcharge de variable (`RunRequest.env` et
  `env_vars` restent vides) : `add-environment-picker`.
- Rendu détaillé du corps de réponse (coloration, pliage, JSON formaté) :
  affiché tel que `bru` le restitue, comme le fait déjà le panneau de
  détail pour le corps d'une requête.

## Decisions

### D1. `update` retourne un `Command` ; la boucle seule fait l'I/O

```rust
/// Effet demandé par `update`, à exécuter par la boucle `run`, qui seule
/// détient le `BruRunner` et tourne dans un contexte tokio.
pub enum Command {
    None,
    /// Démarrer cette exécution ; la boucle appelle `BruRunner::start`
    /// puis renvoie le résultat à `update` via `Message::RunStarted`.
    StartRun { request: RunRequest, target: PathBuf },
}
```

`update(model: &mut Model, message: Message) -> Command` (signature
modifiée ; actuellement `-> ()`). Chaque site d'appel existant dans les
tests (`update(&mut model, Message::X);` en position d'instruction)
continue de compiler sans changement, `Command` n'étant pas annoté
`#[must_use]` : on l'ignore là où il vaut toujours `Command::None`, ce qui
concerne tous les tests déjà écrits pour `tui-shell`.

Pourquoi un effet plutôt qu'un appel direct : `BruRunner::start` fait un
`tokio::spawn`, qui panique hors runtime. `RunHandle::cancel`, lui,
n'exécute qu'un envoi `oneshot` synchrone et peut rester appelé
directement dans `update` (D4) : l'indirection par `Command` n'est
introduite que là où elle est réellement nécessaire.

Alternative écartée : passer un `Arc<BruRunner>` à `update` pour qu'elle
lance elle-même l'exécution. Rejeté : romprait la garantie « aucune I/O »
documentée dans `update.rs`, et ferait paniquer tous les tests
synchrones existants qui n'ont pas de runtime tokio actif.

### D2. Forme des données de résultat dans `Model` (contrat stable)

Ajoutée à `src/app/model.rs`, sous un nouveau champ `pub run: RunState` de
`Model` :

```rust
/// État des exécutions : au plus une active, un résultat par requête.
#[derive(Debug, Default)]
pub struct RunState {
    /// Exécution en cours, s'il y en a une.
    pub active: Option<ActiveRun>,
    /// Dernier résultat connu par requête. Clé : chemin relatif à la
    /// racine de la collection, identique à `RequestNode::path` et à
    /// `RequestResult::test.filename` (avec l'extension `.bru`) —
    /// aucune conversion n'est nécessaire pour indexer ou pour retrouver
    /// un nœud de l'arbre à partir d'une clé.
    pub outcomes: HashMap<PathBuf, RequestOutcome>,
    /// Dernière exécution n'ayant produit aucun résultat exploitable.
    /// Effacé au lancement réussi d'une nouvelle exécution.
    pub last_failure: Option<RunFailure>,
}

pub struct ActiveRun {
    pub id: runner::RunId,
    /// Chemin lancé : la requête, ou le dossier en mode récursif.
    pub target: PathBuf,
    pub recursive: bool,
    /// `None` juste après une annulation déjà demandée : un second appui
    /// sur la touche d'annulation reste sans effet (idempotent).
    pub handle: Option<runner::RunHandle>,
}

/// Résultat conservé pour une requête, issu d'un rapport valide — que le
/// verdict de la requête soit un succès ou un échec.
pub struct RequestOutcome {
    pub result: runner::report::RequestResult,
    pub exit_code: Option<i32>,
}

/// Exécution n'ayant pas produit de résultat exploitable.
pub enum RunFailure {
    /// `bru` n'a pas pu s'exécuter ou son rapport est inexploitable.
    Error { target: PathBuf, error: runner::RunError },
    Cancelled { target: PathBuf },
}
```

C'est le contrat que peuvent lire les changements sœurs :
- `add-diagnostics-and-history` parcourt `model.run.outcomes` (chaque
  valeur porte déjà `RequestResult::is_failure()`) pour lister les
  requêtes en échec, sans dépendre de l'arbre ni du chargement ;
- `add-response-filter` lit `model.run.outcomes.get(path).result.response`
  pour la requête sélectionnée, sans relancer d'exécution ni relire le
  rapport brut.

Pourquoi une `HashMap<PathBuf, _>` plutôt qu'un champ sur `RequestNode` :
`Collection` (dans `bru-parser`) est immuable après chargement et n'a
aucune notion d'exécution ; y ajouter un champ mutable romprait la
séparation entre les deux capacités. Indexer par chemin, déjà unique et
stable pour la durée d'une session, est la solution la plus simple qui
retrouve un nœud sans le modifier.

Pourquoi conserver `RunError` et `RequestResult` tels quels plutôt qu'un
type miroir : ce sont déjà des types stables, exhaustifs, avec un
`Display` sûr (`RunError`) et une méthode de verdict déjà correcte
(`RequestResult::is_failure`). Dupliquer leurs champs dans un type propre
à `app` ajouterait une resynchronisation à maintenir pour un bénéfice nul,
`bru-runner` étant une dépendance interne stable du même dépôt.

### D3. `Message` et branchement des touches

```rust
pub enum Message {
    // ... existants inchangés ...
    /// Touche `r`, hors navigation : agit sur `model.tree.selected`.
    RunSelected,
    /// `Ctrl+X`.
    CancelRun,
    /// Renvoyé par la boucle après avoir exécuté `Command::StartRun`.
    RunStarted { id: runner::RunId, target: PathBuf, recursive: bool, handle: runner::RunHandle },
    /// Issue reçue sur le canal d'événements.
    RunFinished(runner::RunEvent),
}
```

Dans `message.rs::key_message` : ajout de `KeyCode::Char('r') =>
Message::RunSelected` dans le match principal (sans modificateur) ; le
branchement `Ctrl+*` devient `match key.code { Char('c') => ForceQuit,
Char('x') => CancelRun, _ => return None }` au lieu du simple test
d'égalité à `'c'`. Ces deux touches sont actives quel que soit le focus
(`Tree` ou `Detail`) : dans `update`, elles sont traitées dans le match
de premier niveau, au même titre que `NextFocus`/`FocusTree`/`Quit`, pas
dans `navigate_tree`/`scroll_detail`.

### D4. Traitement dans `update`

- `RunSelected` : si `model.run.active.is_some()`, `Command::None` (spec
  « Une exécution à la fois »). Sinon, selon `model.selected_node()` :
  `Request(node)` → cible `node.path.clone()`, non récursif ;
  `Folder(node)` → cible `folder.path.clone()`, récursif ; `Error(_)` ou
  absence de sélection (collection non chargée ou vide) →
  `Command::None`. Sinon, `model.run.last_failure = None`, et retour de
  `Command::StartRun { request: RunRequest { collection_root:
  model.loaded().root.clone(), targets: vec![target.clone()], recursive,
  env: None, env_vars: Vec::new() }, target }`.
- `CancelRun` : `if let Some(active) = &mut model.run.active { if let
  Some(handle) = active.handle.take() { handle.cancel(); } }` (appel
  synchrone, cf. D1) ; `active` reste renseigné jusqu'à la confirmation
  par `RunFinished`. `Command::None` dans tous les cas.
- `RunStarted { id, target, recursive, handle }` : renseigne
  `model.run.active = Some(ActiveRun { id, target, recursive, handle:
  Some(handle) })`. `Command::None`.
- `RunFinished(RunEvent { id, outcome })` : sans effet si
  `model.run.active` est absent ou porte un autre `id` (événement
  obsolète). Sinon, `let active = model.run.active.take().unwrap();` puis
  selon `outcome` :
  - `Completed { report, exit_code }` : pour chaque `RequestResult` de
    `report.iterations().iter().flat_map(|it| it.results)` (en les
    déplaçant hors du rapport, sans clone), insertion dans
    `model.run.outcomes` avec pour clé `PathBuf::from(&result.test.filename)`
    (D2) ; `model.run.last_failure = None`.
  - `Failed(error)` : `model.run.last_failure = Some(RunFailure::Error {
    target: active.target, error })`.
  - `Cancelled` : `model.run.last_failure = Some(RunFailure::Cancelled {
    target: active.target })`.

  `Command::None`.

### D5. Boucle `run()` : construction du `BruRunner` et adaptation du canal

`BruRunner` publie sur un `mpsc::Sender<runner::RunEvent>`, distinct du
canal `AppEvent` de la boucle. `run()` crée son propre canal interne et
une tâche d'adaptation :

```rust
pub async fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    loader: Arc<dyn CollectionLoader>,
    source: PathBuf,
    sender: mpsc::Sender<AppEvent>,
    mut events: mpsc::Receiver<AppEvent>,
    bru_program: OsString,   // nouveau paramètre : "bru" en usage réel,
                             // chemin du faux bru dans les tests
) -> Result<Exit, B::Error> {
    let (run_tx, mut run_rx) = mpsc::channel(EVENT_BUFFER);
    let runner = BruRunner::with_program(bru_program, run_tx);
    {
        let sender = sender.clone();
        tokio::spawn(async move {
            while let Some(event) = run_rx.recv().await {
                if sender.send(AppEvent::Run(event)).await.is_err() {
                    break;
                }
            }
        });
    }
    // ... reste de la boucle inchangé, avec dans la traduction
    // AppEvent → Message : Run(event) => Message::RunFinished(event) ...
    // et après update(), si Command::StartRun { request, target } :
    //   let handle = runner.start(request);
    //   update(&mut model, Message::RunStarted {
    //       id: handle.id(), target, recursive: request.recursive, handle,
    //   });
}
```

`AppEvent` gagne une variante `Run(runner::RunEvent)`. `main.rs` passe
`"bru".into()` ; les tests passent le chemin de
`tests/fixtures/fake-bru/fake-bru.sh`, comme le fait déjà
`tests/runner_fake_bru.rs` pour le runner seul.

Pourquoi un canal et une tâche d'adaptation plutôt que de changer le type
du canal de `BruRunner` : `bru-runner` n'est pas modifié par ce
changement (périmètre de la proposition). L'adaptation est une poignée de
lignes et isole complètement `app` du type `RunEvent` du point de vue du
reste de la boucle, qui ne connaît que `AppEvent`.

### D6. Rendu : marqueur de statut, indicateur de progression, section résultat

- `tree::row_line` : pour `TreeNode::Request`, si `model.run.outcomes`
  contient une entrée pour son chemin, ajoute un suffixe de statut —
  succès (vert) si `!result.is_failure()`, échec (rouge, glyphe distinct
  du `✗` déjà utilisé par les nœuds de parsing en erreur) sinon. Si
  `model.run.active` cible ce chemin, ajoute plutôt un indicateur de
  progression (glyphe distinct, ex. `…`), prioritaire sur un éventuel
  ancien statut. `row_line` doit donc recevoir le `Model` (ou les deux
  informations utiles) en plus du nœud ; actuellement il ne reçoit que
  `(node, depth, expanded)`.
- Barre d'état (`status_line`) : si `model.run.active.is_some()`,
  remplace le rappel de touches habituel par un message identifiant la
  cible en cours et rappelant la touche d'annulation. Sinon, si
  `model.run.last_failure.is_some()`, affiche le message d'échec
  correspondant (une ligne, tronquée à la largeur du panneau) à la place
  du rappel habituel, jusqu'au prochain `RunSelected` réussi.
- `detail::detail_text` : pour `TreeNode::Request`, après les sections
  existantes, si `model.run.outcomes` contient une entrée pour ce chemin,
  ajoute une section « Résultat » : verdict, statut de réponse (code,
  « aucune réponse » + message d'erreur, ou « ignorée »), temps de
  réponse, en-têtes de réponse, corps de réponse, puis pour chaque
  assertion et chaque test (dont pré-requête et post-réponse) sa
  description ou son expression, son statut et son message d'erreur s'il
  y en a un.

Les glyphes et couleurs précis ne sont pas fixés par le spec (qui ne
mandate qu'un marqueur « distinct ») ; ce document fixe le choix
d'implémentation, dans le même esprit que `tui-shell`/D7 pour `▸`/`▾`/`✗`.

## Risks / Trade-offs

- [Étendre `Message`/`update`/`view` alourdit ces fichiers déjà
  substantiels] → suivre le découpage déjà en place (`view/tree.rs`,
  `view/detail.rs`) plutôt que d'ajouter un module séparé pour un
  affichage qui reste une extension des mêmes fonctions ; pas de nouveau
  fichier de production sauf si la taille le justifie en cours
  d'implémentation.
- [Une seule exécution à la fois est plus restrictif que ce que
  `bru-runner` permet] → limite assumée (D-scope) pour garder un seul
  indicateur de progression et une seule touche d'annulation sans
  ambiguïté ; `bru-runner` n'empêche pas un futur changement d'autoriser
  plusieurs exécutions concurrentes sans changer son API.
- [`RunFinished` reçu après que `model.run.active` a déjà été effacé, par
  exemple par une remise à zéro future du modèle] → l'événement est
  silencieusement ignoré (comparaison d'`id`), jamais appliqué à la
  mauvaise exécution.
- [Changer la signature de `update` (`-> Command` au lieu de `-> ()`)
  touche un point déjà largement testé dans `tui-shell`] → aucun test
  existant n'assigne ni n'inspecte la valeur de retour actuelle
  (recherche faite dans `src/app/update.rs`, `src/app/view/mod.rs`,
  `src/app/model.rs`, `tests/app_shell.rs`) : ils continuent de compiler
  sans modification.
- [Le canal d'adaptation `RunEvent → AppEvent` ajoute une tâche tokio
  supplémentaire par session] → coût négligeable, une seule tâche pour
  toute la durée de l'application, symétrique au thread de lecture du
  terminal déjà présent.
