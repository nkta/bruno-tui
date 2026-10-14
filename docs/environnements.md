# Spécification — Environnements : créer, renommer, cloner, supprimer

Statut : **approuvée par Nicolas le 2026-10-14**, options recommandées
ci-dessous (1 à 8). Cette spec lève, pour ces opérations seulement, la règle
de `CLAUDE.md` sur l'écriture des fichiers d'environnement.

Décisions à prendre par Nicolas :

1. **Touches dans le panneau Environnement** :
   - *Option recommandée* : `a` créer, `F2` renommer, `p` cloner, `d` mettre à la corbeille.
   - *Raison* : strict alignement avec les touches de l'arbre (`docs/edition-phase3.md`). Ces touches sont libres dans le panneau Environnement (`Focus::EnvironmentPicker`).
2. **Effet de la suppression sur les environnements enfants (`extends`)** :
   - *Option recommandée* : ne pas modifier les autres fichiers d'environnement.
   - *Raison* : comportement exact de Bruno 4.2.1 (`renderer:delete-environment` n'efface que le fichier ciblé). Le résolveur d'héritage de Bruno tolère une référence manquante sans casser. Évite des réécritures en cascade non demandées par l'utilisateur. Le TUI affichera le parent manquant (ex. `staging ← base (introuvable)`).
   - *Alternative* : supprimer la ligne `extends` dans les fichiers enfants. (Non recommandé : réécrit des fichiers tiers sans demande explicite).
3. **Effet du renommage sur les environnements enfants (`extends`)** :
   - *Option recommandée* : réécrire chirurgicalement la référence `extends` dans les fichiers frères de `environments/`.
   - *Raison* : comportement exact de Bruno 4.2.1 (`renameEnvironmentExtendsReferences` dans `/src/utils/environments.js`). Préserve l'intégrité de la hiérarchie d'héritage.
   - *Alternative* : ne renommer que le fichier cible et laisser les références enfants inchangées. (Non recommandé : rompt l'héritage).
4. **Nom proposé au clonage** :
   - *Option recommandée* : proposer `<nom> copie` dans le popup.
   - *Raison* : cohérent avec le clonage des requêtes du TUI (`docs/edition-phase3.md`) et en français.
   - *Alternative* : proposer la chaîne anglaise de Bruno desktop (`<nom> - Copy`).
   - Dans les deux cas, un nom déjà pris est **refusé**, comme à la création (voir § 2.2).
5. **Bascule activée / désactivée (`~`) dans le popup de variables** :
   - *Option recommandée* : ajouter la touche `Espace` dans le popup de variables pour basculer l'état `~`.
   - *Raison* : `Espace` sert déjà à basculer les lignes dans tous les autres popups d'édition du TUI.
6. **Variables secrètes dans le popup de variables** :
   - *Option recommandée* : maintenir la séparation stricte actuelle. Le popup n'édite que les variables publiques (`vars`). Les variables secrètes restent gérées dans le panneau Secrets (`S`).
   - *Raison* : respecte le principe non négociable « secrets jamais écrits sur disque ni loggués ».
   - *Alternative* : ajouter une touche dans le popup pour déclarer un nom dans `vars:secret [ ... ]` sans valeur.
7. **Format YAML (`environments/*.yml`)** :
   - *Option recommandée* : lecture seule stricte.
   - *Raison* : règle générale du TUI (`CLAUDE.md`, `README.md`). Refus de toute création, renommage, clonage ou suppression en YAML.
8. **Environnements globaux et de workspace** :
   - *Option recommandée* : hors périmètre.
   - *Raison* : aligné avec `docs/workspace.md`. Bruno stocke les globaux dans la configuration Electron locale et les environnements de workspace en YAML.

---

## 1. Périmètre

| Opération | Collection `.bru` | Collection YAML |
|---|---|---|
| Créer un environnement | oui | non (lecture seule) |
| Renommer un environnement | oui | non (lecture seule) |
| Cloner un environnement | oui | non (lecture seule) |
| Mettre à la corbeille | oui | non (lecture seule) |
| Basculer variable `~` (`Espace`) | oui | non (lecture seule) |

Hors périmètre :
- Collections OpenCollection YAML (`environments/*.yml`) : toujours en lecture seule.
- Environnements globaux de l'application (`~/.config/Bruno/global-environments.json`).
- Environnements de workspace (`<workspace>/environments/*.yml`).
- Modification de la couleur d'affichage (`color: ...`).
- Configuration de gestionnaires de secrets externes (`vars:externalsecrets:*` Vault, AWS).
- Résolution ou calcul des variables par le TUI (délégués à `bru run`).

---

## 2. Comportement de Bruno (référence)

Relevé dans Bruno desktop 4.2.1 (`/snap/bruno/current/resources/app.asar`) et les bibliothèques officielles `@usebruno/*` (`~/.local/lib/bru-cli/node_modules/@usebruno/`) :

### 2.1 Canaux IPC et actions

- **Création d'environnement** :
  - Canal IPC : `renderer:create-environment` dans `/src/ipc/collection.js` (l. 784).
  - Paramètres : `(collectionPathname, name, variables, color, inheritedEnvironmentName, externalSecrets)`.
  - Action renderer : fonction thunk `eU` dans `/web/static/js/index.c9f1eb9b.js` (position 3494923).
  - Validation du nom : composant modal `cm` dans le renderer (position 311960). Schéma Yup : 1 à 255 caractères, `validateName`, non vide, et unicité insensible à la casse (`Environment already exists`).
- **Sauvegarde d'un environnement** :
  - Canal IPC : `renderer:save-environment` dans `/src/ipc/collection.js` (ligne 846).
  - Verrouillage fichier : `withFileLock` (`/src/utils/filesystem.js`).
  - Sérialisation : `stringifyEnvironment` (`@usebruno/filestore`).
- **Renommage d'un environnement** :
  - Canal IPC : `renderer:rename-environment` dans `/src/ipc/collection.js` (ligne 881).
  - Paramètres : `(collectionPathname, environmentName, newName)`.
  - Action renderer : champ d'édition inline dans `LX` (`data-testid="env-rename-input"`, position 2553500) et thunk `(0, q.Oc)` (position 3495855).
  - Vérification collision : `safeToRename` ; si le fichier cible existe déjà, rejet par une exception `environment: ... already exists` (aucun suffixage automatique au renommage).
  - Renommage disque : `fs.renameSync(envFilePath, newEnvFilePath)`. Le contenu d'un fichier `.bru` n'est pas modifié car le nom de l'environnement est uniquement son nom de fichier.
  - Mise à jour des références `extends` : appel à `renameEnvironmentExtendsReferences` dans `/src/utils/environments.js`. Tous les fichiers `.bru` du dossier `environments/` ayant `extends === oldName` sont réécrits avec `extends = newName`.
  - Coffre de secrets : `environmentSecretsStore.renameEnvironment(...)` dans `/src/store/env-secrets.js`.
- **Suppression d'un environnement** :
  - Canal IPC : `renderer:delete-environment` dans `/src/ipc/collection.js` (ligne 925).
  - Paramètres : `(collectionPathname, environmentName)`.
  - Action renderer : modal de confirmation `_D` (« Are you sure you want to delete <name>? », position 2456989) et thunk `(0, q.jA)` (position 3496165).
  - Suppression disque : `fs.unlinkSync(envFilePath)`. Bruno efface définitivement le fichier.
  - Références `extends` orphelines : Bruno ne modifie **aucun** autre fichier. Si un environnement référence l'environnement supprimé, la fonction `getInheritedEnvironments` (`@usebruno/common/src/utils/environment-inheritance.ts`) interrompt la chaîne et renseigne `missingInheritedEnvironmentName` sans planter.
  - Nettoyage secrets : `environmentSecretsStore.deleteEnvironment(...)`.
- **Clonage / Copie d'un environnement** :
  - Il n'existe **aucun canal IPC dédié** `renderer:copy-environment`.
  - Action renderer : modal `_z` intitulé « Copy Environment » (position 2454500).
  - Nom par défaut : `<nom> - Copy`. Validation : 1 à 50 caractères dans le modal.
  - Mécanisme : appelle directement `renderer:create-environment` avec les variables, la couleur, `extends` et `externalSecrets` de l'environnement source (thunk `eH`, position 3495347).
  - Gestion des secrets : les noms déclarés dans `vars:secret` sont recopiés dans le fichier `.bru`. Si des valeurs secrètes existent dans le coffre Electron (`environmentSecretsStore`), elles sont dupliquées sous le nouveau nom.
  - Le fichier du clone est **réécrit** par `stringifyEnvironment` : ce n'est pas une copie à l'octet près.
- Les positions citées dans `index.c9f1eb9b.js` sont des positions en octets (fichier minifié sur une ligne).

### 2.2 Règles de nom et collisions

- **Règles de validation** (`validateName` dans `@usebruno/common/src/utils/naming.ts`) :
  - 1 à 255 caractères.
  - Caractères interdits : `< > : " / \ | ? *` et caractères de contrôle (0x00 à 0x1F).
  - Début interdit : espace ou tiret `-`.
  - Fin interdite : espace ou point `.`.
  - Noms réservés Windows interdits : `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9` (insensible à la casse).
- **Nettoyage automatique** (`sanitizeName` dans `@usebruno/common/src/utils/naming.ts`) :
  - Remplacement des caractères interdits par `-`.
  - Suppression des espaces et tirets en tête.
  - Suppression des espaces et points en fin.
- **Nom déjà pris** :
  - Le renderer **refuse** avant tout appel IPC, à la création comme au renommage : « Environment already exists ».
    La comparaison ignore la casse et les espaces de début et de fin (`toLowerCase().trim()`, popup `cm` et renommage `G`).
  - Repli côté electron seulement (`generateUniqueName`, `/src/utils/filesystem.js`, l. 574) : `<nom> copy`, puis `<nom> copy 2`, `<nom> copy 3`…
    L'interface de Bruno n'y arrive pas en usage normal.
  - Le TUI suit le renderer : **refus**, jamais de suffixe.

### 2.3 Contenu exact d'un fichier `.bru` d'environnement

Relevé via `envJsonToBruV2` (`@usebruno/lang/v2/src/jsonToEnv.js`) :
- **Fichier vide** (création sans variable) :
```bru
vars {
}
```
(suivi d'un saut de ligne `\n`, soit exactement 9 octets).
- **Ordre canonique des blocs** :
  1. `extends: <nom>` (ou `extends [\n  ...\n]`)
  2. `vars {\n  ...\n}`
  3. `vars:secret [\n  ...\n]`
  4. `vars:externalsecrets:<type> {\n  ...\n}`
  5. `color: <valeur>`
- **Variables et masquage des secrets** :
  - Une variable désactivée est préfixée par un tilde : `~cle: valeur`.
  - Le bloc `vars:secret` ne contient **jamais de valeur**, uniquement les noms :
```bru
vars:secret [
  api_key,
  ~disabled_secret
]
```

### 2.4 Emplacement des environnements globaux et workspace

- Environnements globaux de l'application : stockés dans le coffre Electron `~/.config/Bruno/global-environments.json` (`/src/store/global-environments.js`).
- Environnements de workspace : stockés dans `<workspacePath>/environments/<nom>.yml` (`/src/store/workspace-environments.js`).

---

## 3. Touches

### 3.1 Dans le panneau Environnement (`Focus::EnvironmentPicker`)

Touches actives lorsque le focus est sur le panneau Environnement (ouvert par `E` ou un clic) :

| Touche | Effet | Conditions |
|---|---|---|
| `a` | ouvrir le popup « Nouvel environnement » | toujours disponible (sauf collection YAML) |
| `F2` | ouvrir le popup « Renommer l'environnement » | sur un environnement existant valide |
| `p` | ouvrir le popup « Cloner l'environnement » | sur un environnement existant valide |
| `d` | demander confirmation pour mettre à la corbeille | sur un environnement ou un fichier en erreur |
| `e` | ouvrir le popup d'édition des variables | existant (sur environnement valide) |
| `Entrée` / `→` | activer l'environnement sous le curseur | existant (« Aucun » désactive) |
| `↑`/`k`, `↓`/`j` | naviguer dans la liste | existant |
| `Début`/`g`, `Fin`/`G` | début ou fin de la liste | existant |
| `Échap` / `E` | fermer le panneau et ramener le focus à l'arbre | existant |

Règles de sélection et restrictions :
- Sur la première ligne « Aucun » : `F2`, `p`, `d` et `e` sont refusés avec le message : « Aucun n'est pas un environnement ». La touche `a` reste active.
- Sur une ligne en erreur (fichier `.bru` malformé) : `F2`, `p` et `e` sont refusés ; la touche `d` est autorisée afin de permettre le nettoyage du fichier invalide vers la corbeille.
- Collection YAML : `a`, `F2`, `p`, `d`, `e` affichent : « Collection OpenCollection (YAML) en lecture seule ».
- Conflits vérifiés : `a`, `F2`, `p`, `d` ne sont pas utilisés dans `Focus::EnvironmentPicker` aujourd'hui (`src/app/view/help.rs`, `src/app/message.rs`).

### 3.2 Dans le popup d'édition des variables (`EnvironmentEditSession`)

| Touche | Effet |
|---|---|
| `Entrée` | modifier la valeur sous le curseur |
| `Espace` | basculer la variable entre activée et désactivée (`~`) |
| `a` | ajouter une variable (saisie clé puis valeur) |
| `d` | supprimer la variable sous le curseur (sans confirmation) |
| `Ctrl+S` | enregistrer sur le disque et fermer le popup |
| `Échap` | annuler la saisie en cours, sinon fermer le popup |

---

## 4. Popups

Modèle identique aux popups de fichiers de l'arbre (`docs/edition-phase3.md`) : centré, bordure simple, validation par `Entrée` ou `Ctrl+S`, annulation par `Échap`.

### 4.1 Nouvel environnement (`a`)

```
╭ Nouvel environnement ────────────────────────╮
│ Nom        production                        │
│                                              │
│ Entrée créer   Échap annuler                 │
╰──────────────────────────────────────────────╯
```

- Le champ `Nom` est focalisé au démarrage.
- Validation immédiate à la frappe selon les règles de Bruno (`validateName`).
- Si invalide, le motif de refus s'affiche en rouge en bas du popup.
- Nom déjà pris (sans tenir compte de la casse ni des espaces de bord) : refus avec « Un environnement de ce nom existe déjà », comme Bruno.
- Fichier créé : `environments/<nom>.bru` contenant exactement `vars {\n}\n`.
- Après création : la collection est rechargée, le panneau Environnement reste ouvert et le nouvel environnement est sélectionné sous le curseur (non activé).

### 4.2 Renommer l'environnement (`F2`)

```
╭ Renommer l'environnement — staging ──────────╮
│ Nom        staging-v2                        │
│                                              │
│ Entrée appliquer   Échap annuler             │
╰──────────────────────────────────────────────╯
```

- Champ pré-rempli avec le nom actuel.
- Validation à la saisie (`validateName`, max 255 caractères).
- Si le nom est inchangé : fermeture sans opération disque.
- Si le nom existe déjà : refus avec le message : « Un environnement de ce nom existe déjà » (aucun suffixage automatique au renommage).
- Traitement de l'environnement actif : si l'environnement renommé était l'environnement actif de la collection, `current_environment` est immédiatement mis à jour avec le nouveau nom.
- Mise à jour en cascade : les fichiers du dossier `environments/` qui référencent l'ancien nom dans `extends` sont mis à jour chirurgicalement.

### 4.3 Cloner l'environnement (`p`)

```
╭ Cloner l'environnement — staging ────────────╮
│ Nom        staging copie                     │
│                                              │
│ Entrée cloner   Échap annuler                │
╰──────────────────────────────────────────────╯
```

- Champ pré-rempli avec `<nom> copie`.
- Nom déjà pris : refus, comme à la création.
- Fichier créé : copie à l'octet près du fichier source (le nom d'un environnement `.bru` est son nom de fichier).
  Différence avec Bruno, qui réécrit le clone avec `stringifyEnvironment` : le TUI garde le fichier tel quel, comme pour le clonage des requêtes.
- Variables secrètes : le bloc `vars:secret [ ... ]` est dupliqué à l'identique dans le fichier `.bru`. Les secrets saisis en mémoire dans la session active (`model.secrets.typed`) étant indexés par nom de variable, ils s'appliquent immédiatement au clone.

### 4.4 Mettre à la corbeille (`d`)

Confirmation demandée dans la barre de statut (comme pour les fichiers de l'arbre) :
`Mettre l'environnement « staging » à la corbeille ? (y/n)`

- `y` confirme ; `n` ou `Échap` annule (comme dans l'arbre).
- Le fichier `environments/<nom>.bru` est déplacé vers la corbeille utilisateur (`$XDG_DATA_HOME/Trash` via `src/writer/trash.rs`). Jamais d'effacement définitif.
- Si l'environnement supprimé était l'environnement actif de la collection : `current_environment` bascule à `None` (« Aucun »).
- Les autres fichiers d'environnement ne sont pas modifiés (comportement Bruno 4.2.1).

---

## 5. Règles communes

1. **Exécution et résolution** : l'exécution d'une requête est toujours déléguée à `bru run --env <nom>`. Le TUI n'évalue ni ne résout jamais les variables d'environnement ni la hiérarchie `extends`.
2. **Écriture chirurgicale** :
   - Pour la mise à jour de `extends` lors d'un renommage, seule la ligne `extends:` change dans les fichiers dépendants.
     Bruno, lui, réécrit tout le fichier enfant avec `stringifyEnvironment` (`/src/utils/environments.js`) ; il ne compare que les `extends` simples, à l'identique (casse comprise). Le TUI applique la même règle de correspondance.
   - Le reste du fichier (commentaires, espacements, variables, secrets) est préservé à l'octet près.
3. **Contrôle de fraîcheur** : toute écriture ou renommage capture au préalable un `FileStamp`. Si le fichier a été modifié sur le disque par un processus externe depuis son chargement, l'opération est refusée avec une erreur claire (`Stale`).
4. **Écriture atomique** : création d'un fichier temporaire dans le même répertoire (`environments/.<nom>.tmp.<random>`), synchronisation, puis renommage atomique vers la cible finale.
5. **Confinement dans la collection** : interdiction absolue de toute écriture ou déplacement en dehors du répertoire `environments/` de la collection active (`validatePathIsInsideCollection`).
6. **Corbeille** : mise à la corbeille conforme à la spécification freedesktop.org (`Trash`). En cas de système de fichiers différent empêchant le déplacement, l'opération est refusée avec le message « Impossible de mettre à la corbeille : autre disque ». Aucun effacement définitif en repli.
7. **Pendant une exécution `bru run`** : toute opération de création, renommage, clonage ou suppression d'environnement est bloquée pendant qu'une exécution de requêtes ou de campagne est en cours dans la collection.
8. **Secrets protégés** : aucune valeur secrète n'est écrite dans les fichiers `.bru`, ni dans les fichiers temporaires, ni consignée dans les journaux d'erreurs.

---

## 6. Tests et critères d'acceptation

1. **Validation et assainissement des noms** :
   - Même matrice de cas que Bruno (`validateName`, `sanitizeName`). Caractères réservés Windows, caractères interdits, espaces en tête/queue.
2. **Création d'un environnement** :
   - Vérification que le fichier généré contient exactement `vars {\n}\n` (9 octets).
   - Relecture par `@usebruno/filestore` (`parseEnvironment`) et `@usebruno/lang` (`bruToEnvJsonV2`).
3. **Collisions de noms** :
   - À la création, au clonage et au renommage : refus d'un nom déjà pris (casse et espaces de bord ignorés), sans modification du disque.
4. **Renommage et mise à jour d'héritage** :
   - Renommage d'un parent référencé par `extends: parent`.
   - Vérification que le fichier enfant a été mis à jour avec `extends: nouveau_nom`.
   - Vérification de la conservation à l'octet près du reste du fichier enfant.
5. **Clonage** :
   - Fidélité à l'octet près du contenu (variables, commentaires, `vars:secret`, `color`, `extends`).
6. **Mise à la corbeille** :
   - Fichier déplacé dans le dossier `files/` de la corbeille utilisateur temporaire.
   - Création d'un fichier `.trashinfo` valide avec URL et date d'effacement ISO.
   - Refus sur point de montage différent (simulation d'erreur).
7. **Sécurité et restrictions** :
   - Refus sur collection YAML (OpenCollection).
   - Refus sur l'entrée « Aucun ».
   - Refus pendant une exécution active.
8. **Qualité du code** :
   - `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
9. **Validation réelle** :
   - Essai interactif sur une copie de la collection `examples/jsonplaceholder`.
   - Lancement réussi de `bru run --env <nouvel_env>` sur les environnements créés et clonés.
