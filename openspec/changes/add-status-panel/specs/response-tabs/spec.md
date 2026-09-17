## MODIFIED Requirements

### Requirement: Bandeau de statut et onglets du panneau Réponse
Quand le panneau Réponse affiche le résultat d'une requête exécutée
(`tui-shell`), l'interface SHALL présenter, dans l'ordre : le message
d'erreur rapporté par `bru` quand le résultat en porte un, toujours
visible quel que soit l'onglet actif ; puis la barre d'onglets et le
contenu de l'un des trois onglets — **Corps** (corps de la réponse, ou
résultat filtré quand `response-filter` est actif), **En-têtes**
(en-têtes de la réponse), **Tests** (assertions, tests, tests
pré-requête, tests post-réponse). Le verdict, le statut de la réponse
et le temps de réponse SHALL être portés par le panneau Statut
(`status-panel`) et MUST NOT être répétés dans le panneau Réponse. Le
message d'erreur MUST être affiché dès que le résultat en porte un,
quel que soit le statut de la réponse, et MUST NOT être affiché quand
le résultat n'en porte pas. L'onglet Corps SHALL être actif par défaut
à chaque nouvel affichage du panneau. Un panneau Réponse sans résultat
exploitable (message unique déjà défini par `tui-shell`) n'a pas
d'onglet.

#### Scenario: Onglet Corps actif par défaut
- **WHEN** une requête exécutée est sélectionnée pour la première fois
- **THEN** l'onglet Corps est actif et son contenu est affiché sous la
  barre d'onglets
- **AND** le panneau Réponse n'affiche ni verdict, ni statut, ni temps
  de réponse, portés par le panneau Statut

#### Scenario: Bandeau visible quel que soit l'onglet
- **WHEN** la requête sélectionnée porte un message d'erreur et
  l'onglet En-têtes ou l'onglet Tests est actif
- **THEN** le message d'erreur reste visible au-dessus de la barre
  d'onglets
- **AND** le verdict, le statut et le temps de réponse restent visibles
  dans le panneau Statut

#### Scenario: Message d'erreur d'une requête en échec avant envoi
- **WHEN** la requête sélectionnée a été exécutée et `bru` rapporte
  qu'elle n'a pas été envoyée, avec l'erreur `Missing required
  environment variables: oktaClientSecret`
- **THEN** le panneau Réponse affiche le message `Missing required
  environment variables: oktaClientSecret` au-dessus de la barre
  d'onglets
- **AND** le panneau Statut affiche le verdict « échec » et l'absence
  de réponse

#### Scenario: Pas de message d'erreur sans erreur
- **WHEN** la requête sélectionnée a reçu une réponse HTTP 200 et le
  résultat ne porte aucune erreur
- **THEN** le panneau Réponse commence directement par la barre
  d'onglets, sans ligne d'erreur
