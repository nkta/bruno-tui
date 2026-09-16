## Context

Voir `proposal.md` pour le pourquoi. État actuel du code, vérifié avant
d'écrire ce document :

- `detail::result_lines(outcome, filter)` (`src/app/view/detail.rs`)
  construit, dans l'ordre : `section("Résultat")` + Verdict + Statut
  (+ Erreur) + Temps de réponse, puis `section("En-têtes de réponse")`
  + les en-têtes, puis soit la ligne de filtre et son résultat/erreur,
  soit `section("Corps de réponse")` + le corps brut, puis quatre blocs
  `push_checks` (Assertions, Tests, Tests pré-requête, Tests
  post-réponse), chacun avec son propre titre de section. C'est
  `detail::response_text(model)` qui appelle `result_lines` et renvoie
  le tout comme un seul `Text`.
- Aucune fonction de `response_line_count`, `response_max_scroll`,
  recherche ou sélection de la réponse ne dépend d'un décalage de ligne
  codé en dur : toutes lisent `response_text(model).lines.len()` ou la
  position de défilement dynamiquement. Découper `response_text` en
  bandeau + contenu d'onglet ne casse donc rien de ce côté (contrairement
  à la contrainte `cursor_position_in_detail` côté Détail, qui ne
  concerne pas la réponse).
- `message.rs::key_message` route toute touche vers `capture_message`
  dès qu'une capture de texte est active (`Model::text_capture()`,
  recherche ou filtre en cours de saisie), avant même d'atteindre le
  mapping normal de `h`/`l`/flèches. Les nouvelles touches de bascule
  d'onglet sont donc automatiquement inactives pendant une saisie, sans
  code supplémentaire.
- Dans le `match model.focus` de la navigation générique
  (`src/app/update.rs`), `Focus::Response` appelle aujourd'hui
  `scroll_response(model, navigation)` pour toute touche de navigation,
  y compris `Left`/`Right` — que `scroll_response` ignore déjà
  (`_ => scroll`, aucun effet). Aucune collision à lever pour leur
  donner un sens ici.
- `open_filter` (`src/app/update.rs`) et `clear_detail_view_state`
  sont les deux points qui réinitialisent déjà l'état d'affichage de la
  réponse (filtre ouvert, sélection changée) : ce sont les points
  naturels pour y ajouter la réinitialisation de l'onglet actif.

## Goals / Non-Goals

**Goals:**
- Le corps de la réponse visible sans défiler au-delà des en-têtes.
- Aucun changement à `response-filter` ni `search-and-yank` : ils
  continuent d'opérer sur « ce qui est affiché », qui devient
  naturellement le contenu de l'onglet actif.
- Aucun décalage de ligne codé en dur à introduire (leçon retenue de
  `cursor_position_in_detail` côté Détail) : la position de la barre
  d'onglets et du contenu reste dérivée dynamiquement de
  `response_text`.

**Non-Goals:**
- Aucune persistance de l'onglet actif par requête : revenir sur une
  requête déjà consultée réaffiche toujours l'onglet Corps (cohérent
  avec la réinitialisation déjà systématique du défilement de la
  réponse au changement de sélection).
- Aucun changement à `layout()` ni aux proportions de colonnes.
- Aucun changement au panneau Détail.

## Decisions

### D1 — Trois fonctions de contenu, un bandeau, une barre d'onglets
`result_lines` se découpe en quatre fonctions pures :
- `status_band(outcome) -> Vec<Line>` : `section("Résultat")` + Verdict
  + Statut (+ Erreur) + Temps de réponse (identique à l'existant, sans
  les en-têtes).
- `body_tab_lines(outcome, filter) -> Vec<Line>` : la ligne de filtre et
  son résultat/erreur, ou le corps brut — identique à l'existant, mais
  **sans** le `section("Corps de réponse")` qui l'introduisait : la
  barre d'onglets (D2) porte désormais ce rôle, le répéter serait
  redondant.
- `headers_tab_lines(outcome) -> Vec<Line>` : les en-têtes de réponse,
  identique à l'existant, sans le `section("En-têtes de réponse")` pour
  la même raison.
- `tests_tab_lines(outcome) -> Vec<Line>` : les quatre blocs
  `push_checks` existants, inchangés — leurs titres restent
  nécessaires puisqu'un seul onglet Tests regroupe quatre catégories.

`response_text(model)` assemble : `status_band`, une ligne vide, la
barre d'onglets (D2), une ligne vide, puis la fonction de contenu
correspondant à `model.response_tab`.

Alternative écartée : garder les titres de section existants en plus de
la barre d'onglets — rejetée, doublon visuel immédiat entre le nom de
l'onglet actif et un titre de section identique juste en dessous.

### D2 — Barre d'onglets : une `Line` stylée, pas un nouveau widget
Une fonction `tab_bar(active: ResponseTab) -> Line<'static>` construit
une ligne unique avec les trois libellés (« Corps », « En-têtes »,
« Tests ») séparés par deux espaces, l'onglet actif en `theme::SECTION`
(déjà Bold + souligné + vert, utilisé ailleurs pour un titre en
valeur), les deux autres en `theme::LABEL` (Dim). Aucune nouvelle
constante de `theme.rs`, aucun nouveau widget `ratatui` : reste un
`Text`/`Line` rendu par le même `Paragraph` que le reste du panneau
Réponse, cohérent avec la contrainte déjà posée par `visual-theme`
(rester dans `Style`/`Color`/`Modifier`).

Alternative écartée : un widget `Tabs` dédié de `ratatui` — rejetée,
`render_response` construit aujourd'hui un seul `Paragraph` à partir
d'un `Text` ; ajouter un widget séparé demanderait de recalculer une
sous-zone dans `areas.response` pour un gain visuel marginal par
rapport à une ligne stylée.

### D3 — État : un enum `ResponseTab`, pas trois jeux de défilement
`ResponseTab { Body, Headers, Tests }` (`Body` par défaut, `Copy`,
`PartialEq`), un seul nouveau champ `Model::response_tab`. Changer
d'onglet réinitialise `response_scroll`, `response_match` et
`response_selection` à leur valeur par défaut (comme un changement de
nœud sélectionné) plutôt que de garder un défilement par onglet :
plus simple, et cohérent avec le Non-Goal « pas de persistance par
onglet ». `previous_response_tab`/`next_response_tab`
(`src/app/update.rs`) appliquent `ResponseTab::previous`/`next` (cycle
circulaire à trois valeurs, définies sur l'enum dans `model.rs`) puis
cette réinitialisation.

### D4 — Bascule d'onglet : interception de `Left`/`Right` dans la branche `Focus::Response`
```
Focus::Response => match navigation {
    Message::Left => previous_response_tab(model),
    Message::Right => next_response_tab(model),
    other => scroll_response(model, other),
},
```
Aucun changement à `message.rs` : `h`/`←` et `l`/`→` produisent déjà
`Message::Left`/`Message::Right` sans condition de focus ; le filtrage
« hors saisie » est déjà assuré par `capture_message` (voir Context).

### D5 — Réinitialisation : deux points d'écriture, pas de nouveau mécanisme
`clear_detail_view_state` (déjà appelée à chaque changement de
sélection) gagne `model.response_tab = ResponseTab::Body;`.
`open_filter` gagne la même ligne, ajoutée juste après ses gardes
existantes (nœud requête, résultat exploitable, réponse HTTP, corps non
nul) — inutile de tester si l'onglet Corps est déjà actif, l'affecter
inconditionnellement à `Body` est un no-op dans ce cas.

### D6 — Corps par défaut mis en forme par le même moteur jq que le filtre
Retour utilisateur (Nicolas) une fois `response-tabs` implémenté une
première fois : le corps brut (sans filtre) restait sérialisé sur une
seule ligne compacte (`Value::to_string()` de `serde_json`), illisible
pour un corps un peu long — alors que `response-filter` produit déjà,
pour un filtre validé, une sortie indentée lisible via le moteur jq
(`jaq`) déjà intégré (`src/app/filter.rs`, sans I/O). `filter::evaluate`
gagne une fonction sœur `pub fn pretty_print(data: &Value) -> String`,
qui réutilise la même configuration de mise en forme
(`jaq_json::write::Pp`, indentation de deux espaces) mais sans passer
par le chargement/la compilation d'un filtre jq : elle convertit
directement la valeur et appelle le même écrivain (`jaq_json::write::
write`). `detail::body_lines` (utilisée par l'onglet Corps quand aucun
filtre n'est actif) l'appelle pour tout corps objet ou tableau ; une
chaîne continue d'être affichée telle quelle (comportement déjà en
place, à préserver : un corps HTML ou texte brut ne doit pas se
retrouver entre guillemets JSON avec ses retours à la ligne échappés).

Alternative écartée : appeler `filter::evaluate(".", data)` (le filtre
identité) plutôt qu'une fonction dédiée — rejetée, ça ferait passer
tout affichage par défaut par le chargeur et le compilateur jq à chaque
rendu pour un résultat déterministe et sans branchement conditionnel
réel, un coût et une indirection inutiles face à un appel direct à
l'écrivain déjà utilisé par `evaluate` elle-même.

## Risks / Trade-offs

- [Supprimer les titres de section « Corps de réponse » et « En-têtes
  de réponse » change le texte affiché, alors que les changements
  précédents (`improve-visual-design`,
  `split-request-response-panels`) avaient pris soin de ne jamais
  changer le contenu textuel] → changement délibéré et justifié : la
  barre d'onglets introduite par ce changement assume désormais ce
  rôle, contrairement aux changements précédents qui ne touchaient que
  la présentation sans rien ajouter au contenu. Les tests existants qui
  cherchent ce texte (`filter_applied_replaces_body_with_formatted_
  result_and_filter_line` notamment) seront mis à jour pour chercher le
  libellé d'onglet à la place.
- [Réinitialiser `response_match`/`response_selection` à chaque
  changement d'onglet peut surprendre un utilisateur qui cherchait un
  motif présent dans plusieurs onglets] → cohérent avec la
  réinitialisation déjà systématique au changement de nœud sélectionné ;
  documenté dans la exigence « Changement d'onglet au clavier ».

## Migration Plan

Aucune migration : changement d'état en mémoire et de rendu uniquement.
Les tests de contenu de la réponse qui cherchaient un texte maintenant
porté par la barre d'onglets (« Corps de réponse », « En-têtes de
réponse ») seront mis à jour pour chercher le libellé d'onglet ou le
contenu de l'onglet actif à la place ; aucun autre test de contenu
textuel ne devrait changer.
