## MODIFIED Requirements

### Requirement: Panneau de réponse
L'interface SHALL afficher, à côté du panneau de détail, un panneau
séparé montrant le résultat d'exécution de la requête sélectionnée tel
que défini par `request-execution`, quand un tel résultat existe ; le
verdict, le statut de la réponse et le temps de réponse de ce résultat
sont affichés par le panneau Statut (`status-panel`) placé au-dessus,
et le panneau Réponse en porte le reste (message d'erreur, corps,
en-têtes, vérifications) selon `response-tabs`. Si la sélection courante
n'a aucun résultat d'exécution disponible — aucune exécution encore
lancée pour cette requête, ou nœud sélectionné qui n'est pas une
requête — le panneau SHALL afficher un message unique l'indiquant,
plutôt que de rester vide sans indication. Le panneau Réponse MUST se
mettre à jour à chaque changement de sélection, comme le panneau de
détail. Le focus SHALL pouvoir se porter sur ce panneau indépendamment
du panneau de détail ; quand il a le focus, `↓`/`j` et `↑`/`k` MUST le
faire défiler d'une ligne, `Page suivante` et `Page précédente` d'une
hauteur de panneau, `Début` et `Fin` MUST aller au début et à la fin,
sans dépasser la fin de son propre contenu. Changer de nœud sélectionné
MUST ramener ce panneau en haut, indépendamment de l'état de défilement
du panneau de détail. Le panneau ayant le focus MUST être visuellement
distingué, comme pour tout autre panneau.

#### Scenario: Résultat affiché dans son propre panneau
- **WHEN** la requête `green` de `runner-probe` a été exécutée avec
  succès et est sélectionnée
- **THEN** le panneau Statut affiche son verdict, le code de statut
  HTTP et le temps de réponse, et le panneau Réponse affiche le corps
  de la réponse
- **AND** le panneau de détail n'affiche aucun de ces éléments

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
le chemin pendant le chargement), l'arbre, le détail et une colonne de
réponse côte à côte dans cet ordre, la colonne de réponse empilant le
panneau Statut (`status-panel`) au-dessus du panneau Réponse, et une
barre d'état rappelant les touches utiles au focus courant. La largeur
de la colonne de réponse SHALL être celle qu'occupait le panneau
Réponse seul, et l'arbre et le détail SHALL conserver toute la hauteur
disponible. Elle MUST se redessiner après un redimensionnement du
terminal. En dessous de 60 colonnes ou de 10 lignes, elle MUST afficher à
la place un message demandant d'agrandir le terminal, et MUST NOT
paniquer quelle que soit la taille, y compris 1×1.

#### Scenario: Colonne de réponse
- **WHEN** la collection est chargée sur un terminal de 100×30
- **THEN** l'arbre et le détail occupent toute la hauteur entre le titre
  et la barre d'état
- **AND** la colonne de droite affiche le panneau Statut puis, en
  dessous, le panneau Réponse, sur la même largeur

#### Scenario: Terminal trop petit
- **WHEN** le terminal fait 30 colonnes sur 8 lignes
- **THEN** l'interface affiche un message demandant d'agrandir le terminal
  au lieu de l'arbre

#### Scenario: Taille minimale absurde
- **WHEN** le terminal fait 1 colonne sur 1 ligne
- **THEN** le rendu se termine sans panique
