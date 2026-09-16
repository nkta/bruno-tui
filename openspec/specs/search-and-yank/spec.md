# search-and-yank Specification

## Purpose

Permettre de retrouver rapidement un nœud ou un passage du détail par
motif de texte, et de copier un passage du détail vers le presse-papiers
du système, sans modifier le contrat de navigation déjà posé par
`tui-shell`.

## Requirements

### Requirement: Ouverture et saisie d'une recherche
Depuis l'arbre ou le détail, `/` SHALL ouvrir une ligne de saisie de motif
à la place de la barre d'état, associée au panneau qui avait le focus au
moment de l'appui. Pendant la saisie, tout caractère imprimable sans
modificateur MUST être ajouté au motif et `Retour arrière` MUST retirer le
dernier caractère ; les autres touches de navigation (flèches, `Tab`,
pagination) MUST être sans effet, à l'exception de `Ctrl+C` qui MUST
continuer à fermer l'application. `Entrée` SHALL valider le motif et
lancer la recherche décrite par les exigences suivantes ; un motif vide
SHALL annuler la saisie sans lancer de recherche, comme `Échap`. `Échap`
SHALL fermer la ligne de saisie sans modifier la sélection, le défilement
ni le dernier motif validé par une recherche précédente.

#### Scenario: Ouverture depuis l'arbre
- **WHEN** l'arbre a le focus et l'utilisateur appuie sur `/`
- **THEN** la barre d'état est remplacée par une ligne de saisie vide
- **AND** taper `post` puis `Retour arrière` laisse le motif `pos` affiché

#### Scenario: Annulation par Échap
- **WHEN** une recherche `griffe` est en cours de saisie et l'utilisateur
  appuie sur `Échap`
- **THEN** la ligne de saisie se ferme, la sélection et le défilement sont
  inchangés, et un motif validé par une recherche précédente reste
  disponible pour `n`/`N`

#### Scenario: Validation d'un motif vide
- **WHEN** l'utilisateur appuie sur `/` puis directement sur `Entrée`
- **THEN** la ligne de saisie se ferme sans qu'aucune recherche ne soit
  lancée

### Requirement: Recherche dans l'arbre
Une recherche validée depuis l'arbre SHALL parcourir tous les nœuds de la
collection, dossiers repliés compris, et comparer le motif au nom affiché
de chaque nœud (`display_name`) par sous-chaîne insensible à la casse.
Une correspondance trouvée après le nœud actuellement sélectionné (ordre
d'affichage, dossiers dépliés en profondeur d'abord) MUST devenir la
sélection ; le système MUST déplier tout dossier ancêtre nécessaire pour
que le nœud trouvé soit visible, comme le fait déjà la navigation entre
parent et enfant. Si aucun nœud après la sélection ne correspond, la
recherche MUST reprendre depuis le premier nœud de la collection
(recherche circulaire). Si aucun nœud de la collection ne correspond, la
sélection et le dépliage MUST rester inchangés et l'absence de résultat
MUST être signalée dans la barre d'état.

#### Scenario: Correspondance dans un dossier replié
- **WHEN** l'arbre de `parser-cases` est chargé, tous les dossiers
  repliés, et l'utilisateur cherche `inherit`
- **THEN** `Groupe` est déplié et sa requête `inherit` est sélectionnée

#### Scenario: Recherche circulaire
- **WHEN** `unknown-block` (dernier nœud du premier niveau) est
  sélectionné et l'utilisateur cherche `ping`
- **THEN** la recherche reprend depuis le début de la collection et
  sélectionne `ping`

#### Scenario: Aucune correspondance
- **WHEN** l'utilisateur cherche `zzz-inexistant` dans l'arbre
- **THEN** la sélection ne change pas et la barre d'état signale
  l'absence de résultat pour ce motif

### Requirement: Recherche dans le détail
Une recherche validée depuis le détail SHALL comparer le motif, par
sous-chaîne insensible à la casse, au texte de chaque ligne affichée du
nœud sélectionné. Une correspondance trouvée après la ligne actuellement
en haut du panneau MUST faire défiler le panneau pour rendre cette ligne
visible, au sommet du panneau quand le défilement le permet, sinon à la
position de défilement maximale déjà définie pour ce panneau. La
sous-chaîne trouvée sur cette ligne MUST être mise en évidence
visuellement jusqu'à la prochaine recherche ou le changement de nœud
sélectionné. Si aucune ligne après la position courante ne correspond, la
recherche MUST reprendre depuis la première ligne (recherche circulaire).
Si aucune ligne ne correspond, le défilement MUST rester inchangé et
l'absence de résultat MUST être signalée dans la barre d'état. Changer de
nœud sélectionné MUST effacer la mise en évidence en cours.

#### Scenario: Correspondance au-delà de l'écran
- **WHEN** le détail de `scripted` a le focus, défile en haut de son
  contenu, et l'utilisateur cherche `res.body.ok`
- **THEN** le panneau défile jusqu'à rendre visible la ligne contenant
  `res.body.ok: isTrue`, avec `res.body.ok` mis en évidence

#### Scenario: Recherche sans résultat dans le détail
- **WHEN** l'utilisateur cherche `ne-figure-nulle-part` dans le détail de
  `post-json`
- **THEN** le défilement ne change pas et la barre d'état signale
  l'absence de résultat

### Requirement: Répétition de la recherche
Une fois un motif validé au moins une fois, `n` SHALL rechercher la
prochaine correspondance dans le même sens que la dernière recherche, et
`N` SHALL rechercher dans le sens opposé, quel que soit le panneau ayant
le focus au moment de l'appui : `n`/`N` MUST utiliser le panneau associé
au motif enregistré (celui actif au moment de sa validation), pas le
panneau ayant le focus courant. `n`/`N` MUST NOT avoir d'effet tant
qu'aucun motif n'a été validé.

#### Scenario: n répète dans l'arbre depuis le détail
- **WHEN** une recherche `x` a été validée dans l'arbre, puis le focus
  passe au détail, et l'utilisateur appuie sur `n`
- **THEN** la sélection de l'arbre avance à la correspondance suivante,
  sans changer le focus

#### Scenario: N inverse le sens
- **WHEN** une recherche a fait passer la sélection de `simple-get.bru` à
  `no-seq.bru`, et l'utilisateur appuie sur `N`
- **THEN** la sélection revient à `simple-get.bru`

#### Scenario: Aucun motif validé
- **WHEN** l'utilisateur appuie sur `n` avant toute recherche validée dans
  cette session
- **THEN** rien ne change

### Requirement: Sélection visuelle dans le détail
Quand le détail a le focus, hors saisie de recherche, `v` SHALL activer
une sélection visuelle de lignes ancrée sur la ligne actuellement en haut
du panneau visible. Tant que la sélection est active, les touches de
défilement déjà définies par `tui-shell` (`↑`/`j`, `↓`/`k`, `Page
précédente`, `Page suivante`, `Début`, `Fin`) MUST continuer à déplacer le
défilement exactement comme sans sélection active, et MUST en plus étendre
la sélection jusqu'à la ligne en bas du panneau visible après ce
déplacement, de sorte que `Fin` porte la sélection jusqu'à la dernière
ligne du contenu. La sélection est l'ensemble des lignes entre l'ancrage
et cette borne, incluses, quel que soit leur ordre. Un second appui sur
`v`, ou `Échap`, SHALL annuler la sélection sans rien copier ni déplacer
le défilement. Changer de nœud sélectionné dans l'arbre MUST annuler toute
sélection visuelle en cours.

#### Scenario: Étendre jusqu'à la fin du contenu
- **WHEN** le détail de `scripted` a le focus, défilé en haut, et
  l'utilisateur appuie sur `v` puis sur `Fin`
- **THEN** la sélection couvre depuis la première ligne jusqu'à la
  dernière ligne du détail, y compris des lignes jamais atteignables comme
  seul sommet de panneau

#### Scenario: Annulation sans copie
- **WHEN** une sélection visuelle est active dans le détail et
  l'utilisateur appuie sur `Échap`
- **THEN** la sélection est annulée, le défilement ne change pas, et le
  presse-papiers n'est pas modifié

### Requirement: Copie vers le presse-papiers
Dans le détail, `y` SHALL copier du texte brut vers le presse-papiers du
système : le texte des lignes de la sélection visuelle si elle est active,
sinon la seule ligne actuellement en haut du panneau visible. Après une
copie réussie, toute sélection visuelle active MUST être levée sans
déplacer le défilement, et la barre d'état MUST confirmer la copie. Si le
presse-papiers du système est inaccessible, l'échec MUST être signalé
dans la barre d'état sans modifier la sélection ni le défilement, et
MUST NOT interrompre ni ralentir perceptiblement la boucle d'événements.
`y` dans l'arbre ou pendant une saisie de recherche SHALL être sans effet.

#### Scenario: Copie d'une ligne sans sélection
- **WHEN** le détail de `simple-get.bru` a le focus, sans sélection
  active, et l'utilisateur appuie sur `y`
- **THEN** la ligne actuellement en haut du panneau est copiée dans le
  presse-papiers du système et la barre d'état le confirme

#### Scenario: Copie d'une sélection
- **WHEN** une sélection visuelle couvre plusieurs lignes du détail et
  l'utilisateur appuie sur `y`
- **THEN** le texte de ces lignes, dans l'ordre d'affichage et séparé par
  des sauts de ligne, est copié dans le presse-papiers du système
- **AND** la sélection est levée

#### Scenario: Presse-papiers inaccessible
- **WHEN** aucun serveur d'affichage n'est accessible et l'utilisateur
  appuie sur `y`
- **THEN** l'application reste réactive, la barre d'état signale l'échec
  de la copie, et rien n'est modifié dans la sélection ou le défilement

### Requirement: Compatibilité avec la navigation existante
En dehors d'une saisie de recherche active, les touches déjà définies par
`tui-shell` (navigation de l'arbre, défilement du détail, focus, sortie)
MUST conserver exactement le comportement de leur spécification d'origine.
Les nouvelles touches (`/`, `n`, `N`, `v`, `y`) MUST NOT réutiliser une
touche déjà affectée par `tui-shell`.

#### Scenario: Navigation de l'arbre inchangée
- **WHEN** aucune recherche ni sélection visuelle n'est active
- **THEN** `↓`/`j`, `↑`/`k`, `→`/`l`/`Entrée`, `←`/`h`, `Début`/`g`,
  `Fin`/`G`, `Tab`, `Échap` et `q` se comportent comme décrit dans
  `tui-shell`

#### Scenario: Défilement du détail inchangé hors sélection
- **WHEN** le détail a le focus, sans sélection visuelle ni recherche
  active
- **THEN** `↑`/`k`, `↓`/`j`, `Page précédente`, `Page suivante`, `Début`
  et `Fin` défilent exactement comme décrit dans `tui-shell`, sans mise en
  évidence de ligne courante ajoutée par cette capacité
