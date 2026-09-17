## Why

bruno-tui ne s'utilise aujourd'hui qu'au clavier : crossterm ne reçoit
aucun événement souris (capture jamais activée, `AppEvent::Terminal(_)`
ignoré dans `to_message`). Pour explorer une collection, désigner un champ
à modifier ou copier un passage de réponse, cliquer ou faire défiler au
survol est plus direct que d'enchaîner `Tab`, flèches et `v`/`y`, surtout
avec trois panneaux côte à côte.

## What Changes

- Activation de la capture souris du terminal au démarrage, désactivée à
  la fermeture et en cas de panique, comme le mode brut et l'écran
  alternatif.
- Clic gauche sur un panneau (arbre, détail, réponse) : lui donne le
  focus. Sur une ligne de l'arbre : sélectionne ce nœud ; sur le nœud déjà
  sélectionné s'il s'agit d'un dossier : le déplie ou le replie.
- Molette : fait défiler le panneau survolé (arbre, détail ou réponse),
  sans changer le focus ni la sélection.
- Clic sur un champ éditable du panneau Détail d'une requête (URL,
  en-tête, paramètre, corps éditable) : ouvre au besoin la session
  d'édition directe (`field-editing`), place le curseur de champ sur ce
  champ et commence sa Saisie, comme `Entrée`. Pendant une Saisie, un
  clic ailleurs valide d'abord la saisie comme `Tab` (aucune perte de
  texte, aucune écriture) ; un clic dans l'arbre respecte le refus de
  changer de requête tant que la session porte des modifications non
  enregistrées.
- Panneau Statut (`status-panel`) : non focalisable, clics et molette y
  sont sans effet.
- Glisser dans le détail ou la réponse : sélection de lignes, affichée
  comme la sélection visuelle existante et copiée par `y` via le chemin de
  copie de `search-and-yank` (presse-papiers asynchrone, jeton,
  confirmation dans la barre d'état).
- Conflit avec la sélection native du terminal : option CLI `--no-mouse`
  (capture jamais activée) et touche `M` qui bascule la capture pendant la
  session ; l'état courant est rappelé dans la barre d'état.
- Aucune touche existante ne change de comportement (`M` et `y` restent
  du texte en Saisie) ; les zones cliquables
  sont calculées par des fonctions pures partagées avec `view` (taille du
  terminal + modèle), sans I/O.

## Capabilities

### New Capabilities
- `mouse-support` : capture souris et sa désactivation, focus et sélection
  au clic, défilement à la molette, saisie d'un champ au clic, sélection
  de lignes au glisser et copie, cohabitation avec les saisies, panneaux
  superposés, panneau Statut et confirmations.

### Modified Capabilities
- `tui-shell` : la ligne de commande accepte `--no-mouse` ; la restauration
  du terminal inclut la désactivation de la capture souris ; l'invariant
  « nœud sélectionné toujours visible » est précisé pour la navigation au
  clavier, la molette pouvant faire défiler l'arbre sans déplacer la
  sélection.

## Impact

- Code : `src/main.rs` (activation/désactivation de la capture, hook de
  panique), `src/app/cli.rs` (`--no-mouse`), `src/app/message.rs`
  (traduction des `Event::Mouse`), `src/app/update.rs` (nouveaux messages,
  `Command::SetMouseCapture`), `src/app/mod.rs` (exécution de la commande),
  `src/app/model.rs` (état de glisser, capture active, borne explicite de
  sélection), `src/app/view/` (module de hit-testing pur, qui réutilise
  `request_text_and_fields` pour les lignes des champs éditables).
- Dépendances : aucune nouvelle ; `ratatui` (crossterm) fournit déjà
  `EnableMouseCapture`/`DisableMouseCapture` et `Paragraph::line_count`
  (feature `unstable-rendered-line-info` déjà activée).
- Aucun accès disque, aucun processus nouveau ; `bru` n'est pas concerné.
- Intègre `improve-direct-editing` et `add-status-panel`, déjà fusionnés
  sur `main` ; point de contact avec `add-entry-management`, en cours en
  parallèle : voir `design.md`.
