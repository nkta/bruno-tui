# bruno-tui
TUI for bruno

## Édition d'une requête

Dans le panneau de détail, `Entrée` (ou `e`) ouvre une session d'édition
de la requête sélectionnée. Rien n'est écrit sur disque avant `Ctrl+S`.

Sélection de champ :

| Touche | Effet |
|---|---|
| `↑`/`k`, `↓`/`j` | champ précédent, suivant |
| `Entrée` | modifier la valeur ; sur une ligne « + Ajouter », ajouter une entrée |
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
