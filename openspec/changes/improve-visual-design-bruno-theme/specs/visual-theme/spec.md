## ADDED Requirements

### Requirement: Fond d'application cohérent
L'interface SHALL appliquer un fond de couleur cohérent à l'ensemble de
la zone du terminal qu'elle occupe, plutôt que de laisser apparaître le
fond par défaut hérité du terminal de l'utilisateur. Ce fond SHALL
rester le même sur tout l'écran (titre, panneaux, barre d'état), pour
que l'interface se présente comme une surface unique plutôt que comme
des panneaux flottant sur un arrière-plan qui leur est étranger.

#### Scenario: Fond identique sur toute la hauteur de l'écran
- **WHEN** la collection est chargée et affichée sur un terminal de
  taille quelconque au-dessus du minimum supporté
- **THEN** la couleur de fond de la ligne de titre, des panneaux et de
  la barre d'état est la même partout à l'écran
