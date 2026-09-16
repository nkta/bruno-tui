# tui-shell Specification

## Purpose

Fournir l'interface plein écran de bruno-tui : lancer l'application sur une
collection Bruno, la charger sans bloquer la saisie, et la rendre
explorable au clavier avec un détail en lecture seule de chaque nœud.

## Requirements

### Requirement: Ligne de commande
Le binaire `bruno-tui` SHALL accepter au plus un argument positionnel, le
chemin de la collection ou d'un fichier ou dossier qu'elle contient, et
utiliser le répertoire courant en son absence. Il SHALL accepter `-h` ou
`--help`, qui affiche l'usage sur la sortie standard et termine avec le
code 0, et `-V` ou `--version`, qui affiche le nom et la version et termine
avec le code 0. Une option inconnue ou plus d'un argument positionnel MUST
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

### Requirement: Chargement non bloquant
L'interface SHALL s'afficher immédiatement avec un état « chargement »
indiquant le chemin demandé, puis charger la collection hors du fil qui
traite la saisie et le rendu. Pendant le chargement, la touche de sortie
MUST rester effective. À la fin du chargement, l'interface MUST afficher
l'arbre de la collection et son nom. En cas d'échec du chargement (chemin
hors de toute collection, `bruno.json` invalide, erreur d'accès),
l'interface MUST afficher un état d'erreur avec la raison, et rester
ouverte jusqu'à ce que l'utilisateur la quitte.

#### Scenario: Chargement lent
- **WHEN** le chargement de la collection prend plusieurs secondes
- **THEN** l'état « chargement » est affiché pendant ce temps
- **AND** un appui sur `q` quitte l'application sans attendre la fin du
  chargement

#### Scenario: Collection chargée
- **WHEN** le chargement de `parser-cases` se termine
- **THEN** le titre affiche `parser-cases` et l'arbre affiche les enfants
  de la racine

#### Scenario: Pas une collection
- **WHEN** le chemin fourni n'est dans aucune collection Bruno
- **THEN** l'interface affiche un état d'erreur contenant le chemin et
  la raison, et ne se ferme pas d'elle-même

### Requirement: Arbre de la collection
L'interface SHALL afficher les nœuds de la collection sous forme d'arbre
indenté, dans l'ordre fourni par le chargement, sans le réordonner. Chaque
dossier MUST afficher son nom et un indicateur replié ou déplié ; chaque
requête MUST afficher sa méthode HTTP et son nom ; chaque nœud en erreur
MUST afficher son nom de fichier avec un marqueur d'erreur. Un dossier dont
le fichier de méta-données est invalide MUST porter le même marqueur tout
en restant dépliable. Tous les dossiers MUST être repliés au chargement et
le premier nœud MUST être sélectionné. Une collection sans aucun nœud MUST
afficher une mention « collection vide ».

#### Scenario: Premier affichage de parser-cases
- **WHEN** la collection `parser-cases` vient d'être chargée
- **THEN** l'arbre affiche au premier niveau, dans cet ordre, `Groupe`,
  `ping`, `post-json`, `scripted`, `badmeta`, `broken.bru`, `misc`,
  `multiline`, `no-method.bru`, `no-seq`, `unknown-block` (nom déclaré
  s'il existe, sinon nom du dossier ou du fichier)
- **AND** `Groupe` est sélectionné et replié
- **AND** `broken.bru`, `no-method.bru` et `badmeta` portent le marqueur
  d'erreur

#### Scenario: Requête affichée avec sa méthode
- **WHEN** l'arbre affiche la requête `simple-get.bru` dont le nom déclaré
  est `ping`
- **THEN** sa ligne contient `GET` et `ping`

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

Le nœud sélectionné MUST rester visible dans le panneau de l'arbre, quelle
que soit la hauteur du terminal, y compris après un redimensionnement.
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

### Requirement: Panneau de détail
L'interface SHALL afficher, à côté de l'arbre, le détail en lecture seule
du nœud sélectionné, mis à jour à chaque changement de sélection :
- **requête** : nom, chemin relatif, méthode, URL, mode d'auth déclaré,
  en-têtes, paramètres de requête et de chemin avec leurs entrées
  désactivées signalées comme telles, type et contenu du corps, et
  présence ou absence des blocs de script pré-requête, post-réponse, de
  tests et d'assertions ;
- **dossier** : nom, chemin relatif, `seq`, nombre d'enfants, et état de
  son fichier de méta-données (absent, présent avec la présence d'auth,
  d'en-têtes, de scripts et de tests, ou en erreur avec sa raison) ;
- **nœud en erreur** : chemin relatif et raison de l'erreur.

Les valeurs MUST être affichées telles qu'écrites dans le fichier : aucun
motif `{{variable}}` n'est résolu et le mode d'auth `inherit` est affiché
comme tel.

#### Scenario: Détail d'une requête POST JSON
- **WHEN** `post-json` est sélectionné
- **THEN** le détail affiche `POST`, l'URL `https://{{host}}/items`,
  l'en-tête `Content-Type`, le type de corps `json` et le corps JSON
  désindenté contenant `"s": "}"`

#### Scenario: Entrée désactivée
- **WHEN** `scripted` est sélectionné
- **THEN** l'en-tête `X-Debug` est affiché avec une mention « désactivé »
- **AND** le détail indique la présence des scripts pré-requête et
  post-réponse, des tests et des assertions

#### Scenario: Auth héritée
- **WHEN** `inherit` est sélectionné dans `Groupe`
- **THEN** le détail affiche le mode d'auth `inherit`, sans jeton ni mode
  emprunté au dossier

#### Scenario: Nœud en erreur
- **WHEN** `broken.bru` est sélectionné
- **THEN** le détail affiche le chemin `broken.bru` et une raison
  mentionnant le bloc `headers` non fermé et sa ligne

#### Scenario: Dossier à méta-données invalides
- **WHEN** `badmeta` est sélectionné
- **THEN** le détail affiche que son fichier de méta-données est en erreur,
  avec la raison, et le nombre d'enfants 1

### Requirement: Défilement du détail
L'interface SHALL permettre de basculer le focus entre l'arbre, le
détail et la réponse avec `Tab`, dans cet ordre, qui reboucle sur
l'arbre après la réponse, et de revenir à l'arbre avec `Échap` depuis le
détail ou depuis la réponse, sauf si une session d'édition de champ
(`field-editing`) est active sur la requête affichée dans le détail :
dans ce cas, `Échap` referme d'abord cette session (avec confirmation si
elle porte des modifications non sauvegardées) et laisse le focus sur le
détail ; un second `Échap`, une fois la session fermée, rend le focus à
l'arbre comme précédemment. Quand le détail a le focus, `↓`/`j` et
`↑`/`k` MUST le faire défiler d'une ligne, `Page suivante` et `Page
précédente` d'une hauteur de panneau, `Début` et `Fin` MUST aller au
début et à la fin. Le défilement MUST NOT dépasser la fin du contenu.
Changer de nœud sélectionné MUST ramener le détail en haut. Le panneau
ayant le focus MUST être visuellement distingué.

#### Scenario: Tab cycle sur les trois panneaux
- **WHEN** l'arbre a le focus et l'utilisateur appuie trois fois de
  suite sur `Tab`
- **THEN** le focus passe au détail après le premier appui, à la réponse
  après le second, et revient à l'arbre après le troisième

#### Scenario: Corps plus long que le panneau
- **WHEN** le détail d'une requête dépasse la hauteur du panneau, que le
  détail a le focus et que l'utilisateur appuie sur `Fin`
- **THEN** la dernière ligne du détail est visible
- **AND** un appui supplémentaire sur `↓` ne change pas l'affichage

#### Scenario: Retour à l'arbre
- **WHEN** le détail a le focus, qu'aucune session d'édition n'est
  active, et l'utilisateur appuie sur `Échap`
- **THEN** l'arbre reprend le focus et `↓` change la sélection

#### Scenario: Échap referme d'abord la session d'édition
- **WHEN** une session d'édition sans modification est active sur la
  requête affichée et l'utilisateur appuie sur `Échap`
- **THEN** la session se ferme, le focus reste sur le détail, en lecture
  seule
- **AND** un second `Échap` rend alors le focus à l'arbre

### Requirement: Panneau de réponse
L'interface SHALL afficher, à côté du panneau de détail, un panneau
séparé montrant le résultat d'exécution de la requête sélectionnée tel
que défini par `request-execution`, quand un tel résultat existe. Si la
sélection courante n'a aucun résultat d'exécution disponible — aucune
exécution encore lancée pour cette requête, ou nœud sélectionné qui
n'est pas une requête — le panneau SHALL afficher un message unique
l'indiquant, plutôt que de rester vide sans indication. Le panneau
Réponse MUST se mettre à jour à chaque changement de sélection, comme le
panneau de détail. Le focus SHALL pouvoir se porter sur ce panneau
indépendamment du panneau de détail ; quand il a le focus, `↓`/`j` et
`↑`/`k` MUST le faire défiler d'une ligne, `Page suivante` et `Page
précédente` d'une hauteur de panneau, `Début` et `Fin` MUST aller au
début et à la fin, sans dépasser la fin de son propre contenu. Changer de
nœud sélectionné MUST ramener ce panneau en haut, indépendamment de
l'état de défilement du panneau de détail. Le panneau ayant le focus
MUST être visuellement distingué, comme pour tout autre panneau.

#### Scenario: Résultat affiché dans son propre panneau
- **WHEN** la requête `green` de `runner-probe` a été exécutée avec
  succès et est sélectionnée
- **THEN** le panneau Réponse affiche son verdict, le code de statut
  HTTP, le corps de la réponse et le temps de réponse
- **AND** le panneau de détail ne les affiche pas

#### Scenario: Aucun résultat disponible
- **WHEN** une requête sans aucune exécution est sélectionnée
- **THEN** le panneau Réponse affiche un message unique indiquant
  l'absence de résultat, plutôt qu'un contenu vide

#### Scenario: Sélection d'un dossier ou d'un nœud en erreur
- **WHEN** un dossier ou un nœud en erreur est sélectionné
- **THEN** le panneau Réponse affiche le même message que pour une
  requête sans résultat

#### Scenario: Défilement indépendant du détail
- **WHEN** le détail est défilé au milieu de son contenu, et
  l'utilisateur bascule le focus sur la réponse puis la fait défiler
- **THEN** le défilement du détail reste inchangé pendant que celui de
  la réponse évolue

### Requirement: Disposition et barre d'état
L'interface SHALL afficher un titre contenant le nom de la collection (ou
le chemin pendant le chargement), l'arbre, le détail et la réponse côte à
côte dans cet ordre, et une barre d'état rappelant les touches utiles au
focus courant. Elle MUST se redessiner après un redimensionnement du
terminal. En dessous de 60 colonnes ou de 10 lignes, elle MUST afficher à
la place un message demandant d'agrandir le terminal, et MUST NOT
paniquer quelle que soit la taille, y compris 1×1.

#### Scenario: Terminal trop petit
- **WHEN** le terminal fait 30 colonnes sur 8 lignes
- **THEN** l'interface affiche un message demandant d'agrandir le terminal
  au lieu de l'arbre

#### Scenario: Taille minimale absurde
- **WHEN** le terminal fait 1 colonne sur 1 ligne
- **THEN** le rendu se termine sans panique

### Requirement: Sortie et restauration du terminal
L'interface SHALL se fermer sur `q` quand l'arbre ou le détail a le focus,
et sur `Ctrl+C` en toutes circonstances, sauf si une session d'édition de
champ porte des modifications non sauvegardées au moment de l'appui : la
fermeture est alors précédée d'une demande de confirmation, et une
confirmation refusée annule la fermeture sans quitter l'application. À la
fermeture, le terminal MUST retrouver son état antérieur : mode brut
désactivé, écran alternatif quitté, curseur visible. Cette restauration
MUST aussi avoir lieu si l'application panique, avant l'affichage du
message de panique. Le code de sortie MUST être 0 après une fermeture
volontaire.

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

### Requirement: Aucun effet de bord
Pendant que l'interface est ouverte, l'application MUST NOT écrire de
fichier, lancer de processus externe (dont `bru`), ni écrire sur la sortie
standard ou d'erreur autrement que par le rendu de l'interface. Aucune
valeur issue de la collection (en-têtes, corps, variables) MUST NOT être
journalisée.

#### Scenario: Exploration de la collection
- **WHEN** l'utilisateur ouvre `parser-cases`, déplie tous les dossiers et
  consulte le détail de chaque nœud
- **THEN** aucun fichier n'est créé ni modifié et aucun processus n'est
  lancé
