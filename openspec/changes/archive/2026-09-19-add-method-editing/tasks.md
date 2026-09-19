## 1. `bru-writer` : renommage du bloc de méthode

- [x] 1.1 Ajouter `FieldEdit::Method(String)` dans `src/writer/edit.rs`
      (valeur en minuscules, une des 9 de `ast::METHODS`). Vérifier
      `cargo build`.
- [x] 1.2 Dans `src/writer/draft.rs`, ajouter `MethodState { span,
      original, current }` et `method_state(ast)` (analogue à
      `url_state`, mais localisant le `Node` du bloc de méthode pour
      obtenir `node.span.start`), et le champ `method:
      Result<MethodState, EditError>` sur `Draft`. Vérifier avec un test
      unitaire qui calcule la tranche attendue sur une fixture connue
      (ex. `get {` en tête de `parser-cases/simple-get.bru`).
- [x] 1.3 Traiter `FieldEdit::Method` dans `Draft::apply` : refuser une
      valeur hors de `ast::METHODS` sans la citer dans l'erreur (nouvelle
      variante ou réutilisation d'une variante existante d'`EditError`,
      cohérente avec les refus déjà en place). Vérifier avec un test qui
      demande une méthode inconnue et vérifie l'absence d'écriture et
      l'absence de la valeur refusée dans le message d'erreur.
- [x] 1.4 Émettre le remplacement de la tranche du nom de bloc dans
      `Draft::replacements()`, uniquement si `current != original`.
      Vérifier avec un test qui compare le fichier réécrit à l'octet près
      pour un changement `get` → `post` sur une fixture à URL, `body` et
      `auth` déjà présents (le reste du bloc doit être identique à
      l'octet près).
- [x] 1.5 Ajouter une fixture d'écriture dans
      `tests/fixtures/collections/writer-cases/` (méthode avant/après)
      et sa vérification par le vrai parser Bruno (même principe que les
      fixtures existantes, `writer_fixtures.rs` et la vérification Bruno
      à la demande).

## 2. Modèle de session

- [x] 2.1 Ajouter `EditableField::Method` dans `src/app/model.rs`, en
      première position de `EditableField::list_for` (avant `Url`).
      Adapter `display_name`, `entry()`, `section()` pour ce nouveau cas
      (aucune section, comme `Url`).
- [x] 2.2 Ajouter le troisième état de session (sélecteur de méthode
      ouvert, portant l'indice présélectionné dans `ast::METHODS`) à côté
      de `EditState::FieldSelect` et `EditState::Input`. Vérifier
      `cargo build`.

## 3. Comportement (`src/app/update.rs`, `src/app/message.rs`)

- [x] 3.1 Sur `Entrée` en Sélection de champ avec le curseur sur
      `EditableField::Method`, ouvrir le sélecteur avec la méthode
      actuelle de la session présélectionnée, au lieu de commencer une
      saisie de texte.
- [x] 3.2 Router `↓`/`j`, `↑`/`k` vers le déplacement de la présélection
      (borné aux extrémités) pendant que le sélecteur est ouvert, sans
      effet sur le curseur de champ de la session.
- [x] 3.3 `Entrée` valide la présélection : applique
      `FieldEdit::Method` à l'aperçu de la session, marque la session
      modifiée si la valeur diffère de la méthode chargée, referme le
      sélecteur vers l'état Sélection de champ, curseur de champ sur
      Méthode.
- [x] 3.4 `Échap` referme le sélecteur sans rien changer, quelle que soit
      la présélection au moment de la fermeture.
- [x] 3.5 Étendre l'arbitrage des touches (déjà en place pour `Input`) au
      nouvel état : seules `↑`/`k`, `↓`/`j`, `Entrée`, `Échap` et
      `Ctrl+C` ont un effet ; toute autre touche, y compris les
      raccourcis globaux, est sans effet.
- [x] 3.6 Vérifier que `field_value`/`field_enabled`
      (`src/app/model.rs`) et le calcul de modification non enregistrée
      traitent `EditableField::Method` de façon cohérente avec les
      autres champs à valeur unique (comme `Url`).

## 4. Rendu (`src/app/view/detail.rs`, `src/app/view/mod.rs`)

- [x] 4.1 Construire la boîte du sélecteur (titre « Méthode », une ligne
      par valeur des 9, celle sous la présélection stylée avec
      `FIELD_CURSOR_STYLE`) avec `boxed_section`/`SectionBox`
      (`add-boxed-detail-sections`), insérée dans la séquence de lignes
      juste après la ligne Méthode + URL, uniquement pendant que le
      sélecteur est ouvert pour la session de cette requête.
- [x] 4.2 Vérifier que le tampon virtuel et le calcul de hauteur
      (`content_height`, `compose`) prennent en compte cette boîte
      supplémentaire sans modification de leur logique (ils ne
      connaissent que la séquence de lignes et les `SectionBox`).
- [x] 4.3 Ajouter le texte du sélecteur à la barre d'aide de la session
      (navigation, validation, annulation).
- [x] 4.4 Vérifier que `render_insert_cursor` n'affiche aucun curseur de
      texte pendant que le sélecteur est ouvert (aucun état `Input`
      actif).

## 5. Mise à jour des tests existants

- [x] 5.1 Mettre à jour tous les tests qui supposent l'URL en première
      position de curseur de champ ou qui comptent des positions/lignes
      exactes (`src/app/update.rs`, `src/app/view/detail.rs`,
      `src/app/view/hit.rs`, `src/app/view/mod.rs`), pour refléter la
      méthode en première position et les lignes de la boîte du
      sélecteur quand elle est ouverte.
- [x] 5.2 Ajouter les tests couvrant les scénarios ajoutés à
      `field-editing` (ouverture avec présélection, choix d'une autre
      méthode, annulation, extrémités, raccourci sans effet) et à
      `bru-writer` (renommage, méthode inconnue refusée, méthode
      inchangée, contenu du bloc préservé).

## 6. Non-régression et validation

- [x] 6.1 Exécuter `cargo fmt`, `cargo clippy -- -D warnings`, puis
      `cargo test`, et vérifier qu'ils passent sans erreur ni
      avertissement.
- [x] 6.2 Lancer `bruno-tui` sur `tests/fixtures/collections/parser-cases`
      et vérifier manuellement : ouverture du sélecteur sur une requête
      `GET`, changement vers `POST`, sauvegarde (`Ctrl+S`), rechargement
      du fichier confirmant le changement ; annulation (`Échap`) laissant
      la méthode inchangée ; navigation aux extrémités de la liste.
- [x] 6.3 Bug remonté par l'utilisateur en test manuel : la souris restait
      inerte tant que le sélecteur de méthode était ouvert
      (`mouse_accepted` excluait `TextCapture::MethodPicker`), rendant
      l'édition « trop rigide » dès qu'on cliquait sur le champ Méthode.
      Corrigé : `mouse_accepted` accepte cet état, et `mouse_press` ferme
      le sélecteur sur tout clic (comme `Échap`) sans agir sur une autre
      cible dans le même geste (la fermeture décale les lignes suivantes,
      rendant la position cliquée obsolète pour une résolution dans le
      même geste). Vérifié par
      `click_elsewhere_closes_the_method_picker_without_leaving_the_mouse_inert`
      et par la correction de `click_on_the_field_being_edited_changes_nothing`
      (qui passait pour la mauvaise raison : toute souris étant inerte).
