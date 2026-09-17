## MODIFIED Requirements

### Requirement: Champs éditables et curseur de champ
Dans l'état Sélection de champ, le système SHALL exposer un curseur qui
se déplace, par `↓` ou `j` et `↑` ou `k`, exclusivement parmi les
positions de la session, dans l'ordre où elles apparaissent dans le
détail : l'URL, puis chaque en-tête suivi d'une ligne « + Ajouter un
en-tête », chaque paramètre de requête suivi d'une ligne « + Ajouter un
paramètre de requête », chaque paramètre de chemin suivi d'une ligne
« + Ajouter un paramètre de chemin », puis le corps si son type est
éditable par `bru-writer` (`json`, `text`, `xml`, `sparql`, `graphql`).
Les trois lignes d'ajout MUST être présentes même quand leur section est
vide. Les en-têtes et paramètres listés MUST être ceux de l'aperçu de la
requête après application des modifications validées de la session : une
entrée ajoutée, ou créée par la synchronisation de l'URL, y figure ; une
entrée supprimée n'y figure plus. Le déplacement MUST NOT avoir d'effet
aux extrémités de cette liste. Un corps non éditable (forme formulaire,
absence de corps) MUST NOT apparaître dans cette liste. La position sous
le curseur MUST être visuellement distinguée du reste du détail. Les
lignes d'ajout MUST NOT être affichées hors session.

#### Scenario: Parcours des champs
- **WHEN** une session est ouverte sur une requête ayant une URL, deux
  en-têtes, aucun paramètre et un corps `json`
- **THEN** sept positions de curseur existent, dans cet ordre : URL,
  premier en-tête, second en-tête, ajout d'en-tête, ajout de paramètre de
  requête, ajout de paramètre de chemin, corps

#### Scenario: Corps non éditable exclu
- **WHEN** la requête a un corps de type `formUrlEncoded`
- **THEN** aucune position de curseur ne porte sur le corps

#### Scenario: Extrémités du curseur de champ
- **WHEN** le curseur de champ est sur le premier champ et l'utilisateur
  appuie sur `↑`
- **THEN** le curseur ne bouge pas

#### Scenario: Liste suivant les modifications validées
- **WHEN** une session porte, non enregistrés, l'ajout d'un en-tête et la
  suppression d'un paramètre de requête
- **THEN** le curseur peut se placer sur l'en-tête ajouté et ne peut plus
  se placer sur le paramètre supprimé

#### Scenario: Lignes d'ajout absentes hors session
- **WHEN** le détail affiche une requête sans session d'édition ouverte
- **THEN** aucune ligne « + Ajouter » n'est affichée

### Requirement: Sauvegarde explicite
Le système SHALL enregistrer sur disque uniquement sur `Ctrl+S`, pendant
une session d'édition, en appelant `bru-writer` avec l'instantané de
fraîcheur de la session et l'ensemble des modifications validées depuis.
Dans l'état Saisie, `Ctrl+S` MUST d'abord valider la saisie en cours,
exactement comme `Tab`, puis enregistrer ; par exception, pendant la
saisie de la clé d'une nouvelle entrée, `Ctrl+S` MUST valider la clé et
ajouter l'entrée avec une valeur vide, puis enregistrer. Si cette
validation est refusée (exigence « Refus immédiat d'une modification
invalide »), `Ctrl+S` MUST NOT enregistrer et la saisie reste ouverte.
Sur une session sans modification après cette éventuelle validation,
`Ctrl+S` MUST NOT déclencher d'écriture. Aucune autre touche ni aucun
autre événement (validation d'un champ, fermeture de session, changement
de focus, sortie de l'application) MUST NOT déclencher d'écriture. Une
sauvegarde réussie MUST effacer l'indicateur de modification, remplacer
l'instantané de fraîcheur de la session par celui retourné, et laisser
la session dans l'état Sélection de champ. Le déclenchement de
l'écriture MUST NOT bloquer la saisie ni le rendu.

#### Scenario: Sauvegarde réussie
- **WHEN** une session modifiée dans l'état Sélection de champ reçoit
  `Ctrl+S` et l'écriture réussit
- **THEN** l'indicateur de modification disparaît et la session reste
  ouverte sur la requête, valeurs enregistrées affichées

#### Scenario: Sauvegarde depuis la saisie
- **WHEN** l'utilisateur est en train de saisir une nouvelle URL, sans
  l'avoir validée, et appuie sur `Ctrl+S`
- **THEN** la nouvelle URL est validée puis enregistrée, et la session
  revient à l'état Sélection de champ

#### Scenario: Sauvegarde pendant la saisie de la clé d'une nouvelle entrée
- **WHEN** l'utilisateur ajoute un en-tête, a tapé la clé `X-Trace` sans
  la valider, et appuie sur `Ctrl+S`
- **THEN** l'en-tête `X-Trace` est ajouté avec une valeur vide, le
  fichier est enregistré, et la session revient à l'état Sélection de
  champ

#### Scenario: Sauvegarde sans modification
- **WHEN** une session sans modification reçoit `Ctrl+S`
- **THEN** aucune écriture n'est déclenchée

#### Scenario: Validation sans écriture
- **WHEN** l'utilisateur valide une saisie par `Entrée` ou `Tab`, puis
  ferme l'application en acceptant la confirmation
- **THEN** le fichier sur disque est inchangé

#### Scenario: Ancienne touche w retirée
- **WHEN** une session modifiée est dans l'état Sélection de champ et
  l'utilisateur appuie sur `w`
- **THEN** aucune écriture n'est déclenchée

#### Scenario: Sauvegarde pendant une saisie longue
- **WHEN** l'utilisateur déclenche une sauvegarde alors que l'écriture
  disque est lente
- **THEN** l'interface continue de répondre aux touches pendant l'écriture

### Requirement: Barre d'aide de la session
Le système SHALL afficher, quand une session d'édition est ouverte, à la
place de la barre d'état habituelle du panneau de détail : l'état courant
(Sélection de champ ou Saisie), le nom du champ sous le curseur, un
indicateur visible tant que la session porte des modifications non
enregistrées, et les touches utiles dans l'état courant. En Sélection de
champ, ces touches MUST inclure l'ouverture de la saisie, l'enregistrement
et la fermeture, ainsi que `a`, `d` et `c` quand le curseur est sur un
en-tête ou un paramètre. En Saisie, elles MUST inclure la validation,
l'annulation et l'enregistrement, et distinguer le corps (où `Entrée`
insère un saut de ligne) des champs à une ligne. Pendant l'ajout d'une
entrée, la barre MUST nommer la section visée et l'étape (clé ou valeur) ;
pendant un renommage, elle MUST l'indiquer. Un message de statut
prioritaire (exécution, échec de sauvegarde, refus de changement de
sélection, refus d'une modification invalide) MAY remplacer
temporairement cette barre, comme pour la barre d'état habituelle.

#### Scenario: Barre d'aide en saisie de l'URL
- **WHEN** l'état Saisie est actif sur le champ URL
- **THEN** la barre indique l'état Saisie, le nom du champ URL, et
  rappelle `Entrée` pour valider, `Échap` pour annuler et `Ctrl+S` pour
  enregistrer

#### Scenario: Barre d'aide en saisie du corps
- **WHEN** l'état Saisie est actif sur le corps
- **THEN** la barre rappelle que `Entrée` insère un saut de ligne et que
  `Tab` valide

#### Scenario: Indicateur de modification
- **WHEN** une session porte une modification validée non enregistrée
- **THEN** la barre affiche l'indicateur de modification non enregistrée
- **AND** l'indicateur disparaît après une sauvegarde réussie

#### Scenario: Barre d'aide sur un en-tête
- **WHEN** l'état Sélection de champ est actif et le curseur est sur un
  en-tête
- **THEN** la barre rappelle `a` pour ajouter, `d` pour supprimer et `c`
  pour renommer

#### Scenario: Barre d'aide pendant l'ajout
- **WHEN** l'utilisateur ajoute un en-tête et en est à la saisie de la
  clé
- **THEN** la barre indique l'état Saisie, la section En-têtes et l'étape
  de saisie de la clé

### Requirement: Saisie d'un champ
Dans l'état Sélection de champ, le système SHALL commencer la saisie du
champ sous le curseur sur `Entrée`, avec un curseur de texte
initialement en fin de valeur actuelle du champ (valeur validée la plus
récente, sinon valeur chargée). Pendant la saisie, le champ affiche le
texte en cours de saisie. La saisie se termine de trois manières
exclusives :
- validation, par `Tab` sur tout champ, ou par `Entrée` sur un champ à
  une ligne (URL, en-tête, paramètre) : le texte saisi devient la valeur
  du champ en mémoire, sans écriture sur disque, et si elle diffère de la
  valeur chargée depuis le fichier, la session MUST être marquée
  modifiée ;
- annulation, par `Échap` : le champ MUST reprendre exactement la valeur
  qu'il avait au moment où la saisie a commencé, sans confirmation ;
- validation suivie d'un enregistrement, par `Ctrl+S` (voir « Sauvegarde
  explicite »).
Dans les trois cas, la session revient à l'état Sélection de champ, le
curseur de champ sur le même champ. Par exception, une validation refusée
par les règles de `bru-writer` (exigence « Refus immédiat d'une
modification invalide ») MUST laisser la saisie ouverte, texte et curseur
intacts. Sur une ligne « + Ajouter », `Entrée` commence l'ajout d'une
entrée (exigence « Ajout d'une entrée ») et non la saisie d'un champ.

#### Scenario: Modification validée de l'URL
- **WHEN** le curseur de champ est sur l'URL, l'utilisateur appuie sur
  `Entrée`, tape des caractères puis appuie sur `Entrée`
- **THEN** l'état redevient Sélection de champ, la valeur affichée de
  l'URL reflète la saisie, et la session est marquée modifiée

#### Scenario: Annulation d'une saisie
- **WHEN** l'utilisateur commence la saisie d'un en-tête dont la valeur
  est `abc`, tape `xyz` puis appuie sur `Échap`
- **THEN** l'en-tête affiche `abc` et la session n'est pas marquée
  modifiée par cette saisie

#### Scenario: Annulation après une validation précédente
- **WHEN** l'URL a déjà été validée à `http://b`, puis l'utilisateur
  recommence sa saisie, tape des caractères et appuie sur `Échap`
- **THEN** l'URL affiche `http://b` et la session reste modifiée

#### Scenario: Entrée dans le corps
- **WHEN** l'état Saisie est actif sur le corps et l'utilisateur appuie
  sur `Entrée`
- **THEN** un saut de ligne est inséré à la position du curseur de texte,
  sans terminer la saisie

#### Scenario: Validation du corps par Tab
- **WHEN** l'état Saisie est actif sur le corps, après ajout d'une ligne,
  et l'utilisateur appuie sur `Tab`
- **THEN** l'état redevient Sélection de champ et le corps affiché
  contient la ligne ajoutée

#### Scenario: Valeur inchangée
- **WHEN** l'utilisateur commence la saisie d'un champ puis la valide
  sans avoir changé le texte
- **THEN** la session n'est pas marquée modifiée par ce seul aller-retour

#### Scenario: Validation refusée d'un paramètre de requête
- **WHEN** l'état Saisie est actif sur un paramètre de requête avec le
  texte `a#b` et l'utilisateur appuie sur `Entrée`
- **THEN** un message signale un caractère interdit et la saisie reste
  ouverte avec `a#b`
- **AND** si l'utilisateur appuie ensuite sur `Échap`, le paramètre
  reprend sa valeur d'avant la saisie

### Requirement: Arbitrage des touches pendant une session
Pendant l'état Saisie, le système SHALL traiter toute touche imprimable
comme du texte, y compris les lettres et symboles qui portent un
raccourci global hors saisie (`q`, `r`, `/`, `|`, `n`, `v`, `y`, `e`,
`a`, `d`, `c`, `D`, `H`, `E`, `S`, espace, etc.), et MUST NOT déclencher
l'action associée. Les seules combinaisons qui gardent un sens hors du
texte en Saisie sont `Ctrl+C` (fermeture, avec confirmation selon
l'exigence de confirmation), `Ctrl+X` (annulation d'une exécution en
cours) et `Ctrl+S` (enregistrement). `Tab` y valide la saisie et MUST NOT
changer le focus. Toute autre touche non décrite par les exigences de
saisie MUST être sans effet.

Dans l'état Sélection de champ, le système SHALL redéfinir uniquement
`↑`/`k`, `↓`/`j`, `Entrée`, `Espace`, `Échap`, `Ctrl+S`, `a`, `d` et `c`
comme décrit par les autres exigences de cette capacité ; toute autre
touche MUST conserver le sens que lui donnent les autres capacités
lorsque le détail a le focus. Les anciennes touches de session `i` et `w`
MUST NOT avoir d'effet propre à l'édition.

#### Scenario: Lettres de raccourci en saisie
- **WHEN** l'état Saisie est actif sur un en-tête et l'utilisateur tape
  `r/qS`
- **THEN** ces quatre caractères sont insérés, aucune exécution n'est
  lancée, aucune recherche ni panneau ne s'ouvre, et l'application reste
  ouverte

#### Scenario: Lettres d'ajout et de suppression en saisie
- **WHEN** l'état Saisie est actif sur un en-tête et l'utilisateur tape
  `adc`
- **THEN** ces trois caractères sont insérés, aucune entrée n'est
  ajoutée, supprimée ni renommée

#### Scenario: Tab en saisie
- **WHEN** l'état Saisie est actif et l'utilisateur appuie sur `Tab`
- **THEN** la saisie est validée et le focus reste sur le détail

#### Scenario: Annulation d'exécution pendant la saisie
- **WHEN** une exécution est en cours, l'état Saisie est actif, et
  l'utilisateur appuie sur `Ctrl+X`
- **THEN** l'exécution est annulée et la saisie continue, texte intact

#### Scenario: Raccourci global en sélection de champ
- **WHEN** une session est dans l'état Sélection de champ et
  l'utilisateur appuie sur `r`
- **THEN** l'exécution de la requête se lance comme hors session, sur le
  contenu du fichier tel qu'il est sur disque

#### Scenario: Ancienne touche i
- **WHEN** une session est dans l'état Sélection de champ et
  l'utilisateur appuie sur `i`
- **THEN** aucune saisie ne commence

## ADDED Requirements

### Requirement: Ajout d'une entrée
Dans l'état Sélection de champ, le système SHALL commencer l'ajout d'une
entrée par `Entrée` sur une ligne « + Ajouter » (dans la section de cette
ligne), ou par `a` sur un en-tête, un paramètre ou une ligne « + Ajouter »
(dans la section de cette position). Sur l'URL ou le corps, `a` MUST NOT
avoir d'effet. L'ajout enchaîne deux saisies, avec les mêmes touches
d'édition de texte que l'exigence « Curseur de texte libre » pour un
champ à une ligne :
- saisie de la clé, texte initialement vide : `Entrée` ou `Tab` valide la
  clé et passe à la saisie de la valeur ;
- saisie de la valeur, texte initialement vide : `Entrée` ou `Tab` valide
  la valeur ; une valeur vide MUST être acceptée.
`Échap` MUST abandonner l'ajout entier, à l'une ou l'autre étape, sans
rien ajouter et sans confirmation ; le curseur de champ revient à la
position d'où l'ajout a commencé. À la validation de la valeur, l'entrée
MUST être ajoutée, activée, en fin de sa section, la session marquée
modifiée, l'état redevient Sélection de champ et le curseur de champ est
placé sur la nouvelle entrée. Pendant l'ajout, une ligne provisoire
affiche la clé et la valeur en cours de saisie en fin de section. Aucune
écriture disque MUST NOT avoir lieu avant `Ctrl+S`. Sur une ligne
« + Ajouter », `Espace`, `d` et `c` MUST NOT avoir d'effet.

#### Scenario: Ajout d'un en-tête depuis un en-tête existant
- **WHEN** le curseur de champ est sur un en-tête, l'utilisateur appuie
  sur `a`, tape `X-Trace`, `Entrée`, `abc`, `Entrée`
- **THEN** un en-tête `X-Trace` de valeur `abc` apparaît en dernier dans
  la section En-têtes, le curseur est dessus, et la session est marquée
  modifiée

#### Scenario: Ajout d'un paramètre de requête depuis sa ligne d'ajout
- **WHEN** l'URL est `https://h/items`, le curseur est sur la ligne
  « + Ajouter un paramètre de requête », et l'utilisateur appuie sur
  `Entrée`, tape `page`, `Tab`, `2`, `Entrée`
- **THEN** le paramètre de requête `page` de valeur `2` apparaît, et l'URL
  affichée devient `https://h/items?page=2`

#### Scenario: Abandon pendant la saisie de la valeur
- **WHEN** l'utilisateur a validé la clé d'un nouvel en-tête et appuie sur
  `Échap` pendant la saisie de la valeur
- **THEN** aucun en-tête n'est ajouté, l'état redevient Sélection de
  champ et l'indicateur de modification reste dans l'état où il était
  avant l'ajout

#### Scenario: Ajout dans une section vide
- **WHEN** la requête n'a aucun paramètre de chemin, le curseur est sur
  la ligne « + Ajouter un paramètre de chemin », et l'utilisateur ajoute
  le paramètre `id` de valeur `42`
- **THEN** la section Paramètres de chemin affiche `id` et le curseur de
  champ est sur cette entrée

#### Scenario: a sans effet sur l'URL
- **WHEN** le curseur de champ est sur l'URL et l'utilisateur appuie sur
  `a`
- **THEN** aucun ajout ne commence

### Requirement: Suppression d'une entrée
Dans l'état Sélection de champ, quand le curseur de champ est sur un
en-tête ou un paramètre, le système SHALL supprimer cette entrée de la
session sur `d`, immédiatement, sans confirmation, et marquer la session
comme modifiée. Le curseur de champ MUST rester au même indice dans la
liste des positions, borné à sa dernière position. Sur l'URL, le corps ou
une ligne « + Ajouter », `d` MUST NOT avoir d'effet. La suppression d'un
paramètre de requête MUST mettre à jour l'URL affichée selon les règles
de `bru-writer`. Une suppression est une modification non enregistrée au
sens de l'exigence « Changement de requête pendant une session
modifiée ».

#### Scenario: Suppression d'un en-tête
- **WHEN** le curseur de champ est sur le second de trois en-têtes et
  l'utilisateur appuie sur `d`
- **THEN** l'en-tête n'est plus affiché, le curseur est sur l'ancien
  troisième en-tête, et la session est marquée modifiée

#### Scenario: Suppression d'un paramètre de requête
- **WHEN** l'URL affichée est `https://h/items?page=2&size=10`, le curseur
  est sur le paramètre `size` et l'utilisateur appuie sur `d`
- **THEN** l'URL affichée devient `https://h/items?page=2`

#### Scenario: Sans effet sur le corps
- **WHEN** le curseur de champ est sur le corps et l'utilisateur appuie
  sur `d`
- **THEN** rien ne change

#### Scenario: Changement de requête bloqué après suppression
- **WHEN** l'utilisateur a supprimé un en-tête sans enregistrer, rend le
  focus à l'arbre et sélectionne une autre requête
- **THEN** la sélection ne change pas et le message invitant à
  enregistrer ou fermer la session s'affiche

### Requirement: Renommage d'une clé
Dans l'état Sélection de champ, quand le curseur de champ est sur un
en-tête ou un paramètre, le système SHALL commencer sur `c` une saisie de
sa clé, pré-remplie avec la clé actuelle, curseur de texte en fin de clé,
avec les touches d'un champ à une ligne. `Entrée` ou `Tab` MUST valider
le renommage et marquer la session modifiée si la clé a changé ; `Échap`
MUST annuler la saisie en laissant la clé inchangée ; `Ctrl+S` MUST
valider puis enregistrer. L'entrée MUST conserver sa valeur et son état
activé ou désactivé. Sur l'URL, le corps ou une ligne « + Ajouter », `c`
MUST NOT avoir d'effet.

#### Scenario: Renommage d'un paramètre de requête
- **WHEN** l'URL affichée est `https://h/items?p=2`, le curseur est sur le
  paramètre `p`, l'utilisateur appuie sur `c`, efface la clé, tape `page`
  puis `Entrée`
- **THEN** le paramètre affiché s'appelle `page`, garde la valeur `2`,
  l'URL affichée devient `https://h/items?page=2`, et la session est
  marquée modifiée

#### Scenario: Renommage annulé
- **WHEN** l'utilisateur commence le renommage d'un en-tête, tape des
  caractères puis `Échap`
- **THEN** la clé affichée est celle d'avant `c` et la session n'est pas
  marquée modifiée par cette saisie

### Requirement: Refus immédiat d'une modification invalide
Le système SHALL vérifier chaque modification au moment où elle serait
validée (validation d'une valeur de champ, d'une clé d'ajout, d'une
valeur d'ajout, d'un renommage, y compris via `Ctrl+S`), avec les règles
de `bru-writer`. Une modification refusée (clé invalide, `&`, `#` ou saut
de ligne dans un paramètre de requête) MUST afficher un message nommant
la nature du problème sans citer le texte saisi, MUST laisser la saisie
ouverte avec son texte et son curseur, et MUST NOT être ajoutée aux
modifications validées. `Échap` reste disponible et annule la saisie. Une
clé déjà présente dans la section MUST être acceptée.

#### Scenario: Clé d'en-tête invalide
- **WHEN** pendant l'ajout d'un en-tête, l'utilisateur tape `X Trace`
  puis `Entrée` à l'étape de la clé
- **THEN** un message signale une clé invalide, la saisie de la clé reste
  ouverte avec `X Trace`, et rien n'est ajouté

#### Scenario: Ctrl+S sur une clé invalide
- **WHEN** pendant l'ajout d'un en-tête, l'utilisateur tape `X:Trace`
  puis `Ctrl+S`
- **THEN** un message signale une clé invalide, aucune écriture n'est
  déclenchée et la saisie de la clé reste ouverte

#### Scenario: Clé dupliquée acceptée
- **WHEN** la section En-têtes contient `Accept` et l'utilisateur ajoute un
  en-tête de clé `Accept`
- **THEN** l'ajout est accepté et deux en-têtes `Accept` sont affichés

### Requirement: Détail affiché depuis l'aperçu de la session
Pendant une session d'édition, le système SHALL afficher dans le panneau
de détail l'URL, les en-têtes et les paramètres tels que les expose
l'aperçu `bru-writer` des modifications validées, de sorte que ce qui est
affiché avant `Ctrl+S` soit ce qui sera écrit. Une sauvegarde réussie
MUST laisser le détail inchangé visuellement, hormis la disparition de
l'indicateur de modification.

#### Scenario: URL synchronisée avant sauvegarde
- **WHEN** l'utilisateur valide l'URL `https://h/items?page=3` dans une
  requête dont la section Paramètres de requête affichait `page` de valeur
  `2`
- **THEN** dès la validation, la section affiche `page` de valeur `3`,
  sans écriture disque

#### Scenario: Touche capturée pendant la saisie d'une clé
- **WHEN** la saisie de la clé d'un ajout est ouverte et l'utilisateur
  tape `q`
- **THEN** `q` est ajouté au texte de la clé et l'application ne se ferme
  pas
