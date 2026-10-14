# Workspace de démonstration

Regroupe les deux collections d'exemple :

- `../jsonplaceholder` (JSONPlaceholder) ;
- `../httpbin` (httpbin).

## Essayer

```
bruno-tui examples/workspace
```

1. `→` déplie une collection.
2. `E` choisit l'environnement `public` de la collection active.
3. `r` lance la requête sélectionnée.

Chaque collection garde son propre environnement : passer de l'une à
l'autre change l'environnement affiché.

## Pour ses propres collections

Copier `workspace.yml` dans un dossier, et lister ses collections dans
`collections` (`path` relatif à ce dossier, ou absolu).
