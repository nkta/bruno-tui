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
suivantes : méthode HTTP, statut d'exécution en succès, statut
d'exécution en échec, statut d'exécution en cours, nœud ou fichier en
erreur. Deux catégories distinctes MUST NOT partager la même couleur.
Une même catégorie SHALL porter la même couleur partout où elle apparaît
(arbre, détail, diagnostics, historique).

#### Scenario: Statut d'exécution dans l'arbre et dans l'historique
- **WHEN** une requête a échoué lors de sa dernière exécution
- **THEN** son marqueur d'échec dans l'arbre et son verdict dans le
  panneau d'historique portent la même couleur

#### Scenario: Erreur de chargement distincte du statut d'échec
- **WHEN** l'arbre affiche un nœud en erreur de chargement à côté d'une
  requête dont la dernière exécution a échoué
- **THEN** les deux marqueurs sont visuellement distincts

### Requirement: Distinction visuelle entre libellés et valeurs
Dans le panneau de détail, l'interface SHALL présenter chaque libellé de
champ (par exemple « Chemin », « Méthode », « Auth ») avec un style
distinct de celui de la valeur qui le suit, pour que l'œil distingue
immédiatement la structure de l'information de son contenu.

#### Scenario: Champ du détail d'un dossier
- **WHEN** le détail d'un dossier affiche son chemin relatif
- **THEN** le style du libellé « Chemin » diffère de celui de la valeur
  affichée

### Requirement: Présentation du détail en fiche lisible
Le panneau de détail SHALL distinguer visuellement trois niveaux de
hiérarchie déjà présents dans son contenu : le titre du nœud sélectionné,
les titres de section (en-têtes, paramètres, résultat, etc.), et les
champs qu'ils contiennent. Le titre du nœud MUST rester le plus mis en
valeur des trois niveaux. Cette hiérarchie SHALL s'appliquer identiquement
à une requête, un dossier, et un nœud en erreur, sans changer les
informations déjà spécifiées par `tui-shell` pour chacun.

#### Scenario: Trois niveaux distincts sur une requête
- **WHEN** le détail d'une requête avec au moins un en-tête est affiché
- **THEN** le titre du nœud, le titre de la section « En-têtes », et la
  ligne de l'en-tête lui-même portent chacun un style distinct

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

### Requirement: Non-régression du contenu affiché
Aucune information visible avant ce changement ne SHALL disparaître, et
aucune information nouvelle ne SHALL apparaître : seule la présentation
(couleur, style, disposition) change. Les scénarios déjà spécifiés par
`tui-shell`, `diagnostics-and-history`, `field-editing`,
`response-filter`, `search-and-yank` et `environment-picker` sur le
contenu affiché restent vrais sans modification.

#### Scenario: Contenu du détail inchangé
- **WHEN** le détail de `post-json` (fixture `parser-cases`) est affiché
  avant et après ce changement
- **THEN** le texte brut affiché (méthode, URL, en-têtes, corps) est
  identique dans les deux cas, seul son style diffère
