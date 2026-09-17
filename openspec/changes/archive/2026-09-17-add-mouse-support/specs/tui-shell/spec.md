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
accepter de valeur de secret. Il SHALL accepter l'option sans valeur
`--no-mouse`, éventuellement répétée, qui ouvre l'interface sans activer
la capture souris décrite par `mouse-support` ; l'usage MUST la
mentionner. Une option inconnue, plus d'un argument
positionnel, `--secret` sans argument ou avec une forme invalide, ou
`--no-mouse` suivi d'une valeur accolée (`--no-mouse=…`) MUST
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

#### Scenario: Sans souris
- **WHEN** l'utilisateur lance `bruno-tui --no-mouse ./c`
- **THEN** l'interface charge `./c` sans activer la capture souris

#### Scenario: Option sans souris avec valeur
- **WHEN** l'utilisateur lance `bruno-tui --no-mouse=1`
- **THEN** l'usage est affiché sur la sortie d'erreur et le code de sortie
  est 2

### Requirement: Navigation au clavier
Quand l'arbre a le focus, l'interface SHALL réagir aux touches suivantes :
- `↓` ou `j` : sélectionne le nœud visible suivant ; `↑` ou `k` : le
  précédent ; sans effet aux extrémités ;
- `Début` ou `g` : premier nœud visible ; `Fin` ou `G` : dernier ;
- `→`, `l` ou `Entrée` sur un dossier replié : le déplie ; sur un dossier
  déplié non vide : sélectionne son premier enfant ; sans effet sur une
  requête ou un nœud en erreur ;
- `←` ou `h` sur un dossier déplié : le replie ; sur tout autre nœud :
  sélectionne son dossier parent, sans effet au premier niveau.

Le nœud sélectionné MUST être visible dans le panneau de l'arbre après
chacune de ces touches et après tout changement de sélection, quelle que
soit la hauteur du terminal, et après un redimensionnement. Seul le
défilement de l'arbre à la molette (`mouse-support`) peut laisser le
nœud sélectionné hors de l'écran, jusqu'à la prochaine de ces touches ou
le prochain redimensionnement, qui le ramène à l'écran.
Replier un dossier MUST conserver la sélection sur ce dossier.

#### Scenario: Déplier puis entrer dans un dossier
- **WHEN** `Groupe` est sélectionné et replié, et l'utilisateur appuie deux
  fois sur `→`
- **THEN** `Groupe` est déplié et son premier enfant `x` est sélectionné

#### Scenario: Retour au parent
- **WHEN** `x` est sélectionné dans `Groupe` déplié et l'utilisateur appuie
  sur `←`
- **THEN** `Groupe` est sélectionné et reste déplié
- **AND** un second appui sur `←` replie `Groupe`

#### Scenario: Défilement d'un arbre plus haut que l'écran
- **WHEN** l'arbre compte plus de nœuds visibles que de lignes disponibles
  et l'utilisateur appuie sur `Fin`
- **THEN** le dernier nœud est sélectionné et visible à l'écran

#### Scenario: Extrémités
- **WHEN** le premier nœud est sélectionné et l'utilisateur appuie sur `↑`
- **THEN** la sélection ne change pas

#### Scenario: Retour de la sélection après la molette
- **WHEN** la molette a fait sortir le nœud sélectionné de l'écran et
  l'utilisateur appuie sur `↓`
- **THEN** le nœud suivant est sélectionné et visible à l'écran

### Requirement: Sortie et restauration du terminal
L'interface SHALL se fermer sur `q` quand l'arbre ou le détail a le focus,
et sur `Ctrl+C` en toutes circonstances, sauf si une session d'édition de
champ porte des modifications non sauvegardées au moment de l'appui : la
fermeture est alors précédée d'une demande de confirmation, et une
confirmation refusée annule la fermeture sans quitter l'application. À la
fermeture, le terminal MUST retrouver son état antérieur : capture souris
désactivée, mode brut désactivé, écran alternatif quitté, curseur
visible. Cette restauration MUST aussi avoir lieu si l'application
panique, avant l'affichage du message de panique. Le code de sortie MUST
être 0 après une fermeture volontaire.

#### Scenario: Sortie par q
- **WHEN** l'utilisateur appuie sur `q` et qu'aucune session d'édition
  modifiée n'est active
- **THEN** l'application se termine avec le code 0 et le terminal est
  restauré

#### Scenario: Panique
- **WHEN** l'application panique alors que l'interface est ouverte
- **THEN** le terminal est restauré avant que le message de panique ne
  soit affiché

#### Scenario: Sortie demandée avec une session modifiée
- **WHEN** une session d'édition porte une modification non sauvegardée
  et l'utilisateur appuie sur `q`
- **THEN** une confirmation est demandée avant toute fermeture du
  terminal
- **AND** si l'utilisateur annule, l'application reste ouverte

#### Scenario: Souris rendue au terminal
- **WHEN** l'application se ferme alors que la capture souris est active,
  volontairement ou sur panique
- **THEN** un glisser dans le terminal après la fermeture produit la
  sélection native du terminal et aucune séquence d'événement souris
  n'est écrite dans le shell
