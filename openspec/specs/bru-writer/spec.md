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
paramètre, son indice dans la liste courante du bloc : l'URL du bloc de
méthode ; la valeur ou l'état activé/désactivé d'une entrée de `headers`,
`params:query` ou `params:path` ; l'ajout d'une entrée à l'un de ces trois
blocs, la suppression d'une de leurs entrées et le renommage de la clé
d'une de leurs entrées ; le contenu d'un bloc de corps texte déjà présent
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

### Requirement: Fidélité du reste du fichier
Le système SHALL réécrire le fichier de sorte que tout ce qui n'a pas été
demandé en modification reste identique à l'octet près : les blocs non
concernés, les entrées non éditées d'un bloc édité, les blocs inconnus,
l'ordre des blocs, les lignes vides, et les fins de ligne. Une entrée
dictionnaire modifiée MUST conserver sa clé et son préfixe `~` d'origine
sauf demande explicite de renommage ou de changement d'état, et MUST être
réécrite avec l'indentation de deux espaces du bloc. Une valeur d'en-tête
ou de paramètre de chemin contenant un saut de ligne MUST être réécrite au
format `'''` multi-ligne avec ses lignes intérieures indentées de quatre
espaces, de sorte qu'un rechargement du fichier réécrit expose la même
valeur. Toute ligne écrite par le système (entrée modifiée, entrée
ajoutée, bloc créé) MUST utiliser la fin de ligne dominante du fichier.

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
- **WHEN** l'appelant remplace la valeur d'un en-tête par un texte
  contenant deux sauts de ligne
- **THEN** le fichier réécrit encode cette valeur entre `'''`, et recharger
  ce fichier avec `bru-parser` expose une valeur identique à celle fournie

#### Scenario: Fins de ligne préservées
- **WHEN** un fichier utilise des fins de ligne `\r\n` et qu'un en-tête y
  est modifié, puis qu'un autre y est ajouté
- **THEN** la ligne réécrite et la ligne ajoutée se terminent par `\r\n`,
  comme le reste du fichier

### Requirement: Refus d'écrire un fichier modifié entre-temps
Le système SHALL exiger un instantané (taille et date de modification) du
fichier capturé au moment du chargement. Avant d'écrire, il MUST comparer
cet instantané à l'état actuel du fichier sur disque et refuser l'écriture,
sans modifier le fichier, si l'un des deux diffère. Cette règle MUST
s'appliquer à toute écriture, quelle que soit la nature des modifications
demandées (valeur, état, ajout, suppression, renommage, URL, corps).
L'erreur retournée MUST nommer le fichier concerné, sans révéler le
contenu du fichier.

#### Scenario: Fichier modifié par un tiers pendant l'édition
- **WHEN** le fichier a été modifié sur disque après le chargement de son
  instantané et avant l'appel d'écriture
- **THEN** l'écriture échoue avec une erreur nommant le fichier, et le
  contenu du fichier sur disque reste celui du tiers

#### Scenario: Fichier inchangé
- **WHEN** le fichier sur disque est resté identique en taille et en date
  de modification depuis l'instantané
- **THEN** l'écriture procède normalement

#### Scenario: Ajout refusé sur un fichier modifié entre-temps
- **WHEN** l'appelant demande l'ajout d'un en-tête alors que le fichier a
  été modifié par un tiers depuis l'instantané
- **THEN** l'écriture échoue avec l'erreur de fichier modifié et le
  fichier sur disque ne contient pas l'en-tête demandé

### Requirement: Écriture atomique et permissions préservées
Le système SHALL écrire le nouveau contenu dans un fichier temporaire situé
dans le même répertoire que le fichier cible, puis le rendre visible par un
renommage sur le fichier cible. Le fichier temporaire MUST recevoir les
mêmes permissions Unix que le fichier d'origine avant d'être rendu visible.
Le système MUST NOT laisser de fichier temporaire résiduel en cas de succès,
et MUST NOT avoir modifié le fichier cible si l'écriture échoue avant le
renommage. Une liste de modifications MUST être appliquée en entier ou pas
du tout : si l'une d'elles est refusée, aucune écriture n'a lieu.

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

#### Scenario: Une modification refusée annule toute la liste
- **WHEN** l'appelant fournit l'ajout valide d'un en-tête suivi du
  renommage d'un paramètre vers une clé invalide
- **THEN** une erreur est retournée, le fichier cible est inchangé et ne
  contient pas l'en-tête ajouté

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

### Requirement: Application ordonnée des modifications
Le système SHALL appliquer une liste de modifications dans l'ordre fourni.
L'indice porté par une modification d'entrée MUST désigner la position de
l'entrée dans la liste du bloc telle qu'elle résulte de l'application des
modifications précédentes de la même liste, entrées désactivées comprises.
Sans ajout ni suppression dans la liste, cet indice MUST coïncider avec
l'indice exposé par `bru-parser` pour le fichier chargé.

#### Scenario: Indice après suppression
- **WHEN** un bloc `headers` contient `A`, `B`, `C` et que la liste demande
  de supprimer l'indice 0 puis de modifier la valeur de l'indice 0
- **THEN** le fichier réécrit ne contient plus `A`, et c'est la valeur de
  `B` qui est modifiée

#### Scenario: Modification d'une entrée ajoutée dans la même liste
- **WHEN** un bloc `headers` contient deux en-têtes et que la liste demande
  d'ajouter `X-Id: 1` puis de désactiver l'indice 2
- **THEN** le fichier réécrit contient `~X-Id: 1` en dernière entrée du
  bloc

### Requirement: Ajout d'une entrée
Le système SHALL ajouter une entrée, avec une clé, une valeur (vide
autorisée) et un état activé ou désactivé, à la fin du bloc `headers`,
`params:query` ou `params:path` demandé. L'entrée MUST être écrite comme
une nouvelle ligne indentée de deux espaces, préfixée de `~` si elle est
désactivée, insérée immédiatement après la dernière entrée existante du
bloc, ou juste après sa ligne d'ouverture s'il n'en a aucune. Si le bloc
est absent du fichier, le système MUST le créer en l'insérant, précédé
d'une ligne vide, immédiatement après le dernier bloc présent parmi ceux
qui le précèdent dans l'ordre d'écriture de Bruno : bloc de méthode, puis
`params:query`, puis `params:path`, puis `headers`. Aucun autre octet du
fichier MUST NOT être modifié par un ajout.

#### Scenario: Ajout à un bloc existant
- **WHEN** un bloc `headers` contient trois en-têtes et que l'appelant
  ajoute `X-Trace: abc`
- **THEN** le fichier réécrit contient les trois lignes d'origine à
  l'octet près suivies de la ligne `  X-Trace: abc`, puis la ligne de
  fermeture du bloc

#### Scenario: Création du bloc headers absent
- **WHEN** une requête ne contient ni `params:query`, ni `params:path`, ni
  `headers`, et que l'appelant ajoute l'en-tête `Accept: text/plain`
- **THEN** un bloc `headers` contenant cette seule entrée est inséré,
  précédé d'une ligne vide, juste après le bloc de méthode, et le reste
  du fichier est inchangé

#### Scenario: Création de params:path entre deux blocs existants
- **WHEN** une requête contient un bloc `params:query` puis un bloc
  `headers`, sans `params:path`, et que l'appelant ajoute le paramètre de
  chemin `id: 42`
- **THEN** le bloc `params:path` est inséré après `params:query` et avant
  `headers`

#### Scenario: Ajout d'une entrée désactivée
- **WHEN** l'appelant ajoute au bloc `headers` l'entrée `X-Debug: 1` à
  l'état désactivé
- **THEN** la ligne ajoutée est `  ~X-Debug: 1`

### Requirement: Suppression d'une entrée
Le système SHALL supprimer l'entrée désignée d'un bloc `headers`,
`params:query` ou `params:path`, en retirant exactement ses lignes source
(toutes ses lignes pour une valeur `'''` multi-ligne), fins de ligne
comprises. Si le bloc ne contient plus aucune entrée après application de
la liste, le système MUST retirer le bloc entier ainsi qu'une ligne vide
qui le précède immédiatement, si elle existe, comme Bruno qui n'écrit pas
de bloc vide. Un bloc déjà vide dans le fichier chargé et non touché MUST
rester en place.

#### Scenario: Suppression d'une entrée au milieu d'un bloc
- **WHEN** un bloc `headers` contient trois en-têtes et que l'appelant
  supprime l'indice 1
- **THEN** le fichier réécrit contient les lignes du premier et du
  troisième en-tête à l'octet près, sans la ligne du second

#### Scenario: Suppression de la dernière entrée d'un bloc
- **WHEN** un bloc `params:path` ne contient qu'une entrée et que
  l'appelant la supprime
- **THEN** le bloc `params:path` et la ligne vide qui le précède
  disparaissent du fichier réécrit, et les blocs voisins restent
  identiques à l'octet près

#### Scenario: Suppression d'une valeur multi-ligne
- **WHEN** l'entrée supprimée a une valeur `'''` répartie sur trois lignes
- **THEN** aucune de ses lignes, délimiteurs compris, ne subsiste dans le
  fichier réécrit

### Requirement: Renommage d'une clé
Le système SHALL remplacer la clé de l'entrée désignée d'un bloc
`headers`, `params:query` ou `params:path` par la clé demandée, en
conservant sa valeur et son préfixe `~`. Seules les lignes de cette
entrée MUST être réécrites.

#### Scenario: Renommage d'un en-tête désactivé
- **WHEN** le bloc `headers` contient `~X-Debug: 1` et que l'appelant
  renomme cette entrée en `X-Verbose`
- **THEN** la ligne réécrite est `  ~X-Verbose: 1` et les autres lignes du
  bloc sont inchangées

### Requirement: Validité des clés et clés dupliquées
Le système SHALL refuser, sans écrire, un ajout ou un renommage dont la
clé est vide, contient un espace, une tabulation, un saut de ligne ou le
caractère `:`, ou commence par `~` ou `"`. Pour `params:query`, la clé
MUST en outre ne contenir ni `&`, ni `=`, ni `#`, et la valeur d'un
paramètre de requête ajouté ou modifié MUST ne contenir ni `&`, ni `#`, ni
saut de ligne, faute de quoi la modification est refusée. Une clé déjà
présente dans le bloc MUST être acceptée : les entrées dupliquées sont
autorisées, comme dans Bruno, et restent distinguées par leur indice.
L'erreur de refus MUST nommer le bloc, l'indice le cas échéant, et la
nature du problème, sans citer la clé ni la valeur.

#### Scenario: Clé contenant un espace
- **WHEN** l'appelant ajoute au bloc `headers` une entrée de clé `X Trace`
- **THEN** aucune écriture n'a lieu et une erreur de clé invalide nommant
  le bloc `headers` est retournée

#### Scenario: En-tête dupliqué
- **WHEN** le bloc `headers` contient `Accept: text/plain` et que
  l'appelant ajoute `Accept: application/json`
- **THEN** le fichier réécrit contient les deux lignes `Accept`, et
  recharger le fichier expose deux en-têtes `Accept` distincts

#### Scenario: Valeur de paramètre de requête contenant &
- **WHEN** l'appelant modifie la valeur d'un paramètre de requête en
  `a&b`
- **THEN** aucune écriture n'a lieu et une erreur nommant le bloc
  `params:query` et l'indice est retournée

### Requirement: Cohérence entre params:query et l'URL
Le système SHALL maintenir, à chaque modification qui les touche, la
cohérence entre les entrées de `params:query` et la chaîne de requête de
l'URL, selon les règles de Bruno. La chaîne de requête d'une URL est la
partie située après le premier `?` et avant le premier `#` ; elle se
découpe en paires sur `&`, chaque paire en nom et valeur sur le premier
`=` (valeur vide en l'absence de `=`), les paires de nom vide étant
ignorées ; aucun encodage ni décodage n'est appliqué.

Après toute modification portant sur `params:query` (ajout, suppression,
renommage, valeur, état), le système MUST reconstruire la chaîne de
requête de l'URL à partir des seules entrées activées, dans leur ordre,
sous la forme `nom=valeur` (ou `nom` si la valeur est vide) jointes par
`&`, en conservant la partie de l'URL avant le `?` et le fragment `#`
éventuel ; sans entrée activée, l'URL MUST ne plus porter de `?`.

Après toute modification de l'URL, le système MUST recalculer les entrées
activées de `params:query` à partir de la chaîne de requête de la nouvelle
URL : la i-ème entrée activée prend le nom et la valeur de la i-ème paire,
les paires excédentaires sont ajoutées en fin de bloc comme entrées
activées, les entrées activées excédentaires sont supprimées. Les entrées
désactivées MUST rester inchangées et à leur place. Les règles d'ajout, de
suppression de bloc et de fidélité MUST s'appliquer aux entrées ainsi
touchées ; une entrée ou une URL dont le texte final est identique à
l'original MUST NOT être réécrite. Les modifications de `headers` et de
`params:path` MUST NOT modifier l'URL.

#### Scenario: Ajout d'un paramètre de requête
- **WHEN** l'URL vaut `https://{{host}}/items` sans bloc `params:query`,
  et l'appelant ajoute le paramètre activé `page: 2`
- **THEN** le fichier réécrit porte l'URL `https://{{host}}/items?page=2`
  et un bloc `params:query` contenant `page: 2`

#### Scenario: Paramètre désactivé absent de l'URL
- **WHEN** l'URL vaut `https://h/items?page=2&size=10` et l'appelant
  désactive le paramètre `size`
- **THEN** le fichier réécrit porte l'URL `https://h/items?page=2` et
  l'entrée `~size: 10`

#### Scenario: Suppression du dernier paramètre activé
- **WHEN** l'URL vaut `https://h/items?page=2#top` et l'appelant supprime
  l'unique paramètre de requête `page`
- **THEN** le fichier réécrit porte l'URL `https://h/items#top` et ne
  contient plus de bloc `params:query`

#### Scenario: Modification de l'URL recalculant les paramètres
- **WHEN** le bloc `params:query` contient `page: 2`, `~debug: 1`,
  `size: 10`, et l'appelant remplace l'URL par `https://h/items?page=3`
- **THEN** le bloc réécrit contient, dans cet ordre, `page: 3` et
  `~debug: 1`, et l'entrée `size` a disparu

#### Scenario: Paramètre sans valeur et paramètres dupliqués
- **WHEN** l'appelant remplace l'URL par `https://h/items?tag=a&tag=b&flag`
  sur une requête sans bloc `params:query`
- **THEN** le bloc `params:query` créé contient `tag: a`, `tag: b` et
  `flag:` avec une valeur vide, dans cet ordre

#### Scenario: En-tête sans effet sur l'URL
- **WHEN** l'appelant ajoute un en-tête à une requête dont l'URL et les
  paramètres de requête sont incohérents dans le fichier chargé
- **THEN** l'URL et le bloc `params:query` restent identiques à l'octet
  près

### Requirement: Aperçu des modifications sans écriture
Le système SHALL fournir, sans aucun accès disque, l'état qu'exposerait la
requête après application d'une liste de modifications : URL, en-têtes,
paramètres de requête et de chemin avec leurs clés, valeurs et états, et
corps. Cet aperçu MUST appliquer exactement les mêmes règles que
l'écriture (ordre, validité, synchronisation de l'URL) et MUST retourner
la même erreur que celle que retournerait l'écriture pour une liste
refusée. Pour une liste acceptée, l'aperçu MUST être identique à la vue
qu'expose `bru-parser` en rechargeant le fichier écrit.

#### Scenario: Aperçu conforme au fichier écrit
- **WHEN** une liste contenant un ajout de paramètre de requête, une
  suppression d'en-tête et un renommage de paramètre de chemin est
  prévisualisée, puis écrite, puis le fichier rechargé
- **THEN** l'URL, les en-têtes et les paramètres de l'aperçu sont égaux à
  ceux de la vue rechargée

#### Scenario: Aperçu d'une liste refusée
- **WHEN** une liste contenant un renommage vers une clé vide est
  prévisualisée
- **THEN** l'aperçu retourne l'erreur de clé invalide et aucun fichier
  n'est lu ni écrit

### Requirement: Vérification des fichiers écrits par le parser de Bruno
Le projet SHALL disposer de fixtures d'écriture couvrant l'ajout (bloc
existant, bloc créé), la suppression (entrée isolée, bloc retiré), le
renommage, les clés dupliquées, les entrées désactivées, la synchronisation
`params:query` ↔ URL dans les deux sens et les fins de ligne `\r\n`, chacune
accompagnée du fichier attendu à l'octet près. Chaque fichier attendu MUST
être relu par le parser `.bru` officiel de Bruno livré avec `bru`, et les
URL, en-têtes et paramètres qu'il en extrait MUST correspondre à des
valeurs attendues versionnées à côté de la fixture. Cette vérification
MUST pouvoir être exécutée à la demande et MUST NOT être requise par la
suite de tests par défaut, qui ne dépend pas de Node.

#### Scenario: Fichier attendu relu par Bruno
- **WHEN** la vérification Bruno est lancée sur la fixture d'ajout d'un
  paramètre de requête
- **THEN** le parser de Bruno expose l'URL avec `?page=2` et un paramètre
  de requête `page` activé de valeur `2`, conformément aux valeurs
  attendues versionnées

#### Scenario: Suite par défaut sans Node
- **WHEN** `cargo test` est lancé sur une machine sans `node` ni `bru`
- **THEN** la suite passe, la vérification Bruno étant ignorée
