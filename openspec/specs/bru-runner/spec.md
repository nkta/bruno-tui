# bru-runner Specification

## Purpose

Exécuter des requêtes et des campagnes Bruno en déléguant intégralement à
`bru run`, et restituer au reste de l'application un rapport typé et une
issue d'exécution sans ambiguïté, sans jamais bloquer l'interface.

## Requirements

### Requirement: Délégation de l'exécution à bru run
Le système SHALL exécuter toute requête ou campagne exclusivement en
lançant la commande `bru run` avec un rapport JSON demandé, depuis le
répertoire racine de la collection. Il MUST transmettre à `bru` les cibles
(une ou plusieurs requêtes ou dossiers, chemins relatifs à la racine), le
mode récursif, le nom d'environnement et les surcharges de variables
fournis par l'appelant, et MUST NOT interpréter lui-même les variables,
scripts, tests ou assertions.

#### Scenario: Exécution d'un dossier en récursif avec environnement
- **WHEN** l'appelant demande l'exécution du dossier `users` en récursif
  avec l'environnement `local` sur la collection située en `/c`
- **THEN** le système lance `bru run users -r --env local` avec un rapport
  JSON demandé, le répertoire de travail étant `/c`

#### Scenario: Collection entière
- **WHEN** l'appelant demande l'exécution sans cible explicite en récursif
- **THEN** le système lance `bru run -r` à la racine de la collection

### Requirement: Exécution non bloquante
Le système SHALL lancer `bru` de façon asynchrone et rendre la main à
l'appelant immédiatement. L'issue de chaque exécution MUST être délivrée
comme un message unique sur un canal fourni par l'appelant, portant
l'identifiant de l'exécution qui l'a produite.

#### Scenario: Lancement sans attente
- **WHEN** l'appelant lance une exécution dont `bru` met plusieurs
  secondes à terminer
- **THEN** l'appel de lancement retourne sans attendre la fin de `bru`
- **AND** un message unique contenant l'identifiant de l'exécution et son
  issue est reçu sur le canal à la fin de `bru`

#### Scenario: Exécutions concurrentes
- **WHEN** deux exécutions sont lancées l'une après l'autre
- **THEN** chaque message reçu porte l'identifiant de l'exécution
  correspondante, quel que soit l'ordre de fin

### Requirement: Rapport jamais persisté sur disque
Le rapport JSON produit par `bru` contient requêtes, en-têtes et corps de
réponse, donc potentiellement des secrets. Le système MUST récupérer ce
rapport sans qu'aucun fichier régulier le contenant ne soit créé, même
temporairement, et MUST NOT écrire dans les logs ni sur disque le rapport,
la sortie de `bru` ou les valeurs de surcharge de variables.

#### Scenario: Aucun fichier de rapport résiduel
- **WHEN** une exécution se termine, avec succès, en échec ou annulée
- **THEN** aucun fichier régulier contenant tout ou partie du rapport
  n'existe ni n'a existé dans le système de fichiers

#### Scenario: Surcharge de variable sensible
- **WHEN** l'appelant fournit la surcharge `token=s3cr3t`
- **THEN** la valeur `s3cr3t` n'apparaît dans aucune sortie de log de
  l'application

### Requirement: Modèle de rapport typé
Le système SHALL désérialiser le rapport en un modèle typé comprenant :
la liste des itérations ; pour chaque itération, ses résultats de requête
et son résumé ; pour chaque résultat, le nom, le chemin et le fichier de
la requête, la requête envoyée (méthode, URL, en-têtes), la réponse
(statut, texte de statut, en-têtes, corps, temps de réponse), l'erreur
éventuelle, le statut rapporté par `bru`, les résultats d'assertions, de
tests, de tests pré-requête et post-réponse, et la durée. La méthode,
l'URL et les en-têtes de la requête MUST être optionnels : quand la
requête n'a pas été envoyée (échec avant envoi, par exemple un script
pré-requête qui lève une exception), `bru` les rapporte à `null`, et la
désérialisation MUST réussir en les exposant comme absents. Le statut de
réponse MUST distinguer un code HTTP numérique, une réponse en erreur
(aucune réponse reçue) et une requête ignorée (sautée par un script). Le
statut rapporté par `bru` MUST distinguer au moins `pass`, `fail`, `error`
et `skipped`, et une valeur inconnue MUST être conservée comme telle sans
faire échouer la désérialisation. Le corps de réponse MUST être conservé quelle que
soit sa forme JSON (texte, objet, tableau, nul). Les champs supplémentaires
inconnus MUST être ignorés sans faire échouer la désérialisation.

#### Scenario: Réponse HTTP reçue
- **WHEN** le rapport contient un résultat dont `response.status` vaut `200`
- **THEN** le modèle expose un statut HTTP 200 avec ses en-têtes et son
  corps

#### Scenario: Aucune réponse reçue
- **WHEN** le rapport contient un résultat dont `response.status` vaut
  `"error"` et `error` vaut `"connect ECONNREFUSED 127.0.0.1:18799"`
- **THEN** le modèle expose une réponse en erreur, sans code HTTP, et le
  message d'erreur associé

#### Scenario: Requête en échec avant envoi
- **WHEN** le rapport contient un résultat de statut `error` dont
  `request.method`, `request.url`, `request.headers` et `request.data`
  valent `null`, dont `response.status` vaut `"error"` et dont `error`
  vaut le message levé par le script pré-requête
- **THEN** la désérialisation du rapport entier réussit
- **AND** le modèle expose une requête sans méthode, sans URL ni
  en-têtes, une réponse en erreur sans code HTTP, et le message d'erreur
- **AND** le verdict de ce résultat est « en échec »

#### Scenario: Autres résultats d'une campagne contenant un échec avant envoi
- **WHEN** le rapport contient, à côté d'un résultat en échec avant
  envoi, des résultats de requêtes envoyées normalement
- **THEN** ces autres résultats restent exposés avec leur méthode, leur
  URL et leurs en-têtes

#### Scenario: Requête ignorée par script
- **WHEN** le rapport contient un résultat de statut `skipped` dont
  `response.status` vaut `"skipped"`
- **THEN** le modèle expose une requête ignorée, sans code HTTP ni erreur

#### Scenario: Corps de réponse JSON
- **WHEN** `response.data` est l'objet `{"a":[1,2],"b":null}`
- **THEN** le modèle conserve cet objet à l'identique

#### Scenario: Assertion en échec
- **WHEN** le rapport contient une assertion `res.status: eq 404` en échec
  avec l'erreur `expected 200 to equal 404`
- **THEN** le modèle expose l'expression, l'opérateur, l'opérande, le
  statut en échec et le message d'erreur

#### Scenario: Champ inconnu ajouté par une version de bru
- **WHEN** le rapport contient un champ absent du modèle
- **THEN** la désérialisation réussit et le champ est ignoré

### Requirement: Verdict d'échec par requête
Le système SHALL fournir pour chaque résultat de requête un verdict
« en échec » vrai si et seulement si la requête est en erreur, ou si au
moins une assertion, un test, un test pré-requête ou un test post-réponse
est en échec. Ce verdict MUST NOT reposer sur le seul statut global du
résultat, que `bru` rapporte à `pass` même lorsque des assertions ou tests
échouent.

#### Scenario: Statut pass avec assertion en échec
- **WHEN** un résultat a le statut `pass` et une assertion en échec
- **THEN** le verdict de ce résultat est « en échec »

#### Scenario: Test post-réponse en échec
- **WHEN** un résultat a le statut `pass`, aucune assertion ni test, et un
  test post-réponse en échec
- **THEN** le verdict de ce résultat est « en échec »

#### Scenario: Requête ignorée
- **WHEN** un résultat a le statut `skipped` sans erreur ni test en échec
- **THEN** le verdict de ce résultat n'est pas « en échec »

#### Scenario: Requête entièrement réussie
- **WHEN** un résultat n'a pas d'erreur et toutes ses assertions et tous
  ses tests sont en succès
- **THEN** le verdict de ce résultat n'est pas « en échec »

### Requirement: Issue d'exécution classifiée
Le système SHALL délivrer pour chaque exécution exactement une issue
parmi :
- **Terminée** : un rapport valide a été obtenu ; l'issue porte le rapport
  et le code de sortie de `bru`, qu'il soit nul ou non (des échecs de
  tests produisent un code non nul et restent une issue Terminée) ;
- **bru introuvable** : le binaire `bru` n'a pas pu être lancé ;
- **Aucun rapport** : `bru` s'est terminé sans produire de rapport (chemin
  inexistant, répertoire qui n'est pas une racine de collection, erreur
  fatale) ; l'issue porte le code de sortie et la sortie de `bru` à des
  fins de diagnostic affiché ;
- **Rapport invalide** : un rapport a été produit mais ne correspond pas
  au modèle ; l'issue porte la description de l'erreur de
  désérialisation, sans le contenu du rapport ;
- **Annulée** : l'exécution a été annulée par l'appelant.

#### Scenario: Échecs de tests
- **WHEN** `bru` termine avec le code 1 et produit un rapport valide
  contenant des tests en échec
- **THEN** l'issue est Terminée avec le rapport et le code de sortie 1

#### Scenario: Binaire absent
- **WHEN** `bru` n'est pas trouvable dans le `PATH`
- **THEN** l'issue est « bru introuvable »

#### Scenario: Cible inexistante
- **WHEN** la cible demandée n'existe pas et `bru` termine avec le code 5
  sans rapport
- **THEN** l'issue est « Aucun rapport » avec le code 5 et la sortie de
  `bru` contenant `Path not found`

#### Scenario: Répertoire hors collection
- **WHEN** le répertoire fourni n'est pas une racine de collection et
  `bru` termine avec le code 0 sans rapport
- **THEN** l'issue est « Aucun rapport » et non Terminée

#### Scenario: JSON non conforme
- **WHEN** `bru` produit un rapport qui n'est pas conforme au modèle
- **THEN** l'issue est « Rapport invalide »

### Requirement: Annulation d'une exécution
Le système SHALL permettre à l'appelant d'annuler une exécution en cours.
L'annulation MUST terminer le processus `bru` et ses ressources associées,
et l'issue délivrée MUST être Annulée, sans rapport partiel.

#### Scenario: Annulation d'une campagne longue
- **WHEN** l'appelant annule une exécution dont `bru` est encore en cours
- **THEN** le processus `bru` est terminé
- **AND** l'issue délivrée pour cette exécution est Annulée

#### Scenario: Abandon du lanceur
- **WHEN** le composant ayant lancé l'exécution est détruit avant la fin
- **THEN** le processus `bru` est terminé et ne subsiste pas

### Requirement: Couverture par fixtures réelles
Chaque type du modèle de rapport MUST être couvert par au moins un test de
désérialisation utilisant un rapport produit par un vrai `bru run`, stocké
dans les fixtures du dépôt avec la version de `bru` qui l'a produit. Chaque
forme de résultat que le modèle distingue — y compris une requête en échec
avant envoi — MUST provenir d'un tel rapport, régénérable depuis une
collection de fixture du dépôt, et jamais d'un JSON écrit à la main.

#### Scenario: Fixture de campagne mixte
- **WHEN** les tests s'exécutent sur les fixtures contenant une requête en
  erreur de connexion, une requête ignorée, un corps JSON, des assertions,
  des tests, des tests pré-requête et post-réponse en succès et en échec
- **THEN** la désérialisation réussit et chaque champ du modèle est
  vérifié contre les valeurs de la fixture

#### Scenario: Fixture d'échec avant envoi
- **WHEN** les tests s'exécutent sur la fixture produite par un vrai
  `bru run` d'une collection de fixture dont le script pré-requête lève
  une exception
- **THEN** la désérialisation réussit et la requête absente, la réponse
  en erreur, le statut `error`, le message d'erreur et le résumé sont
  vérifiés contre les valeurs de la fixture
