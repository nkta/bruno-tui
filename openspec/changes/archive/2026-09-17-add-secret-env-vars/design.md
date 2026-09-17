## Context

Voir `proposal.md` — Why. État observé du code :

- `RunRequest.env_vars: Vec<(String, SecretString)>` existe et
  `bru_args()` produit déjà `--env-var nom=valeur` (testé dans
  `src/runner/request.rs`) ; `SecretString` masque `Debug`/`Display`.
- `run_selected` (`src/app/update.rs`) construit la `RunRequest` avec
  `env_vars: Vec::new()`, pour `r` comme pour le rejeu d'historique.
- `Environment.secret_names` (`src/collection/view.rs`) expose déjà les
  noms de `vars:secret`, sans valeur.
- `bru` 2.13.2 (`@usebruno/cli/src/commands/run.js`) :
  - `--env-var` est parsé par `^([^=]+)=(.*)$` puis écrit dans `envVars`,
    ce que lit `bru.getEnvVar` ; `.` ne franchit pas un saut de ligne ;
  - `<racine>/.env` est lu par `dotenv.parse` (sans expansion) et fusionné
    **par-dessus** `process.env` dans `processEnvVars`, qui ne sert qu'à
    `{{process.env.X}}` : il n'alimente jamais `bru.getEnvVar` ;
  - `--env-file` est exclusif de `--env`.
- La boucle (`src/app/mod.rs`) exécute des `Command` hors de `update`
  (`StartRun`, `CopyToClipboard`, `SaveEdit`) et renvoie leurs résultats
  en messages ; `Model::text_capture()` sélectionne une `TextCapture`
  (`Search`, `Insert`, `Filter`) qui transforme les touches en texte.
- `src/app/cli.rs` est un analyseur manuel (un positionnel, `-h`, `-V`).

## Goals / Non-Goals

**Goals:**
- Couvrir le cas réel `oktaClientSecret` ← `.env:OKTA_CLIENT_SECRET` sans
  aucune saisie : automatiquement si le nom est déclaré en `vars:secret`,
  avec `--secret oktaClientSecret` sinon.
- Ne rien réimplémenter de `bru` : la TUI ne fait que fournir des
  `--env-var`, toute interpolation reste chez `bru`.
- Rester testable sans modifier l'environnement du processus de test.

**Non-Goals:**
- Persister une valeur ou un mapping (fichier de config, trousseau
  système, `secrets.json`/coffres externes de Bruno).
- Recherche automatique pour des noms qui ne sont ni en `vars:secret` ni
  déclarés par `--secret` (on ne devine pas les variables lues par les
  scripts).
- Masquer les secrets qui réapparaîtraient dans la réponse affichée
  (en-têtes renvoyés, corps) : hors périmètre, inchangé.
- Effacement mémoire garanti (`zeroize`) : aucune dépendance ajoutée
  pour cela ; les valeurs vivent dans `SecretString` jusqu'à la fin du
  processus.

## Decisions

### Options étudiées et recommandation

| Option | Couvre `vars:secret` | Couvre `oktaClientSecret` (.env, nom différent) | Friction | Risque |
|---|---|---|---|---|
| A. Saisie masquée seule | oui | oui, mais à chaque session | élevée en usage répété | faible |
| B. `--env-var nom=valeur` en CLI | oui | oui | faible | valeur dans l'historique du shell et `ps` de `bruno-tui` : **rejeté** |
| C. `--secret NOM=CLÉ`, valeur lue dans le shell | oui | seulement après `set -a; . ./.env` | moyenne | faible |
| D. Comme C, avec lecture du `.env` de la collection | oui | oui, directement | faible | lecture d'un fichier à secrets (lecture seule) |
| H. Recherche automatique des `vars:secret` (nom exact puis `UPPER_SNAKE_CASE`, `.env` puis shell) | oui, sans option | si déclaré en `vars:secret` | nulle | variable du shell homonyme |
| E. Fichier de mapping persistant (`.bruno-tui.toml`) | oui | oui | très faible | nouveau format, nouvelle dépendance, nouveau fichier à versionner |
| F. `{{process.env.X}}` dans les `.bru` | — | — | — | écriture dans les `.bru` : **interdit** |
| G. `--env-file /dev/fd/N` généré | oui | oui | — | exclusif de `--env` : obligerait à réimplémenter le chargement d'environnement : **interdit** |

**Retenu : H + D + A** (décision utilisateur : recherche automatique).
Les `vars:secret` de l'environnement courant sont cherchés sans option ;
`--secret NOM[=CLÉ]` couvre les noms non déclarés en `vars:secret` ou une
clé qui ne suit pas la règle de nommage ; le panneau `S` avec saisie
masquée couvre le reste et prime sur tout. La ligne de commande ne porte
jamais de valeur (rejet de B).

### Clés candidates

Pour un nom, la liste ordonnée des clés candidates est :

- `--secret NOM=CLÉ` : `[CLÉ]` uniquement (clé explicite) ;
- `--secret NOM` ou nom `vars:secret` sans `--secret` : `[NOM,
  upper_snake(NOM)]`, dédoublonnée (`TOKEN` → `[TOKEN]`).

Un `--secret` explicite pour un nom aussi déclaré en `vars:secret`
remplace la règle automatique pour ce nom.

`upper_snake` : `-`, `.` et les espaces deviennent `_` ; un `_` est inséré
avant une majuscule précédée d'une minuscule ou d'un chiffre, ou précédée
d'une majuscule et suivie d'une minuscule (fin d'acronyme) ; le tout est
mis en majuscules. Exemples : `oktaClientSecret` → `OKTA_CLIENT_SECRET`,
`apiURLKey` → `API_URL_KEY`, `oauth2Token` → `OAUTH2_TOKEN`,
`x-api-key` → `X_API_KEY`, `client_secret` → `CLIENT_SECRET`.

### Ordre de priorité : saisie > `.env` > shell, puis ordre des clés

La saisie est l'intention la plus explicite. Ensuite la source prime sur
la clé : toutes les clés candidates dans `.env`, puis toutes dans le
shell. Entre `.env` et shell, on suit `bru`, qui fait gagner le `.env`
sur `process.env` (`{...process.env, ...dotenv}`) : un même `.env` donne
ainsi la même valeur pour `{{process.env.OKTA_CLIENT_SECRET}}` et pour
`bru.getEnvVar("oktaClientSecret")`. Dans une source, le nom exact passe
avant la forme `UPPER_SNAKE_CASE`.

### Analyse `.env` écrite à la main, calquée sur `dotenv.parse`

`bru` utilise `dotenv.parse` (npm), **sans** `dotenv-expand`. Le crate
`dotenvy` effectue une substitution `${VAR}` et divergerait ; il est
écarté, ce qui évite aussi une dépendance. Le module
`src/secrets/dotenv.rs` reproduit l'expression `LINE` de `dotenv.parse`
(16.6.1, embarqué par `bru` 2.13.2) par un petit moteur à retour arrière
dédié à cette seule expression (ordre des alternatives, quantificateurs
gourmands et paresseux, `\s` et `$` au sens JavaScript), puis le
post-traitement : `trim`, retrait des guillemets englobants, `\n`/`\r`
interprétés uniquement entre `"`, dernière occurrence gagnante. Les
fixtures de test sont vérifiées une fois contre `dotenv.parse` réel
(valeurs attendues notées dans le test avec la version de `dotenv`),
conformément à la règle « pas de structure devinée ». Les valeurs sont
rendues en `SecretString` ; les erreurs ne contiennent jamais de contenu
de ligne.

### Nouveau module `src/secrets/`, hors `collection`

Le parseur `.bru` ne doit rien résoudre ; la résolution n'appartient pas
non plus à `runner` (qui reste agnostique de la provenance). Module
dédié :

- `SecretMapping { name, key: Option<String> }` (issu de la CLI) ;
- `SecretLookup { name, keys: Vec<String> }` (clés candidates) ;
- `SecretSource { Typed, DotEnv { key }, Shell { key }, Missing { keys }, Invalid { key, reason } }` —
  `reason` est une énumération fixe (multiligne, non UTF-8, `.env`
  illisible), jamais une valeur ;
- `resolve(root, lookups, lookup_env: &dyn Fn(&str) -> Option<OsString>) -> Vec<Resolved>`,
  où `Resolved` porte `name`, `source` et `Option<SecretString>`. L'accès à
  l'environnement est injecté : en usage réel `std::env::var_os`, en test
  une table, sans `set_var` (non sûr en édition 2024).

### Résolution asynchrone par `Command`, une fois par chargement

La source d'un nom ne dépend que du nom, pas de l'environnement courant.
`update` reste pur : sur `CollectionLoaded(Ok)`, il construit les
`SecretLookup` pour **tous** les noms connus (mappings `--secret` et
`vars:secret` de tous les environnements valides) et renvoie
`Command::ResolveSecrets { root, lookups }` (ou `Command::None` s'il n'y
en a aucun). La boucle l'exécute dans `spawn_blocking` et renvoie
`AppEvent::SecretsResolved { root, resolved }`, ignoré si la racine ne
correspond plus à la collection chargée. Changer d'environnement ne
relance donc aucune I/O.

### État dans le `Model`

```text
SecretsState {
    mappings: Vec<SecretMapping>,             // CLI, immuable
    resolved: Vec<Resolved>,                  // dernier SecretsResolved
    typed: Vec<(String, SecretString)>,       // saisies de session
    added: Vec<String>,                       // noms ajoutés via `a`
    selected: usize,
    input: Option<SecretInput>,               // Name(String) | Value { name, buffer: SecretString, adding }
    error: Option<SecretError>,               // refus de nom, affiché dans le panneau
    pending_run: Option<(PathBuf, bool)>,     // exécution en attente
    acknowledged: HashSet<String>,            // environnements acquittés
}
```

La liste affichée et les `env_vars` sont **calculées** (fonction pure
`secret_rows(model)`) à partir de `mappings`, des `secret_names` de
l'environnement courant, de `added`, `typed` et `resolved` — pas de
liste dupliquée à resynchroniser au changement d'environnement. Le
tampon de saisie est un `SecretString` dès le premier caractère, pour que
`Debug` du `Model` ne l'expose jamais. `Model::new` reste inchangé :
`app::run` reçoit les mappings et les place dans `model.secrets`, ce qui
évite de modifier la vingtaine d'appels existants.

### Focus, touches et capture

- `Focus::Secrets`, sur le patron de `Focus::EnvironmentPicker`.
- Nouveaux messages hors saisie : `ToggleSecrets` (`S`),
  `AddSecret` (`a`), `ForgetSecret` (`d`) — `a`, `d`, `S` sont libres
  dans la table actuelle ; hors `Focus::Secrets`, `a`/`d` sont sans
  effet. `Entrée` (`Right`), `↑`/`↓`, `Échap` (`FocusTree`) et `r`
  (`RunSelected`) sont réinterprétés selon le focus, comme ailleurs.
- `TextCapture::SecretName` et `TextCapture::SecretValue` avec leurs
  messages `SecretInput(MaskedChar)`, `SecretBackspace`,
  `ConfirmSecretInput`, `CancelSecretInput`. `MaskedChar` masque son
  `Debug` : un `Message` formaté ne révèle aucun caractère tapé.
- La vue affiche `•` par caractère pendant la saisie de valeur (retour
  visuel attendu) ; hors saisie, l'état seulement, jamais de longueur.
- Un ajout abandonné pendant la saisie de la valeur (ou validé vide)
  n'ajoute pas le nom.

### Proposition au lancement

`run_selected` calcule la cible comme aujourd'hui, puis, si
l'environnement courant a des `secret_names` toujours non fournis après
recherche automatique et n'est pas dans `acknowledged`, stocke
`pending_run`, passe le focus à `Secrets` et renvoie `Command::None`.
`r` en focus `Secrets` consomme `pending_run` (ou ne fait rien s'il n'y
en a pas), acquitte l'environnement et renvoie `Command::StartRun`. La
vérification « exécution déjà en cours » reste **avant** cette logique
pour ne pas ouvrir le panneau inutilement. L'acquittement est par nom
d'environnement et remis à zéro au rechargement de collection, comme
`current_environment`.

### `env_vars` construites à un seul endroit

Une fonction `secret_env_vars(model) -> Vec<(String, SecretString)>`
alimente toutes les `RunRequest` (`r`, `r` en attente, rejeu), ce qui
garantit que l'historique n'a pas à stocker de valeur : `HistoryEntry`
reste inchangé.

### CLI

`cli::Command::Run { path, secrets: Vec<SecretMapping> }` ; formes
acceptées : `--secret X`, `--secret X=Y`, `--secret=X`, `--secret=X=Y`.
Validation : non vide, sans `=` supplémentaire ni espace blanc. Nouvelles
variantes `UsageError::MissingSecretArgument` et
`UsageError::InvalidSecret(String)` — le texte d'erreur ne reprend que
l'argument fautif, qui par construction est un nom et non une valeur.
Un même nom répété garde sa première déclaration.

## Risks / Trade-offs

- [Variable du shell homonyme transmise par erreur : un `vars:secret`
  nommé `user`, `path` ou `home` trouve `USER`, `PATH` ou `HOME` dans le
  shell et part vers `bru` sans action de l'utilisateur] → Le panneau `S`
  affiche la source et la clé utilisée pour chaque nom ; une saisie
  prime ; `--secret NOM=CLÉ` force une clé précise. La recherche ne porte
  que sur les noms déclarés secrets, jamais sur un nom arbitraire, et la
  valeur ne va qu'à `bru` lancé localement.
- [Les valeurs sont visibles dans `/proc/<pid>/cmdline` de `bru` pendant
  l'exécution] → Limite inhérente à `--env-var`, déjà acceptée par
  `bru-runner` ; seule alternative sans écrire les `.bru` (`--env-file`)
  est exclusive de `--env`. Documenté dans l'aide de `--secret`.
- [Le rapport JSON et le panneau Réponse peuvent contenir le secret si
  l'API le renvoie] → Rapport jamais persisté (exigence existante) ;
  masquage dans la vue hors périmètre.
- [Divergence du parseur `.env` avec `dotenv.parse`] → Expression
  reproduite fidèlement, fixtures vérifiées contre `dotenv.parse` réel ;
  une ligne non comprise est ignorée (variable non fournie, donc visible
  dans le panneau).
- [La proposition au lancement bloque un premier `r`] → Une seule fois
  par environnement et par session, seulement si la recherche automatique
  n'a rien trouvé ; `r` dans le panneau lance quand même.
- [Écran partagé / enregistrement de terminal pendant la saisie] → Seuls
  des `•` sont rendus.

## Migration Plan

Additif : sans `--secret` et sans `vars:secret`, comportement identique
(aucune surcharge, aucun panneau). Une collection dont les `vars:secret`
trouvent une valeur dans `.env` ou le shell transmet désormais ces
valeurs à `bru` : c'est l'effet recherché. Retour arrière = revert du
commit.
