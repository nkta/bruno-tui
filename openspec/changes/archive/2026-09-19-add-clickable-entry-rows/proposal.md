## Why

L'utilisateur a signalé que la ligne « + Ajouter » (en-têtes, paramètres)
n'est pas cliquable. Vérification faite dans le code : ce n'est vrai que
dans un cas précis. Une fois une session d'édition déjà ouverte, cliquer
sur une ligne « + Ajouter » existante démarre déjà correctement l'ajout
(`mouse_release` → `field_at_line` → `begin_input_at` → `begin_input` →
`start_add`, vérifié par test). Le vrai manque : **quand aucune session
n'est ouverte et qu'une section n'a aucune entrée**, le détail affiche
seulement la ligne `  aucun` (`field-editing` : « Les lignes d'ajout MUST
NOT être affichées hors session ») — il n'existe alors aucune ligne
« + Ajouter » à cliquer, et `  aucun` n'est pas un champ éditable au sens
de `mouse-support`, donc le clic ne fait que donner le focus. C'est ce
que l'utilisateur a rencontré (test sur une requête sans paramètre).

## What Changes

- Un clic sur la ligne `aucun` d'une section vide (en-têtes, paramètres
  de requête ou de chemin), quand aucune session d'édition n'est ouverte
  sur la requête affichée, SHALL désormais ouvrir une session et
  commencer l'ajout d'une entrée à cette section — exactement le même
  effet que cliquer sur sa ligne « + Ajouter » une fois la session déjà
  ouverte (déjà correct aujourd'hui).
- Aucun changement à `field-editing` : les lignes « + Ajouter » restent
  invisibles hors session pour un utilisateur clavier ; ce changement
  n'ajoute qu'une nouvelle cible de clic sur la ligne `aucun` déjà
  affichée, pas une nouvelle ligne.
- Hors périmètre : une section qui a déjà des entrées mais aucune session
  ouverte n'affiche aucune ligne vide à cliquer pour ajouter (pas de
  ligne « aucun » dans ce cas) ; ce changement ne crée pas de nouvelle
  zone cliquable pour ce cas, qui resterait un changement de disposition
  distinct.

## Capabilities

### New Capabilities

Aucune.

### Modified Capabilities

- `mouse-support` : l'exigence « Saisie d'un champ au clic » gagne un cas
  supplémentaire — clic sur la ligne `aucun` d'une section vide, hors
  session, ouvre la session et démarre l'ajout à cette section.

`field-editing` n'est pas modifié : le comportement clavier et la règle
de visibilité des lignes d'ajout restent identiques.

## Impact

- Code : `src/app/view/detail.rs` (`editable_entries`, branche
  `values.is_empty() && session.is_none()` : enregistrer une `FieldLine`
  pour `EditableField::AddRow(section)` sur la ligne `aucun`, au lieu de
  ne rien enregistrer). Aucun changement attendu dans
  `src/app/update.rs` (`mouse_press`/`mouse_release`/`begin_input_at`
  gèrent déjà `AddRow` correctement dès que `field_at_line` le retourne).
- Aucune nouvelle dépendance, aucun changement de format sur disque.
