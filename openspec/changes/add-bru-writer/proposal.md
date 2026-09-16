## Why

L'édition dans l'interface (changement à venir `add-field-editing`) ne peut
commencer tant que rien ne sait écrire un fichier `.bru`. `bru-parser`
charge en lecture seule, par construction : son AST conserve des tranches
brutes de source précisément pour qu'une réécriture fidèle reste possible
plus tard (voir `openspec/specs/bru-parser/spec.md`, exigence « AST fidèle
au fichier source »). Il faut maintenant cette brique : sérialiser une
modification de champ vers le fichier, sans toucher à ce que l'utilisateur
n'a pas édité.

## What Changes

- Nouveau module `writer` (bibliothèque interne, sans UI) qui :
  - décrit une modification de requête comme une liste de `FieldEdit`
    ciblant un champ déjà présent dans le fichier : URL, valeur ou état
    activé/désactivé d'un en-tête existant, d'un paramètre de requête ou
    de chemin existant, et contenu d'un bloc de corps texte déjà présent
    (`body:json`, `body:text`, `body:xml`, `body:sparql`, `body:graphql`) ;
  - sérialise ces modifications par-dessus l'AST de `bru-parser` en ne
    remplaçant que les tranches de source correspondant aux champs
    édités : tout le reste du fichier — blocs non touchés, entrées
    voisines non éditées, blocs inconnus, ordre, fins de ligne — est
    réémis à l'octet près depuis les tranches déjà conservées par l'AST ;
  - refuse d'écrire si le fichier a changé sur disque depuis son chargement
    (taille et date de modification comparées à un instantané capturé au
    chargement), avec une erreur explicite plutôt qu'un écrasement ;
  - écrit de façon atomique : fichier temporaire dans le même répertoire,
    renommage, permissions Unix d'origine préservées ;
  - n'écrit jamais une valeur secrète dans un message d'erreur ou un log,
    et ne journalise rien.
- Exposition derrière un trait `RequestWriter`, implémenté par `BruWriter`,
  en miroir du trait `CollectionLoader` de `bru-parser`.
- Fixtures écrites à la main dans `tests/fixtures/collections/writer-cases/`
  et tests de sérialisation comparant les fichiers à l'octet près, sauf
  sur les octets attendus modifiés.

Hors périmètre, volontairement, pour ce changement :
- ajouter ou supprimer une entrée d'en-tête, de paramètre ou de variable
  (seules les entrées déjà présentes sont modifiables) ;
- renommer la clé d'une entrée existante ;
- changer le type de corps déclaré (ajouter ou retirer un bloc `body:*`),
  éditer le contenu d'un corps de forme formulaire (`body:form-urlencoded`,
  `body:multipart-form`, `body:file`) ;
- changer le mode d'auth déclaré (ajouter ou retirer un bloc `auth:*`) ;
- toute modification de `collection.bru`, `folder.bru` ou
  `environments/*.bru` ;
- toute modification de structure : création, suppression, renommage ou
  réordonnancement de fichiers ou de dossiers, renumérotation de `seq` ;
- branchement dans l'interface (`add-field-editing`) et dans la boucle
  `AppEvent`.

## Capabilities

### New Capabilities
- `bru-writer` : sérialisation fidèle d'une modification de champ vers un
  fichier `.bru` de requête, en ne réécrivant que les tranches de source
  correspondant aux champs édités, écriture atomique avec préservation des
  permissions, refus d'écrire sur un fichier modifié depuis son chargement,
  exposition derrière le trait `RequestWriter`.

### Modified Capabilities
<!-- Aucune : `bru-parser` n'est pas touché. Son AST (BruFile, spans,
     Entry) est consommé tel quel ; sa spec « Lecture seule » décrit son
     propre comportement de chargement et n'est pas affectée par
     l'existence d'un écrivain séparé. -->

## Impact

- Code : nouveau module `src/writer/` (édition, sérialisation, erreurs,
  écriture atomique), déclaré dans `src/lib.rs`. `src/collection/`,
  `src/runner/` et `src/app/` inchangés.
- Dépendances : aucune nouvelle crate. `std::fs` et `std::time::SystemTime`
  suffisent à l'écriture atomique et à l'instantané de fraîcheur.
- Tests : fixtures `.bru` versionnées, écrites à la main ; tests
  d'intégration comparant systématiquement le fichier réécrit à l'octet
  près en dehors de la modification attendue.
- Environnement : aucun prérequis, `bru` n'est pas nécessaire pour écrire.
