## Context

Voir `proposal.md` (Why) pour le retour utilisateur à l'origine du
changement.

Le panneau Détail est un unique `Text`/`Paragraph` scrollable
(`src/app/view/mod.rs`) : `field-editing` s'appuie sur cette structure
« une ligne logique = un élément de `Text.lines` » pour son défilement
vertical, son décalage horizontal de la ligne éditée et la position du
curseur de texte. Toute restructuration de ce panneau en sous-widgets
séparés romprait ce mécanisme.

Aujourd'hui :
- `field()` (`detail.rs:97`) rend un champ simple comme deux spans —
  libellé stylé `theme::LABEL` (DIM), puis valeur au style normal.
- `entries()` (lecture seule, `detail.rs:108`) et `editable_entries()`
  (session, `detail.rs:211`) construisent chacune une seule chaîne
  concaténée `"  clé: valeur"` puis un unique `Line::raw(...)` : aucune
  distinction de style entre clé et valeur.
- Le corps texte (`detail.rs:394-426`) est une suite de `Line::raw(...)`
  sans style propre ; seule la ligne sous le curseur de session reçoit
  `FIELD_CURSOR_STYLE` (`Modifier::REVERSED`) via `tint_line`.
- `tint_line` (`detail.rs:769`) patche le style de **chaque span** d'une
  `Line`, donc une ligne à plusieurs spans (clé + valeur) reste
  compatible avec la surbrillance de curseur existante sans modification.
- Le rendu du corps est déjà gardé par le même calcul que la navigation
  de session : `EditableField::list_for(view).contains(&EditableField::BodyText)`
  (`detail.rs:413`), qui n'est vrai que pour `json`, `text`, `xml`,
  `sparql`, `graphql`.

## Goals / Non-Goals

**Goals:**
- Rendre la clé d'une entrée (en-tête, paramètre) visuellement distincte
  de sa valeur, en lecture comme en session, avec le même style que les
  autres libellés du détail.
- Rendre le corps éditable identifiable comme zone de saisie, sans
  toucher au calcul de ses lignes ni à la position du curseur.

**Non-Goals:**
- Changer le nombre de lignes logiques produites par `Text` (le
  défilement de `field-editing` compte sur `detail_line_count` restant
  stable) : chaque entrée reste une `Line`, chaque ligne de corps reste
  une `Line`.
- Introduire un widget `Block` bordé autour du corps ou des entrées.
- Toucher `editable_entries` au-delà de la construction des spans :
  logique de curseur, de saisie, de décalage horizontal (`skip_columns`,
  `hscroll`) reste identique.

## Decisions

### Style de la clé : réutiliser `theme::LABEL`
Réutilise le token déjà utilisé par `field()` pour « Chemin », « Méthode »,
etc., plutôt que d'introduire un nouveau style. Cohérent avec l'intention
de `visual-theme` (mêmes rôles, même style partout) et n'ajoute aucun
token.
- Alternative écartée : un style dédié aux clés d'entrées (ex. couleur
  différente de `LABEL`) — rejetée, aucune raison fonctionnelle de
  distinguer un libellé de champ simple d'une clé d'en-tête, et ça
  multiplierait les styles à maintenir.

### `entries()` et `editable_entries()` : construire deux spans au lieu d'une chaîne
Remplace la construction `format!("{prefix}{val}")` + `Line::raw` par
`Line::from(vec![Span::styled(key_part, theme::LABEL), Span::raw(value_part)])`,
sur le modèle de `field()`. Le préfixe `"  "` et le `": "` restent dans le
span de clé (ou dans un span neutre initial), pour ne pas changer le texte
brut retourné par `plain_lines`/la copie (`search-and-yank`), qui
concatène le contenu des spans sans le style.
- Le cas « désactivé » garde son `Style::new().add_modifier(Modifier::DIM)`
  appliqué à la ligne entière via `tint_line`-like patch (ou en stylant
  chaque span avec DIM en plus de son style propre), pour que la mise en
  dim actuelle ne disparaisse pas.
- Le cas « renommage en cours » (la clé est le texte de saisie) garde la
  clé en style `LABEL` ; seule sa valeur reste au style normal.
- L'entrée provisoire d'ajout (clé validée, valeur en cours de saisie)
  applique la même règle dès que la clé existe.
- `prefix_width` (utilisé pour le calcul de colonne du curseur de texte)
  ne change pas : il continue de mesurer la largeur affichée du préfixe,
  indépendamment du style appliqué.

### Corps éditable : fond de ligne, pas de bordure de widget
Applique un nouveau style `theme::EDITABLE_BODY` (fond légèrement
distinct de `theme::BACKGROUND`) à chaque `Line` du corps, avant
l'éventuel `tint_line(..., FIELD_CURSOR_STYLE)` du curseur de session —
`Style::patch` fait que `REVERSED` s'applique par-dessus ce fond sans
conflit. Condition d'application : le même booléen déjà calculé à
`detail.rs:413` (`EditableField::list_for(view).contains(&field)`), pas
une nouvelle vérification sur `BodyContent`/`BodyKind` — garantit que
« Corps sans type éditable non affecté » (scénario de la spec) reste
vrai pour tout type produisant `BodyContent::Text` mais absent de la
liste éditable (ex. `Other`).
- Alternative écartée : un `Block::bordered()` séparé autour de la zone
  de corps — demanderait de sortir le corps du `Text` unique du panneau
  Détail (sous-zone `Rect` dédiée), ce qui casse le défilement vertical
  et le décalage horizontal d'une seule ligne déjà spécifiés par
  `field-editing` sur l'ensemble du panneau. Rejetée : hors périmètre de
  ce changement (proposal.md, Impact).
- Alternative écartée : marqueurs texte (`│`) en début de chaque ligne du
  corps — rejetée, ajoute des caractères au texte brut copié/recherché
  (`plain_lines`), ce qui changerait le comportement de `search-and-yank`
  sur le corps ; un style de fond ne change que le rendu.

## Risks / Trade-offs

- [Risque] Le nouveau token `theme::EDITABLE_BODY` doit rester
  suffisamment proche de `theme::BACKGROUND` pour ne pas paraître comme
  une erreur de rendu ou gêner la lisibilité en thème clair/sombre du
  terminal → choisir une variation de luminosité modeste (proche de
  l'écart déjà utilisé entre `BACKGROUND` et le fond de sélection), à
  valider visuellement pendant l'implémentation.
- [Risque] Construire deux spans par entrée au lieu d'une chaîne unique
  pourrait, par erreur, changer le texte brut concaténé (espace ou `:`
  déplacé) et donc casser un test de `plain_lines`/copie → couvrir par
  les tests existants de `writer_fixtures`/`app_shell` qui comparent déjà
  le texte brut du détail ; ajouter un test si un cas n'est pas déjà
  couvert.

## Migration Plan

Aucune migration : changement de rendu uniquement, aucun format sur
disque ni API modifiés. Déployé avec le prochain build du binaire.
