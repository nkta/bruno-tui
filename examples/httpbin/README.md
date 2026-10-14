# Collection de démonstration : httpbin

Petite collection Bruno contre [httpbin](https://httpbin.org), un service
public qui renvoie en écho ce qu'il reçoit. Elle sert surtout à essayer
les workspaces avec `examples/workspace`.

## Contenu

- **Écho GET** : paramètre de requête `user`, relu dans `args`.
- **Écho POST** : corps JSON, relu dans `json`.
- **En-têtes reçus** : vérifie l'en-tête `Accept` de la collection.

Environnement `public` : `base_url` et `user`. Vérifiée avec
`bru run --env public` (bru 4.2.1) : 3 requêtes, 6 assertions.
