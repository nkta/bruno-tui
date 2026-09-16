## Context

Voir `proposal.md` pour la motivation et
`specs/diagnostics-and-history/spec.md` pour le contrat. État du code
vérifié en lisant `src/app/model.rs`, `src/app/view/mod.rs`,
`src/app/update.rs`, `src/collection/tree.rs` et `src/runner/report.rs`,
ainsi que `proposal.md`/`design.md` du changement sœur `add-request-run`
(non encore implémenté, mais dont le contrat de données est fixé) :

- `Model.focus: Focus` ne connaît que `Tree` et `Detail`
  (`src/app/model.rs`). `status_line` (`view/mod.rs`) retourne aujourd'hui
  un `&'static str` selon `(CollectionState, Focus)` ; `add-request-run`
  le rend déjà dynamique pour afficher la progression et les échecs
  d'exécution — ce changement s'ajoute à cette même fonction, désormais
  déjà non `'static`.
- `Collection` (`bru-parser`) porte l'arbre sous forme de `Vec<TreeNode>`
  récursif ; un `ErrorNode { path, error }` ou un `FolderNode { meta:
  Some(Err(error)), .. }` sont les deux seules formes d'erreur de
  chargement (`src/collection/tree.rs`). `Collection` est immuable après
  chargement : aucune structure supplémentaire n'y est ajoutée par ce
  changement, la liste des diagnostics est calculée à la demande par un
  parcours de l'arbre.
- Le contrat de `add-request-run` (son `design.md`, D2 et D3) fixe :
  `Model.run: RunState` avec `outcomes: HashMap<PathBuf, RequestOutcome>`,
  `RequestOutcome { result: runner::report::RequestResult, exit_code:
  Option<i32> }`, `last_failure: Option<RunFailure>`, et un message
  `Message::RunFinished(runner::RunEvent)` reçu par `update` à la fin
  d'une exécution — c'est l'unique point où une entrée d'historique peut
  être produite, avant que `add-request-run`/D4 ne consomme l'événement
  pour mettre à jour `outcomes`.
- `runner::report::Iteration.summary: Summary` porte des compteurs
  agrégés (`passed_requests`, `failed_requests`, …) calculés par `bru`,
  mais **ne reflète pas** les échecs d'assertions ou de tests quand le
  statut d'une requête reste `pass` (`openspec/specs/bru-runner/spec.md`,
  "Verdict d'échec par requête") : `RequestResult::is_failure()` est la
  seule source de vérité pour un verdict, déjà utilisée par
  `add-request-run`/D4. Le résumé de `bru` MUST NOT être utilisé pour le
  verdict global d'une entrée d'historique.
- `RequestResult.run_duration: f64` (secondes) existe par requête ; rien
  ne donne directement la durée totale d'une exécution multi-requêtes
  sans sommer ces valeurs ou sans horodater soi-même le début et la fin.
- `message.rs::key_message` : `D` et `H` (majuscules) ne sont utilisées
  nulle part, ni dans `tui-shell`, ni dans `add-request-run` (`r`,
  `Ctrl+X`), ni dans `add-search-and-yank` (`/`, `n`, `N`, `v`, `y`).

## Goals / Non-Goals

**Goals :**
- Point de vue agrégé, calculé à la demande, sans dupliquer l'état déjà
  détenu par `Collection` ou par `RunState`.
- Journal borné, en mémoire uniquement, sans nouvelle dépendance.
- Réutiliser le mécanisme de lancement d'exécution de `add-request-run`
  pour le rejeu, plutôt que d'en construire un second.

**Non-Goals :**
- Étendre `ActiveRun` (type de `add-request-run`) pour lui ajouter un
  horodatage de départ : la durée d'une entrée est calculée à partir des
  `run_duration` déjà présents dans le rapport reçu, pas d'un chronomètre
  du côté de l'interface. Ce changement ne modifie aucun type de
  `add-request-run`.
- Filtrage, recherche ou export dans l'un ou l'autre panneau.
- Persistance sur disque du journal (voir `proposal.md`).

## Decisions

### D1. Diagnostics calculés à la demande, jamais stockés dans le Model

```rust
/// Une entrée du panneau de diagnostics.
pub struct DiagnosticEntry<'a> {
    pub path: &'a Path,
    pub reason: String,       // Display de l'erreur portée par le nœud
    pub address: Vec<usize>,  // adresse dans Collection::tree, pour la navigation croisée (D4)
}

/// Parcourt l'arbre et collecte tous les nœuds en erreur, dans l'ordre
/// de l'arbre (même ordre que l'affichage, donc déterministe).
pub fn diagnostics(collection: &Collection) -> Vec<DiagnosticEntry<'_>>;
```

Fonction pure dans un nouveau fichier `src/app/diagnostics.rs`, appelée à
la fois par `update` (pour borner la sélection et résoudre la navigation
croisée) et par `view` (pour le rendu et le badge). Aucun champ n'est
ajouté à `Model` pour stocker cette liste : `Collection` ne change jamais
après chargement pendant la durée d'une session, recalculer à chaque
accès (une poignée de nœuds, un arbre déjà tenu en mémoire en entier)
coûte moins qu'une structure à tenir synchronisée.

Alternative écartée : calculer la liste une fois au chargement et la
stocker dans `Model`. Rejeté pour la duplication d'état sans bénéfice —
`Collection` étant immuable, il n'y a pas de risque de désynchronisation
à éviter, seulement un coût de parcours négligeable à chaque ouverture du
panneau.

### D2. `Focus` gagne deux variantes, `Model` gagne l'historique et les sélections des panneaux

```rust
pub enum Focus { Tree, Detail, Diagnostics, History }

pub struct HistoryEntry {
    pub started_at: SystemTime,
    pub target: PathBuf,
    pub recursive: bool,
    pub outcome: HistoryOutcome,
}

pub enum HistoryOutcome {
    Completed { total: u64, failed: u64, duration_secs: f64 },
    Failed(runner::RunError),
    Cancelled,
}
```

Sur `Model` : `pub history: VecDeque<HistoryEntry>` (borné à
`HISTORY_LIMIT: usize = 200`, choisi comme ordre de grandeur d'une
campagne de TNR déjà large tenue en une session, sans laisser la mémoire
croître sans borne sur une session longue) ; `pub diagnostics_selected:
usize` et `pub history_selected: usize`, sur le même modèle que
`TreeState::selected`, bornés à la longueur de la liste courante à
chaque accès plutôt que maintenus à jour à chaque mutation de la
collection ou du journal (il n'y a qu'un seul point de mutation pour
chacun : le chargement pour les diagnostics, jamais modifié en cours de
session ; l'ajout au journal, qui ne peut que garder ou avancer une
sélection déjà à l'entrée la plus récente — D5).

`total`/`failed` de `HistoryOutcome::Completed` sont calculés par
`RequestResult::is_failure()` sur les résultats du rapport (D3), jamais
lus depuis `Summary` de `bru` (voir Context). `SystemTime` plutôt que
`Instant` : affichable (heure de l'utilisateur), alors qu'`Instant` ne
sert qu'à mesurer un intervalle.

### D3. Ajout au journal dans `update`, sur `RunFinished`

`add-request-run`/D4 traite déjà `Message::RunFinished(RunEvent { id,
outcome })` pour mettre à jour `model.run.outcomes`/`last_failure`. Ce
changement ajoute, au même point de traitement (avant que
`active.target`/`active.recursive` ne soient consommés), la construction
d'une `HistoryEntry` :

```rust
let started_at = SystemTime::now();
let entry = HistoryEntry {
    started_at,
    target: active.target.clone(),
    recursive: active.recursive,
    outcome: match &outcome {
        RunOutcome::Completed { report, .. } => {
            let results: Vec<_> = report.iterations().iter().flat_map(|it| &it.results).collect();
            HistoryOutcome::Completed {
                total: results.len() as u64,
                failed: results.iter().filter(|r| r.is_failure()).count() as u64,
                duration_secs: results.iter().map(|r| r.run_duration).sum(),
            }
        }
        RunOutcome::Failed(error) => HistoryOutcome::Failed(error.clone_for_history()),
        RunOutcome::Cancelled => HistoryOutcome::Cancelled,
    },
};
if model.history.len() == HISTORY_LIMIT { model.history.pop_back(); }
model.history.push_front(entry);
```

`duration_secs` est la somme des `run_duration` du rapport plutôt qu'un
chronomètre côté interface (Non-Goals) : disponible sans modifier
`ActiveRun`, et cohérent avec l'unité déjà utilisée par `bru`.

Point à trancher à l'implémentation, une fois `RunError` disponible :
`RunError` ne dérive pas forcément `Clone` aujourd'hui (à vérifier sur le
code réel de `bru-runner` au moment d'implémenter) ; si ce n'est pas le
cas, l'entrée d'historique conserve la chaîne produite par son `Display`
(`String`) plutôt que la valeur typée — suffisant pour l'affichage,
seule exigence du spec. `clone_for_history()` ci-dessus est un nom
indicatif, pas une API à créer dans `bru-runner`.

### D4. Navigation croisée : retrouver un nœud par son adresse

`DiagnosticEntry::address` (D1) est la même forme `Vec<usize>` que
`Row::address` (`src/app/model.rs`), déjà consommée par
`Model::node_at`. Valider une entrée du panneau de diagnostics :

1. déplie tous les dossiers ancêtres de `address` dans
   `model.tree.expanded` (parcours des préfixes de `address`, comme le
   fait déjà `test_support::select` dans les tests existants) ;
2. `refresh_rows` (déjà `pub(crate)` dans `update.rs`) ;
3. retrouve l'indice de ligne dont `Row::address == address` et
   l'assigne à `model.tree.selected` ;
4. `scroll_tree_into_view` (déjà présent) ;
5. `model.focus = Focus::Tree`.

Aucune nouvelle primitive de recherche de nœud n'est nécessaire : tout
existe déjà côté arbre, seul l'appel manquait.

### D5. Messages et branchement des touches

```rust
pub enum Message {
    // ... existants, et ceux ajoutés par add-request-run (r, Ctrl+X,
    // RunSelected, RunFinished, ...) ...
    ToggleDiagnostics,  // `D`
    ToggleHistory,      // `H`
}
```

Aucun nouveau message n'est nécessaire pour la fermeture d'un panneau :
`Échap` est déjà lié à `Message::FocusTree` (`message.rs`), qui ramène le
focus à l'arbre quel que soit l'état précédent, ce qui couvre exactement
"fermer le panneau courant" pour `Diagnostics` comme pour `History`.

Dans `key_message` (`src/app/message.rs`) : `Char('D') =>
Message::ToggleDiagnostics` et `Char('H') => Message::ToggleHistory`,
ajoutées au match principal (sans modificateur ; crossterm distingue déjà
`'D'` de `'d'`, comme c'est le cas pour `G`/`g`).

`Right`/`Entrée` et `r` sont des messages déjà existants (`Message::Right`
côté navigation, `Message::RunSelected` côté `add-request-run`) : dans
`update`, leur effet dépend de `model.focus`, exactement comme
`navigate_tree`/`scroll_detail` en dépendent déjà aujourd'hui. Pas de
nouveau nom de message pour "valider une entrée de diagnostics" ni pour
"rejouer" : `Message::Right` route vers la navigation croisée (D4) quand
`focus == Diagnostics`, et `Message::RunSelected` route vers le rejeu
(D3 réutilisé avec la cible de l'entrée sélectionnée au lieu du nœud de
l'arbre) quand `focus == History`. `Up`/`Down`/`Home`/`End`/`PageUp`/
`PageDown` routent vers un déplacement de `diagnostics_selected` ou
`history_selected`, bornée à la longueur de la liste courante, sur le
modèle de `navigate_tree`.

`ToggleDiagnostics`/`ToggleHistory` : si `model.focus` est déjà la
variante ciblée, repasse à `Focus::Tree` (comportement d'interrupteur,
comme l'ouverture) ; sinon, y passe. Les deux sont sans effet tant
qu'aucune collection n'est chargée (`CollectionState::Loading`/
`Failed`), comme le reste des raccourcis liés à l'arbre.

### D6. Rendu : badge, et deux panneaux plein corps

- Barre de titre (`title_line`, `view/mod.rs`) : après le nom de la
  collection, si `diagnostics(collection).len() > 0`, ajoute un
  `Span` stylé `⚠ N`. Calcul fait une fois par frame, coût négligeable
  pour une collection de taille réaliste.
- `Focus::Diagnostics`/`Focus::History` sont rendus comme le sont déjà
  `CollectionState::Failed` (un `Paragraph`/`List` occupant `areas.body`
  en entier, bordure focus comme les autres panneaux via la fonction
  `panel` déjà présente), plutôt que de partager l'espace avec l'arbre et
  le détail — ce sont des vues de session, pas des vues de la collection.
- Une liste vide affiche le message du spec ("aucune erreur"/"aucune
  exécution") au lieu d'une `List` vide, sur le modèle de
  `render_tree` pour une collection sans nœud.
- `status_line` gagne deux entrées supplémentaires dans son
  `match (CollectionState, Focus)` pour rappeler les touches de chaque
  panneau (`↑↓ naviguer  → aller au nœud  Échap arbre` pour Diagnostics ;
  `↑↓ naviguer  r rejouer  Échap arbre` pour Historique).

## Risks / Trade-offs

- [Recalculer les diagnostics à chaque accès plutôt que les mettre en
  cache] → coût d'un parcours de l'arbre par ouverture de panneau ou par
  frame pour le badge ; négligeable pour une collection de taille
  réaliste (voir `bru-parser`, borne de profondeur déjà en place). À
  reconsidérer seulement si des collections de plusieurs milliers de
  nœuds posent un problème mesuré.
- [Sommer `run_duration` plutôt qu'horodater le début et la fin d'une
  exécution] → une entrée d'historique ne reflète pas le temps
  d'attente réseau parallèle éventuel ni le temps de démarrage de `bru`
  lui-même ; c'est une durée d'exécution des requêtes, pas une durée
  d'horloge murale. Documenté comme tel dans le spec ("durée") sans
  préciser laquelle, laissant ce choix d'implémentation ouvert.
- [`RunError` pas forcément `Clone`] → repli sur une `String` (D3),
  suffisant pour l'affichage seul exigé par le spec.
- [Historique en mémoire uniquement : perdu à un crash ou un
  redémarrage] → assumé (Non-Goals) ; pas de garantie de durabilité
  offerte par ce changement, seulement une aide pendant la session en
  cours.
- [Deux nouvelles variantes de `Focus` élargissent tous les `match`
  existants sur ce type] → recherche faite dans `src/app/` : trois sites
  (`update.rs` en tête de `update`, `view/mod.rs::view` et
  `status_line`) ; tous font déjà un `match` exhaustif sur `Focus` ou sur
  `(CollectionState, Focus)`, le compilateur signale toute omission.
