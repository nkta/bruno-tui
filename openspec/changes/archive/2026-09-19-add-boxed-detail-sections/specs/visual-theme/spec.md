## MODIFIED Requirements

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
