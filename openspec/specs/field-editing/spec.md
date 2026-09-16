# field-editing Specification

## Purpose

Permettre de modifier, depuis le panneau de détail d'une requête, les
champs déjà présents dans son fichier `.bru` (URL, en-têtes, paramètres,
corps textuel), avec une ergonomie vim (modes Normal et Insert) et une
sauvegarde explicite et sûre vers le disque.

## Requirements

### Requirement: Ouverture et fermeture d'une session d'édition
Le système SHALL permettre d'ouvrir une session d'édition sur la requête
sélectionnée par la touche `e`, uniquement quand le panneau de détail a le
focus, qu'un nœud requête est sélectionné, et qu'aucune session n'est déjà
ouverte. Une session ouverte MUST capturer un instantané de fraîcheur du
fichier au moment de l'ouverture. Sur un nœud dossier ou en erreur, `e`
MUST NOT avoir d'effet.

Le système SHALL fermer la session sur `Échap` en mode Normal de la
session. Si la session porte des modifications non sauvegardées, la
fermeture MUST demander confirmation avant d'être appliquée ; une
confirmation refusée MUST laisser la session ouverte et les modifications
intactes.

#### Scenario: Ouverture sur une requête
- **WHEN** le panneau de détail a le focus sur une requête et qu'aucune
  session n'est ouverte, et l'utilisateur appuie sur `e`
- **THEN** une session d'édition s'ouvre en mode Normal, le curseur de
  champ sur le premier champ éditable

#### Scenario: Sans effet sur un dossier
- **WHEN** le panneau de détail affiche un dossier et l'utilisateur
  appuie sur `e`
- **THEN** aucune session ne s'ouvre

#### Scenario: Fermeture sans modification
- **WHEN** une session sans modification est ouverte et l'utilisateur
  appuie sur `Échap` en mode Normal de la session
- **THEN** la session se ferme sans confirmation, le détail redevient
  celui, en lecture seule, de `tui-shell`

#### Scenario: Fermeture avec modification non sauvegardée
- **WHEN** une session porte une modification non sauvegardée et
  l'utilisateur appuie sur `Échap` en mode Normal de la session
- **THEN** une confirmation est demandée avant que la session ne se ferme
- **AND** si l'utilisateur annule la confirmation, la session reste
  ouverte avec sa modification intacte

### Requirement: Champs éditables et curseur de champ
En mode Normal d'une session, le système SHALL exposer un curseur qui se
déplace, par `↓`/`j` et `↑`/`k`, exclusivement parmi les champs éditables
de la requête, dans l'ordre où ils apparaissent dans le détail : l'URL,
puis chaque en-tête existant, chaque paramètre de requête existant, chaque
paramètre de chemin existant, puis le corps si son type est éditable par
`bru-writer` (`json`, `text`, `xml`, `sparql`, `graphql`). Le déplacement
MUST NOT avoir d'effet aux extrémités de cette liste. Un champ non
éditable (corps de forme formulaire, absence de corps, sections vides)
MUST NOT apparaître dans cette liste. Le champ sous le curseur MUST être
visuellement distingué du reste du détail.

#### Scenario: Parcours des champs
- **WHEN** une session est ouverte sur une requête ayant une URL, deux
  en-têtes et un corps `json`
- **THEN** quatre positions de curseur existent, dans cet ordre : URL,
  premier en-tête, second en-tête, corps

#### Scenario: Corps non éditable exclu
- **WHEN** la requête a un corps de type `formUrlEncoded`
- **THEN** aucune position de curseur ne porte sur le corps

#### Scenario: Extrémités du curseur de champ
- **WHEN** le curseur de champ est sur le premier champ et l'utilisateur
  appuie sur `↑`
- **THEN** le curseur ne bouge pas

### Requirement: Bascule d'activation d'une entrée
En mode Normal d'une session, quand le curseur de champ est sur un
en-tête ou un paramètre existant, le système SHALL basculer son état
activé ou désactivé sur `Espace`, immédiatement, sans passer par le mode
Insert, et marquer la session comme modifiée. Sur l'URL ou le corps,
`Espace` MUST NOT avoir d'effet.

#### Scenario: Désactivation d'un en-tête actif
- **WHEN** le curseur de champ est sur un en-tête activé et l'utilisateur
  appuie sur `Espace`
- **THEN** l'en-tête est affiché désactivé et la session est marquée
  modifiée

#### Scenario: Sans effet sur l'URL
- **WHEN** le curseur de champ est sur l'URL et l'utilisateur appuie sur
  `Espace`
- **THEN** rien ne change

### Requirement: Mode Insert et édition de texte
En mode Normal d'une session, le système SHALL entrer en mode Insert sur
le champ sous le curseur par `i`, avec un curseur de texte initialement en
fin de valeur actuelle. En mode Insert, un caractère imprimable saisi MUST
s'insérer à la position du curseur de texte ; `Retour arrière` MUST
supprimer le caractère précédent ; les flèches gauche et droite MUST
déplacer le curseur de texte sans modifier la valeur. Sur un champ à
valeur unique (URL, en-tête, paramètre), `Entrée` MUST quitter le mode
Insert comme `Échap`, en conservant la valeur ; sur le corps, `Entrée`
MUST insérer un saut de ligne. `Échap` MUST toujours quitter le mode
Insert vers le mode Normal de la session, en conservant la valeur telle
que modifiée jusque-là, sans écrire sur disque. Si la valeur a changé par
rapport à celle chargée, la session MUST être marquée modifiée.

#### Scenario: Modification de l'URL
- **WHEN** le curseur de champ est sur l'URL, l'utilisateur appuie sur
  `i`, tape des caractères puis appuie sur `Échap`
- **THEN** le mode redevient Normal, la valeur affichée de l'URL reflète
  la saisie, et la session est marquée modifiée

#### Scenario: Saut de ligne dans le corps
- **WHEN** le curseur de champ est sur le corps et le mode Insert est
  actif, et l'utilisateur appuie sur `Entrée`
- **THEN** un saut de ligne est inséré dans la valeur du corps, sans
  quitter le mode Insert

#### Scenario: Entrée quitte l'insertion sur un champ à valeur unique
- **WHEN** le curseur de champ est sur un en-tête et le mode Insert est
  actif, et l'utilisateur appuie sur `Entrée`
- **THEN** le mode redevient Normal, la valeur saisie est conservée

#### Scenario: Valeur inchangée
- **WHEN** l'utilisateur entre en mode Insert sur un champ puis en sort
  par `Échap` sans avoir tapé de caractère
- **THEN** la session n'est pas marquée modifiée par ce seul aller-retour

### Requirement: Sauvegarde explicite
En mode Normal d'une session portant au moins une modification, le
système SHALL sauvegarder sur `w` en appelant `bru-writer` avec
l'instantané de fraîcheur capturé à l'ouverture de la session et
l'ensemble des modifications de champ effectuées depuis. Sur une session
sans modification, `w` MUST NOT déclencher d'écriture. Une sauvegarde
réussie MUST effacer l'indicateur de modification et remplacer
l'instantané de fraîcheur de la session par celui retourné. Le
déclenchement de l'écriture MUST NOT bloquer la saisie ni le rendu.

#### Scenario: Sauvegarde réussie
- **WHEN** une session modifiée reçoit `w` et l'écriture réussit
- **THEN** l'indicateur de modification disparaît et la session reste
  ouverte sur la requête, valeurs sauvegardées affichées

#### Scenario: Sauvegarde sans modification
- **WHEN** une session sans modification reçoit `w`
- **THEN** aucune écriture n'est déclenchée

#### Scenario: Sauvegarde pendant une saisie longue
- **WHEN** l'utilisateur déclenche une sauvegarde alors que l'écriture
  disque est lente
- **THEN** l'interface continue de répondre aux touches pendant l'écriture

### Requirement: Échec de sauvegarde sans perte
Le système SHALL afficher un message explicite quand `bru-writer` refuse
l'écriture (fichier modifié depuis l'ouverture de la session, erreur
d'entrée-sortie), et MUST NOT perdre la modification en mémoire dans ce
cas : la session reste ouverte, modifiée, avec les mêmes valeurs.

#### Scenario: Fichier modifié entre-temps
- **WHEN** le fichier de la requête a été modifié sur disque depuis
  l'ouverture de la session, et l'utilisateur sauvegarde
- **THEN** un message signale le conflit, la session reste ouverte et
  modifiée, rien n'est écrasé sur disque

### Requirement: Confirmation avant de quitter avec des modifications non sauvegardées
Le système SHALL demander confirmation avant de fermer le terminal si une
session d'édition porte des modifications non sauvegardées au moment où
`q` ou `Ctrl+C` est reçu. Une confirmation refusée MUST annuler la
fermeture et laisser la session ouverte, modifiée.

#### Scenario: Quitter avec une session modifiée
- **WHEN** une session porte une modification non sauvegardée et
  l'utilisateur appuie sur `q`
- **THEN** une confirmation est demandée avant toute fermeture
- **AND** si l'utilisateur annule, l'application reste ouverte et la
  session reste modifiée

#### Scenario: Quitter sans session active
- **WHEN** aucune session d'édition n'est ouverte et l'utilisateur appuie
  sur `q`
- **THEN** l'application se ferme sans confirmation, comme `tui-shell` le
  définit déjà

### Requirement: Barre de mode
Le système SHALL afficher, quand une session d'édition est ouverte, une
indication du mode courant (Normal ou Insert) et du nom du champ sous le
curseur, à la place ou en complément de la barre d'état habituelle du
panneau de détail.

#### Scenario: Barre de mode en Insert
- **WHEN** le mode Insert est actif sur le champ URL
- **THEN** la barre affiche une indication du mode Insert et du nom du
  champ URL
