# bru-writer Specification

## Purpose

Sérialiser une modification de champ vers le fichier `.bru` d'une requête,
en ne réécrivant que les tranches de source correspondant aux champs
édités, avec une écriture atomique et un refus explicite si le fichier a
changé sur disque depuis son chargement.

## Requirements

### Requirement: Champs éditables
Le système SHALL accepter des modifications sur les champs suivants d'une
requête déjà chargée, chacun désigné par son bloc et, pour un en-tête ou un
paramètre, son indice dans la liste exposée par `bru-parser` : l'URL du
bloc de méthode ; la valeur ou l'état activé/désactivé d'une entrée déjà
présente dans `headers`, `params:query` ou `params:path` ; le contenu
d'un bloc de corps texte déjà présent (`body:json`, `body:text`,
`body:xml`, `body:sparql`, `body:graphql`). Le système MUST NOT permettre
d'ajouter ou de supprimer une entrée, de renommer une clé existante, de
changer le type de corps déclaré, d'éditer un corps de forme formulaire
(`body:form-urlencoded`, `body:multipart-form`, `body:file`), ni de
changer le mode d'auth déclaré.

#### Scenario: Modification de l'URL
- **WHEN** l'appelant demande de remplacer l'URL d'une requête GET par une
  nouvelle valeur
- **THEN** le fichier réécrit contient la nouvelle URL à la place de
  l'ancienne, et les clés `body` et `auth` du même bloc restent inchangées

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

### Requirement: Fidélité du reste du fichier
Le système SHALL réécrire le fichier de sorte que tout ce qui n'a pas été
demandé en modification reste identique à l'octet près : les blocs non
concernés, les entrées non éditées d'un bloc édité, les blocs inconnus,
l'ordre des blocs, les lignes vides, et les fins de ligne. Une entrée
dictionnaire modifiée MUST conserver sa clé et son préfixe `~` d'origine
sauf demande explicite de changement de cet état, et MUST être réécrite
avec l'indentation de deux espaces du bloc. Une valeur contenant un saut
de ligne MUST être réécrite au format `'''` multi-ligne avec ses lignes
intérieures indentées de quatre espaces, de sorte qu'un rechargement du
fichier réécrit expose la même valeur.

#### Scenario: Aucune modification demandée
- **WHEN** l'appelant sérialise une requête sans fournir aucune
  modification
- **THEN** le fichier produit est identique à l'octet près au fichier
  d'origine

#### Scenario: Une seule entrée modifiée parmi plusieurs
- **WHEN** un bloc `headers` contient trois en-têtes et seul le second est
  modifié
- **THEN** les lignes du premier et du troisième en-tête, espacement
  compris, restent identiques à l'octet près dans le fichier réécrit

#### Scenario: Bloc inconnu préservé
- **WHEN** un fichier contient un bloc non reconnu par `bru-parser` et que
  l'URL de la requête est modifiée
- **THEN** le bloc inconnu apparaît, identique à l'octet près, à la même
  position dans le fichier réécrit

#### Scenario: Valeur multi-ligne après édition
- **WHEN** l'appelant remplace la valeur d'un paramètre de requête par un
  texte contenant deux sauts de ligne
- **THEN** le fichier réécrit encode cette valeur entre `'''`, et recharger
  ce fichier avec `bru-parser` expose une valeur identique à celle fournie

#### Scenario: Fins de ligne préservées
- **WHEN** un fichier utilise des fins de ligne `\r\n` et qu'un en-tête y
  est modifié
- **THEN** la ligne réécrite se termine par `\r\n`, comme le reste du
  fichier

### Requirement: Refus d'écrire un fichier modifié entre-temps
Le système SHALL exiger un instantané (taille et date de modification) du
fichier capturé au moment du chargement. Avant d'écrire, il MUST comparer
cet instantané à l'état actuel du fichier sur disque et refuser l'écriture,
sans modifier le fichier, si l'un des deux diffère. L'erreur retournée
MUST nommer le fichier concerné, sans révéler le contenu du fichier.

#### Scenario: Fichier modifié par un tiers pendant l'édition
- **WHEN** le fichier a été modifié sur disque après le chargement de son
  instantané et avant l'appel d'écriture
- **THEN** l'écriture échoue avec une erreur nommant le fichier, et le
  contenu du fichier sur disque reste celui du tiers

#### Scenario: Fichier inchangé
- **WHEN** le fichier sur disque est resté identique en taille et en date
  de modification depuis l'instantané
- **THEN** l'écriture procède normalement

### Requirement: Écriture atomique et permissions préservées
Le système SHALL écrire le nouveau contenu dans un fichier temporaire situé
dans le même répertoire que le fichier cible, puis le rendre visible par un
renommage sur le fichier cible. Le fichier temporaire MUST recevoir les
mêmes permissions Unix que le fichier d'origine avant d'être rendu visible.
Le système MUST NOT laisser de fichier temporaire résiduel en cas de succès,
et MUST NOT avoir modifié le fichier cible si l'écriture échoue avant le
renommage.

#### Scenario: Permissions préservées
- **WHEN** le fichier cible a les permissions `0640` et une modification y
  est écrite
- **THEN** le fichier résultant porte les permissions `0640`

#### Scenario: Échec avant renommage
- **WHEN** l'écriture du fichier temporaire échoue (disque plein, par
  exemple)
- **THEN** le fichier cible n'est pas modifié et aucun fichier temporaire
  ne subsiste dans le répertoire

#### Scenario: Aucun résidu après un succès
- **WHEN** une écriture réussit
- **THEN** le répertoire ne contient, pour ce fichier, que le fichier
  cible ; aucun fichier temporaire ne subsiste

### Requirement: Aucun secret journalisé
Le système MUST NOT écrire dans un message d'erreur, un log ou toute sortie
de diagnostic la valeur d'un champ édité ou d'un champ existant du fichier.
Les erreurs MUST se limiter à nommer le fichier, le bloc et, le cas
échéant, l'indice ou le type de champ concerné.

#### Scenario: Erreur sur un en-tête sensible
- **WHEN** l'appelant tente une modification invalide sur un en-tête dont
  la valeur actuelle est un jeton secret
- **THEN** ni le message d'erreur ni sa représentation de déboguage ne
  contiennent la valeur de ce jeton

### Requirement: Écriture derrière une abstraction interchangeable
Le système SHALL exposer l'écriture derrière une interface unique qui
accepte un chemin de fichier, l'AST chargé, l'instantané de fraîcheur et
une liste de modifications, et retourne le nouvel instantané ou une erreur,
de sorte qu'une implémentation de substitution puisse être utilisée dans
les tests sans écrire réellement sur disque.

#### Scenario: Implémentation de substitution
- **WHEN** une implémentation de test enregistre les modifications reçues
  sans toucher au disque
- **THEN** le code appelant fonctionne à l'identique avec cette
  implémentation
