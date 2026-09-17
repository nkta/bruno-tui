## Purpose

Fournir à `bru` les variables secrètes qu'une exécution attend (déclarées
en `vars:secret` ou lues par un script) sans les écrire dans la collection
ni sur disque : recherche automatique dans le `.env` de la collection ou
le shell, saisie masquée dans l'interface, et transmission par surcharge
de variable à chaque exécution.

## ADDED Requirements

### Requirement: Ensemble des variables secrètes
L'ensemble des variables secrètes de la session SHALL être l'union, sans
doublon de nom et dans cet ordre, des noms déclarés par `--secret` (ordre
de la ligne de commande), des noms `vars:secret` de l'environnement
courant (ordre du fichier), puis des noms ajoutés depuis le panneau
(ordre d'ajout). Changer d'environnement courant SHALL mettre à jour les
noms issus de `vars:secret` sans perdre les valeurs déjà saisies pour
d'autres noms. Un nom MUST être non vide et MUST NOT contenir `=` ni
d'espace blanc, conformément à la forme `nom=valeur` attendue par `bru`.

#### Scenario: Union des sources de noms
- **WHEN** `bruno-tui` est lancé avec `--secret oktaClientSecret`
  et que l'environnement courant `local` déclare `vars:secret [ token ]`
- **THEN** les variables secrètes sont, dans l'ordre, `oktaClientSecret`
  puis `token`

#### Scenario: Aucun environnement
- **WHEN** l'environnement courant est « Aucun » et aucune option
  `--secret` n'a été fournie
- **THEN** l'ensemble des variables secrètes est vide

#### Scenario: Changement d'environnement
- **WHEN** une valeur a été saisie pour `token`, puis l'utilisateur passe
  à un environnement qui ne déclare pas `token`, puis revient à `local`
- **THEN** `token` apparaît de nouveau avec sa valeur saisie

### Requirement: Résolution des valeurs
Chaque variable secrète SHALL avoir une liste ordonnée de clés
candidates : pour `--secret NOM=CLÉ`, la seule clé `CLÉ` ; pour
`--secret NOM` ou pour un nom `vars:secret` sans `--secret`, le nom exact
puis sa forme `UPPER_SNAKE_CASE` si elle diffère. La forme
`UPPER_SNAKE_CASE` SHALL remplacer `-`, `.` et les espaces par `_`,
insérer `_` avant une majuscule précédée d'une minuscule ou d'un chiffre,
ou précédée d'une majuscule et suivie d'une minuscule, puis tout mettre en
majuscules. Un `--secret` explicite pour un nom également déclaré en
`vars:secret` MUST remplacer la recherche automatique pour ce nom. Les
noms ajoutés depuis le panneau n'ont aucune clé candidate.

La valeur transmise SHALL être, par ordre de priorité : la valeur saisie
dans le panneau pendant la session ; sinon la valeur de la première clé
candidate présente dans le fichier `.env` situé à la racine de la
collection chargée ; sinon la valeur de la première clé candidate
présente dans l'environnement du processus `bruno-tui`. Cette recherche
SHALL être automatique pour les noms `vars:secret`, sans option. Le
`.env` SHALL être interprété selon les mêmes règles que celles appliquées
par `bru` au même fichier (sans expansion de variables). Un `.env` absent
MUST être traité comme vide ; un `.env` illisible MUST laisser non
fournies les variables ayant des clés candidates et signaler l'erreur
dans le panneau sans en afficher le contenu. Une valeur contenant un saut
de ligne, ou une variable du shell qui n'est pas de l'UTF-8, MUST être
refusée (non transmise, signalée avec sa clé). Une variable sans valeur
résolue est « non fournie » et MUST NOT être transmise. La résolution
depuis `.env` et le shell SHALL avoir lieu à chaque chargement réussi de
la collection et MUST NOT bloquer l'interface.

#### Scenario: Recherche automatique d'un vars:secret en UPPER_SNAKE_CASE
- **WHEN** l'environnement courant `CI` déclare
  `vars:secret [ oktaClientSecret ]`, aucune option `--secret` n'est
  fournie, et le `.env` de la racine contient `OKTA_CLIENT_SECRET=abc`
- **THEN** `oktaClientSecret` est résolue à `abc` avec la source `.env`
  et la clé `OKTA_CLIENT_SECRET`

#### Scenario: Nom exact prioritaire sur UPPER_SNAKE_CASE
- **WHEN** le `.env` contient à la fois `token=a` et `TOKEN=b` pour le
  nom `vars:secret` `token`
- **THEN** `token` est résolue à `a` avec la clé `token`

#### Scenario: Formes UPPER_SNAKE_CASE
- **WHEN** les noms `apiURLKey`, `oauth2Token`, `x-api-key` et
  `client_secret` sont déclarés en `vars:secret`
- **THEN** leurs secondes clés candidates sont respectivement
  `API_URL_KEY`, `OAUTH2_TOKEN`, `X_API_KEY` et `CLIENT_SECRET`

#### Scenario: Clé explicite
- **WHEN** `--secret oktaClientSecret=OKTA_SECRET` est fourni, le `.env`
  contient `OKTA_SECRET=abc` et `OKTA_CLIENT_SECRET=zzz`
- **THEN** `oktaClientSecret` est résolue à `abc` avec la clé
  `OKTA_SECRET`, et `OKTA_CLIENT_SECRET` n'est pas consultée

#### Scenario: Nom non déclaré en vars:secret
- **WHEN** `--secret oktaClientSecret` est fourni sans clé, l'environnement
  ne le déclare pas, et le `.env` contient `OKTA_CLIENT_SECRET=abc`
- **THEN** `oktaClientSecret` est résolue à `abc` avec la clé
  `OKTA_CLIENT_SECRET`

#### Scenario: Repli sur le shell
- **WHEN** `token` est déclaré en `vars:secret`, le `.env` ne contient ni
  `token` ni `TOKEN`, et le processus a la variable d'environnement
  `TOKEN=xyz`
- **THEN** `token` est résolue à `xyz` avec la source shell et la clé
  `TOKEN`

#### Scenario: .env prioritaire sur le shell
- **WHEN** le `.env` contient `TOKEN=a` et le processus a la variable
  `token=b`, pour le nom `vars:secret` `token`
- **THEN** la valeur du `.env` (`a`) est retenue, la source primant sur
  l'ordre des clés, comme `bru` fait primer le `.env` sur `process.env`

#### Scenario: Saisie prioritaire
- **WHEN** `oktaClientSecret` est résolue depuis le `.env` et l'utilisateur
  saisit une autre valeur dans le panneau
- **THEN** la valeur saisie est retenue avec la source saisie

#### Scenario: Valeurs entre guillemets et commentaires
- **WHEN** le `.env` contient `A="x y"`, `B='z'`, `# commentaire` et
  `C=v # note`
- **THEN** `A` vaut `x y`, `B` vaut `z` et `C` vaut `v`

#### Scenario: Aucune clé trouvée
- **WHEN** `token` est déclaré en `vars:secret` et ni `token` ni `TOKEN`
  n'existent dans le `.env` ou l'environnement du processus
- **THEN** `token` est non fournie, n'est pas transmise, et le panneau
  indique les clés cherchées

#### Scenario: Valeur multiligne
- **WHEN** la clé résolue a une valeur contenant un saut de ligne
- **THEN** la variable est non fournie, marquée en erreur avec sa clé, et
  sa valeur n'est pas affichée

### Requirement: Panneau des variables secrètes
`S`, hors saisie, SHALL ouvrir un panneau listant les variables secrètes
de la session ; `Échap` ou un second `S` SHALL le fermer. `S` MUST être
sans effet tant qu'aucune collection n'est chargée. Chaque ligne SHALL
afficher le nom de la variable et son état : saisie, `.env` (avec la clé),
shell (avec la clé), non fournie (avec les clés cherchées le cas échéant) ou
en erreur (avec la raison, sans valeur). Le panneau MUST NOT afficher une
valeur, une partie de valeur ni sa longueur. `↓`/`j` et `↑`/`k` SHALL
déplacer la sélection sans dépasser les extrémités. Le panneau SHALL
suivre le patron d'ouverture, de fermeture et de focus des panneaux
d'environnements, de diagnostics et d'historique.

#### Scenario: Ouverture et états affichés
- **WHEN** une collection est chargée, `oktaClientSecret` est résolue
  depuis `.env` par la clé `OKTA_CLIENT_SECRET` et `token` est non
  fournie, et l'utilisateur appuie sur `S`
- **THEN** le panneau liste `oktaClientSecret` avec l'état `.env
  (OKTA_CLIENT_SECRET)` et `token` avec l'état non fournie et les clés
  cherchées `token`, `TOKEN`, sans aucune valeur

#### Scenario: Liste vide
- **WHEN** l'ensemble des variables secrètes est vide et l'utilisateur
  appuie sur `S`
- **THEN** le panneau s'ouvre avec une mention indiquant qu'aucune
  variable secrète n'est déclarée et comment en ajouter une

#### Scenario: S sans collection chargée
- **WHEN** la collection n'est pas chargée et l'utilisateur appuie sur `S`
- **THEN** rien ne s'affiche

### Requirement: Saisie masquée d'une valeur
Dans le panneau, `Entrée` sur une variable SHALL ouvrir une saisie de
valeur masquée : chaque caractère tapé est affiché par un même symbole de
masquage, `Retour arrière` retire le dernier caractère, `Entrée` valide
la valeur pour la session, `Échap` abandonne sans modifier la valeur
existante. Une valeur validée vide MUST être traitée comme un oubli de la
saisie. `a` dans le panneau SHALL ouvrir la saisie d'un nom (en clair),
puis enchaîner sur la saisie masquée de sa valeur ; un nom invalide ou
déjà présent MUST être refusé avec un message. `d` sur une variable SHALL
oublier sa valeur saisie, la variable reprenant la valeur issue de `.env`
ou du shell si elle en a une ; `d` sur un nom ajouté depuis le panneau
SHALL en outre le retirer de la liste. Pendant une saisie, toutes les
touches imprimables MUST être capturées comme texte et `Ctrl+C` MUST
rester effectif. Le presse-papiers MUST NOT recevoir une valeur saisie.

#### Scenario: Saisie d'une valeur
- **WHEN** le panneau est ouvert sur `token`, l'utilisateur appuie sur
  `Entrée`, tape `s3cr3t` puis `Entrée`
- **THEN** l'écran n'a montré que des symboles de masquage, jamais
  `s3cr3t`, et `token` a l'état saisie

#### Scenario: Abandon de la saisie
- **WHEN** `token` vaut une valeur issue du shell, l'utilisateur ouvre la
  saisie, tape des caractères puis `Échap`
- **THEN** `token` garde la valeur et la source shell

#### Scenario: Ajout d'une variable non déclarée
- **WHEN** l'utilisateur appuie sur `a`, tape `oktaClientSecret`,
  `Entrée`, puis une valeur et `Entrée`
- **THEN** `oktaClientSecret` apparaît dans la liste avec l'état saisie

#### Scenario: Touches de navigation pendant la saisie
- **WHEN** une saisie masquée est en cours et l'utilisateur tape `q`, `r`
  ou `j`
- **THEN** ces caractères sont ajoutés à la valeur et l'application ne
  quitte pas, ne lance rien et ne déplace pas la sélection

#### Scenario: Oubli d'une valeur saisie
- **WHEN** `oktaClientSecret` a une valeur saisie et une valeur `.env`, et
  l'utilisateur appuie sur `d`
- **THEN** `oktaClientSecret` reprend l'état `.env`

### Requirement: Transmission à l'exécution
Tout lancement d'exécution (`r` sur une requête ou un dossier, rejeu depuis
l'historique) SHALL transmettre chaque variable secrète ayant une valeur
résolue comme surcharge `nom=valeur` de la `RunRequest`, dans l'ordre de
l'ensemble des variables secrètes, et MUST NOT transmettre les variables
non fournies. Les valeurs transmises SHALL être celles en vigueur au
moment du lancement, y compris pour un rejeu. Une entrée d'historique MUST
NOT conserver de valeur secrète.

#### Scenario: Cas oktaClientSecret
- **WHEN** l'environnement courant est `CI`, `oktaClientSecret` est
  résolue à `abc` depuis la clé `OKTA_CLIENT_SECRET` du `.env`, et
  l'utilisateur lance une requête avec `r`
- **THEN** `bru` reçoit `--env CI --env-var oktaClientSecret=abc`

#### Scenario: Aucune variable secrète
- **WHEN** l'ensemble des variables secrètes est vide et l'utilisateur
  lance une requête
- **THEN** la `RunRequest` ne porte aucune surcharge, comme avant ce
  changement

#### Scenario: Rejeu après modification
- **WHEN** une exécution a été faite avec `token` non fourni, puis
  l'utilisateur saisit `token` et rejoue l'entrée depuis l'historique
- **THEN** le rejeu transmet `token` avec la valeur saisie

### Requirement: Proposition de saisie au lancement
Quand l'utilisateur lance une exécution alors que l'environnement courant
déclare au moins un nom `vars:secret` resté non fourni après la recherche
automatique, et que cette situation
n'a pas encore été acquittée pour cet environnement pendant la session,
l'exécution MUST NOT démarrer : le panneau des variables secrètes SHALL
s'ouvrir sur la première variable non fournie et indiquer qu'une exécution
est en attente. Depuis le panneau, `r` SHALL lancer l'exécution en attente
avec les valeurs alors résolues et acquitter l'environnement ; `Échap`
SHALL fermer le panneau, abandonner l'exécution en attente et acquitter
l'environnement. Une fois acquitté, les lancements suivants pour cet
environnement MUST démarrer directement. Les noms déclarés uniquement par
`--secret` ou ajoutés dans le panneau MUST NOT déclencher cette
proposition. Une exécution déjà en cours SHALL conserver la règle existante
(aucune nouvelle exécution) sans ouvrir le panneau.

#### Scenario: Premier lancement avec secret manquant
- **WHEN** l'environnement courant `local` déclare `vars:secret [ token ]`,
  `token` reste non fournie après recherche automatique, et l'utilisateur
  appuie sur `r` sur une requête
- **THEN** aucune exécution ne démarre et le panneau s'ouvre sur `token`
  avec l'indication d'une exécution en attente

#### Scenario: Saisie puis lancement
- **WHEN** le panneau a été ouvert par un lancement en attente,
  l'utilisateur saisit `token` puis appuie sur `r`
- **THEN** l'exécution en attente démarre avec `--env-var token=<valeur>`

#### Scenario: Lancement suivant après acquittement
- **WHEN** l'utilisateur a fermé le panneau par `Échap` après une
  proposition pour `local`, puis appuie de nouveau sur `r`
- **THEN** l'exécution démarre sans surcharge pour `token`

### Requirement: Confidentialité des valeurs
Les valeurs secrètes MUST rester en mémoire uniquement : elles MUST NOT
être écrites dans un fichier (y compris `.bru`, `.env` ou un fichier de
l'application), journalisées, écrites sur la sortie standard ou d'erreur,
affichées dans l'interface, copiées dans le presse-papiers, ni incluses
dans un message d'erreur ou un diagnostic. Elles SHALL être oubliées à la
fin du processus. Le fichier `.env` MUST être seulement lu.

#### Scenario: Aucune trace après une session
- **WHEN** l'utilisateur saisit une valeur, lance plusieurs exécutions
  puis quitte
- **THEN** aucun fichier n'a été créé ou modifié et la valeur n'apparaît
  dans aucune sortie ni aucun rendu de l'interface

#### Scenario: Formatage de débogage
- **WHEN** l'état de l'application contenant une valeur secrète est
  formaté pour le débogage
- **THEN** la valeur n'apparaît pas dans le texte produit
