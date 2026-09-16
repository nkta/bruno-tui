## MODIFIED Requirements

### Requirement: Défilement du détail
L'interface SHALL permettre de basculer le focus entre l'arbre et le
détail avec `Tab`, et de revenir à l'arbre avec `Échap`, sauf si une
session d'édition de champ (`field-editing`) est active sur la requête
affichée : dans ce cas, `Échap` referme d'abord cette session (avec
confirmation si elle porte des modifications non sauvegardées) et
laisse le focus sur le détail ; un second `Échap`, une fois la session
fermée, rend le focus à l'arbre comme précédemment. Quand le détail a le
focus, `↓`/`j` et `↑`/`k` MUST le faire défiler d'une ligne,
`Page suivante` et `Page précédente` d'une hauteur de panneau, `Début` et
`Fin` MUST aller au début et à la fin. Le défilement MUST NOT dépasser la
fin du contenu. Changer de nœud sélectionné MUST ramener le détail en
haut. Le panneau ayant le focus MUST être visuellement distingué.

#### Scenario: Corps plus long que le panneau
- **WHEN** le détail d'une requête dépasse la hauteur du panneau, que le
  détail a le focus et que l'utilisateur appuie sur `Fin`
- **THEN** la dernière ligne du détail est visible
- **AND** un appui supplémentaire sur `↓` ne change pas l'affichage

#### Scenario: Retour à l'arbre
- **WHEN** le détail a le focus, qu'aucune session d'édition n'est
  active, et l'utilisateur appuie sur `Échap`
- **THEN** l'arbre reprend le focus et `↓` change la sélection

#### Scenario: Échap referme d'abord la session d'édition
- **WHEN** une session d'édition sans modification est active sur la
  requête affichée et l'utilisateur appuie sur `Échap`
- **THEN** la session se ferme, le focus reste sur le détail, en lecture
  seule
- **AND** un second `Échap` rend alors le focus à l'arbre

### Requirement: Sortie et restauration du terminal
L'interface SHALL se fermer sur `q` quand l'arbre ou le détail a le focus,
et sur `Ctrl+C` en toutes circonstances, sauf si une session d'édition de
champ porte des modifications non sauvegardées au moment de l'appui : la
fermeture est alors précédée d'une demande de confirmation, et une
confirmation refusée annule la fermeture sans quitter l'application. À la
fermeture, le terminal MUST retrouver son état antérieur : mode brut
désactivé, écran alternatif quitté, curseur visible. Cette restauration
MUST aussi avoir lieu si l'application panique, avant l'affichage du
message de panique. Le code de sortie MUST être 0 après une fermeture
volontaire.

#### Scenario: Sortie par q
- **WHEN** l'utilisateur appuie sur `q` et qu'aucune session d'édition
  modifiée n'est active
- **THEN** l'application se termine avec le code 0 et le terminal est
  restauré

#### Scenario: Panique
- **WHEN** l'application panique alors que l'interface est ouverte
- **THEN** le terminal est restauré avant que le message de panique ne
  soit affiché

#### Scenario: Sortie demandée avec une session modifiée
- **WHEN** une session d'édition porte une modification non sauvegardée
  et l'utilisateur appuie sur `q`
- **THEN** une confirmation est demandée avant toute fermeture du
  terminal
- **AND** si l'utilisateur annule, l'application reste ouverte
