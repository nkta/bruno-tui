## Purpose

Rendre l'interface de bruno-tui utilisable à la souris — focus et
sélection au clic, défilement à la molette, saisie d'un champ du détail,
sélection et copie de lignes — sans retirer aucune commande clavier ni
empêcher l'usage de la sélection native du terminal.

## ADDED Requirements

### Requirement: Capture souris et désactivation
Au démarrage de l'interface plein écran, le système SHALL activer la
capture des événements souris du terminal, sauf si l'option `--no-mouse`
(`tui-shell`) a été fournie. Hors de toute saisie de texte (dont l'état
Saisie de `field-editing`), `M` SHALL basculer la capture pendant la
session : la désactiver si elle est active, l'activer sinon, et la barre
d'état MUST confirmer le nouvel état (souris activée ou désactivée, avec
un rappel que la sélection native du terminal est disponible quand la
capture est désactivée). Désactiver la capture MUST annuler tout glisser
en cours sans rien copier. Quand la capture est désactivée, aucun
événement souris MUST NOT modifier l'état de l'interface. Si l'activation
ou la désactivation échoue, l'échec MUST être signalé dans la barre
d'état, l'application MUST rester ouverte et l'état de capture retenu
MUST rester celui d'avant la tentative. La capture MUST être désactivée
à la fermeture du terminal dans tous les cas prévus par `tui-shell`, y
compris en cas de panique.

#### Scenario: Capture active par défaut
- **WHEN** l'utilisateur lance `bruno-tui tests/fixtures/collections/parser-cases`
- **THEN** la capture souris est active et un clic sur une ligne de
  l'arbre sélectionne ce nœud

#### Scenario: Lancement sans souris
- **WHEN** l'utilisateur lance `bruno-tui --no-mouse`
- **THEN** la capture souris n'est jamais activée et la sélection native
  du terminal fonctionne comme sans bruno-tui

#### Scenario: Bascule pendant la session
- **WHEN** la capture est active, aucune saisie n'est en cours, et
  l'utilisateur appuie sur `M`
- **THEN** la capture est désactivée et la barre d'état l'indique
- **AND** un second appui sur `M` la réactive et la barre d'état l'indique

#### Scenario: M pendant une saisie de recherche
- **WHEN** une saisie de recherche est en cours et l'utilisateur tape `M`
- **THEN** `M` est ajouté au motif et l'état de la capture ne change pas

#### Scenario: M pendant la saisie d'un champ
- **WHEN** l'état Saisie est actif sur l'URL et l'utilisateur tape `M`
- **THEN** `M` est inséré dans l'URL et l'état de la capture ne change
  pas

### Requirement: Événements souris sans effet
Les événements souris SHALL être ignorés, sans aucun effet sur l'état de
l'interface, dans chacune des situations suivantes : collection en cours
de chargement ou en erreur ; terminal sous la taille minimale de
`tui-shell` ; demande de confirmation en attente ; saisie de recherche,
de filtre ou de variable secrète en cours ; panneau de diagnostics,
d'historique, d'environnements ou de variables secrètes ouvert. Un
événement situé hors de l'arbre, du détail et de la réponse — titre,
barre d'état, ou panneau Statut (`status-panel`) —, ainsi que les
boutons autres que le bouton gauche et le défilement horizontal, SHALL
être ignorés de même. En particulier, un clic ou un cran de molette sur
le panneau Statut MUST NOT donner le focus à un panneau, ni valider ou
annuler une saisie de champ en cours.

#### Scenario: Clic pendant une confirmation
- **WHEN** une confirmation de fermeture de session modifiée est affichée
  et l'utilisateur clique sur une ligne de l'arbre
- **THEN** la sélection, le focus et la confirmation sont inchangés

#### Scenario: Clic pendant une saisie de recherche
- **WHEN** une saisie de recherche est en cours et l'utilisateur clique
  dans le panneau Réponse
- **THEN** la saisie reste ouverte avec le même motif et le focus ne
  change pas

#### Scenario: Clic sur la barre d'état
- **WHEN** l'utilisateur clique sur la barre d'état
- **THEN** rien ne change

#### Scenario: Clic sur le panneau Statut
- **WHEN** l'arbre a le focus, une requête exécutée est sélectionnée, et
  l'utilisateur clique dans le panneau Statut
- **THEN** l'arbre garde le focus et rien d'autre ne change

### Requirement: Focus et sélection au clic
Un clic gauche dans l'arbre, le détail ou la réponse SHALL donner le
focus à ce panneau. Dans l'arbre, un clic sur une ligne de nœud SHALL de
plus demander la sélection de ce nœud, avec exactement les mêmes effets
et les mêmes restrictions qu'une sélection au clavier (`tui-shell`,
`field-editing`) : mise à jour du détail et de la réponse, retour en haut
des deux panneaux, annulation des sélections visuelles et mises en
évidence. Un clic sur le nœud déjà sélectionné, si c'est un dossier,
SHALL le déplier s'il est replié et le replier s'il est déplié, en
conservant la sélection sur ce dossier. Un clic sur la bordure d'un
panneau, ou sous la dernière ligne de l'arbre, SHALL seulement donner le
focus à ce panneau. La ligne désignée MUST être celle affichée à la
position du clic, en tenant compte du défilement courant du panneau.

#### Scenario: Sélection d'une requête au clic
- **WHEN** `parser-cases` vient d'être chargée, l'arbre a le focus sur
  `Groupe`, et l'utilisateur clique sur la ligne `post-json`
- **THEN** `post-json` est sélectionné et le détail affiche `POST`

#### Scenario: Focus au clic
- **WHEN** l'arbre a le focus et l'utilisateur clique dans le panneau
  Réponse
- **THEN** la réponse a le focus et `↓` fait défiler la réponse

#### Scenario: Dépliage au clic
- **WHEN** `Groupe` est sélectionné et replié, et l'utilisateur clique
  sur sa ligne
- **THEN** `Groupe` est déplié, reste sélectionné, et son enfant `x`
  apparaît dans l'arbre
- **AND** un nouveau clic sur `Groupe` le replie

#### Scenario: Clic dans un arbre défilé
- **WHEN** l'arbre a été défilé de sorte que sa première ligne visible
  est le cinquième nœud visible, et l'utilisateur clique sur la première
  ligne du panneau
- **THEN** le cinquième nœud visible est sélectionné

### Requirement: Clic pendant une session d'édition
Quand une session d'édition (`field-editing`) est ouverte, le système
SHALL traiter un appui du bouton gauche ainsi :
- si l'état Saisie est actif et que l'appui tombe ailleurs que sur les
  lignes du champ en cours de saisie (autre champ, autre ligne du détail,
  arbre, réponse), la saisie MUST d'abord être validée exactement comme
  par `Tab`, puis l'appui produit son effet ; un appui sur les lignes du
  champ en cours de saisie MUST être sans effet, ni sur le texte, ni sur
  le curseur de texte ;
- un changement de focus vers l'arbre ou la réponse MUST laisser la
  session ouverte, dans l'état Sélection de champ, comme `Tab` ;
- une sélection au clic d'un autre nœud dans l'arbre MUST respecter
  l'exigence de `field-editing` sur le changement de requête pendant une
  session modifiée : si la session porte des modifications non
  enregistrées (y compris celles issues de la validation ci-dessus), la
  sélection reste inchangée, la session reste ouverte avec ses
  modifications et le message invitant à enregistrer ou fermer la
  session s'affiche, le focus étant tout de même donné à l'arbre ; sans
  modification non enregistrée, la session se ferme sans confirmation et
  le nœud cliqué est sélectionné.

Aucun clic MUST NOT déclencher d'écriture sur disque ni fermer une
session portant des modifications non enregistrées.

#### Scenario: Session sans modification
- **WHEN** une session sans modification est ouverte sur `post-json` et
  l'utilisateur clique sur la ligne `ping` de l'arbre
- **THEN** la session se ferme, `ping` est sélectionné et l'arbre a le
  focus

#### Scenario: Session modifiée
- **WHEN** une session porte une modification validée non enregistrée et
  l'utilisateur clique sur la ligne `ping` de l'arbre
- **THEN** l'arbre a le focus, la sélection ne change pas, la session et
  sa modification sont intactes, et le message invitant à enregistrer ou
  fermer la session s'affiche

#### Scenario: Saisie en cours et clic dans l'arbre
- **WHEN** l'état Saisie est actif sur l'URL de `post-json` avec un
  caractère ajouté, et l'utilisateur clique sur la ligne `ping` de
  l'arbre
- **THEN** l'URL modifiée est validée en mémoire, la session est marquée
  modifiée, dans l'état Sélection de champ, le fichier n'est pas écrit
- **AND** `post-json` reste sélectionné, l'arbre a le focus et le message
  invitant à enregistrer ou fermer la session s'affiche

#### Scenario: Saisie en cours et clic dans la réponse
- **WHEN** l'état Saisie est actif sur un en-tête, sans changement du
  texte, et l'utilisateur clique dans le panneau Réponse
- **THEN** la saisie se termine sans marquer la session modifiée, la
  session reste ouverte dans l'état Sélection de champ et la réponse a
  le focus

#### Scenario: Clic sur le champ en cours de saisie
- **WHEN** l'état Saisie est actif sur l'URL, le curseur de texte au
  milieu, et l'utilisateur clique sur la ligne de l'URL
- **THEN** le texte, le curseur de texte et l'état Saisie sont inchangés

### Requirement: Défilement à la molette
La molette SHALL faire défiler le panneau situé sous le pointeur, qu'il
ait le focus ou non, sans changer le focus ni le nœud sélectionné : de
trois lignes par cran vers le haut ou vers le bas, bornées comme le
défilement clavier de ce panneau. Dans le détail et la réponse, ce
défilement MUST avoir le même effet que le défilement clavier
équivalent de `tui-shell`, y compris sur une sélection visuelle clavier
active (`search-and-yank`), mais MUST NOT déplacer le curseur de champ
d'une session d'édition, ni terminer ou modifier une saisie en cours.
Dans l'arbre, la molette SHALL déplacer la première ligne affichée,
entre la première ligne et la dernière position qui remplit encore le
panneau, sans changer la sélection ; le nœud sélectionné peut alors
sortir de l'écran jusqu'à la prochaine navigation au clavier.

#### Scenario: Défilement de la réponse sans focus
- **WHEN** l'arbre a le focus, la requête sélectionnée a une réponse plus
  longue que le panneau, et l'utilisateur tourne la molette d'un cran
  vers le bas au-dessus du panneau Réponse
- **THEN** la réponse défile de trois lignes et l'arbre garde le focus

#### Scenario: Butée en fin de contenu
- **WHEN** le détail est déjà défilé jusqu'à sa fin et l'utilisateur
  tourne la molette vers le bas au-dessus du détail
- **THEN** l'affichage du détail ne change pas

#### Scenario: Molette dans l'arbre
- **WHEN** l'arbre compte plus de nœuds visibles que de lignes
  disponibles, le premier nœud est sélectionné, et l'utilisateur tourne
  la molette vers le bas au-dessus de l'arbre
- **THEN** l'arbre défile de trois lignes et le premier nœud reste
  sélectionné
- **AND** un appui ensuite sur `↓` sélectionne le deuxième nœud et le
  rend visible

#### Scenario: Molette pendant une session d'édition
- **WHEN** une session d'édition est ouverte avec le curseur de champ sur
  l'URL et l'utilisateur tourne la molette au-dessus du détail
- **THEN** le détail défile et le curseur de champ reste sur l'URL

### Requirement: Saisie d'un champ au clic
Quand le détail affiche une requête, un clic gauche (appui et relâchement
sur la même ligne, sans glisser vers une autre ligne) sur une ligne
appartenant à un champ éditable au sens de `field-editing` (URL, valeur
d'un en-tête, d'un paramètre de requête ou de chemin, lignes d'un corps
éditable) qui n'est pas le champ en cours de saisie SHALL : donner le
focus au détail ; ouvrir une session d'édition sur cette requête si
aucune n'est ouverte, avec le même instantané de fraîcheur que `Entrée`
ou `e` ; placer le curseur de champ sur ce champ ; puis commencer la
saisie de ce champ exactement comme `Entrée` dans l'état Sélection de
champ, le curseur de texte en fin de valeur (valeur validée la plus
récente, sinon chargée), l'état devenant Saisie. Une saisie en cours sur
un autre champ est d'abord validée selon l'exigence « Clic pendant une
session d'édition ». Un clic sur une ligne du détail qui n'appartient à
aucun champ éditable SHALL seulement donner le focus au détail (après
validation d'une éventuelle saisie en cours) ; il MUST NOT ouvrir de
session. Sur un dossier ou un nœud en erreur, un clic dans le détail
SHALL seulement donner le focus. Le curseur de texte MUST NOT être placé
à la colonne cliquée.

#### Scenario: Clic sur l'URL sans session
- **WHEN** `post-json` est sélectionné, aucune session n'est ouverte, et
  l'utilisateur clique sur la ligne de l'URL dans le détail
- **THEN** le détail a le focus, une session d'édition est ouverte,
  l'état Saisie est actif sur l'URL, curseur de texte en fin de
  `https://{{host}}/items`

#### Scenario: Clic sur un champ en sélection de champ
- **WHEN** une session est dans l'état Sélection de champ, le curseur de
  champ sur l'URL, et l'utilisateur clique sur la ligne de l'en-tête
  `Content-Type`
- **THEN** l'état Saisie est actif sur la valeur de `Content-Type`

#### Scenario: Passage d'un champ à un autre
- **WHEN** l'état Saisie est actif sur l'URL après y avoir tapé un
  caractère, et l'utilisateur clique sur la ligne de l'en-tête
  `Content-Type`
- **THEN** l'URL conserve le caractère tapé, la session est marquée
  modifiée, et l'état Saisie est actif sur la valeur de `Content-Type`

#### Scenario: Clic sur un libellé de section
- **WHEN** aucune session n'est ouverte et l'utilisateur clique sur le
  titre de section des en-têtes dans le détail
- **THEN** le détail a le focus et aucune session n'est ouverte

#### Scenario: Corps non éditable
- **WHEN** la requête sélectionnée a un corps `formUrlEncoded` et
  l'utilisateur clique sur une ligne de ce corps
- **THEN** le détail a le focus et aucune saisie n'est active

### Requirement: Sélection de lignes au glisser
Dans le détail ou dans la réponse, un appui du bouton gauche suivi d'un
glisser vers une autre ligne SHALL donner le focus à ce panneau et
définir une sélection de lignes du contenu de ce panneau, de la ligne
sous l'appui à la ligne sous le pointeur, incluses, quel que soit leur
ordre, affichée comme la sélection visuelle de `search-and-yank`. Quand
le pointeur dépasse le haut ou le bas du panneau pendant le glisser, le
panneau MUST défiler d'une ligne dans ce sens par événement de glisser,
dans les bornes du défilement, et la sélection MUST s'étendre jusqu'à la
première ou la dernière ligne visible. Au relâchement, la sélection
SHALL rester active, remplaçant toute sélection visuelle précédente de
ce panneau ; ses bornes MUST ensuite rester fixes quel que soit le
défilement ultérieur, jusqu'à sa levée. Elle SHALL être levée par `v`,
par `Échap` dans les mêmes conditions qu'une sélection visuelle clavier,
par un changement de nœud sélectionné, ou après une copie. Un clic sans
glisser dans le détail ou la réponse SHALL lever toute sélection de ce
panneau, qu'elle ait été faite au clavier ou à la souris. Une sélection
au glisser dans un panneau MUST NOT affecter celle de l'autre panneau.
Un glisser MUST NOT ouvrir de session d'édition ni commencer une saisie,
même s'il commence sur un champ éditable ; un glisser qui commence sur
les lignes du champ en cours de saisie MUST être sans effet. La
sélection porte sur des lignes entières : la colonne du pointeur n'a pas
d'effet.

#### Scenario: Glisser sur trois lignes du détail
- **WHEN** le détail de `scripted` est affiché en haut de son contenu, et
  l'utilisateur appuie sur la deuxième ligne du panneau puis glisse
  jusqu'à la quatrième avant de relâcher
- **THEN** le détail a le focus et les deuxième, troisième et quatrième
  lignes du contenu sont affichées sélectionnées

#### Scenario: Glisser au-delà du bas du panneau
- **WHEN** la réponse est plus longue que son panneau et l'utilisateur
  glisse depuis la première ligne jusque sous le bas du panneau
- **THEN** la réponse défile vers le bas à chaque événement de glisser et
  la sélection s'étend jusqu'à la dernière ligne visible

#### Scenario: Défilement après relâchement
- **WHEN** une sélection au glisser couvre les lignes 2 à 4 du détail et
  l'utilisateur tourne la molette vers le bas
- **THEN** le détail défile et la sélection couvre toujours les lignes 2
  à 4

#### Scenario: Glisser commencé sur l'URL
- **WHEN** aucune session n'est ouverte et l'utilisateur glisse depuis la
  ligne de l'URL jusqu'à la ligne suivante
- **THEN** une sélection de ces deux lignes est active et aucune session
  d'édition n'est ouverte

### Requirement: Copie d'une sélection faite à la souris
Quand une sélection faite au glisser est active dans le panneau qui a le
focus et qu'aucune saisie n'est en cours, `y` SHALL copier le texte brut
de ses lignes, dans l'ordre d'affichage et séparées par des sauts de
ligne, vers le presse-papiers du système, avec exactement les mêmes
garanties que la copie de `search-and-yank` : levée de la sélection
après une copie réussie sans déplacer le défilement, confirmation ou
échec signalé dans la barre d'état, boucle d'événements jamais bloquée.
Le relâchement du bouton MUST NOT copier par lui-même.

#### Scenario: Copie après un glisser
- **WHEN** une sélection au glisser couvre deux lignes de la réponse et
  l'utilisateur appuie sur `y`
- **THEN** le texte de ces deux lignes, séparées par un saut de ligne, est
  copié dans le presse-papiers, la sélection est levée et la barre
  d'état confirme la copie

#### Scenario: Relâchement sans copie
- **WHEN** l'utilisateur termine un glisser dans le détail
- **THEN** le presse-papiers du système n'est pas modifié

#### Scenario: Presse-papiers inaccessible
- **WHEN** aucun serveur d'affichage n'est accessible, une sélection au
  glisser est active, et l'utilisateur appuie sur `y`
- **THEN** la barre d'état signale l'échec, la sélection et le défilement
  sont inchangés et l'application reste réactive

### Requirement: Compatibilité avec le clavier
Toutes les touches déjà définies par `tui-shell`, `search-and-yank`,
`field-editing`, `response-tabs`, `response-filter`,
`request-execution`, `environment-picker`, `diagnostics-and-history`,
`secret-env-vars` et `status-panel` MUST conserver exactement leur
comportement, que la capture souris soit active ou non. La nouvelle
touche `M` MUST NOT réutiliser une touche déjà affectée hors saisie.
Dans l'état Saisie de `field-editing`, `M` et `y` MUST rester du texte,
conformément à l'arbitrage des touches de cette capacité ; dans l'état
Sélection de champ comme hors session, ils SHALL garder le sens défini
par cette capacité et par `search-and-yank`.

#### Scenario: Navigation clavier inchangée avec la capture active
- **WHEN** la capture souris est active et l'utilisateur navigue dans
  l'arbre avec `↓`, `→` et `←`
- **THEN** la sélection et le dépliage évoluent exactement comme décrit
  par `tui-shell`

#### Scenario: y en sélection de champ
- **WHEN** une session est dans l'état Sélection de champ, une sélection
  au glisser est active dans le détail, et l'utilisateur appuie sur `y`
- **THEN** les lignes sélectionnées sont copiées et aucune saisie ne
  commence
