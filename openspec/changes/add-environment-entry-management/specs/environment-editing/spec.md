## ADDED Requirements

### Requirement: Ajout d'une variable
Dans le popup d'édition, `a` sur n'importe quelle ligne (une variable ou
la ligne d'ajout) SHALL commencer l'ajout d'une variable, en deux
saisies enchaînées, avec les mêmes touches d'édition de texte qu'une
saisie de valeur :
- saisie de la clé, texte initialement vide : `Entrée` ou `Tab` valide
  la clé et passe à la saisie de la valeur ; une clé invalide (vide,
  contenant un espace, une tabulation, un saut de ligne, `:`, ou
  commençant par `~` ou `"`) SHALL être refusée immédiatement, la saisie
  de la clé reste active ;
- saisie de la valeur, texte initialement vide : `Entrée` ou `Tab`
  valide la valeur ; une valeur vide SHALL être acceptée.

`Échap` SHALL abandonner l'ajout entier, à l'une ou l'autre étape, sans
rien ajouter et sans confirmation ; le curseur revient à la position
d'où l'ajout a commencé. À la validation de la valeur, la variable
SHALL être ajoutée, activée, en fin de tableau, la session marquée
modifiée, l'état redevient Sélection et le curseur est placé sur la
nouvelle variable. Pendant l'ajout, une ligne provisoire affiche la clé
et la valeur en cours de saisie en fin de tableau. Aucune écriture
disque MUST NOT avoir lieu avant `Ctrl+S`/`s`.

#### Scenario: Ajout d'une variable
- **WHEN** le popup d'édition est ouvert sur `staging`, et l'utilisateur
  appuie sur `a`, tape `region`, `Entrée`, `eu-west`, `Entrée`
- **THEN** une variable `region` de valeur `eu-west` apparaît en dernier
  dans le tableau, activée, le curseur dessus, la session marquée
  modifiée

#### Scenario: Ajout dans un environnement sans variable
- **WHEN** le popup d'édition affiche « aucune variable », et
  l'utilisateur ajoute la variable `host` de valeur `localhost`
- **THEN** le tableau affiche `host`, le curseur dessus

#### Scenario: Clé invalide refusée immédiatement
- **WHEN** l'utilisateur appuie sur `a` et tape une clé contenant un
  espace, puis appuie sur `Entrée`
- **THEN** la saisie de la clé reste active, aucune variable n'est
  ajoutée

#### Scenario: Abandon pendant la saisie de la valeur
- **WHEN** l'utilisateur a validé la clé d'une nouvelle variable et
  appuie sur `Échap` pendant la saisie de la valeur
- **THEN** aucune variable n'est ajoutée, l'état redevient Sélection

### Requirement: Suppression d'une variable
Dans le popup d'édition, le curseur sur une variable, `d` SHALL la
supprimer de la session immédiatement, sans confirmation, et marquer la
session modifiée. Le curseur SHALL rester au même indice dans le
tableau, borné à sa dernière position. Sur une ligne d'ajout en cours,
`d` MUST NOT avoir d'effet. La suppression d'une variable est une
modification non enregistrée au sens de la fermeture du popup et de la
fermeture de la collection.

#### Scenario: Suppression d'une variable
- **WHEN** le popup affiche trois variables, le curseur sur la seconde,
  et l'utilisateur appuie sur `d`
- **THEN** la variable n'est plus affichée, le curseur est sur l'ancienne
  troisième variable, et la session est marquée modifiée

#### Scenario: Suppression de la dernière variable
- **WHEN** le popup affiche une seule variable, le curseur dessus, et
  l'utilisateur appuie sur `d`
- **THEN** le popup affiche « aucune variable », la session est marquée
  modifiée

## MODIFIED Requirements

### Requirement: Sauvegarde des variables modifiées
`s` ou `Ctrl+S`, avec au moins une modification en mémoire (valeur
changée, variable ajoutée ou supprimée) dans le popup d'édition d'un
environnement, SHALL écrire sur disque ces modifications dans le bloc
`vars` de cet environnement, par une écriture atomique (fichier
temporaire dans le même répertoire puis renommage) ; le reste du fichier
(variables non modifiées, commentaires, blocs `vars:secret` ou autres,
fins de ligne) SHALL être préservé à l'octet près. Si le bloc `vars`
n'existe pas encore et qu'au moins une variable a été ajoutée, il SHALL
être créé en tout début de fichier. Si le fichier a changé sur disque
depuis son chargement (taille ou date de modification différente de
l'instantané pris à l'ouverture du popup), l'écriture SHALL être
refusée et un message d'erreur affiché, sans modifier le fichier —
mêmes garanties que la sauvegarde d'une requête modifiée. Après une
sauvegarde réussie, la session n'a plus de modification en attente.

#### Scenario: Sauvegarde réussie
- **WHEN** la variable `host` a été modifiée en mémoire dans le popup de
  `staging`, le fichier n'a pas changé sur disque depuis son chargement,
  et l'utilisateur appuie sur `Ctrl+S`
- **THEN** `environments/staging.bru` est réécrit avec la nouvelle
  valeur de `host`, le reste du fichier inchangé à l'octet près, et la
  session n'a plus de modification en attente

#### Scenario: Sauvegarde refusée si le fichier a changé sur disque
- **WHEN** la variable `host` a été modifiée en mémoire, mais
  `environments/staging.bru` a été modifié sur disque depuis son
  chargement, et l'utilisateur appuie sur `Ctrl+S`
- **THEN** l'écriture est refusée, un message d'erreur s'affiche, et le
  fichier sur disque n'est pas modifié

#### Scenario: Sauvegarde d'une variable ajoutée
- **WHEN** la variable `region` de valeur `eu-west` a été ajoutée en
  mémoire dans le popup de `staging`, et l'utilisateur appuie sur
  `Ctrl+S`
- **THEN** `environments/staging.bru` est réécrit avec `region:
  eu-west` en fin du bloc `vars`, le reste du fichier inchangé à l'octet
  près

#### Scenario: Sauvegarde d'une variable supprimée
- **WHEN** la variable `debug` a été supprimée en mémoire dans le popup
  de `local`, et l'utilisateur appuie sur `Ctrl+S`
- **THEN** `environments/local.bru` est réécrit sans la ligne `debug`,
  le reste du fichier inchangé à l'octet près

#### Scenario: Sauvegarde créant le bloc vars
- **WHEN** un environnement n'a pas de bloc `vars`, une variable `host`
  de valeur `localhost` a été ajoutée en mémoire, et l'utilisateur
  appuie sur `Ctrl+S`
- **THEN** le fichier est réécrit avec un bloc `vars` en tout début de
  fichier, contenant `host: localhost`
