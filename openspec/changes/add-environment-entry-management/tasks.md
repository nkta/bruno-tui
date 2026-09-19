## 1. Écriture : factoriser la diffusion ajout/suppression

- [x] 1.1 Créer `src/writer/dictionary.rs` : y déplacer `DraftEntry` et
      `Section` (renommés `DictEntry`/`DictSection`, aucun changement de
      champ), ainsi que `block_node`, `format_added`,
      `preceding_blank_line_len`, `line_end` depuis `src/writer/draft.rs`
      — déplacement mécanique, sans changer leur comportement. Vérifier
      par `cargo build` que `draft.rs` compile toujours en les
      réimportant.
- [x] 1.2 Extraire de `Draft::replacements()` la boucle
      `for section in EntrySection::ALL { ... }` en une fonction libre
      `dictionary_replacements(raw, eol, block_name, block: Option<(Range<usize>,
      &[Entry])>, entries: &[DictEntry], create_anchor: usize) ->
      Vec<Replacement>` dans `dictionary.rs`, sans dépendance à
      `EntrySection`. `Draft::replacements()` l'appelle une fois par
      section, avec l'ancre déjà calculée par `creation_anchor`
      (inchangée). Vérifier que la suite existante de `draft.rs` et
      `tests/writer_fixtures.rs` passe sans modification (design D1) —
      aucun changement de comportement attendu côté requêtes.
- [x] 1.3 Extraire la partie générale de `check_key` (clé vide, espace,
      `:`, `~`/`"` en tête — sans la règle propre aux paramètres de
      requête) en une fonction libre réutilisable dans `dictionary.rs`,
      retournant `EditError::InvalidEntry` (design D4). `check_key`
      (`draft.rs`) l'appelle puis ajoute sa règle spécifique aux
      paramètres de requête. Vérifier que les tests existants de
      validation de clé de requête passent sans modification.

## 2. Écriture : ajout et suppression dans le bloc vars

- [x] 2.1 Étendre `EnvironmentVarEdit` (`src/writer/environment.rs`) en
      énumération : `Value { index, value }` (renommage de la variante
      actuelle), `Add { key, value, enabled }`, `Remove { index }`.
      Adapter `resolve`/`serialize`/`write_environment` en conséquence.
      Vérifier par `cargo build` (met à jour les call sites existants).
- [x] 2.2 `resolve` pour `Add`/`Remove` appelle
      `dictionary_replacements` (tâche 1.2) sur le bloc `vars`, avec
      l'ancre de création à la position 0 du fichier si le bloc
      n'existe pas encore (design D2). Vérifier par des tests : ajout en
      fin de bloc existant, ajout créant le bloc `vars` absent,
      suppression d'une entrée parmi d'autres, suppression de la
      dernière entrée (bloc retiré entièrement), le reste du fichier
      (dont `vars:secret`) inchangé à l'octet près dans tous les cas.
- [x] 2.3 `resolve` refuse une clé invalide pour `Add` (clé vide, espace,
      `:`, `~`/`"` en tête) via la fonction extraite en 1.3, sans
      écriture. Vérifier par un test par cas de clé invalide.
- [x] 2.4 Couverture ajout/suppression : ajoutée directement dans les
      tests unitaires de `src/writer/environment.rs`
      (`resolve_adds_a_variable_to_an_existing_block`,
      `resolve_creates_the_vars_block_when_absent`,
      `resolve_removes_a_variable_among_others`,
      `resolve_removing_the_last_variable_drops_the_block`), qui
      couvrent déjà le même comportement (résolution + sérialisation) à
      partir de sources inline, sans dépendre de fichiers de fixtures
      séparés. Ajustement au tracé initial de la tâche : pas de nouvelle
      paire de fixtures dans `tests/fixtures/collections/writer-cases/`.

## 3. Modèle : état d'ajout dans la session d'édition

- [x] 3.1 Ajouter `AddingKey(TextInput)` et `AddingValue { key: String,
      input: TextInput }` à `EnvironmentEditState` (`src/app/model.rs`,
      design D3). Ajouter `EnvironmentEditSession::begin_add`,
      `commit_add_key` (refuse une clé invalide sans changer d'état),
      `commit_add_value`, `cancel_add`, et un accesseur
      `EnvironmentEditState::text_input_mut()` couvrant `Input`,
      `AddingKey`, `AddingValue`. Vérifier par des tests unitaires
      (`model.rs`) couvrant la séquence complète et l'abandon à chaque
      étape.
- [x] 3.2 `EnvironmentEditSession::has_unsaved()` couvre les nouveaux
      états (une saisie de clé ou de valeur en cours compte comme
      modification en cours, comme `Input`). Vérifier par un test.
- [x] 3.3 Ajouter `pending: Vec<EnvironmentVarEdit>` à
      `EnvironmentEditSession`, sur le modèle de `EditSession.pending`
      (design D3, note « Journal des modifications ») : chaque
      validation de valeur, ajout ou suppression y ajoute l'édition
      correspondante, en plus de mettre à jour `variables` pour
      l'affichage immédiat. Réécrire `save_environment` (`update.rs`)
      pour écrire `session.pending.clone()` telle quelle, au lieu de la
      comparaison indice par indice actuelle (qui casse dès qu'un ajout
      ou une suppression décale les indices). Vérifier par des tests
      couvrant une séquence modification + ajout + suppression avant
      sauvegarde.

## 4. Commandes : a et d dans le popup

- [x] 4.1 Dans `src/app/update.rs`, router `Message::Add` (`a`) vers
      `EnvironmentEditSession::begin_add` quand
      `model.focus == Focus::EnvironmentPicker` et
      `model.environment_editing` est ouverte ; router `Message::Delete`
      (`d`) vers la suppression de la variable sous le curseur,
      immédiate, sans confirmation, curseur borné à la dernière position
      (miroir de la suppression d'en-tête de requête). Vérifier par des
      tests.
- [x] 4.2 Étendre `input_key`, `Message::ValidateInput`,
      `Message::CancelInput` pour reconnaître `AddingKey`/`AddingValue`
      via `text_input_mut()` (tâche 3.1), et appeler
      `commit_add_key`/`commit_add_value`/`cancel_add` selon l'état et
      le message. Vérifier par des tests couvrant `Entrée`, `Tab`,
      `Échap` à chaque étape de l'ajout.

## 5. Rendu : ligne provisoire pendant l'ajout

- [x] 5.1 Dans `render_environment_edit_popup`
      (`src/app/view/panels.rs`), afficher une ligne provisoire en fin
      de tableau pendant `AddingKey`/`AddingValue` (clé en cours de
      saisie, ou clé validée avec valeur en cours de saisie), curseur de
      texte positionné dessus. Vérifier par un test de rendu.
- [x] 5.2 Mettre à jour la barre d'aide du popup
      (`environment_session_help_line`, `src/app/view/mod.rs`) pour
      mentionner `a` et `d` dans l'état Sélection. Vérifier par un test.

## 6. Vérification globale

- [x] 6.1 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test` passent sans erreur ni avertissement — en particulier
      la suite `draft.rs`/`writer_fixtures` de requêtes, inchangée après
      le refactor de la tâche 1.
- [x] 6.2 Vérification manuelle faite en direct dans l'application :
      ouvrir le popup d'édition de `local`, `a` pour ajouter `region` /
      `eu-west` (clé puis valeur, ligne provisoire visible), confirmée
      en fin de tableau, curseur dessus ; `d` pour la supprimer,
      curseur borné sur `debug` ; fermeture par `Échap` sans sauvegarder
      pour ne pas toucher la fixture réelle du dépôt. L'écriture sur
      disque (`Ctrl+S`, y compris création du bloc `vars` absent) est
      vérifiée par les tests d'écriture réelle de
      `src/writer/environment.rs` (répertoire temporaire dédié), pas
      rejouée en direct contre une collection de test versionnée.
