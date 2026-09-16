# Collection de démonstration : JSONPlaceholder

Collection Bruno réelle contre [JSONPlaceholder](https://jsonplaceholder.typicode.com),
une API REST publique, gratuite et sans authentification, faite pour
tester des clients HTTP. Sert à essayer `bruno-tui` en conditions réelles,
au-delà des fixtures manuscrites de `tests/fixtures/`.

## Contenu

- **Posts** (6 requêtes) : lister, lire, commentaires (`params:query`),
  créer, modifier, supprimer. Le serveur accepte les écritures sans
  jamais les persister.
- **Users** (3 requêtes) : lister, lire, albums d'un utilisateur (via
  `params:path`, `/users/:id/albums`).
- **Cas d'échec** (2 requêtes), volontairement en échec :
  - un article inexistant (`/posts/99999`) : la requête réussit, le
    serveur répond 404, et l'assertion `res.status: eq 200` échoue —
    échec d'assertion sur une réponse HTTP obtenue ;
  - un hôte local injoignable : aucune réponse reçue.

## Limite connue : aucune résolution d'environnement

Toutes les URL utilisent `{{base_url}}`, défini dans
`environments/public.bru`. **La capacité `add-request-run` actuelle ne
transmet aucun environnement à `bru`** (`RunRequest.env` reste toujours
`None` — hors périmètre explicite, renvoyé à un futur
`add-environment-picker`). Lancer une requête avec `r` depuis
l'interface échoue donc aujourd'hui avec `getaddrinfo ENOTFOUND
{{base_url}}` : la variable n'est jamais résolue.

Vérifié le 2026-09-16 : le mécanisme d'exécution lui-même (pipe du
rapport JSON, affichage du résultat, historique, diagnostics) fonctionne
correctement une fois cette limite contournée par la ligne de commande :

```bash
cd examples/jsonplaceholder
bru run -r --env public
```

Ceci exécute réellement les 11 requêtes contre l'API : 9 réussissent, les
2 requêtes du dossier « Cas d'échec » échouent comme prévu.

## `bru` sur cette machine

`bru` n'était pas installé lors de la rédaction de cette collection. Un
raccourci a été posé dans `~/.local/bin/bru`, pointant vers une
installation locale d'`@usebruno/cli` dans `~/.local/lib/bru-cli/`
(environ 265 Mo). Pour le retirer :

```bash
rm ~/.local/bin/bru
rm -rf ~/.local/lib/bru-cli
```

**Ne pas utiliser `npx @usebruno/cli` comme raccourci** : `npx` ne
propage pas le descripteur de fichier 3 utilisé par `--reporter-json
/dev/fd/3`, ce qui casse l'échange du rapport avec `bruno-tui` (constaté
en pratique : code de sortie 255, `EACCES: permission denied, open
'/dev/fd/3'`). Une installation directe du binaire est nécessaire.
