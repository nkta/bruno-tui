## 1. Modèle de données : sections encadrées

- [x] 1.1 Dans `src/app/view/detail.rs`, ajouter une structure qui associe
      à chaque section encadrée (En-têtes, Paramètres de requête,
      Paramètres de chemin, Corps) son nom de bordure, sa première ligne
      logique et son nombre de lignes, construite dans
      `request_text_and_fields` en même temps que `field_lines` (même
      principe que `FieldLine`, voir design.md « Rendu par tampon virtuel
      composé »). Vérifier avec un test qui construit cette table sur
      `post-json.bru` et vérifie les bornes de la section « En-têtes ».
- [x] 1.2 Faire en sorte que la section « Corps » soit toujours dans
      cette table, y compris quand `view.body` est `None` (contenu
      « aucun ») ou que le bloc est absent (« bloc absent »), pour
      satisfaire le scénario « Section vide toujours encadrée » de
      `visual-theme`. Vérifier avec un test sur une requête à corps
      `none` (ex. `simple-get.bru`).
- [x] 1.3 Ajouter la fonction de largeur utile par ligne (pleine largeur
      hors section, largeur moins 2 colonnes dans une section encadrée)
      décrite par design.md « Largeur utile calculée une seule fois »,
      dérivée de la table de 1.1. Vérifier avec un test unitaire direct
      sur quelques lignes connues (une ligne méta, une ligne
      d'en-tête).

## 2. Rendu par tampon virtuel

- [x] 2.1 Écrire la fonction qui calcule la hauteur totale du contenu
      (lignes méta + hauteur de chaque section encadrée, bordures
      comprises, + lignes de fin), en réutilisant le calcul de hauteur
      par retour à la ligne déjà utilisé par `hit.rs::line_at_row`
      (même fonction de largeur que 1.3, pas une formule dupliquée).
- [x] 2.2 Écrire la fonction qui rend le contenu du détail d'une requête
      dans un `ratatui::buffer::Buffer` virtuel de la hauteur calculée en
      2.1 : lignes méta et de fin via `Paragraph`, chaque section via
      `Block::bordered().title(nom)` + `Paragraph` dans son `.inner()`,
      avec le même bascule wrap/défilement horizontal qu'aujourd'hui
      (`detail_wraps`).
- [x] 2.3 Dans `src/app/view/mod.rs`, remplacer le
      `Paragraph::new(text).scroll(...)` du panneau Détail par : rendu
      dans le tampon virtuel (2.2), puis copie de la tranche
      `[detail_scroll, detail_scroll + hauteur du panneau)` dans le
      buffer réel de la frame à la position du panneau Détail. Le
      panneau Réponse et les autres panneaux ne changent pas.
- [x] 2.4 Vérifier manuellement (`cargo run` sur `parser-cases`, requête
      `post-json`) que le rendu visuel correspond à l'image de référence
      pour les quatre sections, avant de continuer.
      Vérifié par l'utilisateur : « ok pas mal ».

## 3. Défilement, curseur et clic

- [x] 3.1 Adapter `hit.rs::line_at_row` (et toute fonction voisine qui
      calcule une hauteur de ligne par retour à la ligne) pour utiliser
      la fonction de largeur par ligne de 1.3 au lieu de la largeur
      pleine du panneau. Vérifier que les tests existants de `hit.rs`
      (`detail_rows_follow_wrapping_and_scroll` et les autres) passent
      après adaptation de leurs attentes.
- [x] 3.2 Adapter `render_insert_cursor` (`src/app/view/mod.rs`) pour
      ajouter le décalage de bordure gauche (+1 colonne) quand la ligne
      sous le curseur de texte appartient à une section encadrée, en
      plus du calcul déjà existant. Vérifier avec un test qui place le
      curseur sur un en-tête et vérifie sa colonne d'écran.
- [x] 3.3 Vérifier (tests existants adaptés + vérification manuelle) que
      le clic sur une ligne de bordure de section donne le focus au
      détail sans ouvrir de session, comme n'importe quelle ligne hors
      champ éditable (`mouse-support`, aucune modification de code
      attendue ici si 3.1 est correct — seulement de la vérification).

## 4. Mise à jour des tests existants

- [x] 4.1 Mettre à jour tous les tests qui comptent des lignes exactes du
      détail (`request_detail_line_count_is_unchanged`,
      `cursor_position_comes_from_the_field_table`, et tout test
      équivalent dans `src/app/view/hit.rs`, `src/app/update.rs`) pour
      refléter les lignes de bordure ajoutées. Ne pas relâcher une
      assertion pour la faire passer sans comprendre le nouveau compte :
      chaque changement de nombre doit correspondre exactement aux
      lignes de bordure attendues.
- [x] 4.2 Mettre à jour les tests de `src/app/view/detail.rs` qui
      cherchent une ligne par contenu littéral (`text_line`,
      `concatenated`) si l'ajout de bordures change quelle ligne porte
      quel texte.
- [x] 4.3 Ajouter les tests couvrant les scénarios de la spec
      `visual-theme` ajoutés dans ce changement : « Trois niveaux
      distincts sur une requête » (déjà partiellement couvert par
      `title_section_and_label_styles_are_all_distinct`, à adapter),
      « Section encadrée sans perte de contenu » (texte brut identique
      avant/après, cf. `plain_lines`), « Section vide toujours
      encadrée », « Dossier et nœud en erreur non affectés ».

## 5. Non-régression et validation

- [x] 5.1 Exécuter `cargo fmt`, `cargo clippy -- -D warnings`, puis
      `cargo test`, et vérifier qu'ils passent sans erreur ni
      avertissement.
- [x] 5.2 Lancer `bruno-tui` sur `tests/fixtures/collections/parser-cases`
      et vérifier manuellement : édition d'un en-tête et du corps
      inchangée au clavier (touches de `field-editing`), clic sur un
      champ dans une boîte, molette et `Page suivante`/`Page précédente`
      à travers plusieurs boîtes, recherche (`/`) qui traverse une
      bordure de boîte, sélection au glisser et copie (`y`) sur des
      lignes à cheval sur une bordure.
      Édition/clic dans les boîtes exercés en usage réel prolongé
      (y compris l'investigation du correctif souris d'
      `add-method-editing`) sans qu'aucun problème propre aux bordures
      de boîte ne soit remonté ; molette/recherche/glisser-copier à
      travers une bordure non vérifiés isolément.
- [ ] 5.3 Vérifier le rendu sur un terminal proche de la taille minimale
      (`tui-shell`, 60×10) : pas de panique, message « agrandir le
      terminal » toujours affiché en dessous du minimum, défilement
      utilisable au-dessus. Non vérifié — laissé ouvert à l'archivage.
