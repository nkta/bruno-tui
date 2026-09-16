# response-tabs Specification

## Purpose

Organiser le contenu du panneau Réponse (`tui-shell`) en onglets
navigables — Corps, En-têtes, Tests — avec un bandeau de statut toujours
visible au-dessus, pour que le corps de la réponse, l'information la
plus consultée, reste immédiatement visible sans être précédé par les
en-têtes ou les résultats de tests.

## Requirements

### Requirement: Bandeau de statut et onglets du panneau Réponse
Quand le panneau Réponse affiche le résultat d'une requête exécutée
(`tui-shell`), l'interface SHALL présenter, dans l'ordre : un bandeau
de statut (verdict, statut de la réponse, temps de réponse), toujours
visible quel que soit l'onglet actif ; puis le contenu de l'un des
trois onglets — **Corps** (corps de la réponse, ou résultat filtré
quand `response-filter` est actif), **En-têtes** (en-têtes de la
réponse), **Tests** (assertions, tests, tests pré-requête, tests
post-réponse). L'onglet Corps SHALL être actif par défaut à chaque
nouvel affichage du panneau. Un panneau Réponse sans résultat
exploitable (message unique déjà défini par `tui-shell`) n'a pas
d'onglet.

#### Scenario: Onglet Corps actif par défaut
- **WHEN** une requête exécutée est sélectionnée pour la première fois
- **THEN** l'onglet Corps est actif et son contenu est affiché,
  précédé du bandeau verdict/statut/temps de réponse

#### Scenario: Bandeau visible quel que soit l'onglet
- **WHEN** l'onglet En-têtes ou l'onglet Tests est actif
- **THEN** le verdict, le statut et le temps de réponse restent
  visibles au-dessus du contenu de l'onglet

### Requirement: Changement d'onglet au clavier
Quand le panneau Réponse a le focus, hors saisie de recherche ou de
filtre, `h` ou `←` SHALL activer l'onglet précédent et `l` ou `→`
l'onglet suivant, dans l'ordre Corps, En-têtes, Tests, de façon
circulaire (après Tests, `l`/`→` revient à Corps ; avant Corps, `h`/`←`
va à Tests). Le défilement du panneau Réponse (`tui-shell`) MUST NOT
être affecté par un changement d'onglet autrement qu'en repartant du
haut du nouvel onglet.

#### Scenario: Cycle vers l'onglet suivant
- **WHEN** l'onglet Corps est actif et l'utilisateur appuie deux fois
  sur `l`
- **THEN** l'onglet En-têtes devient actif après le premier appui, puis
  Tests après le second

#### Scenario: Cycle circulaire vers l'avant
- **WHEN** l'onglet Tests est actif et l'utilisateur appuie sur `l`
- **THEN** l'onglet Corps redevient actif

#### Scenario: Cycle vers l'onglet précédent
- **WHEN** l'onglet Corps est actif et l'utilisateur appuie sur `h`
- **THEN** l'onglet Tests devient actif (cycle circulaire vers l'arrière)

### Requirement: Réinitialisation de l'onglet actif
L'onglet actif SHALL revenir à Corps quand le nœud sélectionné change
dans l'arbre, comme le reste de l'état d'affichage du panneau Réponse
(`tui-shell`). Ouvrir la saisie du filtre jq (`response-filter`) SHALL
également faire revenir l'onglet actif à Corps, s'il n'était pas déjà
actif, pour que le résultat du filtre soit immédiatement visible.

#### Scenario: Changement de sélection revient à l'onglet Corps
- **WHEN** l'onglet Tests est actif et l'utilisateur sélectionne une
  autre requête dans l'arbre
- **THEN** le panneau Réponse de la nouvelle sélection affiche l'onglet
  Corps

#### Scenario: Ouvrir le filtre bascule sur l'onglet Corps
- **WHEN** l'onglet En-têtes est actif et l'utilisateur ouvre la saisie
  du filtre jq
- **THEN** l'onglet Corps devient actif

### Requirement: Mise en forme du corps par défaut
Quand aucun filtre (`response-filter`) n'est actif ou validé sur la
requête affichée, l'onglet Corps SHALL mettre en forme un corps de
réponse structuré (objet ou tableau JSON) avec indentation lisible,
plutôt que de l'afficher sérialisé sur une seule ligne compacte, en
utilisant le même moteur jq que celui qui évalue un filtre explicite.
Un corps qui est une simple chaîne de caractères SHALL rester affiché
tel quel, sans guillemets ni caractère d'échappement ajoutés par cette
mise en forme.

#### Scenario: Corps structuré mis en forme sans filtre
- **WHEN** le corps de la réponse est `{"a": [1, 2], "b": null}` et
  aucun filtre n'est actif
- **THEN** l'onglet Corps l'affiche indenté sur plusieurs lignes plutôt
  que sur une seule

#### Scenario: Corps chaîne inchangé
- **WHEN** le corps de la réponse est la chaîne
  `<html><body>probe</body></html>` et aucun filtre n'est actif
- **THEN** l'onglet Corps l'affiche telle quelle, sans guillemets ni
  échappement ajoutés
