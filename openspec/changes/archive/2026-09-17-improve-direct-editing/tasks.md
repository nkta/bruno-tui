## 1. Éditeur de tampon pur

- [x] 1.1 Créer `src/app/text_input.rs` avec `TextInput` (tampon, curseur en caractères, `multiline`, valeur initiale) et les opérations `insert`, `backspace`, `delete`, `left`, `right`, `home`, `end`, `up`, `down`, `newline`, `text`, `cursor_line_col`, `is_modified` (design D2) ; vérifier par `cargo test text_input` couvrant extrémités, caractère multi-octet `é`, jointure et scission de lignes, `up`/`down` sur lignes de longueurs différentes, `newline` sans effet en une ligne
- [x] 1.2 Ajouter `Début`/`Fin` limités à la ligne courante sur un tampon multi-ligne et vérifier par un test dédié du scénario « Début et Fin »

## 2. Modèle de session

- [x] 2.1 Remplacer `EditMode { Normal, Insert }` par `EditState { FieldSelect, Input(TextInput) }` dans `EditSession`, ajouter `hscroll` et `has_unsaved()` (D2, D7) ; adapter `field_value` et `field_value_committed` ; vérifier que `cargo build` passe et que `has_unsaved` est couvert par un test (validé, saisie modifiée, saisie inchangée)
- [x] 2.2 Renommer `TextCapture::Insert` en `TextCapture::Input` et mettre à jour `Model::text_capture()` ; vérifier par le test existant de priorité des captures, adapté

## 3. Correspondance des touches

- [x] 3.1 Introduire `Message::Enter` pour `Entrée` hors capture et `Ctrl+S → Message::SaveEdit` dans le bloc `Ctrl` de `key_message` ; retirer les liaisons `i` (`EnterInsert`) et `w` (`SaveEdit` hors `Ctrl`) ; vérifier par `every_key_binding` mis à jour
- [x] 3.2 Réécrire la fonction de capture de saisie : caractères (`Maj`/`Alt` inclus), `Retour arrière`, `Suppr`, `←`, `→`, `↑`, `↓`, `Début`, `Fin`, `Entrée`, `Tab`, `Échap`, le reste ignoré (D4) ; vérifier par un test de capture qui confirme aussi que `q`, `r`, `/`, `S` produisent des messages de caractère et que `Tab` ne produit pas `NextFocus`

## 4. Logique de session dans `update`

- [x] 4.1 Router `Message::Enter` : ouverture de session dans le détail sans session, début de saisie en `FieldSelect`, sinon repli sur `Right` pour les autres focus et pour la confirmation en attente (D3) ; vérifier par tests : ouverture par `Entrée` et par `e`, sans effet sur un dossier, `Entrée` inchangée dans l'arbre, le sélecteur d'environnement et le panneau des secrets
- [x] 4.2 Implémenter début de saisie, validation (`Entrée` en une ligne, `Tab`) et annulation (`Échap`) sans toucher `pending` à l'annulation ; vérifier par les scénarios « Modification validée de l'URL », « Annulation d'une saisie », « Annulation après une validation précédente », « Entrée dans le corps », « Validation du corps par Tab », « Valeur inchangée »
- [x] 4.3 Brancher les opérations de `TextInput` sur les messages de capture ; vérifier par les scénarios « Insertion au milieu », « Suppression arrière et avant », « Extrémités », « Déplacement vertical dans le corps », « Jointure de lignes », « Caractère multi-octet » exécutés via `update`
- [x] 4.4 `SaveEdit` : valider une saisie en cours puis appeler `save_edit` ; vérifier par tests « Sauvegarde depuis la saisie », « Sauvegarde sans modification », « Ancienne touche w retirée » et par les tests existants `edit_saved_*` toujours verts
- [x] 4.5 Utiliser `has_unsaved()` pour `Quit`/`ForceQuit` et pour la fermeture par `Échap` en `FieldSelect` ; vérifier par tests « Quitter pendant une saisie modifiée » (saisie et curseur intacts après refus) et « q pendant la saisie »
- [x] 4.6 Ajouter `selection_locked` et l'appliquer dans `navigate_tree`, `search_tree` et `cross_navigate_to_tree`, avec `StatusMessage::EditLocked` (D6) ; vérifier par tests « Navigation refusée », « Recherche refusée », navigation croisée refusée, repli du dossier déjà sélectionné permis, « Session propre fermée au changement »
- [x] 4.7 Retirer `EnterInsert`, `LeaveInsert`, `InsertEnter` et les fonctions `insert_*` devenues mortes ; vérifier que `cargo clippy -- -D warnings` ne signale aucun code mort

## 5. Rendu

- [x] 5.1 Faire produire à la construction du détail une table champ → (ligne, nombre de lignes, largeur du préfixe) et réécrire `cursor_position_in_detail` sur cette table, colonne mesurée en largeur d'affichage (D7) ; vérifier par un test de position du curseur sur URL, en-tête, paramètre de requête et deuxième ligne du corps
- [x] 5.2 Implémenter `scroll_edit_into_view` (défilement vertical et `hscroll`) appelé après chaque message de session, et appliquer `hscroll` aux seules lignes du champ en saisie dans `view` ; vérifier par tests « Champ hors écran », « URL plus large que le panneau », « Nouvelle ligne en bas du corps » sur un `TestBackend`
- [x] 5.3 Remplacer la barre de mode par la barre d'aide (état, champ, indicateur non enregistré, touches par état et par type de champ) ; vérifier par tests de rendu « Barre d'aide en saisie de l'URL », « Barre d'aide en saisie du corps », « Indicateur de modification »

## 6. Vérification d'ensemble

- [x] 6.1 Exécuter `cargo fmt`, `cargo clippy -- -D warnings` et `cargo test` ; tous passent
- [x] 6.2 Vérifier manuellement sur `tests/fixtures/` (copie temporaire) : ouvrir une requête, modifier URL et corps multi-ligne, annuler une saisie, enregistrer par `Ctrl+S`, contrôler le `.bru` avec `git diff --no-index` ; vérifier aussi que `Ctrl+S` arrive bien à l'application sous le multiplexeur utilisé (herdr/tmux)
- [x] 6.3 Exécuter `openspec validate improve-direct-editing --strict` ; la validation passe
