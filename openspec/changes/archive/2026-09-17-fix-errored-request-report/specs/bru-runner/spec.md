## MODIFIED Requirements

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
