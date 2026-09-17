## Context

Voir `proposal.md` pour le pourquoi. État du code vérifié avant
rédaction :

- `view::layout` (`src/app/view/mod.rs`) découpe le corps en trois
  colonnes pleine hauteur : arbre (35 %, min 24), détail (50 % du
  reste, min 18), réponse (reste, min 18). `MIN_WIDTH = 60`,
  `MIN_HEIGHT = 10`, soit une hauteur de corps de 8 lignes au minimum.
  `Areas.status` désigne déjà la **barre d'état** du bas d'écran.
- `update.rs` calcule défilement maximal et page de la réponse à partir
  de `layout_for(model.size)` et `inner(areas.response).height`
  (`response_max_scroll`, page suivante, recherche). Si
  `areas.response` devient le rectangle réduit sous le panneau Statut,
  ces calculs restent justes sans modification.
- `detail::status_band` construit le bandeau actuel : `section
  ("Résultat")`, `Verdict`, `Statut`, `Erreur` (optionnelle), `Temps de
  réponse`. `response_text` le fait suivre d'une ligne vide, de la barre
  d'onglets, d'une ligne vide, puis du contenu de l'onglet. Recherche et
  copie de la réponse (`search-and-yank`) indexent `response_text`
  dynamiquement, sans décalage codé en dur.
- `view::run_status` dérive déjà le marqueur de l'arbre :
  `Running` si `active.target == request.path`, sinon
  `is_failure()` → `Failure`, sinon `Success`.
- `ResponseInfo` (`src/runner/report.rs`) porte déjà `status`
  (`Http(u16)`, `Error`, `Skipped`, `Other`), `status_text`
  (`Option<String>`), `headers` (`Option<BTreeMap<String, Value>>`) et
  `response_time` (ms). Aucun champ de taille : `bru` 2.13.2 ne la
  rapporte pas (vérifié dans `tests/fixtures/reports/mixed.json`), mais
  les réponses du serveur de fixture portent `content-length` (en
  minuscules, valeur chaîne `"21"`, `"32"`). `statusText` vaut `"OK"`,
  `null` (erreur) ou `"request skipped via pre-request script"`.
- `theme.rs` : `SUCCESS` vert, `FAILURE` rouge, `RUNNING` bleu,
  `LOAD_ERROR` magenta, `METHOD`/`SECTION` vert clair
  `Rgb(120,200,150)`, `FOCUS` orange `Rgb(224,138,60)`, `BORDER` vert
  `Rgb(90,150,110)`, fond `Rgb(18,22,40)`.

## Goals / Non-Goals

**Goals:**
- Statut lisible d'un coup d'œil sans toucher au `Model`, aux messages
  ni à `update` : panneau calculé purement dans `view`.
- Pas de redondance : chaque information (verdict, statut, temps) n'est
  affichée qu'à un seul endroit.
- Le minimum 60×10 reste utilisable ; aucune panique à toute taille.

**Non-Goals:**
- Chiffres géants en art ASCII (voir D2).
- Afficher la taille calculée depuis le corps, ou un libellé HTTP
  standard déduit du code (voir D5, D4).
- Rendre le panneau focalisable, cliquable ou porteur d'actions.
- Modifier le modèle de rapport ou ajouter une fixture : aucun champ
  nouveau n'est désérialisé.

## Decisions

### D1. Placement : en tête de la colonne Réponse

La colonne Réponse est coupée verticalement : panneau « Statut » en
haut (hauteur fixe), panneau Réponse dessous. L'arbre et le détail
gardent toute la hauteur.

Alternatives écartées :
- *Bande pleine largeur sous le titre* : lisible, mais vole des lignes
  à l'arbre et au détail, qui n'ont rien à voir avec le résultat, et
  éloigne le statut du corps qu'il qualifie.
- *Enrichir la ligne de titre ou la barre d'état* : une seule ligne,
  déjà occupée (nom de collection, badge d'erreurs, environnement ;
  touches, saisie de recherche/filtre, messages d'exécution) ; pas de
  place pour un badge lisible.
- *Quatrième colonne* : impossible sous le minimum de 60 colonnes
  (24 + 18 + 18 déjà consommés).
- *Garder le bandeau dans la réponse mais le styliser* : le bandeau
  défile avec le contenu et disparaît dès qu'on descend dans le corps ;
  un panneau séparé reste visible.

Implémentation : `Areas` gagne `response_status: Rect` (nom choisi pour
ne pas confondre avec `Areas.status`, la barre d'état) ; `Areas.response`
devient le rectangle sous ce panneau. `layout` reste la seule source
des rectangles, donc `update.rs` suit automatiquement.

### D2. « En grand » = badge coloré, pas chiffres géants

Le code est rendu en badge : ` 200 ` en gras, fond = couleur de classe,
texte = couleur du fond d'application (contraste maximal sur les deux
teintes). Les chiffres géants (3 à 5 lignes, ~4 colonnes par chiffre)
ne tiennent pas dans la forme compacte, ne sont pas copiables et
n'ont pas d'équivalent pour « aucune réponse ». Le badge donne le
contraste recherché sur une ligne.

### D3. Hauteur et forme compacte

- Terminal ≥ 20 lignes : panneau de 5 lignes (3 intérieures).
  1. badge + libellé ; 2. `14 ms · 32 o` ; 3. `✓ réussi · 4/4`
     (`✗ échec · 2/4`, ou « aucune vérification »).
- Terminal < 20 lignes : panneau de 3 lignes, une ligne intérieure
  `badge ✓ 14 ms`, tronquée à droite.

À 60×10 : corps 8 lignes → Statut 3 + Réponse 5 (3 intérieures) ; à
60×19 : 3 + 14 ; à 100×20 : 5 + 13. Seuil fixé sur la hauteur du
terminal (pas du corps) pour rester simple à tester via
`layout_for((w, h))`. Constantes `STATUS_PANEL_HEIGHT = 5`,
`STATUS_PANEL_COMPACT_HEIGHT = 3`, `STATUS_PANEL_FULL_MIN_TERMINAL_HEIGHT
= 20` dans `view/mod.rs`.

### D4. Contenu dérivé, jamais inventé

Nouveau module `src/app/view/status.rs`, fonction pure
`status_panel_lines(model, compact) -> Vec<Line<'static>>`, découpée en
aides testables :
- `StatusClass` : `Success | Redirect | ClientError | Failure | Neutral`,
  obtenu par `fn classify(&ResponseStatus) -> StatusClass` (200–299,
  300–399, 400–499, 500–599 / `Error`, tout le reste neutre, y compris
  1xx et `Other`).
- Badge : texte `code` / « aucune réponse » / « ignorée » / valeur brute.
- Libellé : `status_text` filtré sur non vide. Pas de table
  code → phrase : ce serait de l'information que `bru` n'a pas rapportée.
- Décompte : `Pass` comptés réussis ; total = toutes les vérifications
  dont le statut n'est pas `Skipped` ; les statuts `Other` comptent dans
  le total sans être réussis.
- Verdict : `RequestResult::is_failure()`, comme `run_status` — même
  source que le marqueur de l'arbre, donc même verdict garanti.

### D5. Taille = en-tête `content-length` seulement

`bru` ne rapporte pas de taille. Re-sérialiser `response.data` donnerait
une taille fausse (JSON déjà parsé et réordonné, encodage perdu,
décompression). On lit donc l'en-tête `content-length` (recherche
insensible à la casse, valeur chaîne ou nombre JSON, entier `u64`
positif ou nul), sinon rien. Formatage : `< 1024` → `N o` ; sinon `Ko`
puis `Mo`, base 1024, une décimale, virgule décimale française
(`1,5 Ko`). Seul `content-length` est lu parmi les en-têtes : aucune
autre valeur ne peut fuiter dans ce panneau.

### D6. Exécution en cours

`fn run_concerns(active: &ActiveRun, path: &Path) -> bool` :
`active.target == path`, ou `active.recursive &&
path.starts_with(&active.target)` (comparaison par composants de
`Path`, donc `folder2/x` ne correspond pas à `folder`). Plus large que
`run_status` de l'arbre, qui ne marque que la cible exacte : l'arbre
signale *ce qui a été lancé*, le panneau signale *que ce qui est affiché
va changer*. `run_status` n'est pas modifié (hors périmètre).

Affichage : badge « en cours » style `RUNNING` en fond, puis, si un
résultat précédent existe, en `LABEL` (atténué) et sans verdict pour ne
pas laisser croire qu'il est à jour : `précédent :` puis `200 · 14 ms`
sur les deux lignes suivantes en forme complète (une seule ligne
tronquait dès 18 à 33 colonnes), ou `200 · 14 ms` à la suite du badge en
forme compacte.

### D7. Couleurs : deux nouvelles catégories

- `theme::REDIRECT` : cyan `Color::Cyan` — distinct du bleu `RUNNING` et
  du vert clair `METHOD`.
- `theme::CLIENT_ERROR` : jaune `Color::Yellow` — distinct de l'orange
  `FOCUS` (teinte plus claire, et le focus est une bordure, jamais un
  fond de badge).
- 2xx réutilise `SUCCESS`, 5xx et absence de réponse `FAILURE` :
  c'est le même sens (« ça a marché » / « ça n'a pas marché ») et cela
  limite la palette. La spec `visual-theme` rattache explicitement ces
  classes aux catégories existantes pour que l'exigence « deux
  catégories ne partagent pas une couleur » reste vraie.
- `theme::status_badge(class) -> Style` construit le style de badge
  (`bg` = couleur de catégorie, `fg` = couleur de `BACKGROUND`, gras) ;
  `Neutral` → `LABEL` sans fond.
- Le test `categories_that_could_be_confused_have_distinct_colors` est
  étendu : `REDIRECT` ≠ `RUNNING`, `METHOD`, `SUCCESS` ; `CLIENT_ERROR`
  ≠ `FOCUS`, `FAILURE`, `LOAD_ERROR`.

### D8. Bandeau du panneau Réponse réduit à l'erreur

`status_band` ne produit plus que la ligne `Erreur` quand elle existe
(suivie d'une ligne vide), rien sinon ; la section « Résultat »
disparaît. `response_text` enchaîne ensuite barre d'onglets, ligne vide,
contenu. L'erreur reste dans la réponse et non dans le panneau Statut :
elle peut faire plusieurs lignes (`Missing required environment
variables: …`) et doit pouvoir être recherchée et copiée.

## Risks / Trade-offs

- [Le panneau Réponse perd 3 à 5 lignes] → le bandeau perd de son côté
  4 lignes (section + verdict + statut + temps) : le contenu visible des
  onglets est quasiment inchangé.
- [Recherche/copie ne trouvent plus « Verdict » ni le code] → assumé
  (proposal, Impact) ; les tests existants qui cherchent `Verdict` dans
  l'écran ou dans `response_text` sont réécrits pour viser le panneau
  Statut.
- [`visual-theme` « Non-régression du contenu affiché » interdisait
  toute information nouvelle] → exigence modifiée dans ce changement :
  elle borne les règles de présentation du thème, et autorise
  explicitement une capacité (ici `status-panel`) à déplacer ou ajouter
  de l'information qu'elle spécifie.
- [Jaune peu lisible sur certains thèmes de terminal] → badge sur fond
  sombre imposé (`BACKGROUND` appliqué partout), texte du badge foncé.
- [`content-length` absent en `chunked` ou HTTP/2 compressé] → pas de
  taille affichée, conforme à « si disponible ».

## Points de contact avec les changements parallèles

- **`add-mouse-support`** : si la souris mappe les clics sur les
  rectangles de `Areas`, elle doit connaître `Areas.response_status`
  (non focalisable : clic sans effet, ou focus Réponse — à décider par
  ce changement) et le fait que `Areas.response` ne commence plus en
  haut du corps. La molette sur la réponse doit viser le nouveau
  rectangle. Le premier des deux à fusionner impose la forme de
  `Areas` ; l'autre rebase.
- **`improve-direct-editing`** : pas de conflit fonctionnel (panneau
  Détail et touches d'édition). Conflit textuel possible dans
  `view/mod.rs` (`view`, barre d'état) et dans les tests d'écran qui
  lisent des lignes à des positions fixes (`render(...)[29]`).
- **`add-entry-management`** : renommer ou supprimer une requête change
  ou supprime son chemin, clé de `RunState::outcomes`. Le panneau Statut
  lit la même clé que le panneau Réponse : s'il n'y a pas de migration
  des résultats, il affichera `—`, cohérent avec « aucun résultat ».
  Toute boîte de saisie ou de confirmation affichée en superposition
  doit rester lisible au-dessus de la colonne Réponse redécoupée.
- Les trois changements touchent potentiellement `theme.rs` : les
  nouvelles constantes `REDIRECT`/`CLIENT_ERROR` doivent rester
  distinctes de toute couleur ajoutée ailleurs (ex. surbrillance de
  survol souris, marqueur d'édition).

## Migration Plan

Aucune donnée persistée. Déploiement par fusion de branche ; retour
arrière par revert du commit, sans effet sur les collections.
