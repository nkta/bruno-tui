## Why

Un test manuel du panneau Détail a montré que les en-têtes et paramètres
s'affichent comme du texte plat (`  Content-Type: application/json`), sans
distinction visuelle entre la clé et la valeur, et que le champ Corps
(quand son type le rend éditable) n'a aucune apparence de champ de saisie :
rien ne le distingue du reste du texte affiché. La capacité `visual-theme`
impose déjà cette distinction pour les champs simples du détail (« Chemin »,
« Méthode », « Auth », rendus par `field()` avec un style de libellé dédié),
mais les fonctions qui affichent les en-têtes/paramètres (`entries`,
`editable_entries`) et le corps ne l'appliquent pas : c'est un angle mort
de cette règle, pas un comportement voulu.

## What Changes

- Étendre la règle existante de distinction libellé/valeur du panneau
  Détail aux entrées clé/valeur (en-têtes, paramètres de requête,
  paramètres de chemin) : la clé SHALL porter le même style de libellé
  que les champs simples (« Chemin », « Méthode »...), la valeur restant
  au style normal. S'applique à l'affichage hors session (lecture) et
  pendant une session d'édition (`field-editing`), y compris pour une
  entrée désactivée, en cours de renommage ou d'ajout.
- Donner au champ Corps, quand son type le rend éditable (`json`, `text`,
  `xml`, `sparql`, `graphql`, selon la règle déjà définie par
  `field-editing`), une présentation visuellement identifiable comme zone
  de saisie plutôt que du texte de détail ordinaire.
- Aucun changement de contenu informatif, de touches, de curseur, de
  validation ou de sauvegarde : ce changement ne touche que la
  présentation. Le comportement défini par `field-editing` reste
  inchangé à l'identique.

## Capabilities

### New Capabilities

Aucune.

### Modified Capabilities

- `visual-theme` : la règle de distinction visuelle libellé/valeur du
  panneau Détail (actuellement limitée aux champs simples) est étendue
  pour couvrir explicitement les entrées clé/valeur (en-têtes,
  paramètres) et pour donner au champ Corps éditable une présentation
  de zone de saisie identifiable.

## Impact

- Code : `src/app/view/detail.rs` (`entries`, `editable_entries`, et le
  rendu du champ Corps dans `request_text_and_fields` / la fonction
  associée), `src/app/view/theme.rs` (styles éventuellement nécessaires,
  réutilisation de `LABEL` si suffisant).
- Aucun changement d'API, de dépendance, ou de format sur disque.
- `field-editing` n'est pas modifié : les exigences de comportement
  (touches, curseur, sauvegarde) restent les mêmes ; seule leur
  présentation change.
