## Why

L'édition du panneau Requête (`field-editing`) impose aujourd'hui une
ergonomie vim : `e` ouvre une session en mode Normal, `j`/`k` choisit le
champ, `i` passe en Insert, `Échap` quitte l'Insert **en gardant** la
saisie, `w` enregistre. Pour un usage d'exploration d'API, c'est une
barrière : il faut connaître quatre touches modales avant de corriger une
URL, le curseur de texte ne sait aller ni au début ni à la fin, `Suppr`
n'existe pas, le corps multi-ligne ne se parcourt pas verticalement, et
il n'existe aucun moyen d'annuler une saisie ratée. L'utilisateur veut une
édition directe, façon formulaire, sans renoncer à la sauvegarde explicite.

## What Changes

- **BREAKING** — Les modes vim de la session d'édition sont **remplacés**
  (pas conservés en parallèle) par deux états nommés en clair :
  *Sélection de champ* et *Saisie*.
  - `i` (entrer en Insert) et `w` (enregistrer) sont retirés.
  - `Échap` en Saisie **annule** désormais la saisie en cours (la valeur
    revient à celle d'avant l'entrée dans le champ) au lieu de la garder.
- Ouverture de la session par `Entrée` sur le détail d'une requête
  (`e` reste un alias d'ouverture). `↑`/`↓` (et `j`/`k`) choisissent le
  champ ; `Entrée` commence la saisie du champ sous le curseur ; `Espace`
  bascule toujours l'activation d'un en-tête ou paramètre.
- Saisie avec curseur libre : `←`/`→`, `Début`/`Fin` (de la ligne),
  `Retour arrière` et `Suppr` ; sur le corps, `↑`/`↓` changent de ligne et
  `Entrée` insère un saut de ligne.
- Validation et annulation explicites et uniformes :
  - `Entrée` valide un champ à une ligne, `Tab` valide n'importe quel
    champ (dont le corps) ;
  - `Échap` annule la saisie ;
  - `Ctrl+S` enregistre sur disque, depuis la sélection de champ comme
    depuis la saisie (qu'il valide d'abord). Aucune autre action n'écrit.
- Pendant la Saisie, **toutes** les touches imprimables sont du texte
  (`q`, `r`, `/`, `e`, `D`, `S`, etc.) ; seules `Ctrl+C`, `Ctrl+X` et
  `Ctrl+S` gardent un sens global. En Sélection de champ, seules les
  touches listées ci-dessus sont redéfinies ; les autres raccourcis
  globaux gardent leur comportement.
- Une saisie non validée compte comme modification non enregistrée pour
  la confirmation de sortie.
- Plus de perte silencieuse : tant qu'une session porte des
  modifications non enregistrées, changer de requête sélectionnée (arbre,
  recherche, navigation croisée) est refusé avec un message, au lieu de
  fermer la session sans prévenir comme aujourd'hui.
- Le champ sous le curseur et le curseur de texte restent visibles
  (défilement vertical du détail, décalage horizontal de la ligne
  éditée).
- La barre de mode devient une barre d'aide : état courant, champ,
  indicateur « non enregistré » et touches utiles à l'état.

## Capabilities

### New Capabilities

Aucune.

### Modified Capabilities

- `field-editing` : ouverture de session, curseur de champ, saisie de
  texte (curseur libre, multi-ligne, validation/annulation), sauvegarde
  par `Ctrl+S`, arbitrage des touches globales, protection contre le
  changement de sélection, visibilité du curseur, barre d'aide.

`bru-writer` n'est pas modifié : les éditions produites restent les mêmes
`FieldEdit`. `tui-shell` et `search-and-yank` ne sont pas modifiés : les
touches qu'ils définissent gardent leur sens hors session, et
`field-editing` précise déjà comment la session les surcharge.

## Impact

- Code : `src/app/message.rs` (liaisons `Entrée`, `Ctrl+S`, `Suppr`,
  `Début`/`Fin`, `↑`/`↓` en capture ; retrait de `i`/`w`),
  `src/app/model.rs` (`EditMode` → états Sélection/Saisie,
  `text_capture`), `src/app/update.rs` (édition du tampon, validation,
  annulation, garde de changement de sélection),
  `src/app/view/detail.rs` et `src/app/view/mod.rs` (position du curseur,
  décalage horizontal, barre d'aide).
- Aucune nouvelle dépendance ; aucun changement de format sur disque.
- Habitudes utilisateur : les touches `i`, `w` et le sens de `Échap` en
  saisie changent (BREAKING, documenté dans la barre d'aide).
- Changements parallèles concernés (voir `design.md`) : `add-mouse-support`,
  `add-entry-management`, `add-status-panel`.
