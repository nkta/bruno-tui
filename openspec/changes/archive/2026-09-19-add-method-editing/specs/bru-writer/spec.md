## MODIFIED Requirements

### Requirement: Champs éditables
Le système SHALL accepter des modifications sur les champs suivants d'une
requête déjà chargée, chacun désigné par son bloc et, pour un en-tête ou un
paramètre, son indice dans la liste courante du bloc : le nom du bloc de
méthode (`GET`, `POST`, `PUT`, `DELETE`, `PATCH`, `OPTIONS`, `HEAD`,
`CONNECT`, `TRACE`) ; l'URL du bloc de méthode ; la valeur ou l'état
activé/désactivé d'une entrée de `headers`, `params:query` ou
`params:path` ; l'ajout d'une entrée à l'un de ces trois blocs, la
suppression d'une de leurs entrées et le renommage de la clé d'une de
leurs entrées ; le contenu d'un bloc de corps texte déjà présent
(`body:json`, `body:text`, `body:xml`, `body:sparql`, `body:graphql`). Le
système MUST NOT permettre d'ajouter, de supprimer ou de renommer une
entrée d'un autre bloc (bloc de méthode, `meta`, `assert`, `vars`, blocs
d'auth, corps de forme formulaire, bloc inconnu), de réordonner des
entrées, de changer le type de corps déclaré, d'éditer un corps de forme
formulaire (`body:form-urlencoded`, `body:multipart-form`, `body:file`), ni
de changer le mode d'auth déclaré.

#### Scenario: Modification de l'URL
- **WHEN** l'appelant demande de remplacer l'URL d'une requête GET sans
  chaîne de requête par une nouvelle valeur sans chaîne de requête
- **THEN** le fichier réécrit contient la nouvelle URL à la place de
  l'ancienne, et les clés `body` et `auth` du même bloc restent inchangées

#### Scenario: Changement de méthode
- **WHEN** l'appelant demande de remplacer la méthode `GET` d'une requête
  par `POST`
- **THEN** le fichier réécrit ouvre le bloc par `post {` à la place de
  `get {`, et tout le reste du bloc (URL, en-têtes, corps) reste
  identique à l'octet près

#### Scenario: Désactivation d'un en-tête existant
- **WHEN** l'appelant demande de désactiver l'en-tête d'indice 0 d'une
  requête dont cet en-tête est actuellement activé
- **THEN** le fichier réécrit porte cet en-tête préfixé de `~`, avec la
  même clé et la même valeur

#### Scenario: Modification du contenu d'un corps JSON
- **WHEN** l'appelant demande de remplacer le contenu d'un bloc `body:json`
  existant par un nouveau texte JSON
- **THEN** le fichier réécrit contient le nouveau texte, réindenté de deux
  espaces, entre les mêmes lignes d'ouverture et de fermeture du bloc

#### Scenario: En-tête inexistant
- **WHEN** l'appelant demande de modifier la valeur de l'en-tête d'indice 3
  alors que la requête n'en compte que deux
- **THEN** aucune écriture n'a lieu et une erreur nommant le bloc et
  l'indice est retournée

#### Scenario: Corps de forme formulaire refusé
- **WHEN** l'appelant demande de modifier le contenu d'un corps déclaré
  `formUrlEncoded`
- **THEN** aucune écriture n'a lieu et une erreur explicite est retournée

#### Scenario: Ajout refusé hors des trois blocs autorisés
- **WHEN** l'appelant demande d'ajouter une entrée au bloc `assert`
- **THEN** la demande ne peut pas être exprimée ou est refusée, et aucune
  écriture n'a lieu

## ADDED Requirements

### Requirement: Changement de méthode
Le système SHALL remplacer le nom du bloc de méthode par l'une des 9
valeurs reconnues par `bru-parser` (`get`, `post`, `put`, `delete`,
`patch`, `options`, `head`, `connect`, `trace`), en ne réécrivant que la
tranche source de ce nom : le délimiteur d'ouverture (`{`), le contenu du
bloc et tout le reste du fichier MUST rester identiques à l'octet près.
Le système SHALL refuser, sans écrire, une valeur qui n'est pas l'une de
ces 9, sans citer la valeur refusée dans l'erreur. Une valeur identique
au nom de bloc déjà présent MUST NOT être réécrite.

#### Scenario: Renommage du bloc de méthode
- **WHEN** un fichier ouvre son bloc de méthode par `get {` et l'appelant
  demande la méthode `post`
- **THEN** le fichier réécrit ouvre ce bloc par `post {`, avec la même
  casse que les autres blocs de méthode déjà écrits par le système

#### Scenario: Méthode inconnue refusée
- **WHEN** l'appelant demande la méthode `fetch`
- **THEN** aucune écriture n'a lieu et une erreur nommant le bloc de
  méthode, sans citer `fetch`, est retournée

#### Scenario: Méthode inchangée
- **WHEN** l'appelant demande la méthode déjà en place
- **THEN** le fichier réécrit est identique à l'octet près à l'original
  pour ce qui concerne le nom du bloc

#### Scenario: Contenu du bloc préservé
- **WHEN** l'appelant change la méthode d'une requête dont le bloc
  contient une URL, un `body` et un `auth`
- **THEN** ces trois lignes restent identiques à l'octet près dans le
  fichier réécrit, seul le mot-clé d'ouverture du bloc change
