## Purpose

Permettre de lancer, depuis l'arbre de la collection, l'exécution d'une
requête ou d'un dossier via `bru-runner`, et d'afficher sa progression
puis son résultat détaillé, sans jamais bloquer l'interface.

## ADDED Requirements

### Requirement: Lancement d'une exécution depuis l'arbre
Quand une collection est chargée et qu'une requête ou un dossier est
sélectionné dans l'arbre, l'interface SHALL permettre de lancer son
exécution par une touche dédiée, distincte des touches de navigation
existantes. Sélectionner une requête SHALL lancer cette seule requête ;
sélectionner un dossier SHALL lancer récursivement toutes les requêtes
qu'il contient. La touche SHALL être sans effet quand le nœud sélectionné
est un nœud en erreur de chargement, quand aucune collection n'est
chargée, ou quand la collection est vide.

#### Scenario: Lancement d'une requête simple
- **WHEN** la requête `ok` de la collection `runner-probe` est sélectionnée
  et l'utilisateur appuie sur la touche de lancement
- **THEN** l'exécution démarre pour cette seule requête

#### Scenario: Lancement récursif d'un dossier
- **WHEN** le dossier `folder` est sélectionné et l'utilisateur appuie sur
  la touche de lancement
- **THEN** l'exécution démarre pour ce dossier et ses requêtes, en mode
  récursif

#### Scenario: Nœud en erreur sélectionné
- **WHEN** un nœud en erreur de chargement est sélectionné et l'utilisateur
  appuie sur la touche de lancement
- **THEN** aucune exécution ne démarre

### Requirement: Exécution non bloquante
Le lancement d'une exécution SHALL rendre la main immédiatement : la
navigation dans l'arbre et dans le détail, ainsi que la sortie de
l'application, MUST rester utilisables pendant qu'une exécution est en
cours.

#### Scenario: Navigation pendant une exécution longue
- **WHEN** une exécution est en cours et met plusieurs secondes à se
  terminer
- **THEN** l'utilisateur peut continuer à naviguer dans l'arbre et changer
  la sélection sans attendre la fin de l'exécution

#### Scenario: Sortie pendant une exécution en cours
- **WHEN** une exécution est en cours et l'utilisateur quitte
  l'application
- **THEN** l'application se ferme sans attendre la fin de l'exécution

### Requirement: Une exécution à la fois
Le système SHALL n'autoriser qu'une seule exécution en cours à la fois.
Tant qu'une exécution n'est pas terminée ou annulée, la touche de
lancement MUST rester sans effet, quel que soit le nœud sélectionné.

#### Scenario: Lancement pendant une exécution en cours
- **WHEN** une exécution est en cours et l'utilisateur sélectionne une
  autre requête puis appuie sur la touche de lancement
- **THEN** aucune nouvelle exécution ne démarre et l'exécution en cours se
  poursuit

### Requirement: Annulation d'une exécution en cours
L'interface SHALL permettre d'annuler l'exécution en cours par une touche
dédiée, distincte de la touche de sortie. Après annulation, une nouvelle
exécution MUST pouvoir être lancée dès que l'annulation est confirmée par
`bru-runner`, sans qu'aucun résultat partiel ne soit affiché pour
l'exécution annulée.

#### Scenario: Annulation d'une exécution longue
- **WHEN** une exécution est en cours et l'utilisateur appuie sur la
  touche d'annulation
- **THEN** l'exécution est annulée
- **AND** un message indique que l'exécution a été annulée
- **AND** l'utilisateur peut lancer une nouvelle exécution

#### Scenario: Annulation sans exécution en cours
- **WHEN** aucune exécution n'est en cours et l'utilisateur appuie sur la
  touche d'annulation
- **THEN** rien ne se produit

### Requirement: Indicateur de progression
Pendant qu'une exécution est en cours, l'interface SHALL afficher un
indicateur de progression identifiant la cible en cours d'exécution, à la
fois dans la barre d'état et, quand la cible est une requête actuellement
visible dans l'arbre, sur sa ligne. Cet indicateur MUST disparaître dès
que l'exécution se termine, quelle que soit son issue.

#### Scenario: Indicateur pendant une exécution
- **WHEN** une exécution est en cours sur la requête `ok`
- **THEN** la barre d'état identifie la cible en cours d'exécution
- **AND** la ligne de `ok` dans l'arbre porte un indicateur de progression

#### Scenario: Disparition de l'indicateur
- **WHEN** une exécution en cours se termine
- **THEN** l'indicateur de progression n'est plus affiché

### Requirement: Résultat d'une requête exécutée
Quand l'exécution d'une requête aboutit à un rapport, quel que soit son
verdict, l'interface SHALL conserver et pouvoir afficher, quand cette
requête est sélectionnée : le verdict global (réussie ou en échec), le
statut de la réponse (code HTTP, absence de réponse avec le message
d'erreur, ou requête ignorée), le temps de réponse, les en-têtes et le
corps de la réponse, ainsi que le statut individuel — succès, échec,
erreur ou ignoré — de chaque assertion, test, test pré-requête et test
post-réponse, avec le message d'erreur associé quand il y en a un. Ce
résultat SHALL rester affichable après un changement de sélection puis un
retour sur cette requête, et SHALL être remplacé par celui d'une exécution
plus récente de la même requête, y compris quand elle a été exécutée par
un dossier la contenant.

#### Scenario: Requête entièrement réussie
- **WHEN** l'exécution de la requête `green` de `runner-probe` se termine
  avec toutes ses assertions et tous ses tests en succès, et que `green`
  est sélectionné
- **THEN** le détail affiche un verdict de réussite, le code de statut
  HTTP, le corps de la réponse et le temps de réponse

#### Scenario: Assertion en échec
- **WHEN** l'exécution de la requête `ok` se termine avec une assertion en
  échec, et que `ok` est sélectionné
- **THEN** le détail affiche un verdict d'échec et, pour l'assertion en
  échec, son message d'erreur

#### Scenario: Test post-réponse en échec
- **WHEN** l'exécution de la requête `json` se termine avec un test
  post-réponse en échec, et que `json` est sélectionné
- **THEN** le détail affiche un verdict d'échec et le statut d'échec du
  test post-réponse concerné

#### Scenario: Aucune réponse reçue
- **WHEN** l'exécution de la requête `folder/down` se termine par une
  erreur de connexion, et que cette requête est sélectionnée
- **THEN** le détail affiche l'absence de réponse avec le message d'erreur
  de connexion, sans code HTTP

#### Scenario: Requête ignorée
- **WHEN** l'exécution de la requête `skip` se termine avec la requête
  ignorée par un script, et que `skip` est sélectionné
- **THEN** le détail affiche que la requête a été ignorée, sans verdict
  d'échec

#### Scenario: Résultat conservé après changement de sélection
- **WHEN** un résultat est affiché pour `ok`, que l'utilisateur sélectionne
  une autre requête puis revient sur `ok`
- **THEN** le même résultat est affiché de nouveau

#### Scenario: Résultat mis à jour par une exécution de dossier
- **WHEN** un résultat existe déjà pour `folder/down`, et une exécution
  récursive du dossier parent produit un nouveau résultat pour cette même
  requête
- **THEN** le résultat affiché pour `folder/down` est celui de cette
  exécution la plus récente

### Requirement: Marqueur de statut dans l'arbre
Une requête pour laquelle un résultat est disponible SHALL porter dans
l'arbre un marqueur de succès ou d'échec correspondant au verdict de ce
résultat, visuellement distinct du marqueur utilisé pour un fichier `.bru`
invalide. Une requête sans résultat disponible MUST NOT porter de
marqueur de statut d'exécution.

#### Scenario: Marqueur après une exécution réussie
- **WHEN** l'exécution de `green` se termine sans échec
- **THEN** la ligne de `green` dans l'arbre porte un marqueur de succès

#### Scenario: Marqueur après un échec
- **WHEN** l'exécution de `ok` se termine avec une assertion en échec
- **THEN** la ligne de `ok` dans l'arbre porte un marqueur d'échec,
  distinct du marqueur d'un fichier `.bru` invalide

### Requirement: Issues d'échec classifiées et affichées
Quand une exécution ne produit aucun résultat exploitable, l'interface
SHALL afficher un message qui distingue explicitement : le programme
`bru` introuvable ; `bru` lancé mais n'ayant produit aucun rapport, avec
les éléments de diagnostic disponibles ; un rapport produit mais non
conforme, sans jamais en afficher le contenu ; et une exécution annulée.
Ce message MUST rester visible jusqu'au lancement d'une exécution
suivante.

#### Scenario: Binaire bru introuvable
- **WHEN** le programme `bru` ne peut pas être lancé
- **THEN** l'interface affiche un message indiquant que `bru` est
  introuvable, distinct des autres messages d'échec

#### Scenario: Aucun rapport produit
- **WHEN** `bru` se termine sans produire de rapport
- **THEN** l'interface affiche un message distinct l'indiquant

#### Scenario: Rapport non conforme
- **WHEN** `bru` produit un rapport non conforme au modèle attendu
- **THEN** l'interface affiche un message l'indiquant, sans afficher le
  contenu du rapport

### Requirement: Aucune interpolation ni écriture
Le système MUST NOT interpréter lui-même les variables, l'environnement,
les scripts, les tests ou les assertions d'une exécution : toute
l'exécution SHALL être déléguée à `bru-runner` sans surcharge de variable
ni sélection d'environnement fournie par ce changement. Aucune valeur de
la requête, de la réponse ou d'un résultat MUST NOT être écrite sur disque
ni journalisée par l'interface.

#### Scenario: Exécution sans environnement
- **WHEN** une exécution est lancée depuis l'arbre
- **THEN** elle est lancée sans nom d'environnement ni surcharge de
  variable

#### Scenario: Aucune trace disque du résultat
- **WHEN** une exécution se termine, avec ou sans succès
- **THEN** aucun fichier contenant tout ou partie de son résultat n'existe
  ni n'a existé dans le système de fichiers en dehors de ce que
  `bru-runner` garantit déjà
