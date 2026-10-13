# Fixtures de workspaces

`demo/workspace.yml` reprend le format de `workspace.yml` relevé dans les
tests de Bruno desktop 4.2.1 (`opencollection`, `info.name`, `info.type:
workspace`, `collections[].name/path/remote`, `specs`, `docs`).

- `Runner probe` et `Multi-env` : chemins relatifs vers
  `tests/fixtures/collections/`.
- `Secrets` : chemin absolu fictif. Les tests le réécrivent dans une copie
  temporaire (`/BRUNO_TUI_FIXTURES` → dossier réel des fixtures) ; tel quel,
  il donne une ligne d'erreur.
- `Absente` : chemin invalide, ligne d'erreur dans l'arbre.
