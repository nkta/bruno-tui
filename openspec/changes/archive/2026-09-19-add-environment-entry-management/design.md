## Context

- `src/writer/draft.rs::Draft::replacements()` sait déjà diffuser un
  ajout ou une suppression d'entrée dans un bloc dictionnaire
  (`headers`, `params:query`, `params:path`) vers des tranches de source
  minimales : entrée retirée (tranche vidée), entrée ajoutée (insertion
  après la dernière entrée, ou création du bloc entier s'il n'existait
  pas), bloc entièrement retiré si la dernière entrée en est ôtée. Cette
  logique (environ 90 lignes, `Section`/`DraftEntry`/`block_node`/
  `format_added`/`creation_anchor`/`preceding_blank_line_len`/`line_end`)
  est aujourd'hui écrite uniquement pour `EntrySection` (3 sections de
  requête fixes).
- `src/writer/environment.rs::resolve()` ne sait aujourd'hui gérer que
  des remplacements de valeur sur des entrées déjà présentes — pas
  d'ajout, pas de suppression, pas de création du bloc `vars` s'il
  n'existe pas.
- `check_key` (`draft.rs`) valide déjà une clé (vide, espace, `:`, `~`/
  `"` en tête, plus une règle spécifique aux paramètres de requête) et
  produit `EditError::InvalidEntry { block, index, problem }` — `block`
  est déjà un `&'static str` générique (`"headers"`, `"params:query"`,
  etc.), pas couplé à `EntrySection` dans sa forme.
- `EnvironmentEditSession`/`EnvironmentEditState` (`model.rs`) ne
  portent aujourd'hui que Sélection et Saisie d'une valeur existante.
  `EditSession`/`InputTarget` (requêtes) portent déjà `NewKey`/
  `NewValue` pour l'ajout en deux temps — précédent direct à suivre.
- Les fixtures réelles (`tests/fixtures/collections/*/environments/*.bru`)
  montrent que le bloc `vars`, quand il existe, est le premier bloc du
  fichier d'environnement (avant `vars:secret`).

Voir `proposal.md` pour la motivation.

## Goals / Non-Goals

**Goals :**
- Ajouter et supprimer une variable depuis le popup d'édition, avec les
  mêmes garanties d'écriture (atomique, minimale, refus si fraîcheur
  perdue) que la modification de valeur déjà en place.
- Ne pas dupliquer la mécanique de diffusion d'un ajout/suppression vers
  des tranches de source : un seul endroit sait le faire, pour une
  requête comme pour un environnement.

**Non-Goals :**
- Renommer une clé, activer/désactiver une variable : non demandés (voir
  `proposal.md`).
- Généraliser au-delà du besoin actuel (ex. un N-ième type de bloc
  dictionnaire hypothétique) : l'extraction vise `Draft` et
  l'environnement, pas un cadre pour tout usage futur.

## Decisions

### D1 — Factoriser la diffusion d'un ajout/suppression, ne pas la dupliquer
Extraire de `draft.rs` une fonction libre et générique (nouveau module
`src/writer/dictionary.rs`), prenant : le texte source, la fin de ligne
détectée, le nom du bloc, le résultat de `block_node` (tranche + entrées
d'origine, ou absence), la liste courante des entrées (avec leur indice
d'origine, `None` pour une entrée ajoutée), et l'ancre d'insertion à
utiliser si le bloc doit être créé. Elle retourne les `Replacement` à
appliquer — exactement le corps actuel de la boucle `for section in
EntrySection::ALL` dans `Draft::replacements()`, sans rien qui dépende
d'`EntrySection`. `Draft::replacements()` l'appelle une fois par
section (ancre calculée comme aujourd'hui par `creation_anchor`,
inchangée, propre aux requêtes) ; `writer::environment::resolve()`
l'appelle une fois pour `vars` (ancre : voir D2). `DraftEntry`/`Section`
déménagent dans ce module, renommés `DictEntry`/`DictSection` (aucun nom
ni concept propre à une requête).

**Alternative écartée** : dupliquer une version simplifiée de cette
logique dans `environment.rs`, comme le proposal initial
d'`add-environment-editing` l'avait fait pour la seule modification de
valeur — rejeté ici : l'ajout/suppression est exactement la partie la
plus délicate (calcul de tranche, gestion du bloc absent ou vidé) que ce
choix évitait précisément de dupliquer pour la valeur seule ; la
dupliquer maintenant doublerait le risque de bug sur du code qui
réécrit des fichiers utilisateur, pour un gain nul.

**Risque de régression** : `Draft::replacements()` est déjà couvert par
la suite `writer_fixtures`/les tests unitaires de `draft.rs`
(add-entry-management). Le refactor ne change aucun comportement
observable côté requêtes ; la suite existante, rejouée sans
modification après l'extraction, sert de garde-fou avant d'ajouter quoi
que ce soit côté environnement.

### D2 — Ancre de création du bloc `vars`, propre à l'environnement
Un fichier d'environnement n'a ni bloc de méthode ni ordre de sections à
respecter : quand `vars` n'existe pas encore et qu'une variable est
ajoutée, le nouveau bloc s'insère en tout début de fichier (position 0)
— cohérent avec les fichiers réels observés, où `vars` précède déjà
toujours `vars:secret`. Pas de portage de `creation_anchor` (propre à
l'ordonnancement des sections de requête) : une simple constante suffit
ici.

### D3 — État d'ajout porté directement par `EnvironmentEditState`
Deux nouvelles variantes, `AddingKey(TextInput)` et `AddingValue { key:
String, input: TextInput }`, ajoutées à `EnvironmentEditState` (pas un
champ `target` séparé comme `EditSession` : `EnvironmentEditSession` n'a
qu'une seule « section » possible — `vars` — donc rien à distinguer par
ailleurs). `EnvironmentEditSession` gagne `begin_add`
(Sélection → AddingKey), `commit_add_key` (AddingKey → AddingValue, ou
refuse une clé invalide), `commit_add_value` (AddingValue → Sélection,
variable ajoutée, curseur dessus, modifiée), et `cancel_add` (retour à
Sélection sans rien ajouter, comme `cancel_input`). `input_key` et
`Message::ValidateInput`/`CancelInput`/`Enter` (`update.rs`) sont
étendus pour reconnaître ces deux nouveaux états au même titre que
`Input`, par un petit accesseur `EnvironmentEditState::text_input_mut()`
couvrant les trois variantes qui portent un `TextInput`, pour ne pas
tripler le même bloc de branchement.

**Journal des modifications, comme `EditSession.pending`** : `save_environment`
(`update.rs`) calcule aujourd'hui les `EnvironmentVarEdit` à écrire en
comparant `session.variables` à l'AST d'origine indice par indice — ça
suffit tant que seule la valeur change, mais ça se rompt dès qu'un ajout
ou une suppression décale les indices. `EnvironmentEditSession` gagne
donc `pending: Vec<EnvironmentVarEdit>`, sur le modèle exact de
`EditSession.pending` déjà utilisé pour les requêtes : chaque validation
(valeur, ajout, suppression) y ajoute l'édition correspondante, l'indice
qu'elle porte désignant la position dans la liste courante telle qu'elle
résulte des éditions précédentes de `pending` — exactement la sémantique
déjà documentée sur `FieldEdit`. `save_environment` écrit alors
`session.pending.clone()` tel quel, sans diff.

### D4 — Validation de clé réutilisée telle quelle
`check_key` reste dans `draft.rs` (propre à `EntrySection`, dont la
règle spécifique aux paramètres de requête) mais sa forme générale
(vide, espace, `:`, `~`/`"` en tête) est extraite en une fonction libre
réutilisable par l'environnement, retournant le même
`EditError::InvalidEntry { block: "vars", index, problem }` déjà
générique — aucune nouvelle variante d'erreur.

## Risks / Trade-offs

- [Refactor d'un code d'écriture déjà stable et testé] → Mitigation :
  extraction mécanique (déplacement, pas de réécriture de la logique
  elle-même), suite `draft.rs`/`writer_fixtures` existante rejouée sans
  modification juste après l'extraction, avant tout ajout de code
  d'environnement.
- [Deux appelants du même module de bas niveau (requête, environnement)
  doivent rester d'accord sur son contrat] → Mitigation : le module
  n'expose qu'une fonction pure, sans état partagé ; chaque appelant
  garde son propre calcul d'ancre et sa propre validation de clé.
