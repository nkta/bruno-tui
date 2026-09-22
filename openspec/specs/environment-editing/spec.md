# environment-editing Specification

## Purpose

Permettre de consulter et de modifier, au clavier, la valeur des
variables déjà présentes dans un environnement (`environments/*.bru`)
directement depuis le panneau Environnement, sans quitter l'application
ni éditer le fichier à la main.

## Requirements

### Requirement: Panneau Environnement permanent
Le panneau Environnement SHALL être affiché en permanence, dans sa
propre zone de la mise en page (au-dessus du panneau Statut), quel que
soit le focus courant — comme les panneaux Collection, Détail, Réponse
et Statut. `E` SHALL donner le focus à ce panneau (`Tab` continue par
ailleurs de cycler entre tous les panneaux, Environnement y compris).
`Entrée` ou `Droite` sur une entrée de la liste qui désigne un
environnement valide (ni « Aucun », ni une entrée en erreur) SHALL
l'activer (devient l'environnement courant), sans changer le focus ni
masquer aucun autre panneau. `Entrée` ou `Droite` sur « Aucun » SHALL
désactiver l'environnement courant. `Entrée` ou `Droite` sur une entrée
en erreur SHALL rester sans effet.

#### Scenario: Le panneau est visible sans action de l'utilisateur
- **WHEN** une collection est chargée, quel que soit le focus courant
- **THEN** le panneau Environnement est visible, au-dessus du panneau
  Statut

#### Scenario: Entrée active un environnement valide
- **WHEN** le panneau Environnement a le focus, la sélection est sur
  l'environnement `staging`, et l'utilisateur appuie sur `Entrée`
- **THEN** `staging` devient l'environnement courant, le focus reste sur
  le panneau Environnement

#### Scenario: Entrée sur « Aucun » désactive l'environnement courant
- **WHEN** le panneau Environnement a le focus, un environnement est
  actif, la sélection est sur « Aucun », et l'utilisateur appuie sur
  `Entrée`
- **THEN** plus aucun environnement n'est actif

#### Scenario: Entrée sur une entrée en erreur ne fait rien
- **WHEN** le panneau Environnement a le focus, la sélection est sur un
  environnement dont le fichier est en erreur de lecture, et
  l'utilisateur appuie sur `Entrée`
- **THEN** l'environnement courant ne change pas

### Requirement: Ouverture du popup d'édition
`e` sur une entrée valide (ni « Aucun », ni une entrée en erreur) du
panneau Environnement SHALL ouvrir un popup d'édition centré, superposé
au reste de l'écran, affichant les variables de l'environnement visé
sous forme de tableau à deux colonnes (Clé, Valeur). Un double-clic
(deux clics gauche sur la même entrée du panneau, dans un délai court)
SHALL produire le même effet qu'un simple clic (activer l'entrée) puis,
en plus, ouvrir ce même popup. `Échap`, sans saisie en cours dans le
popup, SHALL le fermer et rendre le focus au panneau Environnement, sans
perdre les modifications déjà validées en mémoire (non sauvegardées).

#### Scenario: e ouvre le popup d'édition
- **WHEN** le panneau Environnement a le focus, la sélection est sur
  l'environnement `staging`, et l'utilisateur appuie sur `e`
- **THEN** le popup d'édition s'ouvre, affichant les variables de
  `staging` en tableau Clé/Valeur

#### Scenario: e sur « Aucun » ou une entrée en erreur ne fait rien
- **WHEN** le panneau Environnement a le focus, la sélection est sur
  « Aucun » ou sur une entrée en erreur, et l'utilisateur appuie sur `e`
- **THEN** aucun popup ne s'ouvre

#### Scenario: Double-clic active et ouvre le popup
- **WHEN** le panneau Environnement a le focus, et l'utilisateur fait
  un double-clic sur l'entrée `staging`
- **THEN** `staging` devient l'environnement courant et le popup
  d'édition s'ouvre sur ses variables

#### Scenario: Échap ferme le popup sans perdre les modifications validées
- **WHEN** le popup d'édition est ouvert, une valeur a déjà été validée
  (marquant la session modifiée), aucune saisie n'est en cours, et
  l'utilisateur appuie sur `Échap`
- **THEN** le popup se ferme, le focus revient au panneau Environnement,
  la modification reste en mémoire (non sauvegardée)

### Requirement: Saisie d'une valeur de variable
Dans le popup d'édition, `Entrée` ou `e` sur une variable activée SHALL
commencer la saisie de sa valeur en texte libre, curseur en fin de
valeur courante — comportement identique à la saisie d'une valeur
d'en-tête de requête. Un clic gauche sur la ligne d'une variable
activée SHALL produire le même effet, comme le clic sur un en-tête de
requête (`mouse-support`, « Saisie d'un champ au clic »). `Entrée`
pendant cette saisie SHALL valider la nouvelle valeur en mémoire sans
écrire sur disque. `Échap` pendant cette saisie SHALL annuler la saisie
et restaurer la valeur affichée avant la saisie, sans modifier la
variable. `Entrée` ou `e` sur une variable désactivée SHALL rester sans
effet : sa valeur n'est pas éditable tant qu'elle reste désactivée. Un
clic gauche, où qu'il tombe dans le popup (variable désactivée, ligne
d'en-tête, ou hors du tableau), SHALL d'abord valider une saisie en
cours sur une autre variable exactement comme le ferait `Entrée` sur
cette variable, avant de produire son propre effet (ou d'en rester
dépourvu s'il ne cible aucune variable activée) — même principe que
« Clic pendant une session d'édition » (`mouse-support`) pour une
requête.

#### Scenario: Modifier la valeur d'une variable
- **WHEN** le popup affiche `host = localhost` activée, le curseur de
  sélection dessus, et l'utilisateur appuie sur `Entrée`, tape
  `staging.example.com`, puis appuie sur `Entrée`
- **THEN** la variable `host` affiche `staging.example.com` en mémoire,
  la session est marquée modifiée

#### Scenario: Modifier la valeur d'une variable à la souris
- **WHEN** le popup affiche `host = localhost` activée, et l'utilisateur
  clique sur sa ligne
- **THEN** la saisie de la valeur de `host` démarre, curseur en fin de
  `localhost`

#### Scenario: Annuler une saisie en cours
- **WHEN** l'utilisateur a commencé la saisie de la valeur de `host` et
  tapé des caractères, puis appuie sur `Échap`
- **THEN** `host` affiche de nouveau sa valeur d'avant la saisie, aucune
  modification n'est retenue

#### Scenario: Une variable désactivée n'est pas éditable
- **WHEN** le popup affiche `debug` désactivée, le curseur de sélection
  dessus, et l'utilisateur appuie sur `Entrée`
- **THEN** aucune saisie ne démarre

#### Scenario: Un clic sur une variable désactivée valide la saisie en cours ailleurs
- **WHEN** l'utilisateur a commencé la saisie de la valeur de `host` et
  tapé des caractères, puis clique sur la ligne de `debug` (désactivée)
- **THEN** `host` affiche la valeur tapée en mémoire, la session est
  marquée modifiée, et aucune saisie ne démarre sur `debug`

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
