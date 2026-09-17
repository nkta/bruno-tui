## MODIFIED Requirements

### Requirement: Bandeau de statut et onglets du panneau Réponse
Quand le panneau Réponse affiche le résultat d'une requête exécutée
(`tui-shell`), l'interface SHALL présenter, dans l'ordre : un bandeau
de statut (verdict, statut de la réponse, message d'erreur rapporté par
`bru` quand il existe, temps de réponse), toujours visible quel que soit
l'onglet actif ; puis le contenu de l'un des trois onglets — **Corps**
(corps de la réponse, ou résultat filtré quand `response-filter` est
actif), **En-têtes** (en-têtes de la réponse), **Tests** (assertions,
tests, tests pré-requête, tests post-réponse). Le message d'erreur MUST
être affiché dès que le résultat en porte un, quel que soit le statut de
la réponse, et MUST NOT être affiché quand le résultat n'en porte pas.
L'onglet Corps SHALL être actif par défaut à chaque nouvel affichage du
panneau. Un panneau Réponse sans résultat exploitable (message unique
déjà défini par `tui-shell`) n'a pas d'onglet.

#### Scenario: Onglet Corps actif par défaut
- **WHEN** une requête exécutée est sélectionnée pour la première fois
- **THEN** l'onglet Corps est actif et son contenu est affiché,
  précédé du bandeau verdict/statut/temps de réponse

#### Scenario: Bandeau visible quel que soit l'onglet
- **WHEN** l'onglet En-têtes ou l'onglet Tests est actif
- **THEN** le verdict, le statut et le temps de réponse restent
  visibles au-dessus du contenu de l'onglet

#### Scenario: Message d'erreur d'une requête en échec avant envoi
- **WHEN** la requête sélectionnée a été exécutée et `bru` rapporte
  qu'elle n'a pas été envoyée, avec l'erreur `Missing required
  environment variables: oktaClientSecret`
- **THEN** le bandeau affiche le verdict « échec », l'absence de réponse
  et le message `Missing required environment variables:
  oktaClientSecret`
- **AND** le message reste visible quel que soit l'onglet actif

#### Scenario: Pas de message d'erreur sans erreur
- **WHEN** la requête sélectionnée a reçu une réponse HTTP 200 et le
  résultat ne porte aucune erreur
- **THEN** le bandeau n'affiche aucune ligne d'erreur
