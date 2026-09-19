## 1. Résolution du corps et du chemin de l'éditeur (pur, sans I/O)

- [x] 1.1 Ajouter une fonction pure qui, à partir du modèle, retourne le
      corps brut de la réponse sélectionnée en texte (même règle que
      `body_lines` : chaîne telle quelle, autre valeur mise en forme via
      `crate::app::filter::pretty_print`, `None` si pas de sélection, pas
      de résultat, ou corps `null`). Vérifier par des tests unitaires
      couvrant chaîne, JSON structuré et absence de corps exploitable.
- [x] 1.2 Ajouter une fonction pure qui résout le programme éditeur à
      lancer : `VISUAL` si définie et non vide, sinon `EDITOR`, sinon
      `"nano"`. Vérifier par des tests unitaires sur les trois cas
      (paramétrée par un accès aux variables d'environnement injectable,
      pas `std::env::var` en dur, pour rester testable).

## 2. Message, Command et raccourci

- [x] 2.1 Ajouter `Message::OpenResponseInEditor` et le brancher sur
      `Ctrl+E` dans `key_message` (`src/app/message.rs`), dans la
      branche `KeyModifiers::CONTROL` déjà prioritaire sur la capture de
      texte, aux côtés de `Ctrl+C`/`Ctrl+X`/`Ctrl+S`. Vérifier par un
      test que `Ctrl+E` produit ce message y compris pendant une saisie
      de champ en cours (capture de texte active).
- [x] 2.2 Dans `update` (`src/app/update.rs`), traiter ce message :
      résoudre le corps (tâche 1.1) et l'éditeur (tâche 1.2) ; sans corps
      exploitable, poser un message de statut et retourner
      `Command::None` ; sinon retourner un nouveau `Command::OpenInEditor
      { program, text }`. Vérifier par des tests unitaires (avec/sans
      résultat exploitable) que le bon `Command`/statut est produit,
      sans toucher au terminal (aucune I/O dans `update`).

## 3. Pause du lecteur de terminal

- [x] 3.1 Modifier `spawn_terminal_reader` (`src/app/event.rs`) pour
      accepter un indicateur de suspension partagé (`Arc<AtomicBool>` ou
      équivalent) et remplacer l'appel bloquant `event::read()` par une
      boucle `event::poll(court délai)` qui n'appelle `read()` que si un
      événement est annoncé et que l'indicateur signale « non
      suspendu » ; sinon attend sans toucher au descripteur. Vérifier
      par un test (ou, si le blocage sur l'entrée standard rend le test
      automatisé impraticable, documenter la vérification manuelle dans
      la tâche 6.2) que l'indicateur suspend effectivement la
      consommation d'événements.
- [x] 3.2 Exposer l'indicateur partagé depuis `main.rs` jusqu'à la
      boucle `run` (même chemin que `sender`/`events` aujourd'hui), pour
      qu'il soit accessible à l'exécution du `Command` de la tâche 4.1.

## 4. Exécution du `Command` dans la boucle `run`

- [x] 4.1 Dans `src/app/mod.rs::run`, ajouter le bras de traitement de
      `Command::OpenInEditor { program, text }` : exécuté de façon
      bloquante (`tokio::task::spawn_blocking(...).await`, pas de retour
      différé par canal), avant le `terminal.draw` de fin de tour —
      conformément à `design.md` (D1). Séquence : positionner
      l'indicateur de suspension ; désactiver la capture souris si
      active (même appel que `Command::SetMouseCapture`) ; 
      `ratatui::try_restore()` ; écrire `text` dans un fichier temporaire
      nommé de façon non prévisible (`std::env::temp_dir()`) ; lancer
      `program` avec ce fichier en argument, stdio hérité, attendre sa
      fin quel que soit le statut de sortie ; supprimer le fichier
      temporaire ; `ratatui::try_init()` ; réactiver la capture souris si
      elle était active ; effacer l'indicateur de suspension. Reporter un
      message de statut (succès, échec de lancement, éditeur introuvable).
- [x] 4.2 Vérifier que `terminal.clear()` (ou équivalent) force un
      redessin complet après la reprise, pas un diff incrémental sur un
      contenu d'écran devenu obsolète pendant que l'éditeur tournait.
- [x] 4.3 Vérifier par un test que le fichier temporaire est supprimé
      même quand le lancement du processus échoue (binaire introuvable).

## 5. Message de statut

- [x] 5.1 Ajouter une variante `StatusMessage` (aucune réponse
  exploitable / échec de lancement de l'éditeur) affichée dans la barre
  de statut, sur le modèle des `StatusMessage` déjà existants
  (`SaveError`, `MouseCaptureError`). Vérifier par un test de rendu.

## 6. Vérification globale

- [x] 6.1 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test` passent sans erreur ni avertissement.
- [x] 6.2 Vérification manuelle : sélectionner une requête exécutée avec
      un corps JSON, `Ctrl+E`, confirmer que `nano` (sans `$EDITOR`
      défini) s'ouvre avec le corps mis en forme, taper du texte dans
      `nano` sans l'enregistrer, quitter, confirmer que l'interface
      `bruno-tui` se redessine correctement et que les frappes faites
      dans `nano` n'ont pas fui vers `bruno-tui` ; répéter avec `$EDITOR`
      défini sur un autre éditeur plein écran pour confirmer la
      priorité ; confirmer que `Ctrl+E` sans réponse exploitable affiche
      un message de statut sans ouvrir d'éditeur.
