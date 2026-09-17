## Why

Aujourd'hui, la session d'édition du panneau Requête ne permet de modifier
que ce qui existe déjà : la spec `bru-writer` interdit explicitement
d'ajouter ou de supprimer une entrée et de renommer une clé. Pour ajouter un
en-tête `Authorization`, un paramètre de requête ou un paramètre de chemin,
il faut donc quitter le TUI et ouvrir Bruno ou un éditeur, ce qui casse le
parcours d'exploration d'API. Le CLAUDE.md interdit toute écriture `.bru`
dont la capacité n'a pas été spécifiée et approuvée : cette proposition
spécifie ces nouvelles écritures avant toute implémentation.

## What Changes

- `bru-writer` accepte trois nouvelles modifications sur les blocs
  `headers`, `params:query` et `params:path` : ajouter une entrée (en fin de
  bloc, bloc créé s'il est absent), supprimer une entrée (bloc retiré s'il
  devient vide), renommer la clé d'une entrée. **BREAKING** (spec) : la
  clause « MUST NOT permettre d'ajouter ou de supprimer une entrée, de
  renommer une clé existante » est levée pour ces trois blocs uniquement.
- Les modifications d'une même sauvegarde s'appliquent dans l'ordre : un
  indice désigne la position dans la liste résultant des modifications
  précédentes. Sans modification structurelle, le comportement actuel est
  inchangé.
- Règles d'écriture explicites : réécriture limitée aux tranches touchées
  (une entrée, ou l'insertion/le retrait d'un bloc entier), fins de ligne
  préservées, écriture atomique et refus si le fichier a changé sur disque
  (exigences existantes, étendues aux nouvelles modifications), clés
  dupliquées autorisées et adressées par indice, entrées désactivées
  écrites avec le préfixe `~`, clés invalides refusées.
- Cohérence `params:query` ↔ URL alignée sur Bruno : toute modification
  d'un paramètre de requête (ajout, suppression, renommage, valeur,
  activation) reconstruit la chaîne de requête de l'URL à partir des
  paramètres activés ; toute modification de l'URL recalcule les
  paramètres activés à partir de sa chaîne de requête. `bru run` n'envoie
  que l'URL : sans cette règle, un paramètre ajouté serait sans effet.
- `bru-writer` expose un aperçu pur (sans I/O) de l'état de la requête
  après application des modifications, pour que la session affiche
  exactement ce qui sera écrit, URL synchronisée comprise.
- `field-editing`, sur l'édition directe de `improve-direct-editing` : en
  Sélection de champ, chaque section (en-têtes, paramètres de requête, de
  chemin) porte une ligne « + Ajouter » (`Entrée` dessus, ou `a` sur une
  entrée de la section, enchaîne la saisie de la clé puis de la valeur),
  `d` supprime l'entrée sous le curseur sans confirmation, `c` renomme sa
  clé. Rien de nouveau en Saisie, où toute lettre reste du texte ; `Échap`
  annule, `Ctrl+S` valide puis enregistre (à l'étape de la clé : entrée
  ajoutée avec une valeur vide). Une modification refusée laisse la saisie
  ouverte. La liste des champs suit l'aperçu, et le blocage du changement
  de requête avec modifications non enregistrées s'applique aux ajouts,
  suppressions et renommages.
- Fixtures de vérification : nouveaux cas `writer-cases/` avec fichiers
  `*.after.bru` attendus à l'octet près, et vérification (test ignoré par
  défaut, comme `runner_real_bru`) que chaque `*.after.bru` est relu par le
  parser officiel de Bruno (`@usebruno/lang`, livré avec `bru`) avec les
  en-têtes, paramètres et URL attendus.

Hors périmètre : renommage ou édition de l'URL côté paramètres de chemin
(aucune synchronisation `params:path` ↔ segments `:nom` de l'URL),
réordonnancement d'entrées, clés entre guillemets (`"ma clé": …`), ajout
d'entrées dans d'autres blocs (`assert`, `vars`, corps formulaire, auth),
annulation fine (undo), encodage d'URL des paramètres.

## Capabilities

### New Capabilities

Aucune.

### Modified Capabilities

- `bru-writer` : levée de l'interdiction d'ajout/suppression/renommage pour
  `headers`, `params:query`, `params:path` ; application séquentielle des
  modifications ; règles d'insertion et de retrait de blocs ; validité des
  clés ; synchronisation `params:query` ↔ URL ; aperçu pur ; fidélité,
  fraîcheur et atomicité étendues aux modifications structurelles.
- `field-editing` : lignes « + Ajouter » et touches `a`, `d`, `c` en
  Sélection de champ ; saisies de clé et de valeur ; refus immédiat d'une
  modification invalide ; `Ctrl+S` pendant la saisie d'une clé ; liste de
  champs et détail dérivés de l'aperçu ; barre d'aide étendue.

## Impact

- Code : `src/writer/edit.rs` (nouvelles variantes de `FieldEdit`,
  résolution séquentielle), `src/writer/format.rs`
  (formatage d'un bloc créé), `src/writer/error.rs` (clé invalide),
  nouveaux `src/writer/draft.rs` (brouillon ordonné, aperçu pur) et
  `src/writer/query.rs` (chaîne de requête de l'URL) ;
  `src/app/model.rs` (`EditSession`, `EditableField`, cible de saisie), `src/app/message.rs` et
  `src/app/update.rs` (messages et routage des touches),
  `src/app/view/detail.rs` et `src/app/view/mod.rs` (rendu depuis
  l'aperçu, lignes d'ajout, barre d'aide) ; `src/app/text_input.rs`
  réutilisé sans modification.
- Tests : `tests/writer_fixtures.rs`, `tests/field_editing.rs`, nouvelles
  fixtures dans `tests/fixtures/collections/writer-cases/`, nouveau
  `tests/writer_bruno_lang.rs` (comparaison aux relectures Bruno versionnées,
  plus un test ignoré qui relance Bruno), scripts
  `scripts/bruno-lang-dump.js` et `scripts/gen-writer-bruno-fixtures.sh`.
- Dépendances : aucune nouvelle dépendance Rust. La vérification Bruno
  réutilise `node` et `@usebruno/lang` déjà installés avec `bru`, hors
  `cargo test` par défaut.
- Autres changements : construit sur `improve-direct-editing` et
  `add-status-panel`, déjà dans `main` ; point de contact avec
  `add-mouse-support` (clic sur une ligne du détail, en cours dans un
  autre worktree) détaillé dans `design.md`.
