## Why

`add-request-run` donne à l'interface le corps de la dernière réponse
d'une requête, mais seulement affiché tel quel. Pour l'exploration
manuelle d'API, un corps JSON volumineux (liste longue, objets imbriqués)
devient vite illisible sans pouvoir en extraire un sous-ensemble. Le
projet `brio` (inspiration déjà retenue pour ce dépôt) montre l'intérêt
d'un filtre jq directement dans le panneau de détail.

## What Changes

- La touche `|` ouvre, sur le panneau de détail d'une requête dont la
  dernière exécution a produit une réponse avec un corps non vide
  (`response.data` non nul), une saisie de filtre au style jq.
- La validation du filtre (`Entrée`) l'exécute sur le corps JSON de la
  réponse et remplace l'affichage du corps par le résultat, mis en forme
  lisible (une valeur par ligne si le filtre produit plusieurs sorties).
  `Échap` annule la saisie sans rien changer à l'affichage.
- Un filtre qui ne compile pas (syntaxe jq invalide) ou dont l'exécution
  échoue (type incompatible, division par zéro, etc.) affiche un message
  d'erreur à la place du résultat, sans jamais faire planter l'interface
  ni perdre le corps original : rouvrir la saisie (`|`) permet de le
  corriger.
- Le filtre est volatile : il ne modifie ni le fichier `.bru`, ni le
  rapport reçu de `bru run`, et se réinitialise dès que la sélection de
  l'arbre change (retour à l'affichage brut du corps).
- Nouvelles dépendances `jaq-core`, `jaq-std`, `jaq-json` (interpréteur
  jq embarqué, licence MIT), justifiées dans `design.md`.

Hors périmètre, pour des changements suivants :
- filtre sur les en-têtes, sur un corps non-JSON, ou appliqué à plusieurs
  requêtes à la fois ;
- mémorisation d'un filtre entre deux sélections ou entre deux lancements
  de l'application ;
- thème de couleurs configurable (autre idée reprise de `brio`) : mérite
  son propre changement (par exemple `add-theme-config`, avec un fichier
  `~/.config/bruno-tui/config.toml`), sans rapport avec le filtrage.

## Capabilities

### New Capabilities
- `response-filter`: saisie et application d'un filtre jq sur le corps
  JSON de la dernière réponse d'une requête, affichage du résultat ou
  d'une erreur non fatale, réinitialisation à chaque changement de
  sélection.

### Modified Capabilities
<!-- Aucune modification de spec existante : ce changement lit
     `model.run` (contrat introduit par le changement sœur
     `add-request-run`, en cours de rédaction) sans changer son contrat,
     et étend l'affichage du détail d'une requête déjà prévu par
     `tui-shell` sans changer son comportement hors filtre actif. -->

## Impact

- Code : nouveau module `src/app/filter.rs` (ou équivalent, précisé dans
  `design.md`) ; extension de `Model`, `Message`, `update` et
  `view/detail.rs`. Aucun fichier de `src/collection/` ni `src/runner/`
  touché.
- Dépendances : `jaq-core`, `jaq-std`, `jaq-json` (feature `serde`),
  ajoutant une trentaine de crates transitives (détail et alternatives
  écartées dans `design.md`).
- Tests : unitaires sur `update` (ouverture/validation/annulation,
  filtre valide, filtre invalide, réinitialisation au changement de
  sélection) ; un test utilisant le corps JSON réel de
  `tests/fixtures/reports/mixed.json`.
- Dépendance de conception : ce changement suppose l'existence du type
  `RunState`/`RequestOutcome` décrit dans
  `openspec/changes/add-request-run/design.md` (D2). Si ce contrat change
  avant l'implémentation, `design.md` du présent changement devra être
  ajusté en conséquence.
