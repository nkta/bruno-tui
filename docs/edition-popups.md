# Spécification — Édition par popups

Statut : **approuvée par Nicolas le 2026-10-13**.
Décisions de Nicolas du 2026-10-13 :

1. L'édition dans le panneau de détail est remplacée par des popups :
   « plus propre et compréhensible ».
2. Un popup par section (onglet), pas un popup par ligne.
3. L'écriture reste au `Ctrl+S`.
4. Toute l'édition passe par les popups ; l'ancien mode disparaît.
5. Choix par défaut acceptés : `m` pour le popup Requête, `Ctrl+S`
   enregistre puis ferme, texte du corps édité dans le popup.

Ce changement ne touche que l'interface. Ce que l'on peut modifier, et la
façon d'écrire les fichiers `.bru`, restent ceux de `docs/edition.md` et
`docs/edition-phase2.md` (écriture chirurgicale, fraîcheur, aucune
interprétation, validation par `@usebruno/lang`). Aucun nouveau droit
d'écriture n'est demandé.

## 1. Principe

- Le panneau de détail redevient en lecture seule : plus de curseur de
  champ, plus de lignes « + Ajouter », plus de « ● non enregistré ».
- `Entrée` (ou `e`) dans le détail ouvre le popup de l'**onglet affiché**.
- `m` ouvre le popup **Requête** (méthode et URL), depuis n'importe quel
  onglet.
- Le popup est centré, par-dessus l'écran, comme le popup d'environnement.
- Ses modifications restent dans le popup jusqu'au `Ctrl+S`.
- Il édite un seul fichier : la requête, le `folder.bru` ou le
  `collection.bru` sélectionné.

## 2. Touches communes à tous les popups

| Touche | Effet |
|---|---|
| `↑` `↓` | ligne précédente, suivante |
| `Tab`, `Maj+Tab` | partie suivante, précédente (ex. pré-requête / post-réponse) |
| `Entrée` | modifier la valeur de la ligne, ou ouvrir la liste d'un choix |
| `←` `→` | sur un choix (mode, méthode, placement) : valeur précédente, suivante |
| `Ctrl+S` | enregistrer, puis fermer le popup |
| `Échap` | annuler la saisie en cours ; sinon fermer le popup |

- Fermer avec des changements non enregistrés demande :
  « Abandonner les modifications ? (y/n) ».
- La dernière ligne du popup rappelle les touches utiles à l'endroit du
  curseur.
- Un clic sur une ligne la sélectionne ; un clic hors du popup ne fait rien.

### Tableaux clé / valeur

En-têtes, paramètres, vars, assert, formulaires du corps :

| Touche | Effet |
|---|---|
| `Entrée` | modifier la valeur |
| `c` | renommer la clé |
| `a` | ajouter une ligne (clé, puis valeur) |
| `d` | supprimer la ligne, sans confirmation |
| `Espace` | activer / désactiver la ligne |

Une ligne désactivée s'affiche grisée, avec « (désactivé) ».

## 3. Les popups

| Popup | Contenu |
|---|---|
| Requête (`m`) | Méthode ‹ GET › ; URL |
| Params | tableau Paramètres de requête ; tableau Paramètres de chemin |
| En-têtes | tableau |
| Auth | Mode ‹ bearer › ; puis les paramètres du mode |
| Corps | Mode ‹ json › ; puis le contenu du mode |
| Vars | tableau Pré-requête ; tableau Post-réponse |
| Assert | tableau (clé = expression, valeur = `eq 200`) |
| Settings | valeurs seulement |
| Script | Pré-requête ; Post-réponse |
| Tests, Docs | une ligne |

Dossier et collection : seuls les popups de leurs onglets existent
(pas de Requête, pas de Params, pas de Corps). Requêtes gRPC, WebSocket et
collections YAML : `Entrée` affiche « lecture seule » dans le statut,
comme aujourd'hui.

### 3.1 Popup Auth

```
╭ Authentification — get-post.bru ──────────╮
│ Mode       ‹ bearer ›                      │
│ token      ••••••••••••                    │
│                                            │
│ ←→ mode  Entrée modifier  Ctrl+S  Échap    │
╰────────────────────────────────────────────╯
```

- Changer de mode remplace aussitôt les lignes de paramètres par celles du
  nouveau mode (clés vides).
- Revenir au mode d'origine avant `Ctrl+S` retrouve les anciennes valeurs.
- Au `Ctrl+S`, si l'ancien bloc avait des valeurs : confirmation
  « L'ancien bloc auth:basic sera supprimé. Continuer ? (y/n) ».
- Secrets : inchangé (`docs/edition-phase2.md` § 5). Saisie masquée,
  `Ctrl+R` pour l'afficher. Un secret en clair ouvre la question dans le
  bas du popup : `Entrée` remplacer par `{{bearerToken}}`, `g` garder,
  `Échap` revenir à la saisie.
- `apikey.placement` est un choix ‹ header › / ‹ queryparams ›.
- OAuth 1, OAuth 2, Akamai : valeurs des clés existantes seulement.

### 3.2 Popup Corps

- Mode ‹ json › : `none`, `json`, `text`, `xml`, `sparql`,
  `formUrlEncoded`, `multipartForm`, `file`. GraphQL : mode fixe.
- Texte (`json`, `text`, `xml`, `sparql`, `graphql`) : zone multiligne
  dans le popup. `Entrée` passe à la ligne ; `Tab` sort de la zone.
- GraphQL : deux parties, Requête et Variables.
- Formulaires : tableau ; champs `@file(...)` : `Espace` et `d` seulement.
- `file` : lecture seule.

### 3.3 Popups Script, Tests, Docs

- `Entrée` sur une ligne ouvre l'éditeur externe (`--editor`, `VISUAL`,
  `EDITOR`, `nano`), comme aujourd'hui.
- À la fermeture de l'éditeur, on revient au popup, qui montre un aperçu
  des premières lignes ; `Ctrl+S` enregistre.

## 4. Barre de statut et aide

- Hors popup, l'aide du détail affiche `Entrée éditer  m requête`.
- `?` (aide) et `README.md` décrivent les popups ; les anciennes touches
  d'édition dans le détail sont retirées de l'aide.

## 5. Réalisation

- Le moteur actuel est gardé : `EditSession`, ses modifications en
  attente, le writer et ses tests ne changent pas.
- Une session est désormais limitée aux champs d'un popup, et s'affiche
  dans le popup au lieu du détail.
- Le rendu inline des champs dans le détail est retiré.

## 6. Tests et critères d'acceptation

1. Pour chaque popup : ouverture depuis son onglet, modification, `Ctrl+S`,
   fichier attendu (tests d'octets existants réutilisés).
2. `Échap` avec et sans changement ; confirmation y/n.
3. Auth : changement de mode, retour au mode d'origine, confirmation,
   secret en clair (3 issues), masquage.
4. Dossier et collection : un cas chacun.
5. Rendu : tests de vue sur une largeur de 80 et de 120 colonnes.
6. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`,
   `writer_bruno_lang` avec `BRUNO_LANG_DIR`.
7. Essai réel dans le TUI, en release, sur une copie de
   `examples/jsonplaceholder`, puis `bru run`.
