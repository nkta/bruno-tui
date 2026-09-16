## Why

Un essai manuel de `bruno-tui` contre une vraie collection
(`examples/jsonplaceholder/`) a montré une interface jugée peu lisible :
un rendu réel capturé en `TestBackend` confirme un usage incohérent des
couleurs (la méthode HTTP, le statut d'exécution et les erreurs sont déjà
colorés, mais les libellés du détail, les titres de section et l'arbre ne
le sont pas), un panneau de détail présenté comme une liste de champs
bruts (`Chemin : `, `Seq : `, `Enfants : `) plutôt qu'une fiche pensée
pour la lecture, et de grands espaces vides dans les panneaux peu remplis
(par exemple le panneau de diagnostics sur une collection sans erreur :
« aucune erreur » suivi de 25 lignes vides sur un terminal de 30 lignes).
L'outil est fonctionnellement complet (9 capacités déjà livrées) mais son
apparence nuit à son adoption pour l'exploration manuelle d'API,
l'un des deux usages visés par le projet.

## What Changes

- Une palette de couleurs cohérente et systématique, appliquée à toutes
  les catégories d'information déjà distinguées par le code mais pas
  toujours par la couleur : méthodes HTTP, statuts d'exécution
  (succès/échec/en cours), erreurs de chargement, focus actif,
  libellés versus valeurs, titres de section.
- Une présentation du panneau de détail organisée en fiche lisible
  (hiérarchie visuelle claire entre le titre du nœud, ses sections, et
  les valeurs), plutôt que la liste actuelle de paires `Libellé : valeur`
  sans distinction visuelle entre elles.
- Un traitement des états peu remplis ou vides (arbre avec peu de nœuds,
  diagnostics sans erreur, historique sans entrée) qui réduit
  l'impression d'espace perdu, sans changer le contenu informatif déjà
  spécifié par chaque capacité concernée.

Hors périmètre : toute nouvelle fonctionnalité, touche, ou panneau ;
tout changement du contenu affiché (les informations déjà spécifiées par
`tui-shell`, `diagnostics-and-history`, `field-editing`,
`response-filter`, `search-and-yank` et `environment-picker` restent les
mêmes, seule leur présentation change) ; toute dépendance externe à un
moteur de thème ou de configuration de couleurs — le changement reste
dans les capacités déjà disponibles de `ratatui` (`Style`, `Color`,
`Modifier`), déjà utilisées par le code actuel.

## Capabilities

### New Capabilities
- `visual-theme`: règles de présentation visuelle communes à tous les
  panneaux de `bruno-tui` — code couleur cohérent par catégorie
  d'information, distinction visuelle entre libellés et valeurs,
  organisation du panneau de détail en fiche lisible, traitement des
  états peu remplis ou vides. Ces règles s'appliquent par-dessus le
  contenu déjà spécifié par les capacités existantes, sans le changer.

### Modified Capabilities
<!-- Aucune : les exigences déjà écrites (ex. « Le panneau ayant le focus
     MUST être visuellement distingué » dans tui-shell) ne précisent
     déjà ni couleur ni disposition exacte — ce changement les respecte
     en changeant seulement la façon dont elles sont satisfaites, ce qui
     relève de l'implémentation (design.md), pas d'une exigence
     nouvelle ou modifiée sur le contenu affiché. -->

## Impact

- Code : `src/app/view/mod.rs`, `src/app/view/tree.rs`,
  `src/app/view/detail.rs`, `src/app/view/panels.rs`. Pas de changement
  à `src/app/model.rs`, `src/app/message.rs`, `src/app/update.rs` : la
  présentation ne dépend d'aucun nouvel état, sauf si le design l'exige
  pour un besoin purement visuel (à confirmer en design.md).
  `src/collection/`, `src/runner/`, `src/writer/` inchangés.
- Dépendances : aucune nouvelle crate ; usage plus systématique de
  `ratatui::style` déjà présent dans `Cargo.toml`.
- Tests : les tests de rendu `TestBackend` existants vérifient déjà le
  contenu textuel affiché (ex. présence de `GET`, d'un marqueur
  d'erreur) ; ce changement ne doit pas les casser puisque le contenu
  reste identique. De nouveaux tests vérifient les styles appliqués
  (couleur, gras) là où c'est observable via l'API de test de `ratatui`
  (`Buffer`/`Cell::style`), en plus du texte déjà couvert.
