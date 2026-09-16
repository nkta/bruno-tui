# environment-picker Specification

## Purpose

Choisir un environnement parmi ceux déclarés par la collection chargée,
l'afficher en permanence, et le transmettre à `bru-runner` au lancement
d'une exécution, pour que les requêtes utilisant des variables
d'environnement (`{{base_url}}` et autres) puissent réellement s'exécuter
depuis l'interface plutôt qu'en ligne de commande seulement.

## Requirements

### Requirement: Ouverture et fermeture du panneau d'environnements
`E`, hors saisie, SHALL ouvrir un panneau listant, dans cet ordre, une
entrée « Aucun » puis chaque environnement de la collection chargée
(`Collection.environments`), dans l'ordre où `bru-parser` les expose. Le
panneau MUST rester ouvert jusqu'à `Entrée`, `Échap`, ou un second `E`.
`Entrée` sur une entrée valide SHALL faire de cet environnement (ou
d'aucun, pour l'entrée « Aucun ») l'environnement courant, puis fermer le
panneau. `Échap` ou un second `E` SHALL fermer le panneau sans changer
l'environnement courant. `E` MUST être sans effet tant qu'aucune
collection n'est chargée.

#### Scenario: Ouverture depuis l'arbre
- **WHEN** une collection avec au moins un environnement est chargée et
  l'utilisateur appuie sur `E`
- **THEN** le panneau s'ouvre avec « Aucun » sélectionné par défaut au
  premier appui, ou l'environnement déjà courant sélectionné à une
  réouverture

#### Scenario: Fermeture sans changement
- **WHEN** le panneau est ouvert et l'utilisateur appuie sur `Échap`
- **THEN** le panneau se ferme et l'environnement courant reste celui
  d'avant l'ouverture, quelle que soit l'entrée survolée au moment de la
  fermeture

#### Scenario: E sans collection chargée
- **WHEN** le chargement de la collection a échoué ou est en cours et
  l'utilisateur appuie sur `E`
- **THEN** rien ne s'affiche et l'environnement courant reste « Aucun »

### Requirement: Navigation et sélection dans la liste
Quand le panneau est ouvert, `↓`/`j` et `↑`/`k` SHALL déplacer la
sélection d'une entrée, sans dépasser les extrémités de la liste.
`Entrée` sur l'entrée courante SHALL la valider comme décrit par
l'exigence précédente.

#### Scenario: Extrémités de la liste
- **WHEN** la première entrée (« Aucun ») est sélectionnée et
  l'utilisateur appuie sur `↑`
- **THEN** la sélection ne change pas

#### Scenario: Choisir un environnement
- **WHEN** le panneau est ouvert sur une collection à deux
  environnements, l'utilisateur descend jusqu'au second puis appuie sur
  `Entrée`
- **THEN** le panneau se ferme et cet environnement devient
  l'environnement courant

### Requirement: Environnement invalide non sélectionnable
Une entrée de `Collection.environments` en erreur (fichier
d'environnement malformé) SHALL apparaître dans la liste avec son nom de
fichier et un marqueur d'erreur, mais `Entrée` sur cette entrée MUST être
sans effet : ni changement de l'environnement courant, ni fermeture du
panneau.

#### Scenario: Tentative de sélection d'un environnement invalide
- **WHEN** la liste contient un environnement en erreur et l'utilisateur
  le sélectionne puis appuie sur `Entrée`
- **THEN** le panneau reste ouvert et l'environnement courant ne change
  pas

### Requirement: Indicateur permanent de l'environnement courant
Tant qu'une collection est chargée, l'interface SHALL afficher en
permanence le nom de l'environnement courant, ou une mention explicite
d'absence d'environnement, en dehors du panneau de sélection lui-même.

#### Scenario: Aucun environnement choisi
- **WHEN** aucune sélection n'a encore eu lieu depuis le chargement de la
  collection
- **THEN** l'indicateur affiche l'absence d'environnement

#### Scenario: Indicateur mis à jour après sélection
- **WHEN** l'utilisateur vient de choisir un environnement
- **THEN** l'indicateur affiche son nom sans qu'aucune autre action ne
  soit nécessaire

### Requirement: Transmission de l'environnement à l'exécution
Le lancement d'une exécution par `r` (requête, dossier, ou rejeu depuis
l'historique) SHALL porter le nom de l'environnement courant dans
`RunRequest.env`, ou `None` si l'environnement courant est « Aucun ».

#### Scenario: Exécution avec un environnement choisi
- **WHEN** l'environnement courant est `public` et l'utilisateur lance
  une requête avec `r`
- **THEN** la `RunRequest` transmise à `bru-runner` porte
  `env: Some("public")`

#### Scenario: Exécution sans environnement choisi
- **WHEN** l'environnement courant est « Aucun » et l'utilisateur lance
  une requête avec `r`
- **THEN** la `RunRequest` transmise porte `env: None`, comme avant ce
  changement

### Requirement: Réinitialisation au rechargement de la collection
Un rechargement de la collection (nouveau `CollectionLoaded`, avec ou
sans succès) SHALL réinitialiser l'environnement courant à « Aucun » et
fermer le panneau de sélection s'il était ouvert : un nom d'environnement
n'a de sens que pour la collection qui l'a déclaré.

#### Scenario: Rechargement après sélection
- **WHEN** un environnement est courant et une nouvelle collection est
  chargée avec succès
- **THEN** l'environnement courant redevient « Aucun »

### Requirement: Compatibilité avec la navigation existante
`E` MUST NOT réutiliser une touche déjà affectée par `tui-shell` ou les
changements déjà fusionnés. Le panneau de sélection SHALL suivre le même
patron d'ouverture, de fermeture et de focus que les panneaux de
diagnostics et d'historique déjà posés par `diagnostics-and-history`, et
MUST NOT changer leur comportement déjà spécifié ni celui d'aucune autre
touche existante.

#### Scenario: Touches existantes inchangées
- **WHEN** le panneau de sélection n'est pas ouvert
- **THEN** `D`, `H`, `/`, `|`, `r` et toutes les touches déjà spécifiées
  se comportent exactement comme avant ce changement
