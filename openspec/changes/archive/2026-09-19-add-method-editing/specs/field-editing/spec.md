## ADDED Requirements

### Requirement: Sélecteur de méthode
Dans l'état Sélection de champ, le système SHALL ouvrir, sur `Entrée`
quand le curseur de champ est sur le champ Méthode, un sélecteur listant
les 9 valeurs que `bru-parser` reconnaît comme bloc de méthode (`GET`,
`POST`, `PUT`, `DELETE`, `PATCH`, `OPTIONS`, `HEAD`, `CONNECT`, `TRACE`,
dans cet ordre), avec la méthode actuelle de la session présélectionnée.
Le système SHALL déplacer la présélection d'une entrée sur `↓`/`j` et
`↑`/`k`, sans dépasser les extrémités de la liste, sur le même principe
que le panneau d'environnements (`environment-picker`). `Entrée` SHALL
valider l'entrée présélectionnée : elle devient la méthode de la session
en mémoire, sans écriture sur disque, et si elle diffère de la méthode
chargée depuis le fichier, la session MUST être marquée modifiée.
`Échap` SHALL fermer le sélecteur sans changer la méthode, quelle que
soit l'entrée présélectionnée au moment de la fermeture. Dans les deux
cas, le sélecteur se ferme et l'état redevient Sélection de champ, le
curseur de champ sur le champ Méthode. Le sélecteur ouvert SHALL
capturer les événements clavier : seules `↓`/`j`, `↑`/`k`, `Entrée`,
`Échap` et `Ctrl+C` (fermeture de l'application, avec confirmation selon
l'exigence de confirmation) ont un effet ; toute autre touche MUST être
sans effet, y compris les raccourcis globaux normalement actifs en
Sélection de champ.

Aucune entrée du sélecteur n'étant cliquable (navigation au clavier
seulement), un appui du bouton gauche pendant que le sélecteur est
ouvert, où qu'il tombe sur l'écran, SHALL le fermer sans changer la
méthode, exactement comme `Échap`, sans agir sur une autre cible dans le
même geste. La souris MUST NOT rester inerte tant que le sélecteur est
ouvert : un appui suivant, une fois le sélecteur fermé, produit son effet
habituel sur sa cible.

#### Scenario: Ouverture avec la méthode courante présélectionnée
- **WHEN** le curseur de champ est sur le champ Méthode d'une requête
  `POST`, et l'utilisateur appuie sur `Entrée`
- **THEN** le sélecteur s'ouvre avec `POST` présélectionné parmi les 9
  valeurs

#### Scenario: Choisir une autre méthode
- **WHEN** le sélecteur est ouvert avec `GET` présélectionné, et
  l'utilisateur descend jusqu'à `POST` puis appuie sur `Entrée`
- **THEN** le sélecteur se ferme, le détail affiche `POST`, et la
  session est marquée modifiée

#### Scenario: Annulation sans changement
- **WHEN** le sélecteur est ouvert sur une requête `GET`, l'utilisateur
  descend jusqu'à `DELETE` puis appuie sur `Échap`
- **THEN** le sélecteur se ferme, le détail affiche toujours `GET`, et
  la session n'est pas marquée modifiée par cette ouverture

#### Scenario: Validation sans changement
- **WHEN** le sélecteur est ouvert sur une requête `GET` et
  l'utilisateur appuie directement sur `Entrée` sans déplacer la
  présélection
- **THEN** le sélecteur se ferme et la session n'est pas marquée
  modifiée par ce seul aller-retour

#### Scenario: Extrémités de la liste
- **WHEN** la première entrée (`GET`) est présélectionnée et
  l'utilisateur appuie sur `↑`
- **THEN** la présélection ne change pas

#### Scenario: Raccourci global sans effet pendant le sélecteur
- **WHEN** le sélecteur est ouvert et l'utilisateur appuie sur `r`
- **THEN** aucune exécution ne se lance et le sélecteur reste ouvert

#### Scenario: Clic pendant que le sélecteur est ouvert
- **WHEN** le sélecteur est ouvert et l'utilisateur clique n'importe où à
  l'écran
- **THEN** le sélecteur se ferme sans changer la méthode
- **AND** un clic suivant sur un autre champ ouvre normalement sa saisie,
  la souris n'étant pas restée inerte

## MODIFIED Requirements

### Requirement: Champs éditables et curseur de champ
Dans l'état Sélection de champ, le système SHALL exposer un curseur qui
se déplace, par `↓` ou `j` et `↑` ou `k`, exclusivement parmi les
positions de la session, dans l'ordre où elles apparaissent dans le
détail : la méthode, puis l'URL, puis chaque en-tête suivi d'une ligne
« + Ajouter un en-tête », chaque paramètre de requête suivi d'une ligne
« + Ajouter un paramètre de requête », chaque paramètre de chemin suivi
d'une ligne « + Ajouter un paramètre de chemin », puis le corps si son
type est éditable par `bru-writer` (`json`, `text`, `xml`, `sparql`,
`graphql`). Les trois lignes d'ajout MUST être présentes même quand leur
section est vide. Les en-têtes et paramètres listés MUST être ceux de
l'aperçu de la requête après application des modifications validées de
la session : une entrée ajoutée, ou créée par la synchronisation de
l'URL, y figure ; une entrée supprimée n'y figure plus. Le déplacement
MUST NOT avoir d'effet aux extrémités de cette liste. Un corps non
éditable (forme formulaire, absence de corps) MUST NOT apparaître dans
cette liste. La position sous le curseur MUST être visuellement
distinguée du reste du détail. Les lignes d'ajout MUST NOT être
affichées hors session.

#### Scenario: Parcours des champs
- **WHEN** une session est ouverte sur une requête ayant une URL, deux
  en-têtes, aucun paramètre et un corps `json`
- **THEN** huit positions de curseur existent, dans cet ordre : méthode,
  URL, premier en-tête, second en-tête, ajout d'en-tête, ajout de
  paramètre de requête, ajout de paramètre de chemin, corps

#### Scenario: Corps non éditable exclu
- **WHEN** la requête a un corps de type `formUrlEncoded`
- **THEN** aucune position de curseur ne porte sur le corps

#### Scenario: Extrémités du curseur de champ
- **WHEN** le curseur de champ est sur le premier champ (la méthode) et
  l'utilisateur appuie sur `↑`
- **THEN** le curseur ne bouge pas

#### Scenario: Liste suivant les modifications validées
- **WHEN** une session porte, non enregistrés, l'ajout d'un en-tête et
  la suppression d'un paramètre de requête
- **THEN** le curseur peut se placer sur l'en-tête ajouté et ne peut
  plus se placer sur le paramètre supprimé

#### Scenario: Lignes d'ajout absentes hors session
- **WHEN** le détail affiche une requête sans session d'édition ouverte
- **THEN** aucune ligne « + Ajouter » n'est affichée

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
entrée (exigence « Ajout d'une entrée ») et non la saisie d'un champ. Sur
le champ Méthode, `Entrée` ouvre le sélecteur de méthode (exigence
« Sélecteur de méthode ») et non une saisie de texte.

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

#### Scenario: Entrée sur le champ Méthode
- **WHEN** le curseur de champ est sur le champ Méthode et l'utilisateur
  appuie sur `Entrée`
- **THEN** le sélecteur de méthode s'ouvre, aucun état Saisie ne
  commence

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

Le sélecteur de méthode ouvert (exigence « Sélecteur de méthode ») SHALL
capturer les événements clavier comme l'état Saisie : seules `↑`/`k`,
`↓`/`j`, `Entrée`, `Échap` et `Ctrl+C` y ont un effet, toute autre touche
MUST être sans effet, y compris les raccourcis globaux.

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

#### Scenario: Raccourci sans effet pendant le sélecteur de méthode
- **WHEN** le sélecteur de méthode est ouvert et l'utilisateur appuie
  sur `/`
- **THEN** aucune recherche ne s'ouvre et le sélecteur reste ouvert

### Requirement: Détail affiché depuis l'aperçu de la session
Pendant une session d'édition, le système SHALL afficher dans le panneau
de détail la méthode, l'URL, les en-têtes et les paramètres tels que les
expose l'aperçu `bru-writer` des modifications validées, de sorte que ce
qui est affiché avant `Ctrl+S` soit ce qui sera écrit. Une sauvegarde
réussie MUST laisser le détail inchangé visuellement, hormis la
disparition de l'indicateur de modification.

#### Scenario: URL synchronisée avant sauvegarde
- **WHEN** l'utilisateur valide l'URL `https://h/items?page=3` dans une
  requête dont la section Paramètres de requête affichait `page` de valeur
  `2`
- **THEN** dès la validation, la section affiche `page` de valeur `3`,
  sans écriture disque

#### Scenario: Méthode synchronisée avant sauvegarde
- **WHEN** l'utilisateur choisit `POST` dans le sélecteur de méthode
  d'une requête `GET`
- **THEN** dès la validation, le détail affiche `POST`, sans écriture
  disque

#### Scenario: Touche capturée pendant la saisie d'une clé
- **WHEN** la saisie de la clé d'un ajout est ouverte et l'utilisateur
  tape `q`
- **THEN** `q` est ajouté au texte de la clé et l'application ne se ferme
  pas

### Requirement: Barre d'aide de la session
Le système SHALL afficher, quand une session d'édition est ouverte, à la
place de la barre d'état habituelle du panneau de détail : l'état courant
(Sélection de champ, Saisie, ou sélecteur de méthode), le nom du champ
sous le curseur, un indicateur visible tant que la session porte des
modifications non enregistrées, et les touches utiles dans l'état
courant. En Sélection de champ, ces touches MUST inclure l'ouverture de
la saisie, l'enregistrement et la fermeture, ainsi que `a`, `d` et `c`
quand le curseur est sur un en-tête ou un paramètre. En Saisie, elles
MUST inclure la validation, l'annulation et l'enregistrement, et
distinguer le corps (où `Entrée` insère un saut de ligne) des champs à
une ligne. Le sélecteur de méthode ouvert MUST rappeler la navigation,
la validation et l'annulation. Pendant l'ajout d'une entrée, la barre
MUST nommer la section visée et l'étape (clé ou valeur) ; pendant un
renommage, elle MUST l'indiquer. Un message de statut prioritaire
(exécution, échec de sauvegarde, refus de changement de sélection, refus
d'une modification invalide) MAY remplacer temporairement cette barre,
comme pour la barre d'état habituelle.

#### Scenario: Barre d'aide en saisie de l'URL
- **WHEN** l'état Saisie est actif sur le champ URL
- **THEN** la barre indique l'état Saisie, le nom du champ URL, et
  rappelle `Entrée` pour valider, `Échap` pour annuler et `Ctrl+S` pour
  enregistrer

#### Scenario: Barre d'aide en saisie du corps
- **WHEN** l'état Saisie est actif sur le corps
- **THEN** la barre rappelle que `Entrée` insère un saut de ligne et que
  `Tab` valide

#### Scenario: Barre d'aide du sélecteur de méthode
- **WHEN** le sélecteur de méthode est ouvert
- **THEN** la barre rappelle `↑`/`↓` pour naviguer, `Entrée` pour valider
  et `Échap` pour annuler

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
