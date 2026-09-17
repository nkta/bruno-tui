# status-panel Specification

## Purpose
Rendre le statut de la dernière réponse de la requête sélectionnée
lisible d'un coup d'œil, dans un panneau dédié placé au-dessus du
panneau Réponse : classe du code HTTP mise en valeur par la couleur,
libellé, temps de réponse, taille et verdict des vérifications.

## Requirements

### Requirement: Panneau Statut au-dessus du panneau Réponse
Quand une collection est chargée et que l'arbre, le détail et la
réponse sont affichés (`tui-shell`), l'interface SHALL afficher un
panneau bordé intitulé « Statut », immédiatement au-dessus du panneau
Réponse et de la même largeur que lui. Ce panneau MUST NOT pouvoir
recevoir le focus : le cycle de `Tab` et le retour par `Échap` définis
par `tui-shell` restent inchangés, et aucune touche ne lui est dédiée.
Il MUST se mettre à jour à chaque changement de sélection dans l'arbre,
à chaque nouveau résultat et à chaque début ou fin d'exécution. Il
MUST NOT être affiché quand un panneau plein corps (diagnostics,
historique, choix d'environnement, secrets) remplace l'arbre, le détail
et la réponse, ni pendant le chargement ou après un échec de
chargement de la collection.

#### Scenario: Panneau présent au-dessus de la réponse
- **WHEN** la collection `runner-probe` est chargée sur un terminal de
  100×30
- **THEN** un panneau « Statut » est affiché au-dessus du panneau
  Réponse, aligné sur ses bords gauche et droit

#### Scenario: Tab ignore le panneau Statut
- **WHEN** l'arbre a le focus et l'utilisateur appuie trois fois sur
  `Tab`
- **THEN** le focus passe au détail, puis à la réponse, puis revient à
  l'arbre, sans jamais se porter sur le panneau Statut

#### Scenario: Masqué par un panneau plein corps
- **WHEN** le panneau d'historique est ouvert
- **THEN** le panneau Statut n'est pas affiché

### Requirement: Contenu du panneau pour une requête exécutée
Quand la requête sélectionnée a un résultat d'exécution
(`request-execution`), le panneau Statut SHALL afficher :
- un **badge de statut**, mis en valeur (texte en gras sur fond de la
  couleur de sa classe, `visual-theme`), contenant le code HTTP quand
  une réponse a été reçue, « aucune réponse » quand la requête est en
  erreur sans réponse, « ignorée » quand la requête a été ignorée, ou
  la valeur brute rapportée par `bru` dans tout autre cas ;
- le **libellé** de statut rapporté par `bru`, à la suite du badge,
  uniquement s'il est présent et non vide ; le panneau MUST NOT
  déduire un libellé du code HTTP ;
- le **temps de réponse** en millisecondes ;
- la **taille du corps**, uniquement quand la réponse porte un en-tête
  `content-length` (nom comparé sans tenir compte de la casse) dont la
  valeur est un entier positif ou nul, exprimée en octets (`o`) sous
  1024, puis en `Ko` ou `Mo` avec une décimale ; aucune taille MUST
  être affichée sinon, et le panneau MUST NOT l'estimer à partir du
  corps ;
- le **verdict** (réussi ou échec), identique au marqueur de l'arbre
  pour cette requête et dans la même couleur, suivi du nombre de
  vérifications réussies sur le nombre de vérifications non ignorées,
  toutes catégories confondues (assertions, tests, tests pré-requête,
  tests post-réponse), ou de la mention « aucune vérification » quand
  ce nombre est nul.

Le message d'erreur rapporté par `bru` MUST NOT être affiché dans ce
panneau : il reste affiché par le panneau Réponse (`response-tabs`).

#### Scenario: Requête entièrement réussie
- **WHEN** `green` de `runner-probe` a reçu une réponse `200` de
  libellé `OK`, en 6 ms, avec `content-length: 21`, et toutes ses
  vérifications réussies, et `green` est sélectionnée
- **THEN** le panneau Statut affiche le badge `200` à la couleur des
  réponses 2xx, le libellé `OK`, `6 ms`, `21 o`, le verdict « réussi »
  et le décompte des vérifications réussies égal à leur total

#### Scenario: Réponse 200 avec vérifications en échec
- **WHEN** `ok` a reçu une réponse `200` et une assertion et un test
  sont en échec, et `ok` est sélectionnée
- **THEN** le badge `200` garde la couleur des réponses 2xx
- **AND** le verdict « échec » est affiché à la couleur d'échec, avec un
  nombre de vérifications réussies inférieur au total

#### Scenario: Requête en erreur sans réponse
- **WHEN** `folder/down` s'est terminée par une connexion refusée, et
  est sélectionnée
- **THEN** le badge affiche « aucune réponse » à la couleur d'échec,
  sans code HTTP, sans libellé ni taille
- **AND** le message d'erreur de connexion n'apparaît pas dans le
  panneau Statut mais reste affiché dans le panneau Réponse

#### Scenario: Requête ignorée
- **WHEN** `skip` a été ignorée par un script, et est sélectionnée
- **THEN** le badge affiche « ignorée » dans le style neutre, suivi du
  libellé rapporté par `bru`
- **AND** aucun verdict d'échec n'est affiché

#### Scenario: Taille absente
- **WHEN** la réponse de la requête sélectionnée ne porte pas d'en-tête
  `content-length`
- **THEN** le panneau Statut n'affiche aucune taille, ni valeur
  estimée ni mention de substitution

### Requirement: Couleur du badge par classe de statut
La couleur du badge SHALL dépendre uniquement de la classe du statut
de la réponse, selon les catégories de `visual-theme` : code 200 à 299
→ succès ; 300 à 399 → redirection ; 400 à 499 → erreur client ; 500 à
599 → échec ; requête en erreur sans réponse → échec ; requête ignorée,
code hors de 200 à 599 ou valeur non reconnue → neutre. La couleur du
badge MUST NOT dépendre du verdict des vérifications.

#### Scenario: Redirection
- **WHEN** la requête sélectionnée a reçu une réponse `302`
- **THEN** le badge `302` porte la couleur de redirection, distincte de
  celles du succès et de l'erreur client

#### Scenario: Erreur client et erreur serveur distinctes
- **WHEN** une requête a reçu `404` et une autre `503`
- **THEN** le badge `404` porte la couleur d'erreur client et le badge
  `503` la couleur d'échec, et les deux couleurs sont différentes

### Requirement: Panneau sans résultat
Quand la sélection n'a aucun résultat d'exécution — requête jamais
exécutée, dossier, nœud en erreur, ou arbre vide — et qu'aucune
exécution en cours ne concerne la sélection, le panneau Statut SHALL
afficher uniquement un tiret `—` dans le style neutre, sans badge
coloré, sans verdict et sans message de substitution, le message
« aucun résultat » restant porté par le panneau Réponse (`tui-shell`).

#### Scenario: Requête jamais exécutée
- **WHEN** une requête sans aucune exécution est sélectionnée
- **THEN** le panneau Statut affiche seulement `—` dans le style neutre

#### Scenario: Dossier sélectionné
- **WHEN** un dossier est sélectionné et aucune exécution n'est en
  cours
- **THEN** le panneau Statut affiche seulement `—` dans le style neutre

### Requirement: Panneau pendant une exécution en cours
Quand une exécution est en cours (`request-execution`) et qu'elle
concerne la requête sélectionnée — cible égale à cette requête, ou
exécution récursive d'un dossier qui la contient —, le panneau Statut
SHALL afficher un indicateur « en cours » à la couleur d'exécution en
cours (`visual-theme`) à la place du badge. Si la requête a déjà un
résultat, le panneau SHALL afficher en plus, atténués, le code (ou
« aucune réponse » / « ignorée ») et le temps de réponse de ce résultat
précédent, sans verdict. Quand l'exécution se termine, quelle que soit
son issue (résultat, échec sans rapport, annulation), l'indicateur
MUST disparaître et le panneau MUST refléter le résultat alors
disponible. Une exécution en cours qui ne concerne pas la requête
sélectionnée MUST NOT modifier le panneau.

#### Scenario: Première exécution en cours
- **WHEN** l'exécution de `ok`, jamais exécutée auparavant, est en
  cours et `ok` est sélectionnée
- **THEN** le panneau Statut affiche l'indicateur « en cours » à la
  couleur d'exécution en cours, sans résultat précédent

#### Scenario: Réexécution avec résultat précédent
- **WHEN** `folder/down` a un résultat sans réponse en 0 ms et une
  exécution récursive du dossier `folder` est en cours, `folder/down`
  étant sélectionnée
- **THEN** le panneau affiche « en cours » et, atténués, « aucune
  réponse » et `0 ms`, sans verdict

#### Scenario: Annulation
- **WHEN** l'exécution en cours de `ok`, jamais exécutée auparavant,
  est annulée alors que `ok` est sélectionnée
- **THEN** le panneau Statut affiche de nouveau seulement `—`

#### Scenario: Exécution d'une autre requête
- **WHEN** `green` a un résultat et est sélectionnée pendant que
  l'exécution de `ok` seule est en cours
- **THEN** le panneau Statut affiche le résultat de `green`, sans
  indicateur « en cours »

### Requirement: Forme compacte sur un terminal bas
Le panneau Statut SHALL occuper 5 lignes (3 lignes intérieures : badge
et libellé ; temps et taille ; verdict et décompte) quand le terminal
fait au moins 20 lignes, et 3 lignes (une seule ligne intérieure)
en dessous. Dans la forme compacte, la ligne unique SHALL présenter,
dans cet ordre, le badge, le marqueur de verdict et le temps de
réponse, le texte excédant la largeur étant tronqué. Le panneau Réponse
MUST conserver au moins une ligne intérieure à toute taille supportée
par `tui-shell`, et le rendu MUST NOT paniquer.

#### Scenario: Terminal de 30 lignes
- **WHEN** le terminal fait 100×30 et une requête exécutée est
  sélectionnée
- **THEN** le panneau Statut occupe 5 lignes et affiche ses trois lignes
  intérieures

#### Scenario: Terminal à la taille minimale
- **WHEN** le terminal fait 60×10 et une requête exécutée est
  sélectionnée
- **THEN** le panneau Statut occupe 3 lignes et commence par le badge
- **AND** le panneau Réponse affiche au moins une ligne de contenu

### Requirement: Aucune donnée sensible ni effet de bord
Le panneau Statut MUST NOT afficher de valeur d'en-tête autre que
`content-length`, ni de contenu du corps, de l'URL ou de variable. Il
MUST NOT lancer de processus, écrire de fichier ni journaliser quoi que
ce soit : il est entièrement dérivé du résultat déjà conservé et de
l'état d'exécution en cours.

#### Scenario: Réponse avec en-têtes sensibles
- **WHEN** la réponse de la requête sélectionnée porte un en-tête
  `set-cookie`
- **THEN** aucune partie de sa valeur n'apparaît dans le panneau Statut
