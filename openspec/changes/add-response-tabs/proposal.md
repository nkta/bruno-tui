## Why

Depuis `split-request-response-panels`, le panneau Réponse affiche tout
le résultat d'exécution en un seul flux défilable : verdict, statut,
temps de réponse, en-têtes de réponse, corps, puis assertions et tests.
Le corps de la réponse — l'information la plus consultée — est enterré
après les en-têtes, qui n'intéressent l'utilisateur que ponctuellement.
Nicolas : « le résultat doit être le plus visible » et souhaite de vrais
onglets pour cette raison, comme sur l'image de référence
(`exempleTui/`), plutôt qu'un simple réordonnancement.

## What Changes

- Le panneau Réponse se divise en un bandeau toujours visible (verdict,
  statut, temps de réponse) et trois onglets en dessous : **Corps**
  (par défaut), **En-têtes**, **Tests** (assertions, tests, tests
  pré-requête, tests post-réponse).
- Nouvelles touches, actives seulement quand la réponse a le focus,
  hors saisie : `h`/`←` onglet précédent, `l`/`→` onglet suivant, cycle
  circulaire. Ces touches sont aujourd'hui sans effet dans ce panneau
  (vérifié dans le code), donc sans collision.
- Changer de nœud sélectionné dans l'arbre revient à l'onglet Corps.
  Ouvrir la saisie du filtre jq (`response-filter`) bascule aussi sur
  l'onglet Corps, s'il n'est pas déjà actif — la partie qu'il modifie.
- Aucun changement à `response-filter` ni `search-and-yank` : le
  filtrage et la recherche/sélection/copie portent déjà sur « ce qui
  est affiché » dans le panneau, qui devient naturellement le contenu
  de l'onglet actif une fois celui-ci introduit.

Hors périmètre : tout ce qui reste hors périmètre depuis
`split-request-response-panels` (barre de recherche en haut, sélecteur
d'environnement en en-tête, fil d'ariane, onglets au niveau de la
fenêtre entière `Response`/`Tests`/`Documentation` vus dans l'image —
ici il s'agit d'onglets *à l'intérieur* du panneau Réponse seulement).
Pas de persistance de l'onglet actif au-delà du nœud sélectionné.

## Capabilities

### New Capabilities
- `response-tabs` : organisation du contenu du panneau Réponse en
  onglets navigables (Corps, En-têtes, Tests), avec un bandeau de
  statut toujours visible au-dessus, pour que le corps de la réponse
  reste immédiatement visible sans défiler au-delà des en-têtes ou des
  tests.

### Modified Capabilities
Aucune : `tui-shell` (« Panneau de réponse »), `response-filter` et
`search-and-yank` restent vrais tels quels — ils portent déjà sur « ce
qui est affiché » dans le panneau, qui devient le contenu de l'onglet
actif sans changer leur texte normatif.

## Impact

- Code : `src/app/model.rs` (nouvel enum `ResponseTab`, nouveau champ
  `Model::response_tab`), `src/app/update.rs` (bascule d'onglet sur
  `Left`/`Right` quand `Focus::Response`, réinitialisation à `Body`
  dans `clear_detail_view_state` et à l'ouverture du filtre),
  `src/app/view/detail.rs` (`response_text` séparé en un bandeau
  toujours construit et un contenu dépendant de l'onglet actif),
  `src/app/view/mod.rs` (affichage des libellés d'onglets dans
  `render_response`).
- Dépendances : aucune nouvelle crate.
- Tests : nouveaux tests sur le cycle d'onglets, la réinitialisation à
  la sélection et à l'ouverture du filtre, et le contenu affiché par
  onglet. Les tests existants de `response-filter` et de recherche/
  sélection/copie dans la réponse doivent continuer de passer (ils
  opèrent sur `green.bru`/`json.bru`, dont le corps est dans l'onglet
  Corps, actif par défaut).
