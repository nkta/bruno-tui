# Spécification — Plusieurs collections (workspace)

Statut : **approuvée par Nicolas le 2026-10-13** (plan validé).
Décisions de Nicolas du 2026-10-13 :

1. Suivre Bruno desktop : un workspace, toutes ses collections dans l'arbre.
2. Plusieurs collections dépliées en même temps ; la collection **active**
   est celle de la ligne sélectionnée.
3. Accepter aussi `bruno-tui A B C` (workspace sans nom, sans fichier).
4. `bruno-tui <collection>` reste identique à aujourd'hui.

Aucune écriture nouvelle : `workspace.yml` est lu, jamais écrit.

## 1. Comportement de Bruno (référence)

Relevé dans Bruno desktop 4.2.1 (`app.asar`) :

- un workspace est un dossier qui contient `workspace.yml` ;
- format (celui des tests de Bruno) :

```yaml
opencollection: 1.0.0
info:
  name: Test
  type: workspace
collections:
  - name: Clients
    path: ../clients
  - name: Factures
    path: /chemin/absolu/factures
    remote: https://exemple/depot.git
specs: []
docs: ''
```

- `path` est relatif au dossier du workspace, ou absolu ;
- la barre latérale affiche toutes les collections, chacune dépliable ;
- chaque collection garde ses environnements ; le workspace a aussi un
  `.env` et des mocks (hors périmètre ici).

## 2. Lancement

| Commande | Résultat |
|---|---|
| `bruno-tui <dossier avec workspace.yml>` | mode workspace, nom = `info.name` |
| `bruno-tui A B C` | workspace sans nom ; chaque chemin doit être une collection |
| `bruno-tui <collection>` ou `bruno-tui` | une collection, comme aujourd'hui |

- `workspace.yml` : seuls `info.name` et `collections[].name` / `path`
  sont lus. Les autres clés sont ignorées sans erreur.
- Un chemin absent ou qui n'est pas une collection donne une ligne
  d'erreur dans l'arbre ; les autres collections se chargent.
- Fichier illisible ou YAML invalide : erreur de chargement claire.

## 3. Arbre

- Une ligne d'en-tête par collection (`▸ Clients`), dépliable avec
  `→` / `←` comme un dossier. Repliée au démarrage, sauf s'il n'y en a qu'une.
- Le titre du panneau montre le nom du workspace (`Workspace : Test`),
  ou « Collections » pour un workspace sans nom.
- Le nom affiché est `collections[].name`, sinon le nom de la collection.

## 4. Collection active

- C'est celle de la ligne sélectionnée (en-tête compris).
- Elle détermine : le panneau Environnement, les Secrets, l'Historique,
  la Campagne, les Diagnostics, la recherche `Ctrl+K`, le filtre `f`,
  la touche `c` (sélection de `collection.bru`) et les campagnes `R`.
- Le titre du panneau Environnement rappelle son nom.
- Chaque collection garde son propre état : environnement choisi, secrets
  saisis, derniers résultats, historique, dossiers dépliés.

## 5. Exécution

- `bru run` part de la racine de la collection active, comme aujourd'hui.
- Avec un `workspace.yml`, `--workspace-path` vaut par défaut le dossier
  du workspace (modifiable dans les options du runner `O`).
- Pendant une exécution, la sélection ne peut pas quitter la collection :
  message dans le statut, comme pour une session d'édition.

## 6. Surveillance

- Un watcher par collection ; un rechargement ne touche que la sienne.

## 7. Hors périmètre

- Écrire `workspace.yml` (ajouter, retirer, réordonner une collection).
- `.env`, environnements globaux et mocks du workspace (au-delà de
  `--workspace-path` transmis à `bru`).
- Recherche, filtre et campagne sur plusieurs collections à la fois.

## 8. Tests et critères d'acceptation

1. Fixture `tests/fixtures/workspaces/demo/workspace.yml` au format du § 1,
   avec un chemin relatif vers une collection de `tests/fixtures/collections/`,
   un chemin absolu (réécrit dans une copie temporaire au test) et un chemin invalide.
2. Tests : lecture de `workspace.yml` ; CLI (1 chemin, workspace, plusieurs
   chemins) ; changement de collection active avec environnement et secrets
   conservés ; refus pendant une exécution ; rechargement de la bonne
   collection ; `bru run` lancé dans la bonne racine.
3. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
4. Essai réel en release, puis `bru run`.
