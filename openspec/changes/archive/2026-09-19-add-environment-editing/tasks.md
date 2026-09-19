## 1. Écriture : bloc `vars` d'un environnement

- [x] 1.1 Créer `src/writer/environment.rs` : résolution d'une
      modification de valeur d'entrée du bloc `vars` (AST d'origine +
      index + nouvelle valeur) vers la ou les tranches de source à
      remplacer, en réutilisant `FileStamp` et l'écriture atomique déjà
      exposées par `src/writer/mod.rs`. Vérifier avec un test unitaire
      qu'une valeur modifiée produit une tranche minimale, le reste du
      fichier inchangé à l'octet près.
- [x] 1.2 Ajouter une fixture de test dédiée (paire
      `tests/fixtures/collections/writer-cases/environment-vars.bru` /
      `.after.bru`, sur le modèle de `method.bru`/`method.after.bru`)
      couvrant une variable activée modifiée et une variable désactivée
      laissée intacte. Vérifier via `tests/writer_fixtures.rs`.
- [x] 1.3 Vérifier le refus d'écriture si le fichier a changé sur disque
      depuis le chargement (instantané `FileStamp` différent) : test
      unitaire simulant une modification externe entre chargement et
      sauvegarde, vérifiant qu'aucune écriture n'a lieu et qu'une erreur
      est retournée.

## 2. Modèle : session d'édition d'environnement

- [x] 2.1 Ajouter `EnvironmentEditSession` dans `src/app/model.rs`
      (chemin, `FileStamp`, `Vec<KeyValue>` en mémoire, curseur, état
      Sélection/Saisie via `TextInput`, indicateur de modification) et un
      champ sur `Model` pour la porter, indépendant de `Model.editing`.
      Vérifier par un test que la session s'initialise avec les valeurs
      actuelles de `Environment.variables`, dans l'ordre du fichier.
- [x] 2.2 Implémenter la validation/annulation d'une saisie de valeur
      (miroir de `begin_input`/`open_input`/`cancel_input` pour
      `EditSession`, restreint à la seule valeur). Vérifier par des
      tests : valider une saisie marque la session modifiée et change la
      valeur en mémoire ; annuler restaure la valeur précédente sans
      marquer la session modifiée.
- [x] 2.3 Refuser la saisie sur une variable désactivée (`Entrée`/`e`
      sans effet). Vérifier par un test dédié.

## 3. Navigation et affichage

- [x] 3.1 Dans `src/app/update.rs::navigate_environment_picker`, traiter
      `Message::Enter` sur une entrée valide (pas « Aucun », pas en
      erreur) pour ouvrir `EnvironmentEditSession` sur l'environnement
      courant ; sans effet sur « Aucun » ou une entrée en erreur.
      Vérifier par des tests couvrant les trois cas.
- [x] 3.2 Brancher `Échap` : annule une saisie en cours si active, sinon
      referme la vue variables et revient à la liste des environnements
      (session d'environnement abandonnée, cohérent avec l'absence
      d'ajout/suppression qui rendrait un abandon partiel ambigu ici).
      Vérifier par un test.
- [x] 3.3 Dans `src/app/view/panels.rs::render_environment_picker`,
      afficher la vue variables à la place de la liste quand
      `Model` porte une `EnvironmentEditSession` ouverte : une ligne par
      variable, clé et valeur, style dédié pour une entrée désactivée
      (réutiliser le style déjà utilisé pour un en-tête désactivé dans le
      détail d'une requête). Vérifier par un test de rendu (snapshot de
      lignes, sur le modèle des tests existants de `detail.rs`).
- [x] 3.4 Ajouter le curseur de sélection (surbrillance de la ligne
      courante) et le rendu de la saisie en cours (texte tapé, curseur de
      texte en fin de valeur). Vérifier par un test de rendu.
- [x] 3.5 Mettre à jour la barre d'aide (`session_help_line` ou
      équivalent) pour afficher les raccourcis actifs dans la vue
      variables (`Entrée` éditer, `Échap` retour/annuler, `Ctrl+S`
      sauvegarder). Vérifier par un test.

## 4. Sauvegarde

- [x] 4.1 Brancher `s`/`Ctrl+S` (`Message::SaveEdit` existant, ou un
      message dédié si le routage l'exige) pour déclencher l'écriture
      via `src/writer/environment.rs` quand une `EnvironmentEditSession`
      a des modifications en attente, avec message de statut identique
      en forme à celui de la sauvegarde d'une requête (succès, refus par
      fraîcheur perdue). Vérifier par des tests couvrant succès et refus.
- [x] 4.2 Vérifier qu'une sauvegarde réussie remet la session à « aucune
      modification en attente » et que les valeurs en mémoire restent
      celles qui viennent d'être écrites (pas de rechargement complet du
      fichier nécessaire). Test dédié.

## 5. Vérification globale

- [x] 5.1 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test` passent sans erreur ni avertissement.
- [x] 5.2 Vérification manuelle : ouvrir le panneau Environnement,
      `Entrée` sur un environnement, modifier une valeur, `Ctrl+S`,
      relire le fichier `.bru` sur disque pour confirmer l'écriture ;
      confirmer qu'une variable secrète n'apparaît jamais dans cette vue.
