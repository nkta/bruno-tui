## Why

Aujourd'hui, seule l'URL est éditable en tête du détail d'une requête : la
méthode HTTP (`GET`, `POST`, ...) est affichée mais ne fait partie
d'aucun champ éditable (`EditableField` n'a pas de variante `Method`, et
`bru-writer` n'a pas de `FieldEdit` pour elle). L'utilisateur veut pouvoir
la changer depuis l'interface, comme les autres champs de tête (URL,
déjà couverte).

## What Changes

- La méthode devient un champ éditable, en première position du curseur
  de champ (avant l'URL, dans l'ordre d'affichage actuel).
- Contrairement à tous les champs déjà éditables (texte libre), la
  méthode se choisit dans une liste fermée des 9 valeurs que `bru-parser`
  reconnaît comme bloc de méthode (`GET`, `POST`, `PUT`, `DELETE`,
  `PATCH`, `OPTIONS`, `HEAD`, `CONNECT`, `TRACE` — `ast::METHODS`) : sur
  `Entrée`, au lieu d'ouvrir une saisie de texte, le système ouvre un
  sélecteur listant ces 9 valeurs, la méthode actuelle présélectionnée,
  navigable par `↑`/`↓`, validé par `Entrée`, annulé par `Échap` sans
  changement — repris du fonctionnement déjà établi par
  `environment-picker` pour le choix d'environnement.
- Choisir une valeur dans le sélecteur modifie l'aperçu de session (pas
  le disque), marque la session modifiée si la valeur diffère de la
  méthode chargée, et suit la même discipline de sauvegarde explicite
  (`Ctrl+S`) que tout autre champ de la session.
- `bru-writer` gagne un nouveau type de modification pour la méthode.
  C'est une différence structurelle avec tous les champs déjà éditables :
  eux sont des `clé: valeur` à l'intérieur du bloc de méthode, alors que
  la méthode EST le nom du bloc lui-même (`get { ... }`, `post { ... }`).
  Éditer la méthode revient à renommer ce mot-clé d'ouverture, jamais
  fait jusqu'ici par `bru-writer` — voir `design.md`.
- Le terme « endpoint » employé par l'utilisateur désigne l'URL, déjà
  éditable : aucun changement de ce côté.

## Capabilities

### New Capabilities

Aucune.

### Modified Capabilities

- `field-editing` : la méthode devient une position de curseur (première
  position, avant l'URL) ; `Entrée` sur ce champ ouvre un sélecteur en
  liste plutôt qu'une saisie de texte ; la barre d'aide et la
  confirmation de changement de sélection en tiennent compte. Aucune
  entrée du sélecteur n'est cliquable (navigation au clavier seulement),
  mais un clic pendant qu'il est ouvert le referme sans rien changer,
  comme `Échap`, pour que la souris ne reste jamais inerte tant qu'il est
  affiché.
- `bru-writer` : les champs éditables incluent désormais le nom du bloc
  de méthode ; une nouvelle règle décrit comment sa tranche source est
  localisée et remplacée sans toucher au reste du bloc (délimiteur,
  contenu), et sa validation (une des 9 valeurs connues).

`environment-picker`, `search-and-yank`, `visual-theme` et
`add-boxed-detail-sections` (changement précédent, implémenté mais pas
encore archivé) ne sont pas modifiés : la ligne Méthode + URL reste hors
boîte de section, comme aujourd'hui. `mouse-support` n'est pas modifié
dans son texte de spécification (son comportement déjà défini pour un
clic hors du champ en cours de saisie continue de s'appliquer) ; le
comportement propre au clic pendant que le sélecteur est ouvert est
entièrement décrit par la nouvelle exigence de `field-editing`
ci-dessus, pas par une exigence de `mouse-support`.

## Impact

- Code : `src/app/model.rs` (`EditableField::Method`, état de session
  pour le sélecteur ouvert), `src/app/update.rs` (ouverture/fermeture du
  sélecteur, navigation, validation), `src/app/message.rs` (touches du
  sélecteur), `src/app/view/detail.rs` et `src/app/view/mod.rs` (rendu du
  sélecteur, barre d'aide), `src/writer/edit.rs` (`FieldEdit::Method`),
  `src/writer/draft.rs` (localisation et remplacement de la tranche du
  nom de bloc), `src/writer/error.rs` (refus d'une méthode inconnue).
- Aucune nouvelle dépendance.
- Aucun changement de format sur disque au-delà de ce que l'utilisateur
  demande explicitement (renommage du bloc de méthode).
