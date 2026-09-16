## 1. Presse-papiers

- [ ] 1.1 Ajouter `arboard` à `Cargo.toml` en `default-features = false` (D1) ; vérifier `cargo build` et que `cargo tree -i arboard` ne fait apparaître ni `image`, ni `png`, ni `wl-clipboard-rs`
- [ ] 1.2 Créer `src/app/clipboard.rs` : trait `Clipboard { fn set_text(&mut self, text: String) -> Result<(), ClipboardError> }`, `ClipboardError` (`thiserror`, message d'`arboard` seul, jamais le texte copié), `SystemClipboard` construisant un `arboard::Clipboard` à chaque appel ; vérifier par un test unitaire qu'une fausse implémentation enregistrant le texte reçu satisfait le trait, et par relecture qu'aucun `unwrap()`/`expect()` n'y figure hors tests
- [ ] 1.3 Vérifier par un test manuel (non automatisé, consigné en commentaire) que `SystemClipboard::set_text` réussit avec un serveur d'affichage disponible et rend une erreur sans blocage perceptible sans `DISPLAY`/`WAYLAND_DISPLAY` (`env -u DISPLAY -u WAYLAND_DISPLAY`)

## 2. Messages et saisie de recherche

- [ ] 2.1 Ajouter à `message.rs` les variantes `StartSearch`, `SearchInput(char)`, `SearchBackspace`, `ConfirmSearch`, `CancelSearch`, `NextMatch`, `PreviousMatch`, `ToggleVisual`, `Yank`, et les liaisons hors saisie `/ n N v y` (D3) ; vérifier par tests unitaires dans le style de `every_key_binding` que chacune produit le message attendu et qu'aucune ne collisionne avec une liaison existante
- [ ] 2.2 Introduire `pub enum TextCapture { Search }` dans `message.rs`, changer la signature en `to_message(event: AppEvent, capture: Option<TextCapture>) -> Option<Message>`, ajouter `capture_message`/`search_capture_message` (`Char` → `SearchInput`, `Backspace` → `SearchBackspace`, `Enter` → `ConfirmSearch`, `Esc` → `CancelSearch`, `Ctrl+C` toujours `ForceQuit`, tout le reste ignoré) avant la table existante, mettre à jour le commentaire de module pour documenter cette exception (D2) ; mettre à jour l'unique appelant dans `mod.rs` ; vérifier par tests unitaires qu'en saisie, `j`, `q`, `Entrée` et `Échap` produisent des messages de saisie et non de navigation, et que `Ctrl+C` reste `ForceQuit`
- [ ] 2.3 Créer `src/app/search.rs` : `SearchScope`, `SearchState` (D4) ; ajouter `Model.search: Option<SearchState>` et `Model::text_capture() -> Option<TextCapture>` (une seule branche pour l'instant, conçue pour que `add-field-editing`/`add-response-filter` y ajoutent la leur) ; implémenter dans `update.rs` `StartSearch` (ouverture, reprise du dernier motif), `SearchInput`/`SearchBackspace` (édition de `draft`), `CancelSearch` et `ConfirmSearch` avec motif vide (fermeture sans recherche) ; vérifier par tests unitaires les trois scénarios « Ouverture depuis l'arbre », « Annulation par Échap » et « Validation d'un motif vide »

## 3. Recherche dans l'arbre et le détail

- [ ] 3.1 Ajouter `model::all_rows` (parcours complet, sans filtre `expanded`, D4a) et `search::find_tree_match` (comparaison insensible à la casse sur `view::tree::display_name`, circulaire) ; vérifier par tests unitaires sur des arbres construits en mémoire : liste vide, un seul nœud, retour circulaire, insensibilité à la casse
- [ ] 3.2 Brancher `ConfirmSearch`/`NextMatch`/`PreviousMatch` en portée `Tree` dans `update.rs` : dépliage des ancêtres du nœud trouvé, `refresh_rows`, sélection, remise à zéro de `detail_scroll` (déjà fait par le changement de sélection) ; vérifier par tests d'intégration sur `parser-cases` chargée les scénarios de la spec : correspondance dans un dossier replié (`inherit`), recherche circulaire (`unknown-block` → `ping`), aucune correspondance (sélection inchangée, `last_result = Some(false)`)
- [ ] 3.3 Promouvoir `plain_lines` hors du module de test de `detail.rs` (D4b) et ajouter `search::find_detail_match` (position en octets du motif dans la ligne trouvée, circulaire) ; vérifier par tests unitaires sur des textes construits en mémoire : motif en fin de contenu, retour circulaire, insensibilité à la casse, aucune correspondance
- [ ] 3.4 Brancher `ConfirmSearch`/`NextMatch`/`PreviousMatch` en portée `Detail` dans `update.rs` : `detail_scroll = ligne.min(detail_max_scroll(model))`, `model.detail_match` renseigné, effacé au changement de sélection ; vérifier par test d'intégration le scénario « Correspondance au-delà de l'écran » sur `scripted` (motif `res.body.ok`) et « Recherche sans résultat dans le détail » sur `post-json`
- [ ] 3.5 Implémenter `NextMatch`/`PreviousMatch` indépendants du focus courant, utilisant `search.scope` et `direction_forward` (inversé pour `PreviousMatch`) ; vérifier par tests unitaires les scénarios « n répète dans l'arbre depuis le détail », « N inverse le sens » et « aucun motif validé sans effet »

## 4. Sélection visuelle

- [ ] 4.1 Ajouter `Model.detail_selection: Option<DetailSelection>` et `update::bottom_of_viewport`/`selection_range` (D5) ; implémenter `ToggleVisual` (pose l'ancrage au défilement courant, ou l'annule) et l'annulation par `Échap` et par tout changement de sélection dans l'arbre ; vérifier par tests unitaires que `v` puis `Échap` n'altère ni sélection ni défilement, et qu'un changement de nœud efface une sélection active
- [ ] 4.2 Vérifier par test d'intégration le scénario « Étendre jusqu'à la fin du contenu » sur `scripted` : `v` puis `Fin` donne une `selection_range` dont la borne haute vaut `plain_lines(model).len() - 1`, y compris quand cette ligne n'a jamais été atteignable comme seul sommet de panneau

## 5. Copie

- [ ] 5.1 Ajouter `AppEvent::ClipboardResult` dans `event.rs`, implémenter `Yank` dans `update.rs` (ignoré en focus Arbre ou en saisie ; sélection → lignes jointes puis sélection levée sans toucher `detail_scroll` ; sinon la ligne du sommet, D6) déclenchant un `tokio::task::spawn_blocking` sur `SystemClipboard::set_text`, dont le résultat revient sur le canal existant ; brancher la réception dans `mod::run` ; vérifier par tests unitaires (fausse implémentation de `Clipboard`) la copie d'une ligne sans sélection et la copie d'une sélection multi-lignes, texte exact et ordre des lignes
- [ ] 5.2 Traduire `ClipboardResult` en confirmation ou en message d'échec affiché par la barre d'état, sans dépendre de l'état de sélection ou de défilement au moment de la réception ; vérifier par test unitaire qu'un résultat `Err` de la fausse implémentation ne modifie ni sélection ni défilement, et que la boucle continue de répondre immédiatement à un message suivant

## 6. Rendu

- [ ] 6.1 Étendre `status_line` pour afficher la ligne de saisie (`/` + `draft`) pendant l'édition, et un rappel des nouvelles touches hors édition (D7) ; vérifier par test `TestBackend` que la ligne de saisie apparaît à la place de la barre d'état pendant l'édition et disparaît après `Entrée`/`Échap`
- [ ] 6.2 Ajouter dans `view/detail.rs` la teinte des lignes de `selection_range` et la surbrillance de la sous-chaîne de `model.detail_match`, avec des styles visuellement distincts entre eux et du surlignage `REVERSED` de l'arbre (D7) ; vérifier par tests `TestBackend` sur `scripted` que la ligne de `res.body.ok: isTrue` porte la surbrillance après une recherche, et qu'une sélection active teinte plusieurs lignes consécutives

## 7. Vérifications transverses

- [ ] 7.1 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests et hors `main` dans `src/app/`, qu'aucun appel à `arboard` n'a lieu ailleurs que dans `spawn_blocking`, et qu'aucun message d'erreur ne formate le texte copié ou le contenu d'une ligne recherchée
- [ ] 7.2 Vérifier par relecture que les scénarios « Navigation de l'arbre inchangée » et « Défilement du détail inchangé hors sélection » du spec `search-and-yank` correspondent, touche par touche, aux tests déjà existants de `tui-shell` (`message.rs`, `update.rs`), sans qu'aucun de ces tests existants n'ait dû être modifié pour rester vert
- [ ] 7.3 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
