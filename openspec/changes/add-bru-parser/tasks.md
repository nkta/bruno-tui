## 1. Squelette du module

- [x] 1.1 Créer `src/collection/{mod.rs, error.rs, lexer.rs, ast.rs, view.rs, tree.rs, loader.rs, config.rs}` avec commentaires de module en français, déclarer `pub mod collection;` dans `src/lib.rs` ; vérifier `cargo build` et `cargo clippy -- -D warnings`
- [x] 1.2 Implémenter `LoadError` et `ParseError` avec `thiserror` (variantes de D8, messages en français citant bloc et ligne, jamais le contenu) ; vérifier par un test unitaire que `Display` et `Debug` d'un `ParseError` construit sur un fichier contenant `Bearer s3cr3t` ne contiennent pas `s3cr3t`

## 2. Fixtures écrites à la main

- [x] 2.1 Créer `tests/fixtures/collections/parser-cases/` selon la table de D9 : `bruno.json` (`ignore` incluant `tmp`, champs inconnus), `collection.bru`, `simple-get.bru`, `post-json.bru` (accolades imbriquées et `"}"` dans une chaîne), `scripted.bru` (scripts pré/post, `tests`, `assert`, en-tête `~`), `unknown-block.bru`, `no-seq.bru`, `multiline.bru` (valeur `'''`), `grp/` avec `folder.bru`, `x.bru`, `y.bru`, `inherit.bru`, `grp/sub/` avec `folder.bru` et `deep.bru`, `misc/z.bru`, `environments/local.bru`, `tmp/ignored.bru`, `notes.md` ; vérifier par relecture que chaque fichier respecte l'indentation de deux espaces et qu'aucun secret réel n'y figure
- [x] 2.2 Créer les fixtures invalides `broken.bru` (bloc `headers` jamais fermé), `no-method.bru` (`meta` + `headers`) et `badmeta/folder.bru` malformé avec `badmeta/ok.bru` valide ; vérifier par relecture que chaque fichier n'a qu'un seul défaut, celui attendu par la spec

## 3. Lexer et AST

- [x] 3.1 Implémenter dans `lexer.rs` le découpage en lignes conservant `\n` ou `\r\n` et l'automate hors bloc / bloc `{` / bloc `[` produisant les nœuds `Blank` et `Block` avec leurs tranches (D3) ; vérifier par tests unitaires sur chaînes inline : ouverture avec espaces de fin, `}` indenté conservé dans le contenu, fermeture `}` suivie d'espaces, fichier sans saut de ligne final, fichier CRLF
- [x] 3.2 Implémenter les erreurs de découpage `TextOutsideBlock`, `BadHeader`, `BadClosing`, `UnclosedBlock` avec numéro de ligne ; vérifier par tests unitaires qu'une ligne hors bloc, une ouverture sans `{`, un `}` suivi de texte et un bloc non fermé produisent la variante et la ligne attendues
- [x] 3.3 Implémenter dans `ast.rs` `BruFile`, `Node`, `NodeKind`, `BlockBody`, `Entry` (D2), la table nom → forme des blocs connus, l'interprétation dictionnaire (`~`, clé/valeur, `'''` désindenté de quatre espaces, ligne sans `:` conservée), texte (`dedented()` retirant au plus deux espaces) et liste ; vérifier par tests unitaires : entrée désactivée, valeur `'''` sur trois lignes, `dedented()` du corps JSON à accolades imbriquées, bloc inconnu en `Unknown` avec son nom
- [x] 3.4 Implémenter `BruFile::raw()` et l'invariant de couverture (tranches contiguës de `0` à `source.len()`) ; vérifier par un test unitaire que l'invariant tient et que `raw()` égale le source pour chaque cas inline de 3.1

## 4. Vue typée

- [x] 4.1 Implémenter dans `view.rs` `RequestView::from_ast` (D4) : `meta`, méthode, URL, en-têtes, paramètres query/path avec état, type et contenu de corps, `AuthMode` depuis la clé `auth`, présence des blocs scripts/tests/assert, assertions ; `MissingMethod` si aucun bloc de méthode ; vérifier par tests unitaires sur des `BruFile` construits depuis des chaînes : GET simple avec `{{host}}` intact, POST `json`, `auth: inherit` exposé tel quel, absence de méthode
- [x] 4.2 Implémenter `FileMeta::from_ast` pour `collection.bru` et `folder.bru` (nom, `seq`, présence en-têtes/auth/scripts/tests/vars) et `Environment::from_ast` (variables avec état, noms secrets sans valeur) ; vérifier par tests unitaires sur chaînes inline : `folder.bru` avec `auth:bearer`, environnement avec `vars` et `vars:secret`

## 5. Configuration, traversée et arbre

- [x] 5.1 Implémenter dans `config.rs` la lecture de `bruno.json` (`name`, `ignore`, champs inconnus ignorés) et la remontée vers la racine (D6) ; vérifier par tests d'intégration : chemin d'une requête deux niveaux sous la racine → racine trouvée, répertoire hors collection → `NotACollection` portant le chemin, `bruno.json` de la fixture lu avec ses champs inconnus
- [x] 5.2 Implémenter dans `tree.rs` le parcours `read_dir` trié, filtres `ignore` + `node_modules` + `.git`, exclusion de `environments`, non-suivi des liens symboliques, profondeur bornée, et la construction des nœuds `Request` / `Folder` / `Error` avec chemins relatifs à la racine ; vérifier par test d'intégration sur `parser-cases/` que `tmp/` et `notes.md` ne produisent aucun nœud, que `misc` apparaît sans `seq`, et que `environments` n'est pas un dossier de l'arbre
- [x] 5.3 Implémenter le tri `(seq.is_none(), seq, nom_de_fichier)` à chaque niveau (D5) ; vérifier par test d'intégration l'ordre attendu de la spec : à la racine `grp` (3), `simple-get` (7), `post-json` (10), `scripted` (12), puis les sans-`seq` par nom ; dans `grp` : `x` (2), `sub` (5), `y` (20), puis `inherit`
- [x] 5.4 Implémenter la tolérance : `broken.bru` et `no-method.bru` en `ErrorNode` avec raison, `badmeta/folder.bru` en erreur de méta-données sans bloquer `badmeta/ok.bru` ; vérifier par test d'intégration que la raison de `broken.bru` mentionne le bloc `headers` non fermé et sa ligne d'ouverture, et que `badmeta` contient bien la requête `ok`
- [x] 5.5 Implémenter le trait `CollectionLoader`, `BruLoader::load` assemblant racine, `collection.bru`, arbre et environnements triés par nom (D7) ; vérifier par test d'intégration qu'un `FakeLoader` retournant un arbre fixe s'utilise derrière `&dyn CollectionLoader` avec le même code appelant que `BruLoader`

## 6. Vérifications transverses

- [x] 6.1 Test d'intégration `tests/parser_fixtures.rs` de round-trip : pour chaque `.bru` valide de `parser-cases/` et de `runner-probe/`, `raw()` égale le contenu du fichier octet pour octet, et la variante CRLF générée en mémoire depuis `simple-get.bru` passe aussi ; vérifier `cargo test`
- [x] 6.2 Test de lecture seule : copier `parser-cases/` dans un répertoire temporaire, relever `(chemin, taille, mtime)` de chaque entrée, charger, comparer ; vérifier qu'aucune différence n'apparaît
- [x] 6.3 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests dans `src/collection/`, qu'aucun appel `std::fs` d'écriture (`write`, `create`, `remove`, `rename`) n'apparaît dans le module, et qu'aucun message d'erreur ne formate le contenu d'une ligne
- [x] 6.4 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
