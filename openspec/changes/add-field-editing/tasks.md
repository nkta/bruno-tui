## 1. Modèle de session d'édition

- [ ] 1.1 Ajouter à `src/app/model.rs` `EditSession`, `EditMode`, `EditableField`, `PendingConfirm`, et les champs `Model.editing`/`Model.confirm` (D1, D6) ; vérifier `cargo build`
- [ ] 1.2 Implémenter `EditableField::list_for(&RequestView)` (D2) ; vérifier par tests unitaires : URL + 2 en-têtes + corps `json` → 4 champs dans l'ordre, corps `formUrlEncoded` → corps exclu, aucun en-tête/paramètre → liste réduite à URL (+ corps si éditable)
- [ ] 1.3 Implémenter `field_value` et l'équivalent pour l'état activé/désactivé, lisant `EditSession.pending` avant de retomber sur `RequestView` (D3) ; vérifier par tests unitaires : aucune édition en attente → valeur d'origine ; une édition en attente sur l'indice courant → valeur éditée ; édition sur un autre indice → sans effet sur le champ interrogé

## 2. Messages et saisie

- [ ] 2.1 Ajouter la variante `Insert` à `TextCapture` et le bras `insert_capture_message` à `capture_message` dans `message.rs` (D4) ; si `add-search-and-yank` n'est pas encore appliqué, créer `TextCapture`/`capture_message`/`to_message(event, capture: Option<TextCapture>)` avec uniquement cette variante (vérifier `openspec status` du changement sœur avant de commencer) ; ajouter le branchement de saisie du mode Insert : `InsertChar`, `InsertBackspace`, `InsertCursorLeft`, `InsertCursorRight`, `InsertEnter`, `LeaveInsert` ; vérifier par tests unitaires chaque touche en saisie et qu'aucune touche non listée n'émet de message
- [ ] 2.2 Ajouter les nouvelles variantes de `Message` et les liaisons hors saisie `e`→`StartEdit`, `Espace`→`ToggleField`, `i`→`EnterInsert`, `w`→`SaveEdit` (D5) ; vérifier par tests unitaires chaque liaison et l'absence de collision avec les liaisons existantes de `tui-shell`
- [ ] 2.3 Ajouter `ConfirmYes`/`ConfirmNo` prioritaires sur toute autre interprétation d'une touche quand `model.confirm.is_some()` (touches à choisir en implémentation, par exemple `y`/`n` ou `Entrée`/`Échap` ; vérifier qu'elles ne collisionnent avec aucune touche déjà réservée par `add-request-run`/`add-search-and-yank`) ; vérifier par tests unitaires

## 3. Ouverture, navigation et bascule

- [ ] 3.1 Implémenter `Message::StartEdit` dans `update` (D1) : ouverture d'une session sur la requête sélectionnée avec `Focus::Detail`, capture du `FileStamp`, initialisation de `fields`/`cursor`/`mode`/`pending`/`dirty` ; sans effet sur un dossier, un nœud en erreur, ou si une session est déjà ouverte ; vérifier par tests unitaires chaque cas
- [ ] 3.2 Implémenter `MoveFieldCursor` avec extrémités sans effet (D5) ; vérifier par tests unitaires le parcours complet d'une session à 4 champs et l'absence d'effet aux deux bornes
- [ ] 3.3 Implémenter `ToggleField` : effet uniquement sur en-tête/paramètre, ajoute un `FieldEdit::*Enabled` à `pending`, marque `dirty` ; vérifier par tests unitaires sur en-tête, sur paramètre, sans effet sur URL et sur corps

## 4. Mode Insert

- [ ] 4.1 Implémenter `EnterInsert` (curseur de texte en fin de valeur courante via `field_value`) et `LeaveInsert`/`InsertEnter` sur champ à valeur unique (fin d'Insert, valeur conservée) (D5) ; vérifier par tests unitaires transition Normal→Insert→Normal sans perte de valeur
- [ ] 4.2 Implémenter `InsertChar`/`InsertBackspace`/`InsertCursorLeft`/`InsertCursorRight` sur la valeur du champ courant, poussant ou mettant à jour le `FieldEdit` correspondant dans `pending` à la sortie d'Insert (pas à chaque frappe) (D3, D5) ; vérifier par tests unitaires : insertion en milieu de chaîne, suppression, déplacement du curseur de texte sans modification, `dirty` non affecté par un aller-retour sans frappe
- [ ] 4.3 Implémenter `InsertEnter` sur le corps (insertion d'un saut de ligne, reste en Insert) distinct du comportement sur champ à valeur unique (D5) ; vérifier par test unitaire

## 5. Sauvegarde

- [ ] 5.1 Implémenter `Message::SaveEdit` retournant `Command::SaveEdit { path, ast, stamp, edits }` quand `dirty`, `Command::None` sinon (D7) ; vérifier par tests unitaires les deux cas
- [ ] 5.2 Implémenter l'exécution de `Command::SaveEdit` dans `src/app/mod.rs::run` : `spawn_blocking` appelant `RequestWriter::write_request` puis, en cas de succès, relecture et reparsing du fichier écrit, émission de `AppEvent::EditSaved`/`Message::EditSaved` (D7) ; vérifier par test d'intégration avec un `FakeWriter`/écriture réelle sur une fixture de `writer-cases/` que l'interface reste réactive pendant l'écriture (technique de barrière comme dans `tests/app_shell.rs`)
- [ ] 5.3 Implémenter `Message::EditSaved` dans `update` : succès → remplace `ast`/`view` du nœud à ce chemin, vide `pending`, `dirty = false`, nouveau `stamp`, recalcule `fields` ; échec → message d'erreur affiché, session inchangée (D7, Requirement « Échec de sauvegarde sans perte ») ; ignore un événement dont le chemin ne correspond plus à la session active ; vérifier par tests unitaires les trois cas, dont un événement obsolète
- [ ] 5.4 Ajouter la fonction de remplacement d'un nœud par chemin dans `Collection`/`Model` (D7, Risks) ; vérifier par test unitaire que seul le nœud ciblé change, le reste de l'arbre étant intact

## 6. Fermeture et confirmation

- [ ] 6.1 Implémenter la fermeture de session par `Échap` en mode Normal de session : fermeture immédiate si `!dirty`, `PendingConfirm::DiscardEdit` sinon (D6) ; vérifier par tests unitaires les deux cas et que la confirmation refusée laisse la session intacte
- [ ] 6.2 Implémenter la confirmation de sortie (`Quit`/`ForceQuit`) : `PendingConfirm::QuitWithUnsavedEdit` si une session modifiée est active, fermeture directe sinon comme aujourd'hui (D6) ; vérifier par tests unitaires les deux cas
- [ ] 6.3 Implémenter `ConfirmYes`/`ConfirmNo` selon la valeur de `model.confirm` (D6) ; vérifier par tests unitaires que `ConfirmYes` applique l'effet différé correct selon la variante et que `ConfirmNo` n'a aucun effet sur le modèle hors l'effacement de `confirm`

## 7. Rendu

- [ ] 7.1 Décorer la ligne du champ sous le curseur et substituer les valeurs éditées dans `view/detail.rs` (D8) ; vérifier par test `TestBackend` que le champ sous le curseur est visuellement distinct et que la valeur affichée reflète une édition en attente
- [ ] 7.2 Ajouter la barre de mode (Normal/Insert + nom du champ) et l'affichage de la question de confirmation, par priorité sur le contenu habituel (D8) ; vérifier par tests `TestBackend` pour chaque état : session Normal, session Insert, confirmation active, aucune session
- [ ] 7.3 Positionner un curseur de texte visible en mode Insert (sonder l'API `ratatui::Frame`/`Terminal` disponible avant de choisir l'implémentation exacte, cf. Risks de design.md) ; vérifier par relecture qu'aucune panique ne peut survenir sur une position de curseur hors bornes

## 8. Vérifications transverses

- [ ] 8.1 Vérifier par relecture que `Model::text_capture()` couvre bien la branche `Insert` en plus de celle(s) déjà posée(s) par les changements sœurs déjà appliqués (`Search` pour `add-search-and-yank`, `Filter` pour `add-response-filter`), sans doublon ni branche oubliée (D4) ; si aucun n'est encore appliqué, vérifier que `TextCapture` ne porte que `Insert`
- [ ] 8.2 Test d'intégration bout en bout sur une fixture de `writer-cases/` : ouverture de session, édition de l'URL, `w`, comparaison octet à octet du fichier réécrit avec le fichier `*.after.bru` attendu (même méthode que les tests de `add-bru-writer`) ; test de refus `Stale` (fichier modifié entre-temps) vérifiant qu'aucune donnée n'est perdue à l'écran ; vérifier `cargo test`
- [ ] 8.3 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests dans `src/app/` pour le code ajouté par ce changement, et qu'aucune valeur de champ (URL, en-tête, corps) n'est jamais formatée dans un message de log
- [ ] 8.4 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
