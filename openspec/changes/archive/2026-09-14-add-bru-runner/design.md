## Context

Dépôt vierge : `src/main.rs` est un « Hello, world », `Cargo.toml` n'a
aucune dépendance, aucun spec n'existe. Voir `proposal.md` pour la
motivation et `specs/bru-runner/spec.md` pour le contrat.

Faits observés sur `bru` 2.13.2 (Node 22, Linux/WSL) pendant la
préparation de ce changement, et qui dictent la conception :

- `--reporter-json <chemin>` écrit le rapport **dans un fichier** ; il n'y
  a pas d'option « rapport sur stdout », et stdout porte la sortie console
  (arbre des résultats, tableau récapitulatif).
- `bru` accepte `--reporter-json /dev/fd/3` et un FIFO : le rapport
  complet y est écrit correctement.
- Le rapport est un **tableau d'itérations** `[{ iterationIndex, results,
  summary }]`, même sans itération CSV.
- `results[].status` vaut `pass` même quand des assertions ou des tests
  échouent ; seul `summary.failedRequests` reflète l'échec. Valeurs vues :
  `pass`, `fail`, `error`, `skipped`.
- `response.status` est polymorphe : entier (`200`), `"error"` (connexion
  refusée, `response.headers`/`data`/`url` à `null`) ou `"skipped"`
  (`statusText` explicatif, champs `headers`/`url` absents).
- `response.data` est une chaîne (HTML) ou du JSON structuré.
- `testResults`, `preRequestTestResults`, `postResponseTestResults`
  partagent la forme `{ description, status, error?, actual?, expected?,
  uid }`.
- Codes de sortie : `0` tout passe, `1` échecs de tests/requêtes (rapport
  écrit), `5` chemin inexistant, `6` environnement introuvable (pas de
  rapport), `0` **sans rapport** quand le cwd n'est pas une racine de
  collection (« You can run only at the root of a collection »). Les
  messages d'erreur fatale sortent sur stdout.

## Goals / Non-Goals

**Goals:**
- API interne minimale, testable sans UI : lancer, annuler, recevoir une
  issue.
- Zéro octet du rapport dans un fichier régulier.
- Types stricts là où la forme est observée, `serde_json::Value` là où
  elle est libre (corps, en-têtes, `actual`/`expected`).

**Non-Goals:**
- Progression requête par requête pendant l'exécution (nécessiterait de
  parser la sortie console, instable) : l'issue arrive en fin de run.
- Windows natif (pas de `/dev/fd`, `bru` lancé via un shim `.cmd`).
- Branchement dans `AppEvent` / la boucle ratatui : le runner émet sur un
  `mpsc::Sender<RunEvent>` générique ; l'adaptation en `AppEvent` se fera
  dans le changement qui introduit la boucle.
- Détection ou vérification de la version de `bru`.

## Decisions

### D1. Transport du rapport : pipe anonyme hérité en fd 3

Le parent crée un pipe anonyme (`std::io::pipe`, stable depuis Rust 1.87 ;
toolchain locale 1.98), mappe l'extrémité écriture sur le fd 3 de l'enfant
et passe `--reporter-json /dev/fd/3`. Le parent ferme sa copie de
l'extrémité écriture juste après le `spawn`, puis lit l'extrémité lecture
jusqu'à EOF, en parallèle de l'attente du processus.

Le mappage de fd dans l'enfant se fait avec la crate `command-fds`
(feature `tokio`), qui encapsule `dup2` dans `pre_exec` derrière une API
sûre. La commande détient la copie parent de l'extrémité écriture : elle
est détruite juste après `spawn`, faute de quoi l'EOF n'arrive jamais.

Alternatives écartées :
- *Fichier temporaire 0600 supprimé après lecture* : contredit « secrets
  jamais écrits sur disque », et laisse un fichier en cas de crash/kill.
- *FIFO dans un répertoire temporaire* : aucune donnée sur disque, mais
  lecture fragile (EOF immédiat tant qu'aucun écrivain n'a ouvert, boucle
  de réveils `EPOLLHUP`, nettoyage de l'inode) et une dépendance `nix`.
- *`/dev/stdout`* : mélangé à la sortie console, non séparable.
- *`pre_exec` + `libc::dup2` à la main* : `unsafe` dans le code du projet
  pour un gain nul par rapport à `command-fds`.

La lecture se fait dans `tokio::task::spawn_blocking` sur le
`PipeReader` bloquant (simple, pas de conversion non bloquante ; un run
occupe un thread du pool bloquant, acceptable pour l'usage).
L'extrémité donnée à l'enfant reste bloquante : Node écrit via
`writeFileSync`, qui ne gère pas `EAGAIN`.

### D2. API du module

```rust
pub struct RunRequest {
    pub collection_root: PathBuf,
    pub targets: Vec<PathBuf>,        // vide = collection entière
    pub recursive: bool,
    pub env: Option<String>,
    pub env_vars: Vec<(String, SecretString)>,
}

pub struct RunId(u64);
pub struct RunEvent { pub id: RunId, pub outcome: RunOutcome }

pub enum RunOutcome {
    Completed { report: Report, exit_code: Option<i32> },
    Failed(RunError),
    Cancelled,
}

#[derive(thiserror::Error)]
pub enum RunError {
    BruNotFound,
    Spawn(io::Error),
    NoReport { exit_code: Option<i32>, output: String },
    InvalidReport { kind: ReportErrorKind, line: usize, column: usize },
    Io(io::Error),
    UnsupportedPlatform,
}

pub struct BruRunner { program: OsString, next_id: AtomicU64, tx: mpsc::Sender<RunEvent> }
impl BruRunner {
    pub fn start(&self, req: RunRequest) -> RunHandle;   // non async, retour immédiat
}
pub struct RunHandle { pub id: RunId, cancel: Option<oneshot::Sender<()>> }
impl RunHandle { pub fn cancel(self); }
```

`program` est injectable (défaut `"bru"`) pour tester « binaire absent »
et utiliser un faux `bru` en test. `start` lance une tâche tokio qui fait
un `select!` entre la fin du run (processus + lecture du rapport) et le
signal d'annulation. `NoReport` et `Io` sont des variantes distinctes de
`Failed` mais correspondent aux issues « Aucun rapport » du spec.

Les noms ci-dessus sont indicatifs ; le contrat est le spec.

Le crate devient `lib` + `bin` : `src/lib.rs` expose `pub mod runner`,
`src/main.rs` reste le point d'entrée. Sans cible `lib`, les tests de
`tests/` ne pourraient pas importer le module. Organisation :
`src/runner/{mod.rs, request.rs, process.rs, report.rs, error.rs}`.

### D3. Règle de classification

Après la fin du processus **et** l'EOF du pipe :
1. `spawn` en `ErrorKind::NotFound` → `BruNotFound`.
2. Rapport vide (0 octet, après `trim`) → `NoReport` avec code de sortie et
   sortie capturée, **quel que soit le code** (le cas cwd hors collection
   sort en 0).
3. Désérialisation en échec → `InvalidReport`. On ne conserve que la
   catégorie serde (syntaxe, tronqué, structure) et la position : le
   message serde brut **cite les valeurs** du rapport
   (`invalid type: string "..."`), vérifié par test.
4. Sinon → `Completed`, le code de sortie est informatif seulement.

Le code de sortie n'est jamais utilisé pour décider du succès : il est
instable (0 sans rapport) et les échecs métier sont lus dans le rapport.

### D4. Annulation et cycle de vie du processus

`Command::kill_on_drop(true)` ; la tâche possède le `Child`. Sur signal
d'annulation, la tâche `kill()` puis `wait()` l'enfant, émet `Cancelled`.
Si le `RunHandle` est détruit sans `cancel`, le `oneshot::Receiver` voit
son émetteur fermé : on traite comme une annulation (spec « Abandon du
lanceur »). Si le runtime s'arrête, `kill_on_drop` termine `bru`. Tuer
l'enfant ferme l'extrémité écriture, donc la lecture bloquante reçoit EOF
et le thread se libère.

`bru` installé par npm/nvm est un script à shebang : le processus lancé
est directement `node`, pas un shell intermédiaire, donc `kill` atteint le
bon processus. `bru` ne lance pas de sous-processus héritant du fd 3 pour
une requête HTTP ; si c'était le cas, l'EOF serait retardé jusqu'à leur
fin (voir risques).

### D5. Sortie console et secrets

stdout et stderr de `bru` sont lus en parallèle (évite le blocage sur
tampon plein) et seuls les derniers 64 Kio combinés sont conservés, pour
`NoReport.output`. Ils ne sont jamais loggués. Les valeurs de `--env-var`
sont stockées dans un type qui masque `Debug`/`Display`
(`SecretString` local de quelques lignes plutôt que la crate `secrecy`) ;
la ligne de commande n'est jamais formatée dans un log ou une erreur.

`stdin` est `Stdio::null()` : `bru` ne doit jamais attendre une saisie.

### D6. Modèle de rapport

- `Report(Vec<Iteration>)` avec `#[serde(transparent)]`.
- `camelCase` via `#[serde(rename_all = "camelCase")]`, pas de
  `deny_unknown_fields` (tolérance aux ajouts de `bru`).
- `ResponseStatus` : désérialiseur manuel ou `#[serde(untagged)]`
  `Http(u16) | Keyword(String)` converti en
  `Http(u16) | Error | Skipped | Other(String)`.
- `ResultStatus` : `Pass | Fail | Error | Skipped | Other(String)` (même
  technique ; `#[serde(other)]` ne conserverait pas la valeur).
- Champs `null` ou absents selon le cas (`headers`, `url`, `statusText`,
  `error`) → `Option<T>` avec `#[serde(default)]`.
- En-têtes : `BTreeMap<String, serde_json::Value>` — seules des chaînes
  ont été observées, les en-têtes multi-valués (`set-cookie`) non ; pas de
  forme devinée.
- `data`, `actual`, `expected` : `serde_json::Value`.
- `runDuration` : `f64` (secondes), `responseTime` : `u64` (ms).
- Verdict : méthode `RequestResult::is_failure()` implémentant la règle du
  spec ; `Report::failures()` itère les résultats en échec toutes
  itérations confondues.

### D7. Dépendances (justification)

| Crate | Features | Pourquoi |
|---|---|---|
| `tokio` | `process`, `rt-multi-thread`, `macros`, `sync`, `io-util`, `time` | Imposée par la stack : process async, `mpsc`, `oneshot`, `select!` ; `time` pour le délai borné de lecture après la fin de `bru`. |
| `serde` | `derive` | Imposée : désérialisation du reporter. |
| `serde_json` | — | Imposée : format du reporter, `Value` pour corps libres. |
| `thiserror` | — | Imposée par les conventions d'erreur. |
| `command-fds` | `tokio` | Seule façon sûre de mapper un fd précis dans l'enfant avec `tokio::process::Command` ; évite `unsafe` dans le projet. Petite crate, dépend de `nix`. Déclarée en `[target.'cfg(unix)'.dependencies]`. |

### D8. Fixtures et tests

- `tests/fixtures/collections/runner-probe/` : collection Bruno minimale
  reproduisant les cas observés (succès + échec d'assertion/test, erreur
  de connexion, requête ignorée, corps JSON, tests pré/post).
- `tests/fixtures/reports/*.json` : rapports capturés par
  `scripts/gen-report-fixtures.sh` (serveur `python3 -m http.server`
  local, `bru run -r --reporter-json`), en-tête `date` et `uid` laissés
  tels quels. Chaque fichier est accompagné d'un `README.md` indiquant la
  version de `bru` et la commande.
- Tests unitaires de désérialisation et de verdict sur ces fixtures.
- Tests du runner avec un **faux `bru`** (script shell dans
  `tests/fixtures/fake-bru/`) qui écrit une fixture dans `/dev/fd/3`, ou
  rien, ou du JSON invalide, ou dort — déterministes, sans Node. Le mode
  est passé en argument (cibles de la requête) plutôt qu'en variable
  d'environnement : `std::env::set_var` est `unsafe` en édition 2024 et
  entre en concurrence entre tests parallèles.
- Un test d'intégration contre le vrai `bru` sur la collection de
  fixture, `#[ignore]` par défaut (réseau local et Node requis).

## Risks / Trade-offs

- [Format du reporter non documenté, évolutif] → tolérance aux champs
  inconnus, variantes `Other(String)`, version de référence notée dans les
  fixtures ; `InvalidReport` explicite plutôt qu'un panic.
- [`bru` écrit le rapport avec un autre mécanisme dans une future version
  (ex. ouverture en `O_EXCL`, écriture atomique par renommage)] → rendrait
  `/dev/fd/3` inopérant ; détecté par le test d'intégration réel, repli
  possible sur FIFO sans changer le spec.
- [Un sous-processus hérite du fd 3 et survit] → EOF retardé ; mitigation
  : après `wait()` de l'enfant, la lecture du rapport et de la sortie
  console est bornée à 2 s ; au-delà, le rapport est considéré absent
  (`NoReport`).
- [Rapport très volumineux (corps de réponse lourds, nombreuses
  itérations)] → tout est chargé en mémoire ; acceptable pour un TUI,
  `--reporter-skip-all-headers` exposable plus tard.
- [Les valeurs `--env-var` sont visibles dans la liste des processus
  (`ps`)] → limite imposée par la CLI `bru`, non écrit sur disque ;
  documenté, préférer les fichiers d'environnement pour les secrets.
- [Unix uniquement] → sur Windows natif, `start` émet immédiatement
  `Failed` avec une erreur « plateforme non supportée » (module compilé
  derrière `cfg(unix)` pour le transport).
- [Un thread bloquant par run en cours] → négligeable au regard du
  nombre de runs simultanés d'un TUI.
