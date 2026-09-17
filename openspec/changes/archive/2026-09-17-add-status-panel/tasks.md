## 1. Thème (D7)

- [x] 1.1 Ajouter `theme::REDIRECT` (`Color::Cyan`) et
      `theme::CLIENT_ERROR` (`Color::Yellow`) avec leur commentaire de
      catégorie ; étendre `categories_that_could_be_confused_have_distinct_colors`
      (`REDIRECT` ≠ `RUNNING`, `METHOD`, `SUCCESS` ; `CLIENT_ERROR` ≠
      `FOCUS`, `FAILURE`, `LOAD_ERROR` ; `REDIRECT` ≠ `CLIENT_ERROR`) et
      `every_category_has_a_foreground_color…`. Vérifié : `cargo test
      theme` passe
- [x] 1.2 Ajouter `theme::status_badge(class: StatusClass) -> Style`
      (fond = couleur de catégorie, texte = couleur de `BACKGROUND`,
      gras ; `Neutral` → `LABEL` sans fond). Vérifié par un test
      unitaire couvrant les cinq classes

## 2. Disposition (D1, D3)

- [x] 2.1 Dans `src/app/view/mod.rs`, ajouter `Areas.response_status`
      et les constantes de hauteur (5 / 3, seuil terminal 20 lignes) ;
      `layout` découpe la colonne réponse en panneau Statut puis
      `Areas.response` en dessous, arbre et détail inchangés. Vérifié
      par `layout_respects_minimums` étendu : à 60×10 statut 3 lignes,
      réponse 5 lignes (≥ 1 intérieure) ; à 100×19 statut 3 ; à 100×20
      statut 5 ; mêmes `x`/`width` pour statut et réponse ; somme des
      hauteurs = hauteur du corps
- [x] 2.2 Vérifier que le défilement et la pagination de la réponse
      (`update.rs`, basés sur `inner(areas.response)`) restent bornés
      avec le rectangle réduit : les tests existants de défilement de la
      réponse passent sans modification (`cargo test response`), et un
      test à 100×12 vérifie qu'un `Fin` affiche la dernière ligne

## 3. Contenu du panneau Statut (D4, D5, D6)

- [x] 3.1 Créer `src/app/view/status.rs` avec `StatusClass` et
      `classify(&ResponseStatus)` ; tests : 200, 299 → `Success` ; 301
      → `Redirect` ; 404 → `ClientError` ; 500, 599, `Error` →
      `Failure` ; 101, 600, `Skipped`, `Other` → `Neutral`
- [x] 3.2 Ajouter la lecture de taille : en-tête `content-length`
      insensible à la casse, valeur chaîne ou nombre, `u64` ; formatage
      `o`/`Ko`/`Mo` base 1024, une décimale, virgule. Tests : `"21"` →
      `21 o`, `1536` → `1,5 Ko`, `"abc"`, `"-1"`, absent → aucune
      taille, `Content-Length` en majuscules reconnu
- [x] 3.3 Ajouter le décompte des vérifications (réussies / non
      ignorées, quatre catégories) et « aucune vérification » quand le
      total est nul. Tests sur `base_result` enrichi : 3 pass + 1 fail +
      1 skipped → `3/4`
- [x] 3.4 Ajouter `run_concerns(active, path)` : cible exacte, ou
      exécution récursive d'un dossier ancêtre par composants. Tests :
      `folder` récursif concerne `folder/down.bru`, ne concerne pas
      `folder2/x.bru` ; `ok.bru` non récursif ne concerne pas `green.bru`
- [x] 3.5 Écrire `status_panel_lines(model, compact)` : `—` neutre sans
      résultat ; forme complète 3 lignes (badge + libellé non vide ;
      temps · taille ; marqueur + verdict · décompte, couleur
      `SUCCESS`/`FAILURE` via `is_failure()`) ; forme compacte 1 ligne
      (badge, marqueur, temps) ; exécution concernée → badge « en
      cours » `RUNNING` + `précédent : <badge> · <temps>` atténué sans
      verdict. Vérifié par des tests sur les résultats `green`, `ok`,
      `folder/down`, `skip` de `tests/fixtures/reports/mixed.json` et
      sur `pre-request-error.json` (badge « aucune réponse », verdict
      échec, message d'erreur absent du panneau)
- [x] 3.6 Vérifier par un test que, pour une réponse portant
      `set-cookie` et un corps non vide, aucune valeur d'en-tête autre
      que `content-length` ni aucun fragment du corps n'apparaît dans
      les lignes produites

## 4. Rendu dans `view` et retrait de la redondance (D8)

- [x] 4.1 Dans `view`, pour `Focus::Tree | Detail | Response`, dessiner
      le panneau bordé « Statut » (bordure `BORDER`, jamais `FOCUS`)
      dans `areas.response_status`, forme compacte selon la hauteur du
      terminal ; ne rien dessiner pour les panneaux plein corps, le
      chargement et l'échec de chargement. Vérifié par un test d'écran
      100×30 (titre « Statut » au-dessus de « Réponse ») et un test
      historique ouvert (aucun « Statut » à l'écran)
- [x] 4.2 Réduire `detail::status_band` à la ligne `Erreur` (plus une
      ligne vide) quand elle existe ; `response_text` commence sinon par
      la barre d'onglets. Mettre à jour les tests `result_section_*`,
      `response_status_band_stays_visible_across_tabs` et
      `response_panel_shows_result_separately_from_detail` pour chercher
      verdict/statut/temps dans le panneau Statut et vérifier leur
      absence de `response_plain_lines`. Vérifié : `cargo test` passe
- [x] 4.3 Test d'écran : `Tab` ×3 depuis l'arbre ne donne jamais le
      style de focus au panneau Statut ; test 60×10 et 1×1 sans panique
      avec une requête exécutée sélectionnée
- [x] 4.4 Test d'intégration du cycle d'exécution via `update` :
      exécution active sur `folder` récursif (posée directement dans
      `model.run.active`, `RunHandle` n'étant pas constructible en test)
      avec `folder/down` sélectionné
      → « en cours » ; `RunFinished` annulé → `—` (ou résultat
      précédent) sans « en cours »

## 5. Finalisation

- [x] 5.1 `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`
      passent sans exception
- [x] 5.2 Vérification manuelle avec `bruno-tui
      tests/fixtures/collections/runner-probe` (serveur `www/` servi
      comme pour `scripts/gen-report-fixtures.sh`) à 120×40 puis 60×12 :
      badges 200 vert, aucune réponse rouge, ignorée neutre ; « en
      cours » pendant une exécution récursive ; noter le résultat dans
      la PR
- [x] 5.3 Relire les points de contact de `design.md` avec
      `add-mouse-support`, `improve-direct-editing` et
      `add-entry-management` au moment de la fusion, et mettre à jour
      `Areas`/tests en conséquence si l'un d'eux a fusionné avant
