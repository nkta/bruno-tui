## Why

Le statut de la réponse de la requête sélectionnée tient aujourd'hui
dans trois lignes de texte du bandeau du panneau Réponse
(`response-tabs`) : « Verdict : échec », « Statut : 404 »,
« Temps de réponse : 14 ms », en couleur de libellé ordinaire. En
exploration comme en campagne de TNR, c'est l'information que l'œil
cherche en premier, et elle ne ressort pas : un 500 ressemble à un 200,
le libellé HTTP et la taille ne sont pas affichés, et le nombre de tests
en échec oblige à ouvrir l'onglet Tests.

## What Changes

- Nouveau **panneau Statut**, non focalisable, placé en haut de la
  colonne Réponse (au-dessus du panneau Réponse, même largeur), qui
  affiche pour la requête sélectionnée :
  - le code HTTP en badge mis en valeur (gras, fond coloré) selon sa
    classe : 2xx, 3xx, 4xx, 5xx, requête en erreur sans réponse,
    requête ignorée ;
  - le libellé rapporté par `bru` (`statusText`), s'il existe ;
  - le temps de réponse ;
  - la taille du corps quand `bru` la rapporte (en-tête
    `content-length` de la réponse), rien sinon ;
  - le verdict (réussi / échec, mêmes couleurs que le marqueur de
    l'arbre) et le décompte des vérifications réussies sur le total.
- Comportements hors résultat : sélection sans résultat → panneau
  neutre ; exécution en cours concernant la requête sélectionnée →
  indicateur « en cours » à la couleur d'exécution en cours, avec le
  rappel atténué du résultat précédent s'il existe.
- Forme compacte (une seule ligne intérieure) sur un terminal de faible
  hauteur, pour que le minimum 60×10 de `tui-shell` reste utilisable.
- **Retrait de la redondance** dans le bandeau du panneau Réponse : le
  titre « Résultat », le verdict, le statut et le temps de réponse
  quittent ce bandeau ; seule la ligne « Erreur » (message de `bru`,
  potentiellement long) y reste, au-dessus de la barre d'onglets.
- Nouvelles catégories de couleur dans le thème : réponse de
  redirection (3xx) et réponse d'erreur client (4xx) ; 2xx réutilise la
  couleur de succès, 5xx et absence de réponse celle d'échec.

## Capabilities

### New Capabilities
- `status-panel` : panneau Statut de la requête sélectionnée — contenu,
  classes de statut, états sans résultat et en cours, forme compacte.

### Modified Capabilities
- `tui-shell` : la disposition place le panneau Statut au-dessus du
  panneau Réponse ; le panneau Réponse n'est plus seul à porter le
  verdict, le statut et le temps de réponse.
- `response-tabs` : le bandeau toujours visible ne contient plus que
  le message d'erreur rapporté par `bru` ; verdict, statut et temps de
  réponse sont portés par `status-panel`.
- `visual-theme` : ajout des catégories de couleur des classes de
  statut HTTP et de leur correspondance avec les catégories existantes ;
  l'exigence de non-régression du contenu est recentrée sur les règles
  de présentation, pour permettre à `status-panel` de déplacer et
  d'ajouter de l'information.

## Impact

- Code : `src/app/view/mod.rs` (`Areas`, `layout`, `view`),
  nouveau module `src/app/view/status.rs`, `src/app/view/detail.rs`
  (`status_band` réduit), `src/app/view/theme.rs` (nouvelles
  constantes). Aucun changement de `Model`, `Message`, `update` ni du
  modèle de rapport (`src/runner/report.rs`) : tout est dérivé de
  `RequestOutcome` et de `RunState::active`.
- Recherche et copie (`search-and-yank`) dans la réponse : le verdict,
  le statut et le temps de réponse ne font plus partie du texte
  recherchable/copiable du panneau Réponse.
- Aucune nouvelle dépendance, aucune écriture disque, aucun nouveau
  type désérialisé (fixtures de rapport existantes suffisantes).
- Points de contact avec `add-mouse-support`, `improve-direct-editing`
  et `add-entry-management` : voir `design.md`.
