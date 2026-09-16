## Context

Voir `proposal.md` pour la motivation et `specs/response-filter/spec.md`
pour le contrat.

Ce changement dépend du contrat introduit par le changement sœur
`add-request-run` (en cours de rédaction en parallèle, non encore
implémenté) — lu intégralement dans
`openspec/changes/add-request-run/design.md` (D2) :

```rust
pub struct RunState {
    pub active: Option<ActiveRun>,
    pub outcomes: HashMap<PathBuf, RequestOutcome>,  // clé = RequestNode::path
    pub last_failure: Option<RunFailure>,
}
pub struct RequestOutcome { pub result: runner::report::RequestResult, pub exit_code: Option<i32> }
```

Le corps de réponse est `RequestOutcome.result.response.data:
serde_json::Value` (`src/runner/report.rs`) : nul, chaîne, ou JSON
structuré selon ce que `bru` a reçu — vérifié sur
`tests/fixtures/reports/mixed.json` (`ok` → chaîne HTML, `json`/`green` →
objet `{"a": [1, 2], "b": null}`, `folder/down` → nul avec
`response.status = Error`, `skip` → nul avec `response.status =
Skipped`).

Coordination avec les changements sœurs `add-search-and-yank` et
`add-field-editing` (lus intégralement) : le premier introduit le
mécanisme de saisie de texte de `tui-shell`, via
`to_message(event, capture: Option<TextCapture>)` et une énumération
`TextCapture` délibérément ouverte (D2 de son design) ; le second y a déjà
ajouté sa propre variante (`Insert`, D4 de son design) plutôt que de
dupliquer le mécanisme. `add-response-filter` fait de même : il ajoute une
variante `Filter` à cette énumération pour sa propre saisie (D3), au lieu
d'introduire un paramètre concurrent. Résolu ici plutôt que laissé comme
risque, voir D3.

Sondé dans le scratchpad (programme Rust complet, compilé et exécuté,
voir historique) : `jaq-core` 3.1.1, `jaq-std` 3.0.3, `jaq-json` 2.0.3
(licences MIT). Faits vérifiés :
- `jaq_std::defs()` référence dans son fichier `defs.jq` des fonctions
  natives (`pow`, `matches`, `fromdateiso8601`, `encode_base64`, ...) qui
  exigent les features `format`/`log`/`math`/`regex`/`time` de
  `jaq-std`. Le compilateur résout **tout** le module de définitions à la
  compilation d'un filtre, même pour un filtre qui n'utilise aucune de
  ces fonctions : sans ces features, `Compiler::compile` échoue avec une
  liste de filtres non résolus. `jaq-std` doit donc être pris avec ses
  features par défaut (qui les incluent toutes) pour que `map`, `select`
  et le reste de la bibliothèque standard jq fonctionnent réellement.
  Coût : l'arbre de dépendances complet (`jaq-core` + `jaq-std` +
  `jaq-json`) compte 36 crates.
- `jaq_json::Val` implémente `serde::Deserialize` derrière la feature
  `serde` de `jaq-json` : `serde_json::from_value::<Val>(data)` convertit
  le corps sans repasser par du texte JSON.
- Aucune conversion retour `Val → serde_json::Value` n'existe dans la
  crate. `jaq_json::write::write(&mut buf, &Pp, level, &val)` (module
  `jaq_json::write`) produit un texte formaté directement dans un
  `Vec<u8>`, avec indentation configurable — utilisé tel quel pour
  l'affichage, sans passer par `serde_json::Value` ni par une conversion
  maison.
- Compilation (`Loader::load` puis `Compiler::compile`) et exécution
  (`filter.id.run(...)`) retournent chacune un `Result` ; un filtre
  malformé (`".a.b |"`) échoue proprement à la compilation, un filtre
  valide mais incompatible avec la valeur (`"chaîne" | map(.)`) échoue
  proprement à l'exécution — aucun panic observé dans les deux cas.
- Une évaluation peut produire zéro, une, ou plusieurs valeurs (jq est un
  langage de flux) : `filter.id.run(...)` rend un itérateur.

## Goals / Non-Goals

**Goals :**
- Filtre jq réellement compatible avec la syntaxe standard (`map`,
  `select`, `length`, indexation, slices, ...), pas un sous-ensemble
  maison.
- Aucune I/O disque, aucune persistance : le filtre vit uniquement dans
  `Model` le temps de la sélection courante.
- Erreurs de filtre traitées comme une valeur affichable, jamais comme un
  panic.

**Non-Goals :**
- Coloration syntaxique du JSON affiché (le panneau de détail n'en fait
  déjà pas pour le corps brut).
- Historique des filtres tapés, complétion, aide interactive sur la
  syntaxe jq.
- Filtrage d'un corps non-JSON au sens strict (en-têtes, texte libre hors
  du corps) : seul `response.data` est concerné.
- Résoudre ici la coordination avec `add-search-and-yank` sur le
  mécanisme de saisie (cf. Context) : documentée, pas tranchée.

## Decisions

### D1. Dépendances : `jaq-core`, `jaq-std`, `jaq-json`

```toml
jaq-core = "3.1.1"
jaq-std = "3.0.3"
jaq-json = { version = "2.0.3", features = ["serde"] }
```

`jaq-std` garde ses features par défaut (nécessaire, cf. Context).
Alternatives écartées :
- *Sous-ensemble de jaq maison (juste l'indexation `.a.b`, sans `map`
  ni `select`)* : ne couvrirait pas ce que `brio` propose ni ce qu'un
  utilisateur familier de jq attend en tapant `|`. Rejeté.
- *Appeler un binaire `jq` externe en sous-processus* : ajoute une
  dépendance d'exécution non garantie sur la machine de l'utilisateur,
  contredit l'esprit « tout intégré » du reste du projet, et
  réintroduirait un `Command::new` en dehors de `bru`, jamais fait
  ailleurs dans le code. Rejeté.
- *`gojq` (Go) équivalent inexistant en Rust* : `jaq` est l'implémentation
  Rust de référence, activement maintenue, utilisée par le binaire `jaq`
  lui-même en CLI.

### D2. État du filtre

```rust
// src/app/filter.rs
pub struct FilterState {
    /// Chemin de la requête pour laquelle ce filtre est ouvert.
    pub target: PathBuf,
    pub editing: bool,
    pub draft: String,
    /// Filtre validé et son résultat, `None` avant toute validation
    /// réussie ou après annulation.
    pub applied: Option<FilterResult>,
}

pub enum FilterResult {
    Output(Vec<String>),  // une entrée par valeur produite, déjà mise en forme
    Error(String),        // message de compilation ou d'exécution
}
```

`Model.filter: Option<FilterState>`. Lié à une requête par son `target` :
changer de sélection (D5) vide `Model.filter` plutôt que de le faire
persister par chemin, conformément à l'exigence « filtre volatile ». Pas
de `HashMap` par requête comme `RunState.outcomes` : contrairement au
résultat d'exécution, le filtre n'a aucune valeur à conserver au-delà de
la sélection courante (spec, « Filtre volatile »).

### D3. Messages et liaison de touche

`add-search-and-yank` (D2 de son design) introduit `to_message(event,
capture: Option<TextCapture>)` avec une énumération `TextCapture`
délibérément ouverte, précisément pour que d'autres saisies modales s'y
ajoutent sans réinventer le mécanisme. Ce changement y ajoute la variante
`Filter` :

```rust
pub enum TextCapture {
    Search,   // add-search-and-yank
    Insert,   // add-field-editing, si déjà appliqué
    Filter,   // ajouté ici
}

fn capture_message(event: AppEvent, capture: TextCapture) -> Option<Message> {
    match capture {
        TextCapture::Search => search_capture_message(event),
        TextCapture::Insert => insert_capture_message(event),
        TextCapture::Filter => filter_capture_message(event),
    }
}

pub enum Message {
    // ... existants inchangés ...
    OpenFilter,             // '|', hors saisie, uniquement si dispo (D4)
    FilterInput(char),      // produit par filter_capture_message
    FilterBackspace,
    ConfirmFilter,          // Entrée
    CancelFilter,           // Échap
}
```

`Model::text_capture()` gagne une branche `self.filter.as_ref()
.is_some_and(|f| f.editing).then_some(TextCapture::Filter)`, dans le même
ordre de priorité que documenté par `add-search-and-yank`/D2 — un ordre
qui n'a pas besoin d'arbitrage puisque l'exclusion mutuelle est garantie
par construction (une capture active intercepte les touches qui
ouvriraient une autre capture). Si `add-search-and-yank` et
`add-field-editing` sont déjà appliqués, ce sont des ajouts à des fichiers
existants ; si aucun ne l'est encore, `add-response-filter` crée
`TextCapture` avec uniquement la variante `Filter`.

`|` est libre (vérifié : absent de `openspec/specs/tui-shell/spec.md` et
des changements sœurs déjà écrits, `add-request-run`, `add-search-and-yank`
et `add-field-editing`). Hors saisie, ajout d'une seule entrée dans la
table de `key_message` : `Char('|') → OpenFilter`.

### D4. Disponibilité et ouverture

`OpenFilter` est sans effet (`Command::None`, aucun changement de modèle)
sauf si toutes ces conditions tiennent sur le nœud sélectionné :
- c'est une `TreeNode::Request` ;
- `model.run.outcomes` contient une entrée pour son chemin ;
- `outcome.result.response.status` est `ResponseStatus::Http(_)` (réponse
  effectivement reçue) ;
- `outcome.result.response.data` n'est pas `serde_json::Value::Null`.

Si ces conditions tiennent, `Model.filter` est créé (ou réouvert en
saisie si déjà présent pour ce chemin) avec `draft` initialisé au filtre
déjà appliqué s'il y en a un, sinon vide.

### D5. Réinitialisation à chaque changement de sélection

Chaque message qui change `model.tree.selected` (`Up`, `Down`, `Home`,
`End`, navigation dans `Right`/`Left` qui change la ligne, et la future
`NextMatch`/`PreviousMatch` de `add-search-and-yank`) met
`model.filter = None` si le chemin du nœud nouvellement sélectionné
diffère de `model.filter.target`. Implémenté par une petite fonction
`reset_filter_if_selection_changed(&mut Model, previous_path:
Option<&Path>)` appelée en fin de `navigate_tree`, plutôt que dans chaque
branche : un seul point d'entrée, cohérent avec la structure déjà en
place dans `update.rs`.

### D6. Compilation et exécution du filtre

```rust
fn evaluate(filter_src: &str, data: &serde_json::Value) -> FilterResult {
    // Loader::load + Compiler::compile (jaq_core::defs()+jaq_std::defs()+jaq_json::defs(),
    // jaq_core::funs()+jaq_std::funs()+jaq_json::funs()) → FilterResult::Error au premier échec
    // serde_json::from_value::<jaq_json::Val>(data.clone()) → entrée
    // filter.id.run(...) collecté en Vec<Result<Val, _>> → au premier Err, FilterResult::Error
    // sinon FilterResult::Output(valeurs formatées via jaq_json::write::write,
    //   Pp { indent: Some("  "), sep_space: true, sort_keys: false, .. })
}
```

Appelé de façon synchrone dans `update` (pas de `Command`) : la
compilation et l'exécution d'un filtre jq sur un corps de taille réaliste
sont de l'ordre de la microseconde à la milliseconde (aucune I/O), donc
compatibles avec l'invariant « `update` ne fait aucune I/O », qui porte
sur les opérations bloquantes externes (disque, réseau, sous-processus),
pas sur du calcul en mémoire pur.

### D7. Rendu

`view/detail.rs` : pour une `TreeNode::Request` dont `model.filter` est
`Some` et dont `target` correspond au nœud affiché, la section « Corps »
est remplacée par : la ligne du filtre (`Filtre : <draft ou pattern>`,
avec un curseur si `editing`), puis soit chaque ligne de
`FilterResult::Output`, soit le message de `FilterResult::Error` avec un
style d'erreur (cohérent avec le style déjà utilisé pour un nœud en
erreur de parsing, `view/tree.rs`). Hors filtre actif, la section Corps
est inchangée par rapport à ce que `add-request-run` définit déjà.
Pendant la saisie (`editing: true`), la barre d'état affiche le texte en
cours (même emplacement que la saisie de recherche d'`add-search-and-yank`
et la saisie de champ d'`add-field-editing` : les trois sont mutuellement
exclusives, cf. D3).

## Risks / Trade-offs

- [Trois changements sœurs auraient pu introduire chacun un mécanisme de
  saisie de texte concurrent] → résolu (D3) : `add-response-filter`
  étend l'énumération `TextCapture` d'`add-search-and-yank` par une
  variante `Filter`, au lieu d'en introduire un nouveau ; `update` reçoit
  toujours un message sans ambiguïté sur sa provenance.
- [`jaq-std` en features par défaut ajoute des crates transitives
  (`regex`, `aho-corasick`, `base64`, un crate de date) pour un usage qui
  n'en a peut-être pas besoin] → nécessaire pour que `defs()` compile
  (cf. Context) ; 36 crates au total, jugé raisonnable pour un
  interpréteur jq complet plutôt qu'un sous-ensemble non conforme.
- [Un filtre pathologique (récursion infinie, ex. `def f: f; f`) pourrait
  bloquer `update`, donc la boucle entière] → hors périmètre de ce
  changement d'introduire un délai ou une limite d'itérations ; risque
  jugé faible pour un usage interactif (l'utilisateur tape son propre
  filtre), à revisiter si constaté en pratique.
- [Le corps affiché après filtrage peut dépasser la hauteur du panneau]
  → réutilise le défilement du panneau de détail déjà spécifié par
  `tui-shell`, sans mécanisme propre.
- [Une future montée de version de `jaq-*` change la forme de `Val` ou de
  `write::write`] → confiné à `filter.rs`, sondé et versionné dans
  `Cargo.lock` comme le reste des dépendances du projet.

## Open Questions

- Faut-il aussi permettre de filtrer les en-têtes de réponse avec la même
  mécanique ? Laissé pour un futur changement si le besoin se confirme à
  l'usage ; ne change ni la spec ni les tâches de celui-ci.
