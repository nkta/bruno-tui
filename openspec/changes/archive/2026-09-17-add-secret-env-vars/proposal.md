## Why

`bruno-tui` lance toujours `bru run` avec `RunRequest.env_vars` vide
(`run_selected` dans `src/app/update.rs`), alors que `bru-runner` sait déjà
transmettre des `--env-var nom=valeur` masqués par `SecretString`. Toute
collection dont les scripts ou les requêtes attendent une variable secrète
absente du fichier d'environnement est donc inexécutable depuis
l'interface. Cas réel : l'environnement `CI` ne déclare pas
`oktaClientSecret`, le script pre-request lit
`bru.getEnvVar("oktaClientSecret")` et échoue ; en ligne de commande on
contourne avec `bru run --env CI --env-var oktaClientSecret=$OKTA_CLIENT_SECRET`,
la valeur venant du `.env` de la racine de la collection sous un autre nom
(`OKTA_CLIENT_SECRET`). De même, les `vars:secret [ nom ]` d'un
environnement Bruno n'ont jamais de valeur dans le fichier : `bru` les voit
vides.

`bru` lit bien le `.env` de la collection, mais uniquement pour
`{{process.env.X}}` : il ne l'injecte pas dans les variables
d'environnement lues par `bru.getEnvVar`. Seul `--env-var` le permet, et
modifier les `.bru` pour y écrire `{{process.env.X}}` est exclu.

## What Changes

- Recherche automatique de la valeur de chaque `vars:secret` de
  l'environnement courant, dans le `.env` de la racine de la collection
  puis dans l'environnement du shell, sous le nom exact puis sa forme
  `UPPER_SNAKE_CASE` (`oktaClientSecret` → `OKTA_CLIENT_SECRET`).
- Nouvelle option de ligne de commande répétable
  `--secret NOM[=CLÉ]` : déclare une variable non présente en
  `vars:secret`, ou impose une clé différente. La ligne de commande ne
  porte jamais de valeur, seulement des noms.
- Nouveau panneau « Variables secrètes » (`S`) listant les `vars:secret`
  de l'environnement courant et les noms déclarés par `--secret` ou
  ajoutés dans le panneau, avec pour chacun sa source et la clé utilisée
  (saisie, `.env`, shell, non fournie), sans jamais afficher de valeur.
- Saisie masquée d'une valeur dans ce panneau, gardée en mémoire le temps
  de la session uniquement, prioritaire sur `.env` et le shell.
- Chaque exécution (`r`, rejeu d'historique) transmet toutes les
  variables secrètes résolues dans `RunRequest.env_vars`.
- Au premier `r` d'un environnement dont des `vars:secret` n'ont aucune
  valeur après recherche automatique, le panneau s'ouvre pour proposer
  la saisie ; `r` depuis le panneau lance quand même l'exécution en
  attente.
- Aucune écriture de fichier, aucune valeur journalisée, affichée,
  copiée dans le presse-papiers ou conservée dans l'historique.

## Capabilities

### New Capabilities
- `secret-env-vars` : déclaration, recherche automatique et résolution
  (saisie, `.env`, shell), saisie masquée, affichage sans valeur et
  transmission à `bru` des variables secrètes d'une exécution.

### Modified Capabilities
- `tui-shell` : l'exigence « Ligne de commande » accepte l'option
  répétable `--secret NOM[=CLÉ]` et rejette une forme invalide avec le
  code 2.

## Impact

- `src/app/cli.rs` : analyse de `--secret`, `Command::Run` porte les
  correspondances ; `USAGE` mis à jour.
- Nouveau module `src/secrets/` : analyse `.env` (sémantique de
  `dotenv.parse` utilisée par `bru`, sans dépendance nouvelle) et
  résolution vers `SecretString`.
- `src/app/{model,message,update,mod}.rs` et `src/app/view/` : focus et
  panneau `Secrets`, capture de texte masquée, commande de résolution
  asynchrone après `CollectionLoaded`, remplissage de `env_vars`.
- `src/main.rs` : transmission des correspondances à `app::run`.
- Fixtures : collection de test avec `.env` et `vars:secret` ; faux `bru`
  vérifiant les `--env-var` reçus.
- Aucune dépendance ajoutée ; `bru-runner` inchangé.
