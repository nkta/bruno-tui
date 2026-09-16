## MODIFIED Requirements

### Requirement: Ouverture et saisie d'une recherche
Depuis l'arbre, le détail ou la réponse, `/` SHALL ouvrir une ligne de
saisie de motif à la place de la barre d'état, associée au panneau qui
avait le focus au moment de l'appui. Pendant la saisie, tout caractère
imprimable sans modificateur MUST être ajouté au motif et `Retour
arrière` MUST retirer le dernier caractère ; les autres touches de
navigation (flèches, `Tab`, pagination) MUST être sans effet, à
l'exception de `Ctrl+C` qui MUST continuer à fermer l'application.
`Entrée` SHALL valider le motif et lancer la recherche décrite par les
exigences suivantes ; un motif vide SHALL annuler la saisie sans lancer
de recherche, comme `Échap`. `Échap` SHALL fermer la ligne de saisie sans
modifier la sélection, le défilement ni le dernier motif validé par une
recherche précédente.

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

### Requirement: Recherche dans le détail
Une recherche validée depuis le détail ou depuis la réponse SHALL
comparer le motif, par sous-chaîne insensible à la casse, au texte de
chaque ligne affichée par le panneau depuis lequel elle a été validée —
le contenu de l'autre panneau n'est jamais comparé. Une correspondance
trouvée après la ligne actuellement en haut de ce panneau MUST le faire
défiler pour rendre cette ligne visible, au sommet du panneau quand le
défilement le permet, sinon à la position de défilement maximale déjà
définie pour ce panneau, indépendamment du défilement de l'autre
panneau. La sous-chaîne trouvée sur cette ligne MUST être mise en
évidence visuellement jusqu'à la prochaine recherche ou le changement de
nœud sélectionné. Si aucune ligne après la position courante ne
correspond, la recherche MUST reprendre depuis la première ligne
(recherche circulaire). Si aucune ligne ne correspond, le défilement MUST
rester inchangé et l'absence de résultat MUST être signalée dans la barre
d'état. Changer de nœud sélectionné MUST effacer la mise en évidence en
cours, dans le détail comme dans la réponse.

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

#### Scenario: Recherche dans la réponse
- **WHEN** la réponse a le focus sur une requête exécutée dont le corps
  contient un motif donné, et l'utilisateur valide une recherche sur ce
  motif
- **THEN** le panneau réponse défile jusqu'à la ligne correspondante, la
  met en évidence, et le défilement du détail reste inchangé

### Requirement: Sélection visuelle dans le détail
Quand le détail ou la réponse a le focus, hors saisie de recherche, `v`
SHALL activer une sélection visuelle de lignes, propre au panneau
focalisé, ancrée sur la ligne actuellement en haut de ce panneau. Tant
que la sélection est active, les touches de défilement déjà définies par
`tui-shell` (`↑`/`j`, `↓`/`k`, `Page précédente`, `Page suivante`,
`Début`, `Fin`) MUST continuer à déplacer le défilement de ce panneau
exactement comme sans sélection active, et MUST en plus étendre la
sélection jusqu'à la ligne en bas du panneau visible après ce
déplacement, de sorte que `Fin` porte la sélection jusqu'à la dernière
ligne du contenu de ce panneau. La sélection est l'ensemble des lignes
entre l'ancrage et cette borne, incluses, quel que soit leur ordre. Un
second appui sur `v`, ou `Échap`, SHALL annuler la sélection sans rien
copier ni déplacer le défilement. Changer de nœud sélectionné dans
l'arbre MUST annuler toute sélection visuelle en cours, dans le détail
comme dans la réponse. Une sélection visuelle active dans un panneau
n'affecte pas l'autre : chaque panneau porte la sienne, indépendamment.

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

#### Scenario: Sélections indépendantes entre détail et réponse
- **WHEN** une sélection visuelle est active dans le détail, et
  l'utilisateur bascule le focus vers la réponse puis y active `v`
- **THEN** les deux sélections coexistent indépendamment, chacune bornée
  à son propre panneau

### Requirement: Copie vers le presse-papiers
Dans le détail ou dans la réponse, `y` SHALL copier du texte brut vers le
presse-papiers du système, à partir du panneau qui a le focus : le texte
des lignes de la sélection visuelle de ce panneau si elle est active,
sinon la seule ligne actuellement en haut de ce panneau. Après une copie
réussie, toute sélection visuelle active du panneau concerné MUST être
levée sans déplacer son défilement, et la barre d'état MUST confirmer la
copie. Si le presse-papiers du système est inaccessible, l'échec MUST
être signalé dans la barre d'état sans modifier la sélection ni le
défilement, et MUST NOT interrompre ni ralentir perceptiblement la
boucle d'événements. `y` dans l'arbre ou pendant une saisie de recherche
SHALL être sans effet.

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
