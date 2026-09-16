## Purpose

Donner une vue agrégée, en un seul endroit, des fichiers `.bru` en erreur
d'une collection chargée, et garder la trace des exécutions passées de la
session pour pouvoir les rejouer, sans altérer la façon dont ces
informations sont produites par ailleurs.

## ADDED Requirements

### Requirement: Panneau de diagnostics
Quand une collection est chargée, l'interface SHALL permettre d'ouvrir un
panneau listant tous les nœuds en erreur de la collection : chaque
fichier `.bru` dont le chargement a échoué, et chaque dossier dont le
fichier `folder.bru` est invalide. Chaque entrée MUST afficher le chemin
relatif du fichier concerné et la raison de l'échec telle que produite
par `bru-parser`. Le panneau SHALL rester accessible même si la
collection ne comporte aucune erreur, auquel cas il SHALL afficher un
message indiquant qu'aucune erreur n'est présente.

#### Scenario: Ouverture sur une collection avec erreurs
- **WHEN** la collection `parser-cases` est chargée et l'utilisateur ouvre
  le panneau de diagnostics
- **THEN** le panneau liste `broken.bru`, `no-method.bru` et `badmeta`
  (dossier dont `folder.bru` est invalide), chacun avec son chemin et sa
  raison

#### Scenario: Collection sans erreur
- **WHEN** une collection ne comporte aucun nœud en erreur et l'utilisateur
  ouvre le panneau de diagnostics
- **THEN** le panneau affiche qu'aucune erreur n'est présente

### Requirement: Badge de comptage des erreurs
L'interface SHALL afficher en permanence, tant qu'une collection est
chargée, un indicateur du nombre de nœuds en erreur. L'indicateur MUST
être visible sans ouvrir le panneau de diagnostics, et MUST NOT apparaître
quand ce nombre est nul.

#### Scenario: Badge visible avec des erreurs
- **WHEN** la collection chargée comporte trois nœuds en erreur
- **THEN** un indicateur portant le nombre trois est visible en dehors du
  panneau de diagnostics

#### Scenario: Aucun badge sans erreur
- **WHEN** la collection chargée ne comporte aucun nœud en erreur
- **THEN** aucun indicateur de comptage n'est affiché

### Requirement: Navigation croisée depuis les diagnostics
Valider une entrée du panneau de diagnostics SHALL sélectionner, dans
l'arbre, le nœud correspondant, en dépliant au besoin ses dossiers
parents, et SHALL rendre le focus à l'arbre.

#### Scenario: Aller au nœud depuis le panneau
- **WHEN** le panneau de diagnostics est ouvert, l'entrée `badmeta` est
  sélectionnée et l'utilisateur valide
- **THEN** le focus revient à l'arbre avec le dossier `badmeta`
  sélectionné et déplié

### Requirement: Journal des exécutions de la session
Chaque exécution menée à son terme (achevée, en échec ou annulée) via la
capacité `request-execution` SHALL être ajoutée à un journal, avec au
moins : un horodatage, la cible exécutée (chemin et mode récursif ou
non), un verdict global, et une durée. Le verdict global d'une exécution
achevée MUST être établi selon la même règle que le verdict par requête
de `request-execution` (une requête en échec si elle est en erreur ou si
au moins une assertion ou un test échoue), et non sur le seul résumé
fourni par `bru`, qui ne reflète pas les échecs d'assertions ou de tests.
Le journal SHALL être borné à 200 entrées ; au-delà, la plus ancienne
entrée MUST être retirée à l'ajout d'une nouvelle.

#### Scenario: Ajout après une exécution réussie
- **WHEN** une exécution sur la requête `ok` se termine avec toutes les
  assertions et tous les tests en succès
- **THEN** une entrée est ajoutée au journal avec un verdict de succès

#### Scenario: Verdict d'échec malgré un statut pass de bru
- **WHEN** une exécution se termine avec un résultat de statut `pass`
  rapporté par `bru` mais au moins une assertion en échec
- **THEN** l'entrée ajoutée au journal porte un verdict d'échec

#### Scenario: Exécution annulée
- **WHEN** l'utilisateur annule une exécution en cours
- **THEN** une entrée est ajoutée au journal avec la cible d'origine et un
  verdict d'annulation, sans durée d'exécution significative

#### Scenario: Troncature du journal
- **WHEN** le journal contient déjà 200 entrées et qu'une exécution se
  termine
- **THEN** la nouvelle entrée est ajoutée et l'entrée la plus ancienne est
  retirée, pour un total de 200 entrées

### Requirement: Panneau d'historique
L'interface SHALL permettre d'ouvrir un panneau listant les entrées du
journal, les plus récentes en premier, navigable comme les autres listes
de l'interface. Le panneau SHALL rester accessible même si le journal est
vide, auquel cas il SHALL afficher un message indiquant qu'aucune
exécution n'a encore eu lieu dans la session.

#### Scenario: Ordre des entrées
- **WHEN** deux exécutions se sont terminées l'une après l'autre pendant
  la session
- **THEN** le panneau d'historique liste la plus récente en premier

#### Scenario: Historique vide
- **WHEN** aucune exécution n'a encore eu lieu et l'utilisateur ouvre le
  panneau d'historique
- **THEN** le panneau indique qu'aucune exécution n'a eu lieu

### Requirement: Rejeu d'une entrée de l'historique
Depuis le panneau d'historique, valider une entrée SHALL lancer une
nouvelle exécution sur la cible et le mode (récursif ou non) de cette
entrée, selon les mêmes règles que le lancement d'une exécution depuis
l'arbre (une seule exécution à la fois, non bloquant). Rejouer une entrée
MUST NOT modifier l'entrée elle-même : la nouvelle exécution, une fois
terminée, SHALL produire sa propre entrée distincte.

#### Scenario: Rejeu d'une exécution passée
- **WHEN** le panneau d'historique est ouvert, une entrée ciblant le
  dossier `folder` en mode récursif est sélectionnée, et l'utilisateur
  demande le rejeu
- **THEN** une nouvelle exécution démarre sur `folder` en récursif

#### Scenario: Rejeu pendant une exécution en cours
- **WHEN** une exécution est déjà en cours et l'utilisateur demande le
  rejeu d'une entrée de l'historique
- **THEN** aucune nouvelle exécution ne démarre

### Requirement: Fermeture des panneaux
Depuis l'un ou l'autre panneau, une touche dédiée SHALL rendre le focus à
l'arbre sans modifier la sélection en cours dans celui-ci.

#### Scenario: Fermeture du panneau de diagnostics
- **WHEN** le panneau de diagnostics est ouvert et l'utilisateur ferme le
  panneau sans valider d'entrée
- **THEN** le focus revient à l'arbre, sélection inchangée

### Requirement: Aucune persistance de l'historique
Le journal des exécutions MUST NOT être écrit sur disque, sous aucune
forme, et MUST être vidé à la fermeture de l'application. Aucun contenu
d'en-tête ou de corps de réponse ne fait partie d'une entrée du journal.

#### Scenario: Rien sur disque après une session
- **WHEN** une session comportant plusieurs exécutions se termine
- **THEN** aucun fichier contenant tout ou partie du journal n'existe sur
  le disque
