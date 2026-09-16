## 1. Préalable et dépendances

- [ ] 1.1 Vérifier l'état d'implémentation de `add-request-run` (contrat `RunState`/`RequestOutcome` dans `src/app/model.rs`) ; si le contrat diverge de celui documenté dans `design.md` (D2 du présent changement et de `add-request-run`), mettre à jour ce `design.md` avant de continuer
- [ ] 1.2 Ajouter `jaq-core` 3.1.1, `jaq-std` 3.0.3 (features par défaut) et `jaq-json` 2.0.3 (feature `serde`) à `Cargo.toml` ; vérifier `cargo build` et que `cargo tree` ne fait apparaître qu'une seule version de chaque crate `jaq-*`

## 2. Interprétation jq

- [ ] 2.1 Créer `src/app/filter.rs` avec `FilterState`, `FilterResult`, et `evaluate(filter_src: &str, data: &serde_json::Value) -> FilterResult` (D6) : compilation via `Loader`/`Compiler` avec `jaq_core`/`jaq_std`/`jaq_json` (`defs()` et `funs()` des trois), conversion d'entrée via `serde_json::from_value::<jaq_json::Val>`, formatage de sortie via `jaq_json::write::write` (indentation deux espaces, espace après `:`) ; vérifier par tests unitaires sur le corps `{"a": [1, 2], "b": null}` : `.a` produit `[1, 2]` mis en forme, `.a[]` produit deux sorties `1` et `2`
- [ ] 2.2 Vérifier par tests unitaires qu'un filtre syntaxiquement invalide (`.a.b |`) et qu'un filtre valide mais incompatible avec la valeur (`map(.)` sur la chaîne `"<html><body>probe</body></html>\n"`, tirée de `tests/fixtures/reports/mixed.json`) produisent chacun `FilterResult::Error` avec un message non vide, sans paniquer
- [ ] 2.3 Vérifier par un test que `evaluate` sur une chaîne (`". | length"` sur `"<html>...</html>\n"`) produit la longueur de la chaîne, confirmant qu'un corps non structuré reste un filtrage valide

## 3. Messages et disponibilité

- [ ] 3.1 Ajouter à `Message` les variantes `OpenFilter`, `FilterInput(char)`, `FilterBackspace`, `ConfirmFilter`, `CancelFilter`, la liaison `Char('|') → OpenFilter` dans `key_message`, la variante `Filter` à `TextCapture`, le bras `filter_capture_message` dans `capture_message`, et la branche correspondante de `Model::text_capture()` (D3) ; si `add-search-and-yank`/`add-field-editing` ne sont pas encore appliqués, créer `TextCapture`/`capture_message`/`to_message(event, capture: Option<TextCapture>)` avec uniquement cette variante (vérifier `openspec status` des changements sœurs avant de commencer) ; vérifier par test unitaire que `|` hors saisie produit `OpenFilter` et qu'aucune touche existante n'est modifiée
- [ ] 3.2 Implémenter la condition de disponibilité (D4) : `OpenFilter` sans effet si la sélection n'est pas une requête, si `model.run.outcomes` n'a pas d'entrée pour son chemin, si `response.status` n'est pas `Http`, ou si `response.data` est nul ; sinon création/réouverture de `Model.filter` ; vérifier par tests unitaires les quatre cas d'indisponibilité de la spec (aucune exécution, erreur de connexion, requête ignorée, et le cas disponible en contre-exemple) sur les fixtures réelles `folder/down`, `ok`, `json`, `skip` de `tests/fixtures/reports/mixed.json`

## 4. Saisie et application

- [ ] 4.1 Implémenter le traitement de `FilterInput`/`FilterBackspace` sur `Model.filter.draft`, et `CancelFilter` (referme la saisie sans toucher à `applied`) ; vérifier par test unitaire le scénario de composition et correction de la spec (`.a.b`, deux effacements, puis `x` → `.a.x`)
- [ ] 4.2 Implémenter `ConfirmFilter` : filtre vide équivaut à `CancelFilter` ; sinon appel à `evaluate` (2.1) et stockage dans `model.filter.applied`, saisie refermée ; vérifier par tests unitaires les scénarios de la spec : filtre extrayant une valeur, filtre à plusieurs sorties, validation d'un filtre vide, correction après erreur (un filtre en erreur puis un filtre valide remplace le message)
- [ ] 4.3 Implémenter la réinitialisation du filtre au changement de sélection (D5) : fonction appelée après toute navigation qui change `model.tree.selected`, comparant le chemin du nœud avant/après ; vérifier par test unitaire qu'un filtre appliqué disparaît après `Down`/`Up` vers une autre requête, et qu'y revenir affiche le corps brut sans réappliquer l'ancien filtre

## 5. Rendu

- [ ] 5.1 Étendre `view/detail.rs` : remplacement de la section Corps par la ligne de filtre puis le résultat ou l'erreur quand `model.filter` correspond au nœud affiché (D7), affichage inchangé sinon ; vérifier par test `TestBackend` que le corps de `json` filtré par `.a` affiche `[1, 2]` et que le filtre `.a.b |` affiche un message d'erreur visible sans paniquer sur un terminal 1×1
- [ ] 5.2 Afficher le texte de saisie en cours dans la barre d'état quand `model.filter.editing` est vrai ; vérifier par test `TestBackend` que le texte tapé apparaît à l'écran avant validation

## 6. Vérifications transverses

- [ ] 6.1 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests dans `src/app/filter.rs`, qu'aucun appel `std::fs` ou `std::process` n'y apparaît, et que `FilterResult::Error` ne recopie jamais le corps de réponse complet dans un message de log (aucun `println!`/`eprintln!` dans le module)
- [ ] 6.2 Documenter dans une note de code (commentaire de module de `filter.rs`) le point de coordination avec `add-search-and-yank` sur le mécanisme de saisie de texte (D3/Context de `design.md`), pour qu'il soit visible au moment de l'implémentation de l'autre changement
- [ ] 6.3 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
