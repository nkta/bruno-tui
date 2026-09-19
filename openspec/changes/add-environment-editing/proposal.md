## Why

Le panneau Environnement (`E`) n'affiche aujourd'hui que la liste des noms
d'environnements (`environments/*.bru`) pour choisir lequel est actif —
`render_environment_picker` dans `src/app/view/panels.rs`. Les variables
qu'un environnement contient (`Environment.variables : Vec<KeyValue>`,
issues du bloc `vars`) ne sont visibles nulle part dans l'application : il
faut sortir de `bruno-tui` et éditer le fichier `.bru` à la main pour
changer la valeur d'une variable (par exemple `host` entre `local` et
`staging`). L'utilisateur a demandé un panneau environnement éditable.

## What Changes

- Dans le panneau Environnement, `Entrée` sur un environnement valide
  (pas « Aucun », pas une entrée en erreur — ces deux cas restent sans
  effet, comme aujourd'hui pour `Droite`) affiche désormais ses variables
  à la place de la liste des environnements : une ligne par entrée de
  `Environment.variables`, valeur et état activé/désactivé compris (une
  entrée désactivée reste visible, non éditable dans sa valeur, avec le
  même style que les en-têtes désactivés).
- Dans cette vue, `Entrée`/`e` sur une variable commence la saisie de sa
  **valeur** en texte libre (comportement clavier identique à l'édition
  d'un en-tête existant) ; `Entrée` valide, `Échap` annule la saisie sans
  modifier la variable.
- `Échap` depuis la liste des variables (aucune saisie en cours) revient
  à la liste des environnements.
- `s` / `Ctrl+S` écrit sur disque uniquement les valeurs modifiées, avec
  la même garantie qu'aujourd'hui pour une requête : écriture atomique,
  refus explicite si le fichier a changé sur disque depuis son chargement
  (même mécanisme que `src/writer` pour les requêtes, appliqué ici au
  bloc `vars` du fichier d'environnement).
- `Droite` pour activer un environnement (`select_environment_picker`)
  n'est pas modifié : `Entrée` était sans effet dans la liste
  aujourd'hui (`navigate_environment_picker` ne traite pas `Message::Enter`),
  donc ce changement ne retire aucun raccourci existant.

**Hors périmètre** (confirmé avec l'utilisateur) :
- Ajouter, supprimer ou renommer une variable : non traité ici — seules
  les valeurs des variables déjà présentes deviennent éditables. Les
  lignes d'ajout/suppression restent un changement distinct, symétrique
  à ce qui existe déjà pour les en-têtes de requête (`field-editing`).
- Activer/désactiver une variable (`~` en tête de clé) : non traité, la
  valeur seule est éditable.
- Les variables secrètes (`vars:secret`) : elles n'ont pas de valeur dans
  le fichier et n'apparaissent pas dans `Environment.variables` — ce
  panneau ne les affiche ni ne les édite jamais ; elles restent gérées
  exclusivement par le panneau Secrets existant (`Focus::Secrets`).
- La souris : ce changement ne couvre que le clavier, comme
  `field-editing` avant `add-mouse-support` — un clic dans la vue des
  variables ne fait que donner le focus au panneau, sans démarrer de
  saisie. Le support souris pourrait faire l'objet d'un changement
  ultérieur séparé, une fois ce comportement clavier validé.

## Capabilities

### New Capabilities

- `environment-editing` : affichage et édition au clavier des valeurs de
  variables d'un environnement, avec sauvegarde sur disque.

### Modified Capabilities

Aucune capacité existante n'est modifiée : `field-editing` (requêtes) et
`mouse-support` ne sont pas touchés par ce changement.

## Impact

- `src/app/model.rs` : nouvel état de session d'édition d'environnement
  (parallèle à `EditSession`, pas une extension — `EditSession` est
  couplé à `RequestView`/`EditableField`, qui portent des champs sans
  équivalent ici comme `Url`, `Method`, `BodyText`).
- `src/app/view/panels.rs` : `render_environment_picker` gagne une
  seconde vue (variables) affichée à la place de la liste quand une
  session d'édition d'environnement est ouverte.
- `src/app/update.rs` : navigation et saisie dans la nouvelle vue,
  branchées sous `Focus::EnvironmentPicker` (pas de nouveau `Focus`).
- `src/writer/` : nouveau module minimal scopé au bloc `vars` d'un
  fichier d'environnement (un seul type de modification : la valeur
  d'une entrée existante), réutilisant les briques déjà partagées
  (`FileStamp`, écriture atomique fichier temporaire + renommage) sans
  toucher à `Draft`/`FieldEdit`/`EntrySection`, qui restent spécifiques
  aux fichiers de requête.
- Aucune nouvelle dépendance. Aucun changement au format `.bru` : seule
  la valeur d'une entrée déjà présente dans le bloc `vars` est réécrite.
