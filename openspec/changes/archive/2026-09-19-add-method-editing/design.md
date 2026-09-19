## Context

Voir `proposal.md` (Why) pour la demande. Deux mécanismes existants sont
réutilisés directement plutôt que réinventés :

- `bru-writer` (`src/writer/`) résout déjà une liste de `FieldEdit` en un
  brouillon (`Draft`, `src/writer/draft.rs`) puis en tranches à remplacer
  (`Vec<Replacement>`, `src/writer/edit.rs`, appliquées par
  `src/writer/format.rs`). L'URL suit déjà ce schéma via `UrlState { span,
  key, disabled, original, current }`, construit par `url_state(ast)` qui
  localise le bloc de méthode par `ast.blocks().find(|b|
  METHODS.contains(&b.name.as_str()))` (`crate::collection::ast::METHODS`,
  déjà les 9 valeurs).
- Le panneau Détail (`src/app/view/detail.rs`, `add-boxed-detail-sections`,
  implémenté mais pas encore archivé) sait déjà composer des boîtes
  bordées (`SectionBox`, `boxed_section`, `compose`) dans un tampon
  virtuel, avec curseur, défilement et clic qui ne dépendent que d'une
  séquence ordonnée de lignes (`FieldLine`) — jamais de coordonnées
  d'écran directes.

Particularité de la méthode, absente de tout champ déjà éditable : elle
EST le nom du bloc de méthode dans le fichier (`get { ... }`), pas une
entrée `clé: valeur` à l'intérieur. `Node { span, kind }`
(`src/collection/ast.rs`) donne, pour le nœud de ce bloc, la tranche
complète du bloc (« de la ligne d'ouverture à la ligne de fermeture »,
`src/collection/lexer.rs`) ; comme un bloc de méthode s'écrit toujours en
colonne 0 (aucune indentation avant un bloc de premier niveau, grammaire
de Bruno), la tranche du seul mot-clé de méthode est
`node.span.start..node.span.start + block.name.len()`.

## Goals / Non-Goals

**Goals:**
- Changer la méthode d'une requête depuis l'interface, avec la même
  discipline de sauvegarde explicite que les autres champs.
- Ne jamais pouvoir écrire une méthode absente des 9 valeurs reconnues
  par `bru-parser` : la renforcer par construction (liste fermée), pas
  seulement par validation après coup.
- Réutiliser l'infrastructure de rendu des boîtes de section
  (`add-boxed-detail-sections`) pour le sélecteur, plutôt qu'un nouveau
  mécanisme de survol flottant.

**Non-Goals:**
- Autoriser une méthode personnalisée hors des 9 valeurs (Bruno lui-même
  ne les reconnaît pas comme bloc de méthode).
- Toucher au clic souris pour ouvrir ou piloter le sélecteur : clavier
  seulement, comme `environment-picker`.
- Changer l'ordre d'affichage des autres champs au-delà d'insérer la
  méthode avant l'URL.

**Écart constaté à l'implémentation :** la méthode et l'URL sont rendues
sur deux lignes distinctes plutôt que sur la même ligne « GET
https://... » d'aujourd'hui. Voir « Interface : le champ Méthode a sa
propre ligne » ci-dessous pour la raison (un partage de ligne aurait
demandé de rendre `field_at_line`, et donc le clic, sensible à la
colonne — une extension de `mouse-support` hors du périmètre annoncé
« clavier seulement »).

## Decisions

### `bru-writer` : nouvel état de brouillon pour le nom de bloc
`Draft` gagne un champ `method: Result<MethodState, EditError>` (même
forme que `url`), avec :
```
struct MethodState {
    span: Range<usize>,  // node.span.start .. +block.name.len()
    original: String,     // ex. "get", tel qu'écrit
    current: String,
}
```
construit par `method_state(ast)`, analogue à `url_state(ast)` mais
cherchant le `Node` (pas seulement le `Block`) portant le bloc de
méthode, pour disposer de `node.span.start`. `FieldEdit::Method(String)`
porte la valeur en minuscules (celles de `ast::METHODS`, la casse
qu'écrit Bruno), pas la forme majuscule affichée par l'interface — la
transformation d'affichage reste dans la vue, comme aujourd'hui pour
`view.method` (`to_ascii_uppercase()` déjà appliqué à la lecture).
`Draft::apply` refuse une valeur hors de `ast::METHODS` sans la citer
dans l'erreur (même politique que les autres refus de `bru-writer`).
`replacements()` n'émet une tranche pour la méthode que si
`current != original`, comme l'URL.
- Alternative écartée : un type `enum Method { Get, Post, ... }` au lieu
  d'une `String` validée — plus sûr à la compilation, mais rompt la
  convention déjà établie de `FieldEdit` (toutes ses variantes portent
  des `String`, validées à la résolution comme les clés de paramètres) ;
  la liste fermée est déjà garantie côté interface par le sélecteur
  (aucun texte libre ne peut l'atteindre), la validation de `Draft`
  n'est qu'un filet de sécurité, pas la seule barrière.

### Interface : le champ Méthode a sa propre ligne
`field_at_line` (`mouse-support`) résout un clic uniquement par ligne,
jamais par colonne : aucun champ n'a jamais partagé sa ligne avec un
autre. Garder Méthode et URL sur une seule ligne « GET https://... »
aurait rendu ce partage ambigu pour un clic (toujours résolu vers le
premier champ de la ligne, quelle que soit la colonne cliquée), ce qui
aurait cassé silencieusement le clic sur l'URL — une extension de
`mouse-support` non annoncée dans ce changement. La méthode est donc
affichée sur sa propre ligne, juste avant l'URL, comme n'importe quel
autre champ ; `FieldLine.prefix_width` vaut `0` pour les deux, puisque
chacun occupe sa ligne en entier.

### Interface : le sélecteur est une boîte de section de plus
Plutôt qu'un survol flottant ou un panneau plein écran (comme
`environment-picker`, qui remplace tout le corps), le sélecteur ouvert
s'insère dans la séquence de lignes du détail, juste après la ligne
Méthode, comme une boîte supplémentaire construite avec
`boxed_section`/`SectionBox` (`add-boxed-detail-sections`) : titre
« Méthode », une ligne par valeur des 9, celle sous la présélection
stylée comme n'importe quelle position de curseur de champ
(`FIELD_CURSOR_STYLE`). Elle n'existe dans la séquence que pendant que
le sélecteur est ouvert (comme les lignes d'ajout d'entrée aujourd'hui,
absentes hors session). Ce choix évite d'inventer un mécanisme de
positionnement flottant : le défilement, le curseur et le clic
continuent de ne dépendre que de la séquence ordonnée de lignes, exactement
comme le reste du panneau Détail depuis le changement précédent.
- Alternative écartée : survol flottant positionné par coordonnées
  d'écran près de la ligne Méthode — plus proche d'un widget « dropdown »
  de GUI, mais demanderait un nouveau mécanisme de superposition que rien
  dans l'architecture actuelle ne fournit (le rendu du détail ne connaît
  que sa séquence de lignes, jamais une position d'écran absolue avant le
  calcul de défilement) ; rejeté pour rester dans le même modèle que le
  reste de `field-editing`.
- Alternative écartée : réutiliser `Focus::EnvironmentPicker` (panneau
  plein écran) — cohérent avec l'existant, mais un panneau plein écran
  fermerait visuellement le détail pendant l'édition d'un seul champ
  d'une session déjà ouverte, ce qui n'a pas de précédent (aucun autre
  état de session n'occupe tout le corps) ; rejeté pour rester local à
  la session.

### Modèle : un troisième état de session
`EditState` gagne une variante pour « sélecteur de méthode ouvert »,
portant l'indice présélectionné dans `ast::METHODS` (0..9), aux côtés de
`FieldSelect` et `Input`. Le routage clavier (`update.rs`) traite ce
nouvel état comme un troisième cas de l'arbitrage déjà en place pour
`Input` (capture totale sauf `↑`/`k`, `↓`/`j`, `Entrée`, `Échap`,
`Ctrl+C`), sans toucher à la structure de cet arbitrage.

## Risks / Trade-offs

- [Risque] Le calcul de la tranche du nom de bloc
  (`node.span.start..+len`) suppose qu'aucune indentation ne précède un
  bloc de méthode de premier niveau → déjà une invariante du format
  Bruno et du lexer (`lexer.rs`, doc du module : blocs ouverts en colonne
  0) ; à couvrir par une fixture d'écriture dédiée (voir tasks.md)
  plutôt que supposée sans test.
- [Risque] Oublier de resynchroniser l'ordre d'affichage des positions de
  curseur (méthode avant URL) romprait `mouse-support`/`search-and-yank`,
  qui ne dépendent que de cette séquence → déjà couvert par la spec
  modifiée de `field-editing` (« Champs éditables et curseur de champ »)
  et par les tests existants de position de curseur, à adapter.
- [Risque] La boîte du sélecteur, ouverte puis fermée, change le nombre
  total de lignes du détail pendant qu'elle est affichée → même
  mécanique que les lignes d'ajout d'entrée déjà gérée par
  `field-editing` et par le rendu par tampon virtuel
  (`add-boxed-detail-sections`), pas un nouveau cas.

## Migration Plan

Aucune migration de données. Aucun changement de format sur disque au-delà
de ce que l'utilisateur demande explicitement (renommage du bloc de
méthode, comme tout autre champ édité). Déployé avec le prochain build du
binaire.
