## 1. Ligne de commande et terminal

- [x] 1.1 Ajouter `--no-mouse` à `src/app/cli.rs` (champ `mouse` de `Command::Run`, `--no-mouse=…` refusé, `USAGE` mentionnant `--no-mouse`, `M` et `Shift`+glisser) ; vérifier par des tests unitaires de `parse` (présent, absent, répété, avec valeur → erreur d'usage)
- [x] 1.2 Créer le trait `MouseCapture`, `TerminalMouseCapture` (`Enable/DisableMouseCapture` sur `stdout`) et un faux pour les tests ; vérifier que `cargo clippy --all-targets -- -D warnings` passe sans `unwrap`/`expect` hors tests
- [x] 1.3 Dans `src/main.rs` : activer la capture après `try_init` sauf `--no-mouse`, envelopper le hook de panique pour envoyer `DisableMouseCapture` avant le hook de ratatui, envoyer `DisableMouseCapture` avant `try_restore` ; vérifier que le binaire compile et que `--no-mouse` et l'état effectif sont transmis à `run` (test de `parse` + relecture du chemin dans `main`)

## 2. Modèle, messages et commande de capture

- [x] 2.1 Ajouter `MouseState { capture, drag }` au `Model`, initialisé depuis `run` (nouveau paramètre d'état initial passé par `main` et par les tests) ; vérifier par un test que le modèle reflète l'état fourni
- [x] 2.2 Traduire `Event::Mouse` en `Message::Mouse(MouseInput)` dans `to_message` (gauche Press/Drag/Release, molette haut/bas ; `Moved`, autres boutons et défilement horizontal → `None`) ; vérifier par des tests unitaires de `to_message`
- [x] 2.3 Lier `M` à `Message::ToggleMouseCapture` dans la table hors capture, émettre `Command::SetMouseCapture`, l'exécuter dans la boucle via `MouseCapture` et renvoyer `Message::MouseCaptureChanged` ; `StatusMessage` activée / désactivée / erreur, état inchangé en cas d'échec, `drag` annulé à la désactivation ; vérifier par des tests `update`, un test de boucle avec le faux `MouseCapture` (succès et échec), et des tests montrant que `M` et `y` sont du texte en saisie de recherche et en état Saisie d'un champ
- [x] 2.4 Ajouter `head: Option<u16>` à `DetailSelection` et l'utiliser dans `selection_range`/`response_selection_range` ; vérifier que les tests existants de `search-and-yank` passent inchangés et qu'un nouveau test montre une borne fixe malgré le défilement

## 3. Géométrie pure

- [x] 3.1 Créer `src/app/view/hit.rs` : `hit_test(model, column, row) -> Option<Hit>` à partir de `layout_for`/`inner`, `tree.offset` et des défilements, panneau Statut → `None`, correspondance ligne écran → ligne logique avec retour à la ligne (`Paragraph::line_count`) sauf pendant une saisie dans le détail (même prédicat que `view`) ; vérifier par des tests comparant, sur rendu `TestBackend`, le texte à la position cliquée avec la ligne renvoyée (lignes courtes, lignes retournées, défilement non nul, saisie en cours, bordures, hors contenu, panneau Statut, terminal trop petit)
- [x] 3.2 Ajouter une fonction pure donnant le champ éditable d'une ligne logique du détail à partir de `request_text_and_fields` ; vérifier par des tests sur `post-json` (URL, `Content-Type`, corps multi-lignes), sur une ligne de section et sur un corps non éditable

## 4. Interactions souris dans `update`

- [x] 4.1 Garde unique en tête du traitement souris (capture inactive, confirmation en attente, capture de texte autre que `Input`, panneau superposé, collection non chargée, taille insuffisante, hors zones dont panneau Statut) ; vérifier par des tests qu'aucun de ces états n'est modifié par un clic, un glisser ou un cran de molette, dont le scénario « Clic sur le panneau Statut »
- [x] 4.2 Extraire `select_row` de `navigate_tree` (verrou `selection_locked`, `EditLocked`, `clear_detail_view_state`, filtre, visibilité) et l'utiliser pour le clic dans l'arbre ; focus, bascule du dépliage sur le dossier déjà sélectionné, bordure ou sous la dernière ligne → focus seul ; vérifier par les tests existants de navigation (inchangés) et les scénarios « Sélection d'une requête au clic », « Focus au clic », « Dépliage au clic » et « Clic dans un arbre défilé »
- [x] 4.3 Session d'édition : validation de la Saisie (comme `Tab`) à tout appui hors du champ en saisie, appui sur le champ en saisie sans effet, focus changé sans fermer la session, verrou de changement de requête ; vérifier par les scénarios « Session sans modification », « Session modifiée », « Saisie en cours et clic dans l'arbre », « Saisie en cours et clic dans la réponse » et « Clic sur le champ en cours de saisie »
- [x] 4.4 Molette : défilement du panneau survolé de trois lignes (détail/réponse via le défilement clavier, sans curseur de champ ni saisie modifiés ; arbre via `offset` borné sans toucher la sélection) ; vérifier par les scénarios « Défilement de la réponse sans focus », « Butée en fin de contenu », « Molette dans l'arbre », « Molette pendant une session d'édition » et le scénario `tui-shell` « Retour de la sélection après la molette »
- [x] 4.5 Appui/glisser/relâchement dans le détail et la réponse : focus, ancrage, sélection à borne fixe, défilement d'une ligne au-delà des bords, clic sans glisser qui lève la sélection du panneau, `Release` orphelin sans effet ; vérifier par les scénarios « Glisser sur trois lignes du détail », « Glisser au-delà du bas du panneau », « Défilement après relâchement » et l'indépendance entre détail et réponse
- [x] 4.6 Clic sur un champ du détail : ouverture de session si besoin, curseur de champ, `begin_input` (état Saisie, curseur de texte en fin de valeur) ; clic hors champ → focus seul ; glisser commencé sur un champ → aucune session ; vérifier par les scénarios « Clic sur l'URL sans session », « Clic sur un champ en sélection de champ », « Passage d'un champ à un autre », « Clic sur un libellé de section », « Corps non éditable » et « Glisser commencé sur l'URL »
- [x] 4.7 Copie : vérifier par des tests `update` que `y` après un glisser émet `Command::CopyToClipboard` avec les lignes jointes par `\n` et lève la sélection au succès, qu'un échec la conserve, que le relâchement seul n'émet aucune commande, et que `y` en Sélection de champ copie sans commencer de saisie

## 5. Finalisation

- [x] 5.1 Vérifier la non-régression clavier : un test montre que `↓`, `→`, `←` dans l'arbre se comportent comme avant avec `mouse.capture = true`, et la suite complète reste verte
- [x] 5.2 Mentionner `M` dans le rappel de touches de la barre d'état de l'arbre si la place le permet, sans casser les tests de `status_line` ; vérifier par ces tests
- [x] 5.3 Exécuter `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` et `cargo test` ; vérifier que les trois passent, puis `openspec validate add-mouse-support --strict`
