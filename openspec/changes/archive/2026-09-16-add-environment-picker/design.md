## Context

Voir `proposal.md` pour le pourquoi. Éléments déjà en place dans le code,
vérifiés avant d'écrire ce document :

- `Collection.environments: Environments` où
  `pub type Environments = Vec<Result<Environment, ErrorNode>>;`
  (`src/collection/tree.rs`), triés par nom de fichier.
- `Environment { name, path, variables, secret_names, ast }`
  (`src/collection/view.rs`) — `name` est déjà le nom sans extension,
  celui attendu par `bru run --env`.
- `ErrorNode { path, error: ParseError }` pour une entrée invalide.
- `RunRequest { collection_root, targets, recursive, env: Option<String>,
  env_vars }` (`src/runner/request.rs`) — `env` existe déjà, toujours
  construit à `None` dans `run_selected()` (`src/app/update.rs`).
- `Focus { Tree, Detail, Diagnostics, History }` (`src/app/model.rs`) et
  le patron déjà posé par `Message::ToggleDiagnostics` /
  `Message::ToggleHistory` (`src/app/update.rs`, lignes ~141-156) :
  bascule vers un focus dédié si une collection est chargée, un second
  appui à la même touche revient à `Focus::Tree`.
- `TextCapture { Search, Insert, Filter }` (`src/app/message.rs`) — la
  liste d'environnements n'a pas besoin de saisie de texte, donc ce
  changement n'y touche pas.
- Touche `E` libre (`e` minuscule est déjà `StartEdit`).

## Goals / Non-Goals

**Goals:**
- Un focus dédié `Focus::EnvironmentPicker`, ouvert/fermé par `E` en
  suivant exactement le patron de `Diagnostics`/`History`.
- Un état minimal dans `Model` : environnement courant (`Option<String>`)
  et index de sélection dans la liste pendant que le panneau est ouvert.
- `run_selected()` lit l'environnement courant du modèle au lieu de
  toujours passer `None`.

**Non-Goals:**
- Aucune saisie de texte : pas d'extension de `TextCapture`.
- Aucune persistance entre lancements de `bruno-tui` (voir proposal.md).
- Aucune modification de `src/collection/`, `src/runner/`, `src/writer/`.

## Decisions

### Focus dédié plutôt qu'un état de panneau séparé
`Focus::EnvironmentPicker` rejoint `Diagnostics`/`History` dans l'enum
existant. Alternative rejetée : un booléen `env_picker_open` séparé du
`Focus`, comme les anciens designs avant l'introduction de `Focus` —
rejeté parce que `Focus` porte déjà l'exclusion mutuelle entre panneaux
plein-écran et que dupliquer ce mécanisme casserait la cohérence déjà
établie (un seul panneau plein-écran actif à la fois).

### Représentation de la liste dans `Model`
```rust
/// Environnement courant, `None` = « Aucun ». Nom de fichier sans
/// extension, celui attendu par `RunRequest.env`.
pub current_environment: Option<String>,
/// Indice sélectionné dans le panneau ; 0 = « Aucun », puis un indice
/// par entrée de `Collection.environments` dans l'ordre déjà trié.
pub environment_selected: usize,
```
Pas de copie de `Collection.environments` dans `Model` : la vue lit
`model.collection` directement au rendu, comme le fait déjà le panneau
de diagnostics pour ses propres données. `environment_selected` n'a de
sens que pendant que `Focus::EnvironmentPicker` est actif ; il est
réinitialisé à l'indice de l'entrée courante à chaque ouverture (pour que
rouvrir le panneau retrouve la sélection en cours), pas nécessairement à
0. Alternative rejetée : stocker l'indice sélectionné par nom
(`Option<String>`) plutôt que par position — rejeté par cohérence avec
`diagnostics_selected`/`history_selected`, déjà des `usize`.

### Résolution de l'indice vers un nom
Une fonction pure `environment_name_at(collection: &Collection, index:
usize) -> Option<Option<&str>>` traduit un indice de liste (0 = Aucun,
1..=N = les entrées de `environments` dans l'ordre) vers soit `None`
(entrée invalide dans la liste), soit `Some(None)` (« Aucun » choisi),
soit `Some(Some(name))`. `Entrée` sur un indice qui retombe sur
`Err(ErrorNode)` ne change rien (cf. spec, exigence « Environnement
invalide non sélectionnable ») : `environment_name_at` renvoie `None`
dans ce cas précis et le message ignore l'action.

### Un seul nouveau message : `ToggleEnvironmentPicker`
En codant, il s'avère que `Diagnostics`/`History` n'ont pas de messages
dédiés pour naviguer : la table de touches (`message.rs`) produit déjà
des messages génériques `Up`/`Down`/`Home`/`End`/`Right`/`FocusTree`
(`↑↓`/`jk`, `Début`/`Fin`, `→`/`l`/`Entrée`, `Échap`), et un unique point
de dispatch dans `update` (la branche `navigation => match model.focus`)
les envoie vers `navigate_diagnostics`/`navigate_history` selon le focus
courant. Ajouter des messages dédiés
(`EnvironmentPickerUp`/`Down`/`SelectEnvironment`/
`CancelEnvironmentPicker`, envisagés dans une première version de ce
document avant d'inspecter ce point précis du dispatch) aurait dupliqué
ce mécanisme déjà en place. La seule touche réellement nouvelle est `E`
(hors saisie, comme `D`/`H`), qui bascule `Focus::EnvironmentPicker`.
Navigation et validation passent par une nouvelle fonction
`navigate_environment_picker`, ajoutée comme branche de la même
dispatch : `Up`/`Down`/`Home`/`End` déplacent `environment_selected`,
`Right` valide l'entrée courante (appelle `environment_name_at`), et
`Échap` (`FocusTree`) referme déjà vers `Focus::Tree` sans toucher à
`current_environment`, par construction de ce message générique — aucun
changement à `Message::FocusTree` n'est nécessaire.

### `run_selected()` et le rejeu d'historique
`run_selected()` construit déjà une seule `RunRequest` pour les trois cas
(requête, dossier, rejeu). Un seul point de lecture,
`model.current_environment.clone()`, alimente `RunRequest.env` pour les
trois — pas de branchement par cas, l'environnement courant est une
propriété de session, pas de l'entrée d'historique rejouée.

### Réinitialisation au rechargement
`Message::CollectionLoaded` remet déjà à zéro plusieurs champs de
sélection (lignes ~110-121 de `update.rs`). Ce changement y ajoute
`model.current_environment = None;` et, si `model.focus` valait
`EnvironmentPicker`, le remet à `Focus::Tree` — même traitement que
l'arbre et le détail à ce même endroit.

## Risks / Trade-offs

- [Un nom d'environnement passé à `bru --env` ne correspond plus à un
  fichier existant si l'utilisateur modifie la collection sur disque
  hors de `bruno-tui` entre la sélection et le lancement] → accepté : le
  parser ne surveille pas le disque en continu (déjà vrai pour tout le
  reste du modèle), `bru` renverra son erreur habituelle, affichée comme
  n'importe quel échec de lancement existant.
- [Ajouter une variante à `Focus` touche le filtrage exhaustif existant
  dans `view/mod.rs` et `update.rs`] → mitigation : suivre les sites où
  `Diagnostics`/`History` apparaissent déjà (repérés dans le code avant
  d'écrire ce design) plutôt que d'ajouter un `_ =>` catch-all qui
  masquerait un oubli.

## Migration Plan

Aucune migration : nouvel état par défaut à `None`/`Focus::Tree`, aucun
format sur disque changé. Rétro-compatible avec toute collection déjà
chargée par les versions précédentes.
