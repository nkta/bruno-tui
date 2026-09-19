## Why

Le corps d'une réponse ne se consulte aujourd'hui que dans le panneau
Réponse (`response_text` / `body_lines` dans `src/app/view/detail.rs`),
limité à la largeur et à la hauteur du panneau, sans recherche plein
texte ni outil externe. L'utilisateur a demandé un raccourci pour ouvrir
le résultat dans `nano`.

## What Changes

- `Ctrl+E`, quel que soit le focus, ouvre le **corps brut** de la réponse
  de la requête sélectionnée — la même donnée que l'onglet Corps sans
  filtre appliqué (`body_lines(&outcome.result.response.data)` :
  chaîne affichée telle quelle, JSON mis en forme indentée sinon) — dans
  un éditeur externe : `$VISUAL` si définie, sinon `$EDITOR`, sinon
  `nano`.
- Le terminal est entièrement cédé à l'éditeur externe le temps qu'il
  tourne (mode brut désactivé, écran alternatif quitté, capture souris
  suspendue si active) puis restauré à sa fermeture, avec un redessin
  complet de l'interface.
- Sans réponse exploitable pour la sélection courante (aucune requête
  sélectionnée, jamais exécutée, ou corps `null`), `Ctrl+E` ne fait rien
  et affiche un message de statut plutôt que d'ouvrir un éditeur vide.
- Aucune modification faite dans l'éditeur externe n'est relue par
  `bruno-tui` : c'est une consultation à sens unique, pas une édition du
  corps de réponse. Le fichier temporaire créé pour l'éditeur est
  supprimé dès sa fermeture.

**Hors périmètre :**
- Relire et appliquer les modifications faites dans l'éditeur externe.
- Ouvrir autre chose que le corps brut (pas les en-têtes de réponse, pas
  l'onglet Tests, pas le résultat d'un filtre jq appliqué — toujours la
  donnée non filtrée, cohérent avec « aucune écriture dans un fichier
  `.bru`… » : ceci n'écrit d'ailleurs aucun fichier `.bru`, seulement un
  fichier temporaire hors collection).
- Choix de l'éditeur depuis l'application (pas de préférence en config) :
  uniquement `$VISUAL`/`$EDITOR`/`nano`, comme demandé.

## Capabilities

### New Capabilities

- `external-editor` : ouverture du corps brut de la réponse sélectionnée
  dans un éditeur externe, terminal cédé puis restauré.

### Modified Capabilities

Aucune capacité existante n'est modifiée.

## Impact

- `src/app/message.rs` : nouveau raccourci global `Ctrl+E` (libre —
  seuls `Ctrl+C`, `Ctrl+X`, `Ctrl+S` sont pris ce jour), traité comme
  `Ctrl+S` avant la répartition par capture de texte.
- `src/app/update.rs` : nouveau `Command` demandant l'ouverture externe
  (texte du corps déjà résolu par `update`, aucune I/O dans `update`
  lui-même — même principe que les commandes existantes).
- `src/app/mod.rs` (boucle `run`) : exécution de ce `Command`
  **directement dans la boucle, avant le redessin de fin de tour** —
  contrairement à `CopyToClipboard`/`SaveEdit`/`ResolveSecrets`, qui
  lancent un `spawn_blocking` et laissent la boucle continuer à
  redessiner pendant l'opération. Ici, redessiner pendant que l'éditeur
  possède le terminal corromprait l'affichage : voir `design.md` (D1) 
  pour la justification de cette dérogation ponctuelle au principe « la
  boucle d'événements ne bloque jamais ».
- `src/app/event.rs` (`spawn_terminal_reader`) : le thread de lecture du
  terminal doit cesser d'appeler `crossterm::event::read()` pendant que
  l'éditeur externe possède le terminal, sous peine de lui voler des
  frappes clavier — voir `design.md` (D2).
- Aucune nouvelle dépendance : `std::process::Command` et les primitives
  déjà utilisées par `ratatui::try_init`/`try_restore` suffisent.
- Aucun changement au format `.bru`, aucune écriture dans la collection.
