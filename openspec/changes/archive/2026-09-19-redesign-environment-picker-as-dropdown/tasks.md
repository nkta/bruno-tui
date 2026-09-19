## 1. Rendu : menu déroulant flottant

- [x] 1.1 Dans `src/app/view/mod.rs::view()`, arrêter de router
      `Focus::EnvironmentPicker` vers un remplacement de `areas.body` :
      le corps (arbre/détail/réponse) se dessine désormais dans tous
      les cas où une collection est chargée, avec `focused: false` sur
      les trois panneaux quand `model.focus == Focus::EnvironmentPicker`
      (design D4). Vérifier par `cargo build`.
- [x] 1.2 Ajouter une fonction calculant le `Rect` du menu déroulant à
      partir de `frame.area()` et du nombre d'entrées à afficher
      (environnements, ou variables si `model.environment_editing` est
      `Some`) : largeur `(largeur_écran / 3).clamp(28, 44)`, hauteur
      `(nombre_entrées + 2).min(hauteur_écran - 4)`, position ancrée en
      haut à droite sous la ligne de titre (D2). Vérifier par un test
      unitaire sur plusieurs tailles d'écran (dont proche de
      `MIN_WIDTH`/`MIN_HEIGHT`) que le `Rect` reste toujours dans les
      limites de l'écran.
- [x] 1.3 Quand `model.focus == Focus::EnvironmentPicker`, après le
      rendu du corps normal, vider la zone calculée
      (`frame.render_widget(Clear, area)`) puis y appeler
      `panels::render_environment_picker` (D3) — aucun changement à
      cette fonction ni à `render_environment_variables`, elles
      acceptent déjà n'importe quel `Rect`. Vérifier par un test de
      rendu que le contenu du corps en dessous n'apparaît pas à travers
      le menu (aucun caractère du corps dans la zone du menu après
      rendu).

## 2. Tests de non-régression

- [x] 2.1 Vérifier par un test que l'arbre (et le détail de la requête
      sélectionnée) restent présents dans le buffer de rendu pendant que
      le menu déroulant Environnement est ouvert (scénario « le menu se
      superpose sans masquer le reste »).
- [x] 2.2 Vérifier que les tests existants de navigation/saisie/
      sauvegarde du panneau Environnement (`update.rs`, `model.rs`)
      passent sans modification — ils ne dépendent pas de la zone de
      rendu. Ne pas les modifier sauf échec réel.
- [x] 2.3 Vérifier par un test qu'ouvrir le menu déroulant sur un très
      petit terminal (`MIN_WIDTH` × `MIN_HEIGHT`) ne panique pas et
      produit un `Rect` valide (largeur et hauteur non nulles, dans les
      limites de l'écran).

## 3. Vérification globale

- [x] 3.1 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test` passent sans erreur ni avertissement.
- [x] 3.2 Vérification manuelle : lancer l'app sur une collection avec
      plusieurs environnements, sélectionner une requête, appuyer sur
      `E`, confirmer que le menu apparaît en haut à droite avec le
      détail de la requête toujours visible derrière ; `Entrée` sur un
      environnement affiche ses variables dans le même menu ; `Échap`
      revient à la liste puis ferme le menu ; `Ctrl+S` sauvegarde une
      valeur modifiée.
