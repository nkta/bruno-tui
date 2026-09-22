## Why

`environment-editing` permet de consulter et modifier la valeur d'une
variable déjà présente, mais pas d'en ajouter ni d'en retirer — un choix
de périmètre explicite au départ. L'utilisateur demande maintenant cette
capacité : « il faut être capable d'ajouter ou retirer une ligne aussi ».
`field-editing` a déjà résolu exactement ce problème pour les en-têtes
et paramètres de requête (`a` ajoute en deux temps clé puis valeur, `d`
supprime sans confirmation) — ce changement reproduit la même
interaction dans le popup d'édition d'environnement, sur le même bloc de
données (une liste de `KeyValue`), sans réinventer la mécanique.

## What Changes

- Dans le popup d'édition, `a` sur n'importe quelle ligne (variable ou
  ligne d'ajout) commence l'ajout d'une nouvelle variable : saisie de la
  clé (`Entrée` ou `Tab` valide et passe à la valeur, `Échap` abandonne
  tout), puis saisie de la valeur (`Entrée` ou `Tab` valide, valeur vide
  acceptée, `Échap` abandonne tout). La nouvelle variable apparaît
  activée, en fin de tableau, curseur dessus, session marquée modifiée —
  même séquence que l'ajout d'un en-tête de requête.
- `d` sur une variable la supprime immédiatement, sans confirmation,
  session marquée modifiée ; le curseur reste au même indice, borné à la
  dernière position.
- Mêmes règles de clé invalide que pour une requête (clé vide, espace,
  `:`, `~`/`"` en tête) — refus immédiat, même type d'erreur
  (`EntryProblem`, déjà générique, réutilisé tel quel).
- `Ctrl+S` écrit désormais aussi les ajouts et suppressions dans le bloc
  `vars` du fichier d'environnement, avec les mêmes garanties qu'une
  modification de valeur (atomique, refus si le fichier a changé sur
  disque).
- Hors périmètre, comme précédemment : renommer une clé (`c`),
  activer/désactiver une variable (`Espace`) — non demandés, restent un
  changement distinct si besoin.

## Capabilities

### Modified Capabilities

- `environment-editing` : le popup d'édition gagne l'ajout (`a`) et la
  suppression (`d`) d'une variable, avec sauvegarde sur disque.

## Impact

- `src/writer/environment.rs` : `EnvironmentVarEdit` passe d'une
  structure (valeur seule) à une énumération (`Value`/`Add`/`Remove`).
  La résolution des tranches à réécrire pour un ajout ou une suppression
  (ré-émission du bloc `vars` avec l'entrée en plus ou en moins, création
  du bloc s'il n'existe pas encore) réutilise la même mécanique que
  `src/writer/draft.rs` pour les sections de requête — voir `design.md`
  (D1) pour la décision de factoriser plutôt que dupliquer cette
  mécanique.
- `src/app/model.rs` : `EnvironmentEditState` gagne deux états
  (saisie de la clé, saisie de la valeur) pour l'ajout en deux temps ;
  `EnvironmentEditSession` gagne les méthodes de transition
  correspondantes, sur le modèle de `EditSession`/`InputTarget` déjà
  utilisé pour les requêtes.
- `src/app/update.rs` : `a` et `d` routés vers le popup d'édition
  d'environnement (`Focus::EnvironmentPicker` avec une session ouverte) ;
  validation immédiate des clés invalides.
- `src/app/view/panels.rs` : `render_environment_edit_popup` affiche une
  ligne provisoire (clé/valeur en cours de saisie) pendant l'ajout, en
  fin de tableau.
- Aucune nouvelle dépendance.
