## MODIFIED Requirements

### Requirement: Ligne de commande
Le binaire `bruno-tui` SHALL accepter au plus un argument positionnel, le
chemin de la collection ou d'un fichier ou dossier qu'elle contient, et
utiliser le répertoire courant en son absence. Il SHALL accepter `-h` ou
`--help`, qui affiche l'usage sur la sortie standard et termine avec le
code 0, et `-V` ou `--version`, qui affiche le nom et la version et termine
avec le code 0. Il SHALL accepter l'option répétable `--secret NOM` ou
`--secret NOM=CLÉ`, séparée de sa valeur par un espace ou par `=`
(`--secret=NOM=CLÉ`), qui déclare une variable secrète dont la résolution
est décrite par `secret-env-vars` ; `NOM` et `CLÉ` MUST être non vides et
MUST NOT contenir `=` ni d'espace blanc. La ligne de commande MUST NOT
accepter de valeur de secret. Une option inconnue, plus d'un argument
positionnel, `--secret` sans argument ou avec une forme invalide MUST
afficher l'usage sur la sortie d'erreur et terminer avec le code 2, sans
modifier l'état du terminal. Si le terminal ne peut pas passer en mode
plein écran (sortie non interactive), le binaire MUST afficher la raison
sur la sortie d'erreur et terminer avec le code 1.

#### Scenario: Chemin explicite
- **WHEN** l'utilisateur lance `bruno-tui tests/fixtures/collections/parser-cases`
- **THEN** l'interface s'ouvre et charge cette collection

#### Scenario: Sans argument
- **WHEN** l'utilisateur lance `bruno-tui` sans argument
- **THEN** l'interface charge la collection à partir du répertoire courant

#### Scenario: Aide
- **WHEN** l'utilisateur lance `bruno-tui --help`
- **THEN** l'usage est affiché sur la sortie standard, le code de sortie
  est 0 et aucune interface plein écran n'est ouverte

#### Scenario: Arguments invalides
- **WHEN** l'utilisateur lance `bruno-tui a b` ou `bruno-tui --inconnu`
- **THEN** l'usage est affiché sur la sortie d'erreur et le code de sortie
  est 2

#### Scenario: Sortie non interactive
- **WHEN** la sortie standard n'est pas un terminal
- **THEN** un message d'erreur est affiché sur la sortie d'erreur et le
  code de sortie est 1

#### Scenario: Déclarations de secrets
- **WHEN** l'utilisateur lance
  `bruno-tui --secret oktaClientSecret=OKTA_CLIENT_SECRET --secret token ./c`
- **THEN** l'interface charge `./c` avec deux variables secrètes
  déclarées : `oktaClientSecret` lue sous la clé `OKTA_CLIENT_SECRET`, et
  `token` lue sous les clés `token` puis `TOKEN`

#### Scenario: Déclaration de secret invalide
- **WHEN** l'utilisateur lance `bruno-tui --secret` sans argument, ou
  `bruno-tui --secret =X`, ou `bruno-tui --secret a=b=c`
- **THEN** l'usage est affiché sur la sortie d'erreur et le code de sortie
  est 2
