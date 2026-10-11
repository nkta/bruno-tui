# bruno-tui

TUI en Rust pour explorer et éditer des collections [Bruno](https://www.usebruno.com/)
(fichiers `.bru`, ou OpenCollection YAML en lecture seule) dans le terminal. L'exécution d'une requête est toujours
déléguée au vrai CLI `bru` : ce projet n'interprète ni les variables, ni les
scripts, ni les tests — seulement l'affichage et l'édition des fichiers.

## Lancer l'application

```
bruno-tui [OPTIONS] [CHEMIN]
```

`CHEMIN` : une collection, ou un fichier/dossier qu'elle contient (par
défaut, le répertoire courant).

Options :

| Option | Effet |
|---|---|
| `--secret NOM[=CLÉ]` | variable secrète transmise à `bru` (répétable). Cherchée dans le `.env` de la collection puis le shell, sous `CLÉ` ou `NOM`/`NOM_EN_MAJUSCULES` ; une saisie dans le panneau Secrets prime. Jamais de valeur en clair sur la ligne de commande. |
| `--no-mouse` | n'active pas la capture souris (la sélection native du terminal reste disponible) |
| `--no-watch` | désactive la surveillance et le rechargement automatique de la collection |
| `-h`, `--help` | aide |
| `-V`, `--version` | version |

## Formats de collection

Le format est détecté comme dans Bruno 4, sur le premier répertoire
parent qui marque une collection :

| Fichier à la racine | Format | Édition |
|---|---|---|
| `opencollection.yml` | OpenCollection YAML (`*.yml`, `folder.yml`, `environments/*.yml`) | lecture seule |
| `bruno.json` (sans `opencollection.yml`) | bru-lang (`*.bru`) | oui |

Si les deux fichiers sont présents, `opencollection.yml` l'emporte.

Pour une collection YAML :

- l'arbre, le détail et les environnements s'affichent comme pour une
  collection `.bru` ; le titre indique « OpenCollection YAML · lecture
  seule » ;
- l'exécution reste déléguée à `bru run` (chemins `.yml` relatifs à la
  racine) et les résultats du rapport sont rattachés aux requêtes ;
- l'édition est désactivée : `Entrée`/`e` dans le détail, `e` dans le
  panneau Environnement et `Ctrl+S` affichent « Collection OpenCollection
  (YAML) en lecture seule » ;
- seules les requêtes `http` et `graphql` sont affichées ; les autres
  types (`grpc`, `websocket`, ...) et les fichiers invalides apparaissent
  en erreur dans l'arbre ;
- les clés que le TUI ne sait pas afficher sont ignorées et listées comme
  avis dans le panneau Diagnostics (`D`), sans jamais citer de valeur ;
- le nom d'un environnement est son nom de fichier sans extension, celui
  qu'attend `bru run --env`.

## Vue d'ensemble

L'écran est divisé en panneaux : **Collection** (arbre), **Détail** (requête
sélectionnée), **Environnement** (toujours visible), **Statut** et
**Réponse**. `Tab` fait tourner le focus entre Collection, Détail et
Réponse ; `Échap` y ramène directement depuis n'importe où. `q` quitte
(Collection/Détail), `Ctrl+C` quitte en toutes circonstances — une session
d'édition non enregistrée demande confirmation.

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | naviguer |
| `→`/`l`, `←`/`h` | déplier/replier un dossier (Collection) |
| `Début`/`g`, `Fin`/`G` | début, fin de la liste |
| `PageUp`/`PageDown` | page précédente/suivante |
| `r` | lancer la requête (ou récursivement un dossier) ; `Ctrl+X` annule |
| `f` | filtrer l'arbre de la collection (en direct ; Entrée valide, Échap annule ou efface) |
| `Ctrl+K` | recherche globale par nom, méthode, chemin et URL (popup centré en direct ; ↑↓ choisir, Entrée valide, Échap ferme) |
| `M` | activer/désactiver la capture souris |
| `D` | panneau Diagnostics (erreurs de chargement, avis sur les champs YAML ignorés) |
| `H` | panneau Historique des exécutions |
| `S` | panneau Variables secrètes |
| `?` | aide des raccourcis (popup centré) |
| `C` | panneau Campagne (bilan TNR de la dernière exécution récursive) |
| `O` | options de campagne (tags, délai, bail, tests-only, export de rapports JUnit/HTML) |
| `]` / `[` | requête en échec suivante / précédente (Collection, Détail, Réponse) |
| `z` | basculer le mode plein écran du panneau focalisé (Collection, Détail, Réponse) |

La racine de la collection est surveillée en continu : dès qu'un fichier
`.bru` ou `.yml` (OpenCollection), `bruno.json`, `collection.bru`, `folder.bru` ou un fichier du dossier
`environments/` est créé, modifié, supprimé ou renommé, la collection est
automatiquement rechargée en préservant la sélection (par chemin), les dossiers
dépliés, le focus et l'environnement actif (l'option `--no-watch` permet de
désactiver cette surveillance). Si une session d'édition comporte des
modifications non enregistrées, le rechargement est différé après sa fermeture
et un avertissement signale la modification sur disque.

## Panneau Détail

Affiche la requête sélectionnée selon dix onglets, comme Bruno bureau :
**Params**, **Corps**, **En-têtes**, **Auth**, **Vars**, **Script**,
**Assert**, **Tests**, **Docs** et **Settings**. Les onglets vides sont
atténués ; une barre d'onglets adaptative fait défiler les onglets sur les
écrans étroits.

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler dans le détail |
| `Début`/`g`, `Fin`/`G` | début, fin du détail |
| `←`/`h`, `→`/`l` | onglet précédent/suivant de la requête (hors édition) |
| `Entrée` / `e` | ouvrir une session d'édition |
| `/` | chercher dans l'onglet affiché ; `n`/`N` : occurrence suivante/précédente |
| `v` | sélection visuelle de lignes ; `y` copie dans le presse-papiers |
| `Clic` | cliquer sur un onglet pour l'afficher |

Les onglets Vars, Docs et Settings sont en lecture seule : pendant une
session d'édition, ils ne sont pas des cibles éditables et la navigation
entre champs reste inchangée.

## Édition d'une requête

Dans le panneau Détail, `Entrée` (ou `e`) ouvre une session d'édition de la
requête sélectionnée. Rien n'est écrit sur disque avant `Ctrl+S`.

Sélection de champ (URL, méthode, en-têtes, paramètres de requête et de
chemin, corps) :

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | champ précédent, suivant |
| `Entrée` | modifier la valeur ; sur la méthode, ouvre un sélecteur (`↑↓` choisir, `Entrée` valider, `Échap` annuler) ; sur une ligne « + Ajouter », ajoute une entrée |
| `Espace` | activer ou désactiver l'en-tête ou le paramètre |
| `a` | ajouter une entrée dans la section du champ (clé, puis valeur) |
| `d` | supprimer l'en-tête ou le paramètre, sans confirmation |
| `c` | renommer la clé de l'en-tête ou du paramètre |
| `Ctrl+S` | enregistrer |
| `Échap` | fermer la session (confirmation si modifiée) |

Saisie : `Entrée` ou `Tab` valide (`Entrée` insère un saut de ligne dans le
corps), `Échap` annule, `Ctrl+S` valide puis enregistre ; pendant la saisie
de la clé d'une nouvelle entrée, `Ctrl+S` l'ajoute avec une valeur vide.
Toute lettre tapée en saisie est du texte.

Comme dans Bruno, les paramètres de requête et l'URL restent cohérents :
modifier un paramètre reconstruit la chaîne de requête de l'URL à partir
des paramètres activés, et modifier l'URL recalcule les paramètres activés
(les paramètres désactivés restent à leur place). Une clé vide, contenant
un espace ou `:`, ou commençant par `~` ou `"` est refusée ; dans un
paramètre de requête, `&`, `#` et le saut de ligne sont refusés (ainsi que
`=` dans la clé).

À la souris (sauf `--no-mouse`) : cliquer sur l'URL, un en-tête ou un
paramètre démarre directement sa saisie (ouvre une session si besoin) ;
cliquer ailleurs valide d'abord la saisie en cours. Glisser dans le
détail ou la réponse sélectionne des lignes (`v` bascule aussi une
sélection au clavier), `y` copie la sélection dans le presse-papiers.

## Panneau Environnement

Toujours visible, en haut à droite (au-dessus du panneau Statut) — comme
les autres panneaux, il n'a besoin d'aucune touche pour apparaître.

| Touche | Effet |
|---|---|
| `E` | donner le focus au panneau |
| `↑`/`k`, `↓`/`j` | naviguer dans la liste des environnements |
| `Entrée`/`→` | activer l'environnement sous le curseur (« Aucun » désactive) |
| `e` | ouvrir le popup d'édition de ses variables |

Un double-clic sur une entrée du panneau fait les deux à la fois : active
l'environnement et ouvre le popup.

Dans le popup (tableau Clé/Valeur, centré) :

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | variable précédente, suivante |
| `Entrée` | modifier la valeur (une variable désactivée — `~` en tête de clé dans le fichier — n'est pas éditable) |
| `a` | ajouter une variable (clé, puis valeur) |
| `d` | supprimer la variable sous le curseur, sans confirmation |
| `Ctrl+S` | enregistrer |
| `Échap` | annuler la saisie en cours, sinon fermer le popup |

Un clic sur une ligne démarre directement sa saisie, comme pour un
en-tête de requête. Seules les valeurs sont éditées ici : les variables
secrètes (`vars:secret`) n'apparaissent jamais dans ce popup, elles
restent gérées par le panneau Secrets (`S`).

Environnements Bruno 4 (`extends`, `color`) : la liste affiche le ou les
parents à côté du nom (`staging ← base`) et une pastille `■` de la couleur
déclarée, si c'est une couleur hexadécimale (`#rgb` ou `#rrggbb`). Le TUI
ne résout jamais l'héritage : le popup ne montre que les variables du
fichier lui-même, et c'est `bru` qui fusionne les parents à l'exécution.
Enregistrer depuis le popup conserve `extends` et `color` tels quels.
Attention : `extends` demande `bru` 4.2 ou plus récent ; `bru` 4.1.0 refuse
l'environnement (erreur de lecture).

## Panneau Réponse

Affiche le résultat de la dernière exécution de la requête sélectionnée,
en quatre onglets (Corps, En-têtes, Tests, Timeline). Le corps JSON (brut ou issu d'un
filtre `jq`) bénéficie d'une coloration syntaxique (clés, chaînes, nombres,
booléens, null, ponctuation), tandis qu'un corps textuel ou HTML reste
affiché sans coloration. L'onglet Timeline récapitule la requête envoyée
(méthode, URL résolue, en-têtes, corps) et la réponse reçue (statut, durée,
URL finale si redirigée).

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler |
| `Début`/`g`, `Fin`/`G` | début, fin |
| `←`/`→` (ou `Entrée`) | onglet précédent/suivant |
| `/` | chercher dans l'onglet courant ; `n`/`N` : occurrence suivante/précédente |
| `\|` | filtrer le corps avec une expression `jq` |
| `v` | sélection visuelle ; `y` copie |
| `c` | copier en commande `curl` (en-têtes d'auth masqués) |
| `u` | copier en commande `curl` complète (confirmation si des secrets sont présents) |
| `Ctrl+E` | ouvrir le corps brut de la réponse (non filtré) dans `$VISUAL`, `$EDITOR`, ou `nano` — consultation seule, rien n'est relu |

`Ctrl+E` fonctionne quel que soit le focus courant, tant qu'une requête
sélectionnée a un résultat avec un corps non vide.

## Autres panneaux

- **Diagnostics** (`D`) : erreurs de chargement de la collection ; `→`
  saute au nœud concerné dans l'arbre.
- **Historique** (`H`) : exécutions passées ; `r` relance l'entrée
  sélectionnée.
- **Secrets** (`S`) : variables `vars:secret` de l'environnement actif et
  celles demandées par `--secret` ; `a` ajoute un nom, `Entrée` saisit sa
  valeur, `d` l'oublie. Une valeur secrète n'est jamais écrite sur disque
  ni affichée en clair.
- **Campagne** (`C`) : bilan de la dernière exécution récursive (campagne de
  TNR) ; en-tête avec cible, options actives, taux de succès, durée, filtre
  actif et compteurs (`Tous`, `Échecs`, `Réussis`, `Ignorés`) ; liste des
  requêtes filtrée (par défaut les `Échecs`) avec code HTTP, statut ou raison
  d'échec ; `f` fait tourner le filtre (`Échecs` / `Réussis` / `Ignorés` / `Tous`) ;
  `Entrée` ou `→` sélectionne la requête dans l'arbre et donne le focus au Détail ;
  `R` relance uniquement les requêtes en échec avec les options courantes ;
  `r` relance l'ensemble de la campagne.
- **Options du runner** (`O`) : popup de configuration des options d'exécution
  récursive mémorisées en session :
  - **Tags inclus** (`--tags`) et **tags exclus** (`--exclude-tags`) sous forme
    de liste séparée par des virgules (espaces interdits).
  - **Délai** inter-requêtes en millisecondes (`--delay`).
  - **Bail** (`--bail`) : arrêt dès le premier échec.
  - **Tests-only** (`--tests-only`) : exécution des requêtes comportant des tests uniquement.
  - **Exports de rapports** JUnit XML (`--reporter-junit`) et HTML (`--reporter-html`)
    délégués à `bru` : écrits hors collection (`$XDG_STATE_HOME/bruno-tui/reports/` ou
    `~/.local/state/bruno-tui/reports/`, repli vers `./reports`), horodatés avec le nom
    du dossier cible. Le masquage des en-têtes sensibles (`--reporter-skip-all-headers`)
    est activé par défaut et peut être basculé.
  - `Entrée` édite ou bascule un champ, `Ctrl+R` ou `Ctrl+Enter` lance directement
    la campagne avec les options configurées, `Échap` enregistre et referme le popup.
- **Recherche globale** (`Ctrl+K`) : popup centré de recherche en direct dans l'ensemble
  des requêtes de la collection chargée (nom affiché, méthode HTTP, chemin relatif et URL brute
  telle qu'écrite avec ses variables non résolues). `↑`/`↓` choisit dans la liste ordonnée
  par pertinence (correspondance mise en évidence, URL tronquée proprement), `Entrée` sélectionne
  la requête dans l'arbre en dépliant automatiquement ses dossiers parents et donne le focus
  au panneau Détail, `Échap` referme.

Aucun de ces panneaux ne prend le clic pour l'instant — seul le
clavier y navigue.

## Aide des raccourcis

À tout moment hors saisie de texte, la touche `?` ouvre un popup centré
rappelant l'ensemble des raccourcis clavier de l'application, organisés par
section : Global, Collection, Détail, Édition d'une requête, Réponse,
Environnement (panneau et popup), Diagnostics / Historique / Secrets, et
Campagne (TNR).

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler dans l'aide |
| `Début`/`g`, `Fin`/`G` | début, fin de l'aide |
| `?`, `Échap`, `q` | fermer le popup d'aide |

Toutes les autres touches sont ignorées tant que le popup est ouvert, à
l'exception de `Ctrl+C` qui quitte l'application. À la souris, cliquer en
dehors du popup le referme également.

