## Why

bruno-tui ne peut rien afficher tant qu'il ne sait pas lire une collection
Bruno depuis le disque : la capacité `bru-runner` exécute des requêtes, mais
personne ne sait encore lesquelles existent, dans quel ordre, ni ce qu'elles
contiennent. Il faut une brique de chargement en lecture seule, indépendante
de l'UI, qui rende l'arbre de la collection et le contenu de chaque requête
sous forme typée, sans rien perdre du fichier source afin qu'une édition
fidèle reste possible plus tard.

## What Changes

- Nouveau module `collection` (bibliothèque interne, sans UI) qui :
  - trouve la racine d'une collection à partir d'un chemin quelconque en
    remontant jusqu'au `bruno.json`, et parcourt récursivement le dossier en
    respectant la liste `ignore` de `bruno.json` ;
  - parse les fichiers `.bru` de requête, `collection.bru`, `folder.bru` et
    `environments/*.bru` en un **AST fidèle** : ordre des blocs préservé,
    texte source de chaque bloc conservé à l'octet, blocs inconnus gardés
    tels quels. Rien n'est jeté ;
  - construit un arbre de collection ordonné par le champ `seq` du bloc
    `meta`, dossiers et requêtes confondus, chaque dossier tirant son `seq`
    de son `folder.bru` ;
  - expose une **vue typée** par-dessus l'AST pour l'affichage : méthode,
    URL, en-têtes, paramètres de requête, corps et son type, mode d'auth
    déclaré, et présence des blocs `script:pre-request`,
    `script:post-response`, `tests` et `assert` ;
  - marque en erreur, avec sa raison, tout fichier `.bru` illisible ou
    malformé, sans interrompre le chargement du reste de la collection ;
  - est exposé derrière un trait `CollectionLoader`, pour qu'un loader
    OpenCollection YAML puisse être ajouté sans toucher aux appelants.
- Fixtures `.bru` minimales écrites à la main dans
  `tests/fixtures/collections/parser/` : GET simple, POST à corps JSON avec
  accolades imbriquées et accolades dans une chaîne, requête avec scripts,
  `tests` et `assert`, bloc inconnu, fichier malformé, arborescence à deux
  niveaux avec `seq` non contigus.
- Test de round-trip : la concaténation du texte brut des nœuds de l'AST
  reproduit chaque fixture à l'octet près. La resérialisation depuis la
  vue typée n'est pas implémentée.

Hors périmètre, volontairement :
- résolution ou interpolation de variables ;
- application de l'héritage d'auth entre collection, dossier et requête
  (le mode `inherit` est exposé comme tel, pas résolu) ;
- toute écriture sur disque ;
- exécution de requêtes (capacité `bru-runner`) ;
- affichage (vues ratatui) et branchement dans la boucle `AppEvent`.

## Capabilities

### New Capabilities
- `bru-parser`: découverte de la racine d'une collection Bruno, parsing
  fidèle des fichiers `.bru` en AST conservant le texte source, arbre de
  collection ordonné par `seq`, vue typée pour l'affichage, tolérance aux
  fichiers invalides, exposition derrière le trait `CollectionLoader`.

### Modified Capabilities
<!-- Aucune : `bru-runner` n'est pas touché. Il reçoit déjà des chemins de
     cibles relatifs à la racine ; le parser les fournit, sans changer le
     contrat du runner. -->

## Impact

- Code : nouveau module `src/collection/` (racine et traversée, lexer de
  blocs, AST, vue typée, arbre, trait `CollectionLoader`, erreurs),
  déclaré dans `src/lib.rs`. `src/runner/` et `src/main.rs` inchangés.
- Dépendances : aucune nouvelle crate. `serde`/`serde_json` déjà présents
  servent à lire `bruno.json`. Le parser `.bru` est écrit à la main, sans
  générateur de parseur (justifié dans `design.md`).
- Tests : nouvelles fixtures `.bru` versionnées, écrites à la main ; tests
  d'intégration dans `tests/` sur ces fixtures uniquement.
- Environnement : aucun prérequis, `bru` n'est pas nécessaire pour parser.
