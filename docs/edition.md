# Spécification — Édition, phase 1

Statut : **approuvée par Nicolas le 2026-10-12**.
Décisions de Nicolas du 2026-10-12 :

1. Périmètre : on commence par la phase 1.
2. Scripts et tests : édités dans un éditeur externe, au choix de l'utilisateur.
3. Suppression (phase 3) : vers la corbeille, jamais d'effacement définitif.

Cette spécification lève, pour la phase 1 seulement, la règle de `CLAUDE.md` :
« aucune écriture dans un fichier `.bru` tant que la capacité d'édition
n'a pas été spécifiée et approuvée ».

## 1. Périmètre

### Dans la phase 1

| Famille | Blocs concernés |
|---|---|
| Blocs clé / valeur | `vars:pre-request`, `vars:post-response`, `assert`, `settings` (valeurs seulement) |
| Blocs de texte | `script:pre-request`, `script:post-response`, `tests`, `docs` |

Fichiers concernés :

- les requêtes HTTP (`*.bru`) ;
- les dossiers (`folder.bru`) et la collection (`collection.bru`), pour les blocs
  qu'ils peuvent porter ; leurs en-têtes (`headers`) aussi, avec la mécanique
  déjà utilisée pour les requêtes.

### Hors phase 1

- Auth et mode du corps : phase 2, à spécifier.
- Créer, renommer, cloner, déplacer, supprimer des fichiers : phase 3, à spécifier.
- Collections OpenCollection (YAML) : toujours en lecture seule.
- Requêtes gRPC et WebSocket : toujours en lecture seule.
- Fichiers d'environnement : rien de nouveau (l'édition des `vars` existe déjà).

## 2. Principes (inchangés, rappelés)

1. **Écriture chirurgicale** : seuls les octets des entrées touchées sont
   réécrits. Le reste du fichier (blocs, commentaires, fins de ligne, ordre)
   reste identique à l'octet près.
2. **Fraîcheur** : on refuse d'écrire si le fichier a changé sur disque
   depuis sa lecture (`FileStamp`), comme aujourd'hui.
3. **Rien sur disque avant `Ctrl+S`** : les modifications restent dans la
   session d'édition jusqu'à l'enregistrement. `Échap` demande confirmation
   s'il y a des changements.
4. **Aucune interprétation** : le TUI n'évalue ni les scripts, ni les
   assertions, ni les variables. Il écrit le texte tel que saisi.
5. **Validation par Bruno** : tout fichier produit est relu par le parser
   officiel (`@usebruno/lang`) dans les tests (`tests/writer_bruno_lang.rs`).

## 3. Blocs clé / valeur

### 3.1 Opérations

Mêmes touches que pour les en-têtes, dans une session d'édition :

| Touche | Effet |
|---|---|
| `Entrée` | modifier la valeur |
| `a` | ajouter une entrée (clé, puis valeur) |
| `d` | supprimer l'entrée, sans confirmation |
| `c` | renommer la clé |
| `Espace` | activer / désactiver (préfixe `~`) |

Pour `settings` : seules les valeurs des clés existantes se modifient.
Pas d'ajout, de suppression ni de renommage : les clés sont fixées par Bruno.

### 3.2 Règles

- Clés et valeurs validées selon la grammaire bru-lang
  (`bruToJson.js` de Bruno 4.2.1). Une saisie invalide est refusée avec un
  message clair, comme pour les en-têtes.
- `assert` : la clé est une expression (`res.status`, `res.body.id`), la
  valeur un opérateur suivi d'un opérande (`eq 200`). Aucune vérification
  sémantique de l'opérateur : c'est `bru` qui l'évalue.
- Bloc absent : il est créé à la place où Bruno l'écrirait (ordre de
  `jsonToBru.js`).
- Dernière entrée supprimée : le bloc vide est retiré.

### 3.3 Navigation

Les nouveaux blocs deviennent des champs de la session d'édition, à leur
place dans leur onglet (Vars, Assert, Settings). L'onglet affiché suit le
champ sous le curseur, comme aujourd'hui.

## 4. Blocs de texte (éditeur externe)

### 4.1 Choix de l'éditeur

Ordre de priorité :

1. option `--editor "COMMANDE"` (nouvelle) ;
2. variable `VISUAL` ;
3. variable `EDITOR` ;
4. `nano`.

La commande peut contenir des arguments (`--editor "code --wait"`), avec le
découpage déjà utilisé par `Ctrl+E` (`src/app/editor.rs`).

### 4.2 Déroulé

1. Dans une session d'édition, `Entrée` sur un bloc de texte (Script
   pré-requête, Script post-réponse, Tests, Docs) ouvre l'éditeur.
2. Le TUI se suspend pendant l'édition, comme pour `Ctrl+E`.
3. Le contenu actuel du bloc est écrit dans un fichier temporaire :
   - dans un dossier privé (`$XDG_RUNTIME_DIR`, sinon le dossier temporaire
     du système), droits `0600` ;
   - nom unique, extension `.js` (scripts, tests) ou `.md` (docs), pour la
     coloration dans l'éditeur ;
   - supprimé dès la fermeture de l'éditeur, y compris en cas d'erreur.
4. À la fermeture :
   - contenu inchangé : rien ne se passe ;
   - contenu modifié : il devient une modification en attente de la session,
     écrite seulement au `Ctrl+S` ;
   - contenu vide : le bloc sera retiré à l'enregistrement.
5. Éditeur introuvable ou en erreur : message dans le statut, aucune
   modification.

### 4.3 Écriture

- Le writer réindente le contenu comme Bruno (2 espaces par ligne) et le
  désindente à la lecture, comme pour le corps et `docs` aujourd'hui.
- Un contenu qui fermerait le bloc trop tôt une fois réindenté est
  impossible : chaque ligne est indentée. Un test le vérifie quand même avec
  le parser officiel (lignes `}` et `]`, accolades non équilibrées).
- Fins de ligne : celles du fichier d'origine (`\n` ou `\r\n`).

### 4.4 Sécurité

- Le fichier temporaire peut contenir des secrets écrits en dur dans un
  script : droits `0600`, suppression immédiate, jamais de copie ailleurs.
- Le contenu n'est jamais loggué.

## 5. Tests et critères d'acceptation

1. Pour chaque opération (valeur, ajout, suppression, renommage,
   activation, création et retrait de bloc, texte) :
   - un test d'octets (avant / après) sur une fixture de
     `tests/fixtures/collections/writer-cases/` ;
   - un test de relecture par `@usebruno/lang`.
2. Fichiers de dossier et de collection : au moins un cas par famille.
3. Éditeur externe : testé avec un faux éditeur (script de test qui modifie
   le fichier reçu) ; vérifier la suppression du fichier temporaire.
4. `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
   passent.
5. Essai réel dans le TUI avant fusion.

## 6. Phases suivantes (pour mémoire, non approuvées)

- **Phase 2 — Auth et mode du corps.** Avertir quand un secret est saisi en
  clair et proposer `{{variable}}` à la place.
- **Phase 3 — Fichiers.** Créer, renommer (fichier et `meta.name`, comme
  Bruno), cloner, réordonner (`seq`), supprimer **vers la corbeille**
  (spécification freedesktop.org sous Linux), avec confirmation.
