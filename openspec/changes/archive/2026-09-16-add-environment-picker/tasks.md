## 1. État du modèle

- [x] 1.1 Ajouter `Focus::EnvironmentPicker` à l'enum `Focus`
      (`src/app/model.rs`) et vérifier que `cargo build` échoue d'abord
      sur les matches non exhaustifs dans `update.rs`/`view/`, avant de
      les traiter aux tâches suivantes (confirme qu'aucun site n'est
      oublié)
- [x] 1.2 Ajouter `current_environment: Option<String>` et
      `environment_selected: usize` à `Model` (`src/app/model.rs`),
      initialisés à `None`/`0` dans le constructeur du modèle
- [x] 1.3 Écrire `environment_name_at(collection: &Collection, index:
      usize) -> Option<Option<&str>>` (voir design.md, section
      « Résolution de l'indice vers un nom ») avec des tests unitaires
      couvrant : indice 0 (« Aucun »), indice valide, indice sur une
      entrée `Err(ErrorNode)`, indice hors limites

## 2. Messages et touches

- [x] 2.1 Ajouter le message `ToggleEnvironmentPicker` à l'enum
      `Message` (`src/app/message.rs`). Revu en cours d'implémentation
      (voir design.md, « Un seul nouveau message ») : la navigation et
      la validation dans le panneau réutilisent les messages génériques
      déjà en place, sans nouvelle variante par action
- [x] 2.2 Mapper `E` (hors saisie, comme `D`/`H`) vers
      `ToggleEnvironmentPicker` dans `key_message`
      (`src/app/message.rs`), et vérifier par un test que `e` minuscule
      reste `StartEdit` sans collision
- [x] 2.3 Router `Up`/`Down`/`Home`/`End`/`Right`/`FocusTree`
      (`↑`/`k`, `↓`/`j`, `Début`/`Fin`, `→`/`l`/`Entrée`, `Échap`) vers
      le panneau de sélection quand `model.focus ==
      Focus::EnvironmentPicker`, via la même dispatch générale par
      focus déjà en place pour `Diagnostics`/`History` (`navigate_*`
      dans `update.rs`), et vérifier par un test qu'aucune touche
      existante (`D`, `H`, `/`, `|`, `r`, navigation de l'arbre) ne
      change de comportement hors de ce focus

## 3. Logique dans `update`

- [x] 3.1 Implémenter `Message::ToggleEnvironmentPicker` dans
      `src/app/update.rs` : bascule `Focus::EnvironmentPicker` ↔
      `Focus::Tree` si une collection est chargée (sinon `Command::None`
      sans effet), et à l'ouverture initialise `environment_selected` à
      l'indice correspondant à `current_environment`
- [x] 3.2 Implémenter la navigation dans `navigate_environment_picker`
      (`Up`/`Down`/`Home`/`End`) : déplace `environment_selected` sans
      dépasser les extrémités de la liste (0..=nombre d'environnements),
      avec un test unitaire sur les deux bornes
- [x] 3.3 Implémenter la validation (`Right`, envoyé par `→`/`l`/
      `Entrée`) via `select_environment_picker` : appelle
      `environment_name_at`, si `Some(nom)` met à jour
      `current_environment` et repasse `focus` à `Focus::Tree`, si
      `None` (entrée invalide) ne fait rien ; test unitaire pour les
      trois cas (« Aucun », environnement valide, entrée invalide)
- [x] 3.4 `Échap` (`Message::FocusTree`) referme déjà vers `Focus::Tree`
      sans toucher à `current_environment` par construction de ce
      message générique, sans code supplémentaire ; test unitaire
      vérifiant que l'environnement courant est inchangé même si
      `environment_selected` pointait ailleurs au moment de la fermeture
- [x] 3.5 Modifier `run_selected()` pour lire
      `model.current_environment.clone()` dans `RunRequest.env` au lieu
      de `None`, avec un test unitaire par cas (requête directe, dossier
      récursif, rejeu depuis l'historique) vérifiant que le nom transite
      bien, et un test vérifiant que `RunRequest.env` reste `None` quand
      `current_environment` est `None` (non-régression du comportement
      actuel)
- [x] 3.6 Étendre le handler de `Message::CollectionLoaded` pour remettre
      `current_environment` à `None` et, si `focus` valait
      `EnvironmentPicker`, le remettre à `Focus::Tree`, avec un test
      unitaire chargeant une collection après sélection d'un
      environnement puis rechargeant

## 4. Rendu

- [x] 4.1 Ajouter le rendu du panneau de sélection dans
      `src/app/view/panels.rs`, à l'image de `Diagnostics`/`History` :
      liste « Aucun » + environnements, entrée en erreur visuellement
      marquée et non mise en valeur comme sélectionnable, entrée
      courante indiquée dans la liste
- [x] 4.2 Ajouter l'indicateur permanent de l'environnement courant
      (nom ou mention d'absence) dans la barre de titre, visible tant
      qu'une collection est chargée
- [x] 4.3 Tests de rendu `TestBackend` : panneau fermé n'affiche pas la
      liste, panneau ouvert affiche « Aucun » + les environnements de la
      fixture (voir tâche 5.1) avec l'entrée en erreur marquée,
      indicateur permanent affiche « Aucun » par défaut puis le nom
      choisi après sélection

## 5. Fixtures et non-régression

- [x] 5.1 Ajouter un second environnement valide (`staging.bru`) et un
      fichier d'environnement malformé (`malformed.bru`) à
      `tests/fixtures/collections/parser-cases/environments/` (voir
      proposal.md, section Impact), et mettre à jour les deux tests
      existants qui figeaient un seul environnement
      (`tests/parser_fixtures.rs` : `traversal_skips_ignored_non_bru_
      and_environments`, `round_trip_is_byte_identical_on_every_fixture`
      via sa liste `MALFORMED`) sans changer le comportement déjà
      spécifié du chargeur
- [x] 5.2 Vérifier qu'aucun test n'ouvre `examples/jsonplaceholder/` :
      grep du répertoire `tests/` confirme l'absence de toute référence
      à ce chemin

## 6. Vérification finale

- [x] 6.1 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test`, tous verts (197 tests lib + toutes les suites
      d'intégration, 1 ignoré comme d'habitude faute de `bru` réel dans
      cet environnement de test)
