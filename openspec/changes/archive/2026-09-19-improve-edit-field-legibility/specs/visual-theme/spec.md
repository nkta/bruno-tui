## MODIFIED Requirements

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
