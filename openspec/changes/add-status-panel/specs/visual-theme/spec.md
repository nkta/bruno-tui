## MODIFIED Requirements

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
