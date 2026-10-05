# bruno-tui

TUI en Rust pour explorer et éditer des collections [Bruno](https://www.usebruno.com/)
(fichiers `.bru`) dans le terminal. L'exécution d'une requête est toujours
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
| `-h`, `--help` | aide |
| `-V`, `--version` | version |

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
| `M` | activer/désactiver la capture souris |
| `D` | panneau Diagnostics (erreurs de chargement) |
| `H` | panneau Historique des exécutions |
| `S` | panneau Variables secrètes |
| `C` | panneau Campagne (bilan TNR de la dernière exécution récursive) |
| `]` / `[` | requête en échec suivante / précédente (Collection, Détail, Réponse) |

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

## Panneau Réponse

Affiche le résultat de la dernière exécution de la requête sélectionnée,
en trois onglets (Corps, En-têtes, Tests).

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | défiler |
| `Début`/`g`, `Fin`/`G` | début, fin |
| `←`/`→` (ou `Entrée`) | onglet précédent/suivant |
| `/` | chercher dans l'onglet courant ; `n`/`N` : occurrence suivante/précédente |
| `\|` | filtrer le corps avec une expression `jq` |
| `v` | sélection visuelle ; `y` copie |
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
  TNR) ; en-tête avec cible, taux de succès et durée, puis liste des requêtes
  en échec avec code HTTP et raison d'échec ; `Entrée` ou `→` sélectionne
  la requête dans l'arbre et donne le focus au Détail.

Aucun de ces panneaux ne prend le clic pour l'instant — seul le
clavier y navigue.
