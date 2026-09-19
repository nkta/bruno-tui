# visual-theme Specification

## Purpose

Définir des règles de présentation visuelle communes à tous les panneaux
de `bruno-tui`, pour que l'interface reste lisible et cohérente à mesure
que de nouvelles capacités ajoutent leurs propres panneaux, sans jamais
changer le contenu informatif déjà spécifié par ces capacités.

## Requirements

### Requirement: Code couleur cohérent par catégorie d'information
L'interface SHALL utiliser une couleur distincte et constante, dans tous
les panneaux où l'information apparaît, pour chacune des catégories
suivantes : méthode HTTP, succès, échec, exécution en cours, nœud ou
fichier en erreur, réponse de redirection, réponse d'erreur client.
Deux catégories distinctes MUST NOT partager la même couleur, et les
couleurs de redirection et d'erreur client MUST NOT non plus être
celles de la bordure du panneau ayant le focus. Une même catégorie
SHALL porter la même couleur partout où elle apparaît (arbre, détail,
statut, diagnostics, historique).

Les classes de statut de réponse affichées par `status-panel` SHALL se
rattacher à ces catégories ainsi : une réponse 2xx appartient à la
catégorie succès ; une réponse 5xx et une requête en erreur sans
réponse appartiennent à la catégorie échec ; une réponse 3xx à la
catégorie redirection ; une réponse 4xx à la catégorie erreur client.
Une requête ignorée ou un statut hors de ces classes SHALL utiliser un
style neutre, sans couleur de catégorie.

#### Scenario: Statut d'exécution dans l'arbre et dans l'historique
- **WHEN** une requête a échoué lors de sa dernière exécution
- **THEN** son marqueur d'échec dans l'arbre et son verdict dans le
  panneau d'historique portent la même couleur

#### Scenario: Erreur de chargement distincte du statut d'échec
- **WHEN** l'arbre affiche un nœud en erreur de chargement à côté d'une
  requête dont la dernière exécution a échoué
- **THEN** les deux marqueurs sont visuellement distincts

#### Scenario: Réponse 5xx à la couleur d'échec
- **WHEN** une requête a reçu une réponse `500` et que sa dernière
  exécution est en échec
- **THEN** le badge `500` du panneau Statut et le marqueur d'échec de
  l'arbre portent la même couleur

#### Scenario: Classes 3xx et 4xx distinctes des autres catégories
- **WHEN** le panneau Statut affiche un badge `301` puis, pour une autre
  requête, un badge `404`
- **THEN** leurs deux couleurs diffèrent l'une de l'autre et de celles
  du succès, de l'échec, de l'exécution en cours, de l'erreur de
  chargement, de la méthode HTTP et de la bordure de focus

### Requirement: Distinction visuelle entre libellés et valeurs
Dans le panneau de détail, l'interface SHALL présenter chaque libellé de
champ (par exemple « Chemin », « Méthode », « Auth ») avec un style
distinct de celui de la valeur qui le suit, pour que l'œil distingue
immédiatement la structure de l'information de son contenu.

Cette distinction SHALL aussi s'appliquer à la clé de chaque entrée
clé/valeur (en-tête, paramètre de requête, paramètre de chemin) : la clé
porte le même style de libellé que les champs simples, la valeur le
style normal. Elle s'applique que l'entrée soit affichée hors session
(lecture seule) ou pendant une session d'édition (`field-editing`), y
compris une entrée désactivée, une clé en cours de renommage, ou une
entrée provisoire affichée pendant un ajout.

Quand le corps d'une requête est d'un type éditable par `field-editing`
(`json`, `text`, `xml`, `sparql`, `graphql`), l'interface SHALL le
présenter avec une bordure ou un fond qui le distingue visuellement du
texte de détail environnant, qu'une session d'édition soit ouverte ou
non. Un corps d'un type non éditable (absence de corps, formulaire) MUST
NOT recevoir cette présentation.

#### Scenario: Champ du détail d'un dossier
- **WHEN** le détail d'un dossier affiche son chemin relatif
- **THEN** le style du libellé « Chemin » diffère de celui de la valeur
  affichée

#### Scenario: En-tête affiché hors session
- **WHEN** le détail d'une requête ayant un en-tête `Content-Type` est
  affiché sans session d'édition ouverte
- **THEN** le style de la clé `Content-Type` diffère de celui de sa
  valeur

#### Scenario: En-tête désactivé pendant une session d'édition
- **WHEN** une session d'édition est ouverte et le curseur de champ est
  sur un en-tête désactivé
- **THEN** la clé de cet en-tête porte le style de libellé, distinct de
  celui de sa valeur, en plus de l'indication « (désactivé) »

#### Scenario: Entrée provisoire pendant un ajout
- **WHEN** une session d'édition est en train d'ajouter un en-tête et
  que la saisie de la valeur est en cours
- **THEN** la clé déjà validée de cette entrée provisoire porte le même
  style de libellé que les entrées existantes

#### Scenario: Corps éditable distingué du texte de détail
- **WHEN** une requête dont le corps est de type `json` est sélectionnée,
  avec ou sans session d'édition ouverte
- **THEN** la zone du corps est présentée avec une bordure ou un fond qui
  la distingue visuellement du reste du texte de détail

#### Scenario: Corps sans type éditable non affecté
- **WHEN** une requête dont le corps est `none` est sélectionnée
- **THEN** aucune zone de saisie n'est présentée pour un corps absent, et
  le détail reste inchangé par rapport à avant ce changement

### Requirement: Présentation du détail en fiche lisible
Le panneau de détail SHALL distinguer visuellement trois niveaux de
hiérarchie déjà présents dans son contenu : le titre du nœud sélectionné,
les titres de section (en-têtes, paramètres, résultat, etc.), et les
champs qu'ils contiennent. Le titre du nœud MUST rester le plus mis en
valeur des trois niveaux. Cette hiérarchie SHALL s'appliquer identiquement
à une requête, un dossier, et un nœud en erreur, sans changer les
informations déjà spécifiées par `tui-shell` pour chacun.

Pour une requête, les sections « En-têtes », « Paramètres de requête »,
« Paramètres de chemin » et « Corps » SHALL être présentées chacune dans
un cadre qui les entoure, avec le nom de la section affiché dans la
bordure du cadre plutôt qu'en ligne de titre séparée. Le contenu de
chaque section (entrée clé/valeur, lignes du corps, mention « aucun »
quand la section est vide) reste affiché à l'intérieur de ce cadre, sans
perte ni ajout d'information par rapport à la présentation non encadrée.
Les autres champs du détail d'une requête (nom, chemin, méthode, URL,
auth, indicateurs de scripts/tests/assertions) et le détail d'un dossier
ou d'un nœud en erreur MUST NOT être encadrés par cette exigence : ils
gardent la présentation en ligne simple déjà définie par `tui-shell`.

Cette présentation encadrée MUST NOT changer la séquence logique de
lignes que consomment `field-editing`, `mouse-support` et
`search-and-yank` : chaque ligne de bordure du cadre est une ligne
supplémentaire dans cette séquence, au même titre que l'étaient les
lignes de titre de section qu'elle remplace, et ne porte aucun champ
éditable.

#### Scenario: Trois niveaux distincts sur une requête
- **WHEN** le détail d'une requête avec au moins un en-tête est affiché
- **THEN** le titre du nœud, le cadre de la section « En-têtes » avec son
  nom dans la bordure, et la ligne de l'en-tête lui-même portent chacun
  un style distinct

#### Scenario: Section encadrée sans perte de contenu
- **WHEN** le détail de `post-json` (fixture `parser-cases`) est affiché
  avant et après ce changement
- **THEN** le texte brut de chaque champ (en-tête, corps) affiché dans la
  séquence de lignes du détail est identique dans les deux cas ; seule sa
  mise en forme (cadre, style) diffère

#### Scenario: Section vide toujours encadrée
- **WHEN** une requête sans paramètre de chemin est sélectionnée
- **THEN** le cadre de la section « Paramètres de chemin » est affiché
  avec la mention « aucun » à l'intérieur, plutôt que d'être omis

#### Scenario: Dossier et nœud en erreur non affectés
- **WHEN** un dossier ou un nœud en erreur est sélectionné
- **THEN** son détail ne contient aucun cadre de section, inchangé par
  rapport à avant ce changement

### Requirement: Présentation intentionnelle des panneaux à message unique
Un panneau plein corps dont tout le contenu est un unique message de
substitution (par exemple « aucune erreur » ou « aucune exécution »,
faute d'entrée à lister) SHALL présenter ce message de façon délibérée
(mise en valeur du texte, position dans la zone) plutôt que de le laisser
seul en haut d'une zone par ailleurs vide. Cette exigence porte sur la
présentation, jamais sur le contenu informatif : le panneau continue
d'afficher exactement le message déjà spécifié par sa capacité, et cette
exigence ne s'applique pas à un panneau qui affiche une liste, même
courte (arbre à peu de nœuds, liste d'environnements à une entrée) — une
liste réelle garde sa présentation habituelle, alignée en haut du
panneau.

#### Scenario: Diagnostics sans erreur sur un grand terminal
- **WHEN** la collection ne comporte aucune erreur et le terminal fait
  30 lignes
- **THEN** le panneau de diagnostics affiche toujours le message « aucune
  erreur », présenté de façon délibérée plutôt que seul en haut d'une
  zone autrement vide

### Requirement: Fond d'application cohérent
L'interface SHALL appliquer un fond de couleur cohérent à l'ensemble de
la zone du terminal qu'elle occupe, plutôt que de laisser apparaître le
fond par défaut hérité du terminal de l'utilisateur. Ce fond SHALL
rester le même sur tout l'écran (titre, panneaux, barre d'état), pour
que l'interface se présente comme une surface unique plutôt que comme
des panneaux flottant sur un arrière-plan qui leur est étranger.

#### Scenario: Fond identique sur toute la hauteur de l'écran
- **WHEN** la collection est chargée et affichée sur un terminal de
  taille quelconque au-dessus du minimum supporté
- **THEN** la couleur de fond de la ligne de titre, des panneaux et de
  la barre d'état est la même partout à l'écran

### Requirement: Non-régression du contenu affiché
Les règles de présentation de cette capacité (couleur, style,
disposition) MUST NOT faire disparaître ni ajouter d'information : elles
ne changent que la manière dont le contenu défini par les autres
capacités est présenté. Une information nouvelle ou déplacée d'un
panneau à un autre ne SHALL provenir que d'une capacité qui la spécifie
explicitement (par exemple `status-panel`, qui déplace le verdict, le
statut et le temps de réponse du panneau Réponse vers le panneau Statut
et y ajoute libellé, taille et décompte des vérifications). Les
scénarios de `tui-shell`, `diagnostics-and-history`, `field-editing`,
`response-filter`, `search-and-yank` et `environment-picker` sur le
contenu affiché restent vrais.

#### Scenario: Contenu du détail inchangé
- **WHEN** le détail de `post-json` (fixture `parser-cases`) est affiché
  avant et après ce changement
- **THEN** le texte brut affiché (méthode, URL, en-têtes, corps) est
  identique dans les deux cas, seul son style diffère

#### Scenario: Déplacement porté par une capacité
- **WHEN** une requête exécutée est sélectionnée
- **THEN** son verdict, son statut et son temps de réponse sont affichés
  une seule fois, dans le panneau Statut, conformément à `status-panel`
