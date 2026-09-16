## MODIFIED Requirements

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
- **THEN** le panneau de réponse de la nouvelle sélection affiche son
  corps brut, sans filtre ni erreur résiduelle

#### Scenario: Retour sur la requête filtrée
- **WHEN** l'utilisateur revient sur une requête dont le filtre avait été
  appliqué puis quitté par changement de sélection
- **THEN** son corps brut est affiché, sans réappliquer l'ancien filtre
