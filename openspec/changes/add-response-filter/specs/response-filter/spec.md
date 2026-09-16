## Purpose

Permettre de filtrer, avec la syntaxe jq, le corps JSON de la dernière
réponse d'une requête affichée dans le panneau de détail, pour explorer
un corps volumineux sans quitter l'interface, sans jamais modifier la
collection ni le rapport d'exécution sous-jacent.

## ADDED Requirements

### Requirement: Disponibilité du filtre
Le système SHALL proposer la saisie d'un filtre uniquement quand le nœud
sélectionné est une requête pour laquelle un résultat d'exécution existe,
dont la réponse a été reçue (ni erreur de connexion, ni requête ignorée)
et dont le corps n'est pas nul. Dans tout autre cas — aucune exécution,
requête en erreur ou ignorée, corps nul — la touche d'ouverture du filtre
MUST rester sans effet.

#### Scenario: Corps JSON disponible
- **WHEN** la requête sélectionnée a été exécutée avec succès et sa
  réponse porte le corps `{"a": [1, 2], "b": null}`
- **THEN** la touche d'ouverture du filtre ouvre la saisie

#### Scenario: Aucune réponse reçue
- **WHEN** la requête sélectionnée a échoué par erreur de connexion (pas
  de réponse)
- **THEN** la touche d'ouverture du filtre est sans effet

#### Scenario: Requête ignorée
- **WHEN** la requête sélectionnée a été sautée par un script et n'a donc
  aucun corps
- **THEN** la touche d'ouverture du filtre est sans effet

#### Scenario: Requête jamais exécutée
- **WHEN** la requête sélectionnée n'a aucun résultat d'exécution
- **THEN** la touche d'ouverture du filtre est sans effet

### Requirement: Saisie et validation du filtre
Le système SHALL permettre de composer un filtre au clavier, caractère
par caractère, avec correction (effacement du dernier caractère). La
validation MUST déclencher l'évaluation du filtre sur le corps de la
réponse. L'annulation MUST fermer la saisie sans modifier l'affichage du
corps. Valider un filtre vide MUST équivaloir à annuler.

#### Scenario: Composition et correction
- **WHEN** l'utilisateur ouvre la saisie, tape `.a.b`, efface les deux
  derniers caractères, puis tape `x`
- **THEN** le filtre composé au moment de la validation est `.a.x`

#### Scenario: Annulation
- **WHEN** l'utilisateur ouvre la saisie, tape un filtre, puis annule
- **THEN** l'affichage du corps reste celui d'avant l'ouverture de la
  saisie, inchangé

#### Scenario: Validation d'un filtre vide
- **WHEN** l'utilisateur ouvre la saisie et valide sans rien taper
- **THEN** le comportement est celui d'une annulation

### Requirement: Application du filtre et affichage du résultat
Le système SHALL évaluer un filtre jq valide sur le corps JSON de la
réponse et afficher le résultat à la place du corps brut, mis en forme
lisible. Un filtre produisant plusieurs valeurs de sortie MUST toutes les
afficher, dans l'ordre produit. Le filtre appliqué MUST rester visible à
l'écran tant que son résultat est affiché.

#### Scenario: Filtre extrayant une valeur
- **WHEN** le corps est `{"a": [1, 2], "b": null}` et le filtre validé
  est `.a`
- **THEN** l'affichage montre `[1, 2]` à la place du corps brut, et le
  filtre `.a` reste visible

#### Scenario: Filtre à plusieurs sorties
- **WHEN** le corps est `{"a": [1, 2], "b": null}` et le filtre validé
  est `.a[]`
- **THEN** l'affichage montre les deux valeurs `1` et `2`, dans cet ordre

#### Scenario: Corps non structuré
- **WHEN** le corps est la chaîne `"<html><body>probe</body></html>\n"`
  et le filtre validé est `. | length`
- **THEN** l'affichage montre la longueur de la chaîne, jq traitant une
  chaîne comme une valeur JSON à part entière

### Requirement: Erreur de filtre non fatale
Le système SHALL afficher un message d'erreur, à la place du résultat,
quand un filtre ne compile pas ou que son évaluation échoue, sans jamais
interrompre l'application ni faire disparaître le corps original. Rouvrir
la saisie MUST permettre de corriger le filtre sans perdre l'accès au
corps brut.

#### Scenario: Filtre syntaxiquement invalide
- **WHEN** l'utilisateur valide le filtre `.a.b |` (opérateur sans membre
  droit)
- **THEN** un message d'erreur de syntaxe est affiché, aucune sortie
  n'est produite, et l'application reste utilisable

#### Scenario: Filtre valide mais incompatible avec la valeur
- **WHEN** le corps est la chaîne `"<html><body>probe</body></html>\n"`
  et le filtre validé est `map(.)` (attend un tableau ou un objet)
- **THEN** un message d'erreur d'exécution est affiché, sans plantage

#### Scenario: Correction après erreur
- **WHEN** un filtre en erreur est affiché et l'utilisateur rouvre la
  saisie puis valide un filtre correct
- **THEN** le résultat du nouveau filtre remplace le message d'erreur

### Requirement: Filtre volatile, sans effet de bord
Le système MUST NOT persister un filtre au-delà de la sélection courante :
changer le nœud sélectionné dans l'arbre SHALL réinitialiser
immédiatement l'affichage au corps brut de la nouvelle sélection, filtre
et éventuelle erreur effacés. Le système MUST NOT écrire un filtre, son
résultat ou une erreur de filtre sur le disque, ni modifier le fichier
`.bru` de la requête ou le rapport d'exécution reçu.

#### Scenario: Changement de sélection efface le filtre
- **WHEN** un filtre est appliqué sur la requête sélectionnée et
  l'utilisateur sélectionne une autre requête
- **THEN** le panneau de détail de la nouvelle sélection affiche son
  corps brut, sans filtre ni erreur résiduelle

#### Scenario: Retour sur la requête filtrée
- **WHEN** l'utilisateur revient sur une requête dont le filtre avait été
  appliqué puis quitté par changement de sélection
- **THEN** son corps brut est affiché, sans réappliquer l'ancien filtre
