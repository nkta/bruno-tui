## Why

Nicolas a fourni une image de référence (`exempleTui/Gemini_Generated_Image_suk0qdsuk0qdsuk0.jpeg`),
un mockup du client Bruno (GUI) dont la palette — fond bleu-nuit,
bordures vertes, accents orange sur les titres/éléments actifs, réponse
JSON en vert clair — est plus contrastée et plus proche de l'identité
visuelle de Bruno que la palette actuelle de `bruno-tui` (couleurs
standard de terminal choisies au fil de `improve-visual-design`, sur le
fond par défaut du terminal, jamais habillé). Ce changement fait évoluer
la palette concrète posée par ce précédent changement, sans remettre en
cause la structure de la capacité `visual-theme` qu'il a établie
(catégories distinctes, hiérarchie du détail, traitement des panneaux
vides).

## What Changes

- Remplacement des couleurs concrètes de chaque catégorie déjà
  centralisée dans `src/app/view/theme.rs` (méthode HTTP, succès, échec,
  en cours, erreur de chargement, focus, titre de section, libellé) par
  des couleurs alignées sur la palette de référence (verts et oranges
  en priorité), en conservant la garantie déjà testée que deux
  catégories distinctes ne partagent jamais la même couleur.
- Ajout d'un fond d'application cohérent (bleu-nuit), appliqué à
  l'ensemble du terminal plutôt qu'au fond par défaut hérité du terminal
  de l'utilisateur — un aspect que `visual-theme` ne couvre pas encore
  aujourd'hui.
- **BREAKING** aucune : changement de style pur, aucune interface
  programmatique ni format de fichier concerné.

Hors périmètre : toute nouvelle fonctionnalité, touche, ou panneau ;
tout changement du contenu affiché ; toute configuration de thème par
l'utilisateur (couleurs fixes, choisies dans ce changement, comme pour
`improve-visual-design`) ; tout changement à `layout()`, à `Model`, à
`Message` ou à `update.rs`.

## Capabilities

### New Capabilities
Aucune.

### Modified Capabilities
- `visual-theme` : les couleurs concrètes retenues pour chaque catégorie
  changent (toujours distinctes entre catégories, exigence déjà en
  place et inchangée) ; ajout d'une exigence sur un fond d'application
  cohérent, absente de la version actuelle de la capacité.

## Impact

- Code : `src/app/view/theme.rs` (nouvelles valeurs de couleur par
  catégorie, nouvelle constante de fond), `src/app/view/mod.rs`
  (application du fond sur la zone entière du terminal, avant le rendu
  des panneaux). Pas de changement à `Model`, `Message`, `update.rs`, ni
  à `layout()`. `tree.rs`, `detail.rs`, `panels.rs` inchangés dans leur
  logique : ils continuent d'importer les constantes de `theme.rs`, sans
  modification de leur code (seules les valeurs importées changent).
- Dépendances : aucune nouvelle crate ; usage de `ratatui::style` déjà
  présent dans `Cargo.toml`.
- Tests : les tests de rendu existants qui vérifient le texte affiché ne
  doivent pas changer (aucun changement de contenu). Les tests qui
  vérifient une couleur précise dans `theme.rs`, `tree.rs`, `detail.rs`
  et `panels.rs` seront mis à jour pour refléter la nouvelle palette. De
  nouveaux tests vérifient le fond appliqué là où c'est observable via
  l'API de test de `ratatui` (`Buffer`/`Cell::bg`).
