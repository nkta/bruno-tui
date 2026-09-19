## MODIFIED Requirements

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

Un clic sur la ligne `aucun` d'une section d'en-têtes, de paramètres de
requête ou de paramètres de chemin sans aucune entrée SHALL, que la
requête ait déjà une session d'édition ouverte ou non, ouvrir une session
si besoin puis commencer l'ajout d'une entrée à cette section — le même
effet qu'un clic sur sa ligne « + Ajouter » une fois la session déjà
ouverte, avec le même instantané de fraîcheur qu'un clic sur tout autre
champ. Cette règle ne s'applique qu'à une section sans aucune entrée : une
section qui en a déjà, sans session ouverte, n'affiche aucune ligne
correspondante à cliquer.

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

#### Scenario: Clic sur une section vide sans session
- **WHEN** aucune session n'est ouverte sur une requête sans paramètre de
  requête, et l'utilisateur clique sur la ligne `aucun` de la section
  Paramètres de requête
- **THEN** le détail a le focus, une session d'édition s'ouvre, et
  l'état Saisie est actif sur la clé d'une nouvelle entrée de cette
  section

#### Scenario: Clic sur une section vide, session déjà ouverte
- **WHEN** une session est dans l'état Sélection de champ sur une requête
  sans paramètre de chemin, et l'utilisateur clique sur la ligne `aucun`
  de la section Paramètres de chemin
- **THEN** l'état Saisie est actif sur la clé d'une nouvelle entrée de
  cette section

#### Scenario: Section non vide sans ligne à cliquer pour ajouter
- **WHEN** aucune session n'est ouverte sur une requête ayant déjà un
  en-tête, et l'utilisateur clique sous la ligne de cet en-tête, à
  l'intérieur du cadre de la section
- **THEN** le détail a le focus et aucune session ne s'ouvre : aucune
  ligne d'ajout n'est affichée hors session pour une section qui a déjà
  des entrées
