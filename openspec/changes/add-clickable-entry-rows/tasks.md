## 1. Rendu : enregistrer la cible de clic

- [ ] 1.1 Dans `src/app/view/detail.rs::editable_entries`, branche
      `values.is_empty() && session.is_none()` : avant de pousser la ligne
      `  aucun`, enregistrer une `FieldLine { field:
      EditableField::AddRow(section), line: lines.len(), count: 1,
      prefix_width: 2 }` dans `field_lines`. Vérifier avec
      `cargo build` que la signature de `editable_entries` n'a pas besoin
      de changer (elle reçoit déjà `field_lines` en paramètre).

## 2. Tests

- [ ] 2.1 Dans `src/app/update.rs` (module `mouse_tests`), ajouter un test
      simulant un clic sur la ligne `aucun` d'une section vide (en-têtes
      ou paramètres) d'une requête sans session ouverte, et vérifier que
      la session s'ouvre avec l'état Saisie actif sur la clé d'une
      nouvelle entrée de cette section (même vérification que pour un
      clic sur une ligne « + Ajouter » déjà ouverte, cf. tests existants
      autour de `start_add`/`AddRow`).
- [ ] 2.2 Ajouter un test couvrant les trois sections (en-têtes,
      paramètres de requête, paramètres de chemin) sur une requête où
      elles sont toutes vides, pour vérifier que chacune ouvre l'ajout
      sur la bonne section et non sur une autre.
- [ ] 2.3 Vérifier par un test qu'une section qui a déjà au moins une
      entrée, sans session ouverte, ne réagit toujours qu'en focus au
      clic sous la dernière entrée (aucune ligne d'ajout hors session,
      cf. hors périmètre du proposal.md) — pas de régression sur ce cas.
- [ ] 2.4 Vérifier dans `src/app/view/detail.rs` (module `tests`) qu'un
      test existant ou nouveau confirme que `request_text_and_fields`
      inclut désormais un `FieldLine` pour `EditableField::AddRow(section)`
      sur la ligne `aucun` quand `values.is_empty() && session.is_none()`.

## 3. Vérification globale

- [ ] 3.1 `cargo fmt`, puis `cargo clippy -- -D warnings`, puis
      `cargo test` passent sans erreur ni avertissement.
- [ ] 3.2 Vérification manuelle : lancer l'app sur une requête sans
      en-tête ni paramètre, cliquer sur `aucun` dans chacune des trois
      sections, confirmer que l'ajout démarre à chaque fois sur la bonne
      section.
