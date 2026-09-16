## 1. Squelette du module

- [ ] 1.1 Créer `src/writer/{mod.rs, edit.rs, format.rs, error.rs}` avec commentaires de module en français, déclarer `pub mod writer;` dans `src/lib.rs` (D7) ; vérifier `cargo build` et `cargo clippy -- -D warnings`
- [ ] 1.2 Implémenter `EditError` et `WriteError` avec `thiserror` (variantes de D4, messages en français citant fichier/bloc/indice, jamais une valeur de champ) ; vérifier par un test unitaire que `Display` et `Debug` d'une erreur construite sur une tentative d'édition d'un en-tête dont la valeur d'origine est `Bearer s3cr3t` ne contiennent pas `s3cr3t`

## 2. Fixtures écrites à la main

- [ ] 2.1 Créer `tests/fixtures/collections/writer-cases/` selon la table de D8 : `bruno.json` minimal, `simple.bru` (GET, une URL), `headers.bru` (trois en-têtes dont un désactivé), `multiline-target.bru` (paramètre de requête à valeur courte), `json-body.bru` (`body:json` avec bloc inconnu avant `meta`), `crlf.bru` (mêmes en-têtes que `headers.bru`, fins de ligne `\r\n`), `form-body.bru` (`body: formUrlEncoded` avec `body:form-urlencoded`) ; vérifier par relecture que chaque fichier respecte l'indentation de deux espaces et qu'aucun secret réel n'y figure
- [ ] 2.2 Pour chaque fixture éditable (`simple.bru`, `headers.bru`, `multiline-target.bru`, `json-body.bru`, `crlf.bru`), créer à la main le fichier `*.after.bru` attendu après l'édition de test correspondante (D8) ; vérifier par relecture que seule la zone éditée diffère de la fixture d'origine, octet pour octet ailleurs

## 3. Résolution des éditions

- [ ] 3.1 Implémenter dans `edit.rs` `FieldEdit` (D1) et la résolution d'une cible unique vers `(Range<usize>, valeur/état actuels)` : `Url` via l'entrée `url` du bloc de méthode, `Header*`/`QueryParam*`/`PathParam*` via l'indice dans le bloc correspondant, `BodyText` via le bloc `body:<type>` désigné par `BodyKind::block_name()` ; erreurs `NoSuchBlock`, `IndexOutOfRange`, `MissingField`, `WrongBodyForm` sans effet de bord ; vérifier par tests unitaires sur des `BruFile` construits depuis des chaînes : chaque variante résolue avec succès, indice hors limite, bloc absent, `BodyText` sur un corps `formUrlEncoded`
- [ ] 3.2 Implémenter la fusion de plusieurs `FieldEdit` ciblant la même entrée dans un seul appel (D2, étape 2), le tri par `span.start` et l'erreur défensive `Overlap` ; vérifier par test unitaire que `HeaderValue` puis `HeaderEnabled` sur le même indice produisent un seul remplacement combinant les deux changements, dans l'ordre fourni

## 4. Sérialisation

- [ ] 4.1 Implémenter dans `format.rs` la détection de fin de ligne (`eol`) et le formatage d'une ligne d'entrée dictionnaire à indentation de deux espaces, avec ou sans préfixe `~` (D3) ; vérifier par tests unitaires : valeur simple LF, valeur simple CRLF, entrée désactivée
- [ ] 4.2 Implémenter le formatage d'une valeur multi-ligne au format `'''` (ouverture/fermeture à deux espaces, lignes intérieures à quatre espaces) et la réindentation à deux espaces d'un contenu de corps texte (D3) ; vérifier par tests unitaires que le résultat, reparsé par `bru-parser` (`BruFile::parse` puis lecture de l'entrée ou du bloc), redonne exactement la valeur fournie
- [ ] 4.3 Implémenter `resolve(ast, edits) -> Result<Vec<Replacement>, EditError>` combinant 3.1, 3.2 et le formatage de 4.1/4.2, et la fonction pure `serialize(ast, edits) -> Result<Vec<u8>, EditError>` qui applique les `Replacement` sur `ast.raw()` par concaténation ; vérifier par test unitaire qu'un appel sans aucune édition retourne les octets du fichier d'origine à l'identique

## 5. Écriture atomique

- [ ] 5.1 Implémenter `FileStamp` et `FileStamp::capture` (D5) ; vérifier par test unitaire que deux instantanés successifs d'un fichier inchangé sont égaux, et qu'un instantané après `fs::write` sur le même chemin en diffère
- [ ] 5.2 Implémenter le trait `RequestWriter` et `BruWriter::write_request` selon la séquence de D6 (vérification de fraîcheur, résolution, écriture dans un fichier temporaire du même répertoire, permissions préservées, renommage, nettoyage sur échec) ; vérifier par test d'intégration sur `simple.bru` copié dans un répertoire temporaire que le fichier réécrit égale `simple.after.bru` octet pour octet, et que le nouvel instantané retourné correspond à l'état du fichier après écriture
- [ ] 5.3 Vérifier par tests d'intégration, sur des copies temporaires des fixtures, chaque scénario de fidélité de la spec : `headers.bru` → `headers.after.bru` avec les deux en-têtes non édités identiques à l'octet près, `json-body.bru` → `json-body.after.bru` avec le bloc inconnu intact, `crlf.bru` → `crlf.after.bru` avec les fins de ligne `\r\n` préservées, `multiline-target.bru` → `multiline-target.after.bru` puis relecture par `BruLoader` confirmant la valeur multi-ligne
- [ ] 5.4 Vérifier par tests d'intégration les refus sans écriture : `Stale` quand le fichier est modifié entre `FileStamp::capture` et `write_request` (fichier cible resté celui du tiers), et chaque erreur de résolution de 3.1 sur une copie de fixture (fichier cible inchangé après l'appel)
- [ ] 5.5 Vérifier par test d'intégration `#[cfg(unix)]` que les permissions d'origine (par exemple `0640`, posées avec `fs::set_permissions`) sont celles du fichier après écriture, et qu'aucun fichier temporaire ne subsiste dans le répertoire après un succès ni après un échec simulé (répertoire rendu non inscriptible avant l'écriture du temporaire)

## 6. Abstraction et vérifications transverses

- [ ] 6.1 Implémenter un `FakeWriter` de test derrière `&dyn RequestWriter` qui enregistre les appels sans toucher au disque ; vérifier par test d'intégration qu'un code appelant générique fonctionne à l'identique avec `FakeWriter` et avec `BruWriter`
- [ ] 6.2 Vérifier par `grep` qu'aucun `unwrap()`/`expect()` n'existe hors tests dans `src/writer/`, qu'aucun message d'erreur ne formate une valeur de champ (seulement noms de bloc, indices, chemins), et que `src/collection/` n'a subi aucune modification
- [ ] 6.3 Lancer `cargo fmt --check`, `cargo clippy -- -D warnings` et `cargo test` ; tous doivent passer
