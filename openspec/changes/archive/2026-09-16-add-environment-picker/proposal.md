## Why

Un essai manuel de `bruno-tui` contre une vraie collection (voir
`examples/jsonplaceholder/`, une API publique) a montré que la touche `r`
échoue systématiquement dès qu'une requête utilise une variable
d'environnement (`{{base_url}}`) : `RunRequest.env` reste toujours `None`
dans `add-request-run`, exclu explicitement de son périmètre à l'époque.
La même collection tourne parfaitement en ligne de commande avec
`bru run --env public`. C'est un blocage réel pour l'usage normal de
l'outil, pas un confort : la quasi-totalité des collections Bruno
réelles s'appuient sur un environnement pour leur URL de base.

## What Changes

- Nouvelle touche `E` (hors saisie) qui ouvre un panneau listant les
  environnements de la collection chargée (`Collection.environments`,
  déjà exposés en lecture seule par `bru-parser`, jamais consommés
  jusqu'ici) ainsi qu'une entrée « Aucun ».
- Navigation `↑`/`j`, `↓`/`k` dans la liste, `Entrée` valide le choix et
  ferme le panneau, `Échap` ferme sans changer la sélection courante.
- L'environnement choisi devient l'environnement courant du modèle,
  affiché en permanence (barre de titre ou barre d'état) tant qu'une
  collection est chargée.
- `r` (lancement d'une requête, d'un dossier, ou rejeu depuis
  l'historique) transmet désormais le nom de l'environnement courant à
  `RunRequest.env`, au lieu de toujours envoyer `None`.
- Un fichier d'environnement invalide (`Err(ErrorNode)` dans
  `Collection.environments`) apparaît dans la liste marqué en erreur,
  non sélectionnable.
- Recharger la collection réinitialise l'environnement courant à
  « Aucun » : un nom d'environnement n'a de sens que pour la collection
  qui l'a déclaré.

Hors périmètre, à ne pas implémenter ici :
- Édition des fichiers d'environnement (valeurs, secrets) : hors
  périmètre de `bru-writer`.
- Surcharge de variables individuelles (`env_vars` de `RunRequest`) :
  déjà exclue par `add-request-run`, reste exclue ici.
- Résolution ou affichage des valeurs de variables interpolées :
  `bru-parser` ne résout rien, ce changement ne change pas ça.
- Environnement par défaut persistant entre deux lancements de
  `bruno-tui` (fichier de configuration) : à traiter plus tard si le
  besoin se confirme.

## Capabilities

### New Capabilities
- `environment-picker`: sélection d'un environnement parmi ceux de la
  collection chargée, affichage permanent de l'environnement courant, et
  transmission de son nom à `bru-runner` au lancement d'une exécution.

### Modified Capabilities
<!-- Aucune. `tui-shell` définit déjà le patron des panneaux
     additionnels (Diagnostics, Historique) que celui-ci suit sans
     changer leur contrat ; `request-execution` n'est pas modifié dans
     son comportement observable, seul le champ déjà prévu `env` de
     `RunRequest` cesse d'être systématiquement vide. -->

## Impact

- Code : `src/app/model.rs` (état de la liste, environnement courant,
  variante de `Focus`), `src/app/message.rs` (touche `E`, messages de
  navigation de la liste), `src/app/update.rs` (ouverture/fermeture,
  sélection, transmission à `RunRequest.env`, réinitialisation au
  rechargement), `src/app/view/` (rendu du panneau et de l'indicateur
  permanent). `src/collection/`, `src/runner/`, `src/writer/` inchangés :
  ce changement consomme des types déjà exposés en lecture seule.
- Dépendances : aucune nouvelle crate.
- Tests : unitaires sur `update` (ouverture, navigation, sélection,
  transmission à `RunRequest.env`, réinitialisation au rechargement,
  environnement invalide non sélectionnable), rendu `TestBackend` pour le
  panneau et l'indicateur permanent. `tests/fixtures/collections/
  parser-cases/environments/` n'a aujourd'hui qu'un seul environnement
  (`local`) : ce changement lui ajoute un second environnement valide et
  un fichier d'environnement malformé, pour tester la navigation entre
  plusieurs entrées et l'affichage d'une entrée en erreur, sans jamais
  charger une collection réelle du disque dans les tests automatisés
  (`examples/jsonplaceholder/`, créée pour un essai manuel, reste hors
  des tests).
