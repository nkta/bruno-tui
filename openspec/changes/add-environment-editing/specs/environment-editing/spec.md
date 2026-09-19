## Purpose

Permettre de consulter et de modifier, au clavier, la valeur des
variables déjà présentes dans un environnement (`environments/*.bru`)
directement depuis le panneau Environnement, sans quitter l'application
ni éditer le fichier à la main.

## ADDED Requirements

### Requirement: Affichage des variables d'un environnement
Dans le panneau Environnement (`Focus::EnvironmentPicker`), `Entrée` sur
une entrée de la liste qui désigne un environnement valide (ni « Aucun »,
ni une entrée en erreur) SHALL remplacer la liste des environnements par
la liste de ses variables (`Environment.variables`, une ligne par entrée,
dans l'ordre du fichier), sans changer l'environnement actif. Chaque
ligne SHALL afficher la clé et la valeur courante de la variable. Une
variable désactivée (clé préfixée de `~` dans le fichier) SHALL rester
visible, sa valeur affichée avec un style la distinguant comme non
éditable, cohérent avec l'affichage déjà utilisé pour une entrée
désactivée dans le détail d'une requête. `Entrée` sur « Aucun » ou sur
une entrée en erreur SHALL rester sans effet, comme `Droite` aujourd'hui
sur ces mêmes entrées.

#### Scenario: Entrée sur un environnement valide affiche ses variables
- **WHEN** le panneau Environnement a le focus, la liste affiche
  l'environnement `staging`, et l'utilisateur appuie sur `Entrée`
- **THEN** la vue affiche les variables de `staging`, une ligne par
  entrée de `Environment.variables`, dans l'ordre du fichier

#### Scenario: Entrée sur « Aucun » ne fait rien
- **WHEN** le panneau Environnement a le focus, l'entrée courante est
  « Aucun », et l'utilisateur appuie sur `Entrée`
- **THEN** la liste des environnements reste affichée, sans changement

#### Scenario: Entrée sur un environnement en erreur ne fait rien
- **WHEN** le panneau Environnement a le focus, l'entrée courante est un
  environnement dont le fichier est en erreur de lecture, et
  l'utilisateur appuie sur `Entrée`
- **THEN** la liste des environnements reste affichée, sans changement

#### Scenario: Retour à la liste des environnements
- **WHEN** la vue des variables d'un environnement est affichée, aucune
  saisie n'est en cours, et l'utilisateur appuie sur `Échap`
- **THEN** la liste des environnements réapparaît, la sélection sur
  l'environnement précédemment ouvert

### Requirement: Saisie d'une valeur de variable
Dans la vue des variables d'un environnement, `Entrée` ou `e` sur une
variable activée SHALL commencer la saisie de sa valeur en texte libre,
curseur en fin de valeur courante — comportement identique à la saisie
d'une valeur d'en-tête de requête. `Entrée` pendant cette saisie SHALL
valider la nouvelle valeur en mémoire sans écrire sur disque. `Échap`
pendant cette saisie SHALL annuler la saisie et restaurer la valeur
affichée avant la saisie, sans modifier la variable. `Entrée` ou `e` sur
une variable désactivée SHALL rester sans effet : sa valeur n'est pas
éditable tant qu'elle reste désactivée.

#### Scenario: Modifier la valeur d'une variable
- **WHEN** la vue des variables affiche `host = localhost` activée, le
  curseur de sélection dessus, et l'utilisateur appuie sur `Entrée`, tape
  `staging.example.com`, puis appuie sur `Entrée`
- **THEN** la variable `host` affiche `staging.example.com` en mémoire,
  la session est marquée modifiée

#### Scenario: Annuler une saisie en cours
- **WHEN** l'utilisateur a commencé la saisie de la valeur de `host` et
  tapé des caractères, puis appuie sur `Échap`
- **THEN** `host` affiche de nouveau sa valeur d'avant la saisie, aucune
  modification n'est retenue

#### Scenario: Une variable désactivée n'est pas éditable
- **WHEN** la vue des variables affiche `debug` désactivée, le curseur de
  sélection dessus, et l'utilisateur appuie sur `Entrée`
- **THEN** aucune saisie ne démarre

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
