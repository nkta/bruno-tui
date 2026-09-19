## MODIFIED Requirements

### Requirement: Affichage des variables d'un environnement
Le panneau Environnement (`Focus::EnvironmentPicker`) SHALL se dessiner
dans un menu déroulant flottant ancré en haut à droite de l'écran, sous
la ligne de titre, superposé au reste de l'interface : l'arbre, le
détail et la réponse restent affichés derrière, tels qu'ils étaient
avant l'ouverture. `Entrée` sur une entrée de la liste qui désigne un
environnement valide (ni « Aucun », ni une entrée en erreur) SHALL
remplacer la liste des environnements par la liste de ses variables
(`Environment.variables`, une ligne par entrée, dans l'ordre du
fichier), sans changer l'environnement actif. Chaque ligne SHALL
afficher la clé et la valeur courante de la variable. Une variable
désactivée (clé préfixée de `~` dans le fichier) SHALL rester visible,
sa valeur affichée avec un style la distinguant comme non éditable,
cohérent avec l'affichage déjà utilisé pour une entrée désactivée dans
le détail d'une requête. `Entrée` sur « Aucun » ou sur une entrée en
erreur SHALL rester sans effet, comme `Droite` aujourd'hui sur ces
mêmes entrées.

#### Scenario: Le menu déroulant se superpose sans masquer le reste
- **WHEN** l'arbre affiche une collection avec une requête sélectionnée,
  et l'utilisateur appuie sur `E`
- **THEN** le menu déroulant Environnement apparaît en haut à droite,
  l'arbre et le détail de la requête sélectionnée restent visibles
  derrière

#### Scenario: Entrée sur un environnement valide affiche ses variables
- **WHEN** le menu déroulant Environnement a le focus, la liste affiche
  l'environnement `staging`, et l'utilisateur appuie sur `Entrée`
- **THEN** le menu affiche les variables de `staging`, une ligne par
  entrée de `Environment.variables`, dans l'ordre du fichier

#### Scenario: Entrée sur « Aucun » ne fait rien
- **WHEN** le menu déroulant Environnement a le focus, l'entrée courante
  est « Aucun », et l'utilisateur appuie sur `Entrée`
- **THEN** la liste des environnements reste affichée, sans changement

#### Scenario: Entrée sur un environnement en erreur ne fait rien
- **WHEN** le menu déroulant Environnement a le focus, l'entrée courante
  est un environnement dont le fichier est en erreur de lecture, et
  l'utilisateur appuie sur `Entrée`
- **THEN** la liste des environnements reste affichée, sans changement

#### Scenario: Retour à la liste des environnements
- **WHEN** le menu affiche les variables d'un environnement, aucune
  saisie n'est en cours, et l'utilisateur appuie sur `Échap`
- **THEN** la liste des environnements réapparaît dans le menu, la
  sélection sur l'environnement précédemment ouvert

#### Scenario: Échap depuis la liste ferme le menu
- **WHEN** le menu affiche la liste des environnements (pas la vue des
  variables), et l'utilisateur appuie sur `Échap`
- **THEN** le menu déroulant disparaît, le focus revient à l'arbre
