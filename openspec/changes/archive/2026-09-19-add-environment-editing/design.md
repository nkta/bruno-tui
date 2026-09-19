## Context

- `Environment` (`src/collection/view.rs`) expose déjà `variables: Vec<KeyValue>`
  (bloc `vars`) et `secret_names: Vec<String>` (bloc `vars:secret`, sans
  valeur) — lus depuis l'AST à `Environment.ast: Option<BruFile>`, jamais
  écrits.
- `render_environment_picker` (`src/app/view/panels.rs`) n'affiche que la
  liste des noms d'environnements. `navigate_environment_picker`
  (`src/app/update.rs`) ne traite que `Up`/`Down`/`Home`/`End`/`Right` —
  `Message::Enter` n'a aujourd'hui aucun effet dans ce panneau.
  `Droite` → `select_environment_picker` active l'environnement choisi et
  referme le panneau vers `Focus::Tree`.
- L'édition existante (`field-editing`) est portée par `EditSession`
  (`src/app/model.rs`), couplée à `RequestView`/`EditableField` : `Url`,
  `Method`, en-têtes, paramètres de requête/chemin, corps. Rien de
  réutilisable tel quel pour une variable d'environnement (pas d'URL, pas
  de méthode, pas de corps, pas d'ajout/suppression dans ce périmètre).
- `src/writer/` (`draft.rs`, `edit.rs`, `format.rs`) n'écrit que des
  fichiers de requête : `Draft::from_ast` construit ses trois sections à
  partir de `EntrySection::ALL` (`headers`, `params:query`,
  `params:path`) et porte en plus l'état `url`/`method`/`body`, propre au
  bloc de méthode d'une requête. `writer/mod.rs` porte en revanche des
  briques génériques déjà indépendantes du format de requête :
  `FileStamp` (instantané taille + date de modification) et l'écriture
  atomique (fichier temporaire même répertoire + renommage).

Voir `proposal.md` pour la motivation.

## Goals / Non-Goals

**Goals:**
- Afficher les variables d'un environnement et permettre de modifier la
  valeur d'une variable déjà présente, au clavier, avec sauvegarde sur
  disque aux mêmes garanties (atomique, refus si fraîcheur perdue) que
  l'édition de requête existante.

**Non-Goals (voir aussi proposal.md) :**
- Ajouter, supprimer, renommer ou activer/désactiver une variable.
- Éditer une valeur secrète : structurellement hors de portée, une
  variable secrète n'a pas de valeur dans le fichier.
- Support souris dans la nouvelle vue.
- Généraliser `EditSession`/`Draft` pour qu'ils couvrent à la fois
  requêtes et environnements : les deux formats de bloc édité (bloc de
  méthode avec URL/params vs. simple dictionnaire `vars`) divergent
  assez pour qu'une abstraction commune coûterait plus qu'elle
  n'économiserait, pour un seul type de modification (valeur d'entrée).

## Decisions

### D1 — Vue imbriquée dans `Focus::EnvironmentPicker`, pas un nouveau `Focus`
La vue des variables est un second état d'affichage du même panneau
(liste ↔ variables), pas un nouveau `Focus`. Même précédent que
`EditState::MethodPicker` imbriqué dans `EditSession` plutôt qu'un focus
séparé (`add-method-editing`) : le routage clavier/souris reste à une
seule règle par panneau, et `Échap` retrouve son sens habituel (revenir
d'un cran) sans complexifier le graphe de focus.

**Alternative écartée** : un `Focus::EnvironmentVariables` séparé —
rejeté, multiplierait les branches de routage (`mouse_accepted`,
`session_help_line`, tests de focus) pour un état qui n'a de sens que
comme sous-état du panneau Environnement.

### D2 — `EntrySection::ALL` (`Message::Enter` inutilisé aujourd'hui) porte la bascule liste ↔ variables
`Entrée`, sans effet aujourd'hui dans `navigate_environment_picker`,
déclenche l'entrée dans la vue variables. Aucun raccourci existant n'est
retiré ou réinterprété : `Droite` (activer l'environnement) garde son
comportement et son effet de fermeture du panneau, inchangés.

### D3 — Nouvelle session `EnvironmentEditSession`, pas une extension d'`EditSession`
Structure indépendante (nom provisoire, à ajuster en implémentation) :
chemin du fichier, `FileStamp` capturé à l'ouverture, `Vec<KeyValue>` en
mémoire (valeurs modifiables), curseur de sélection, état
Sélection/Saisie (réutilise le `TextInput` déjà utilisé par
`EditSession`), indicateur de modification. Une session de requête
(`Model.editing`) et une session d'environnement peuvent coexister sans
conflit : elles ciblent des fichiers différents et aucune des deux
n'écrit celui de l'autre.

**Alternative écartée** : étendre `EditableField`/`EditSession` avec des
variantes « variable d'environnement » — rejeté, `EditSession` porte des
champs et invariants propres à une requête (`Url`, `Method`, body,
sections avec ajout/suppression) qui n'ont pas de sens ici et
introduiraient des branches mortes dans tout le code qui filtre déjà sur
`EditableField`.

### D4 — Nouveau module d'écriture minimal, scopé au bloc `vars`
Un module dédié (ex. `src/writer/environment.rs`), pas une extension de
`Draft`/`FieldEdit`/`EntrySection` : un seul type de modification (valeur
d'une entrée existante du bloc `vars`, par index), pas de synchronisation
d'URL, pas de bloc de méthode, pas d'ajout/suppression. Réutilise les
briques déjà génériques de `writer/mod.rs` (`FileStamp`, écriture
atomique) ; réimplémente uniquement la résolution de tranche(s) à
réécrire pour les entrées touchées du bloc `vars`, sur le même principe
que la résolution déjà existante pour une entrée de requête (comparer
l'AST d'origine à l'état modifié, ne réémettre que les tranches
touchées).

### D5 — Les variables secrètes restent hors de portée par construction
La nouvelle session ne lit et n'écrit que `Environment.variables` (bloc
`vars`). `secret_names` (bloc `vars:secret`) n'y figure jamais : aucune
vérification supplémentaire n'est nécessaire pour empêcher l'écriture
d'un secret, la variable n'existe simplement pas dans les données que la
vue manipule.

## Risks / Trade-offs

- [Deux chemins d'écriture `.bru` distincts (requête, environnement)
  dupliquent une partie de la logique de résolution de tranches] →
  Mitigation : seules les briques vraiment génériques (`FileStamp`,
  écriture atomique) sont partagées ; la partie dupliquée (résolution de
  tranches pour un bloc `vars`) est nettement plus simple que `Draft`
  (un seul type de modification, pas de synchronisation), donc le coût
  de duplication reste faible face au coût d'une abstraction commune
  prématurée.
- [Le fichier d'environnement change sur disque pendant que la vue est
  ouverte] → Mitigation : même refus par instantané de fraîcheur
  (`FileStamp`) qu'aujourd'hui pour une requête, message d'erreur
  explicite, aucune écriture partielle.
- [Incohérence perçue si la vue variables affiche un environnement qui
  n'est pas l'environnement actif] → Assumé comme acceptable : le
  proposal distingue explicitement « consulter/éditer les variables d'un
  environnement » de « activer un environnement » (`Droite`), qui
  restent deux actions indépendantes ; documenté ici pour ne pas être
  redécouvert comme un bug lors de l'implémentation.
