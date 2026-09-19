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

### Requirement: Sauvegarde des variables modifiées
`s` ou `Ctrl+S`, avec au moins une variable modifiée en mémoire dans la
vue des variables d'un environnement, SHALL écrire sur disque
uniquement les valeurs des variables modifiées de cet environnement,
par une écriture atomique (fichier temporaire dans le même répertoire
puis renommage) ; le reste du fichier (ordre des entrées, variables non
modifiées, commentaires, blocs `vars:secret` ou autres, fins de ligne)
SHALL être préservé à l'octet près. Si le fichier a changé sur disque
depuis son chargement (taille ou date de modification différente de
l'instantané pris à l'ouverture de la vue), l'écriture SHALL être
refusée et un message d'erreur affiché, sans modifier le fichier —
mêmes garanties que la sauvegarde d'une requête modifiée. Après une
sauvegarde réussie, la session n'a plus de modification en attente.

#### Scenario: Sauvegarde réussie
- **WHEN** la variable `host` a été modifiée en mémoire dans la vue des
  variables de `staging`, le fichier n'a pas changé sur disque depuis
  son chargement, et l'utilisateur appuie sur `Ctrl+S`
- **THEN** `environments/staging.bru` est réécrit avec la nouvelle
  valeur de `host`, le reste du fichier inchangé à l'octet près, et la
  session n'a plus de modification en attente

#### Scenario: Sauvegarde refusée si le fichier a changé sur disque
- **WHEN** la variable `host` a été modifiée en mémoire, mais
  `environments/staging.bru` a été modifié sur disque depuis son
  chargement, et l'utilisateur appuie sur `Ctrl+S`
- **THEN** l'écriture est refusée, un message d'erreur s'affiche, et le
  fichier sur disque n'est pas modifié
