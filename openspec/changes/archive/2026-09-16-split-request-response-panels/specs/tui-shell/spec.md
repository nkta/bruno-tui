## MODIFIED Requirements

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

## ADDED Requirements

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
