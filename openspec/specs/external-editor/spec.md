# external-editor Specification

## Purpose

Permettre de consulter le corps brut de la réponse d'une requête dans
l'éditeur externe préféré de l'utilisateur, pour une recherche ou une
lecture que le panneau Réponse ne permet pas.

## Requirements

### Requirement: Ouverture du corps brut de la réponse
`Ctrl+E`, quel que soit le focus courant et quel que soit l'état de
capture de texte, SHALL, quand la sélection courante est une requête
ayant un résultat d'exécution avec un corps de réponse non `null` :
écrire ce corps tel qu'affiché par l'onglet Corps sans filtre (une
chaîne telle quelle, tout autre type de donnée mis en forme indentée)
dans un fichier temporaire, céder le terminal à un éditeur externe
ouvert sur ce fichier, attendre sa fermeture, puis restaurer
l'interface avec un redessin complet. Le fichier temporaire SHALL être
supprimé après la fermeture de l'éditeur. Aucun contenu modifié dans
l'éditeur externe MUST NOT être relu ou appliqué par `bruno-tui`.

#### Scenario: Ouverture réussie
- **WHEN** la requête sélectionnée a un corps de réponse JSON non `null`,
  et l'utilisateur appuie sur `Ctrl+E`
- **THEN** le terminal est cédé à l'éditeur, qui affiche le corps mis en
  forme indentée ; à sa fermeture, l'interface se redessine
  intégralement et le fichier temporaire n'existe plus

#### Scenario: Corps texte affiché tel quel
- **WHEN** la requête sélectionnée a un corps de réponse qui est une
  chaîne de caractères, et l'utilisateur appuie sur `Ctrl+E`
- **THEN** l'éditeur externe affiche cette chaîne telle quelle, sans
  guillemets ajoutés ni mise en forme JSON

#### Scenario: Filtre actif ignoré
- **WHEN** un filtre jq est appliqué sur le corps de la réponse
  sélectionnée, et l'utilisateur appuie sur `Ctrl+E`
- **THEN** l'éditeur externe affiche le corps brut non filtré, pas le
  résultat du filtre

### Requirement: Sans réponse exploitable, aucun éditeur ne s'ouvre
`Ctrl+E` SHALL rester sans effet sur l'interface (aucun éditeur ouvert,
aucun fichier temporaire créé) quand : aucune requête n'est sélectionnée,
la requête sélectionnée n'a jamais été exécutée, ou son corps de réponse
est `null`. Un message de statut SHALL l'indiquer.

#### Scenario: Aucune sélection
- **WHEN** aucun nœud n'est sélectionné dans l'arbre, et l'utilisateur
  appuie sur `Ctrl+E`
- **THEN** aucun éditeur ne s'ouvre, un message de statut l'indique

#### Scenario: Requête jamais exécutée
- **WHEN** la requête sélectionnée n'a pas de résultat d'exécution, et
  l'utilisateur appuie sur `Ctrl+E`
- **THEN** aucun éditeur ne s'ouvre, un message de statut l'indique

#### Scenario: Corps de réponse `null`
- **WHEN** la requête sélectionnée a un résultat d'exécution dont le
  corps de réponse est `null`, et l'utilisateur appuie sur `Ctrl+E`
- **THEN** aucun éditeur ne s'ouvre, un message de statut l'indique

### Requirement: Choix de l'éditeur externe
L'éditeur lancé SHALL être, dans cet ordre : le contenu de la variable
d'environnement `VISUAL` si elle est définie et non vide, sinon le
contenu de `EDITOR` si elle est définie et non vide, sinon `nano`.

#### Scenario: VISUAL prioritaire
- **WHEN** `VISUAL=code -w` et `EDITOR=vim` sont définies, et
  l'utilisateur appuie sur `Ctrl+E` avec un corps de réponse disponible
- **THEN** `code -w` est lancé sur le fichier temporaire

#### Scenario: EDITOR à défaut de VISUAL
- **WHEN** `VISUAL` n'est pas définie, `EDITOR=vim` l'est, et
  l'utilisateur appuie sur `Ctrl+E` avec un corps de réponse disponible
- **THEN** `vim` est lancé sur le fichier temporaire

#### Scenario: nano par défaut
- **WHEN** ni `VISUAL` ni `EDITOR` ne sont définies, et l'utilisateur
  appuie sur `Ctrl+E` avec un corps de réponse disponible
- **THEN** `nano` est lancé sur le fichier temporaire
