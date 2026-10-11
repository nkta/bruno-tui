# multi-env-probe

Collection de fixture pour les campagnes multi-environnements.

- `up` : `baseUrl` pointe vers `127.0.0.1:18765` (serveur de fixture).
- `down` : `baseUrl` pointe vers `127.0.0.1:18799` (port fermé).

`data` diverge (réussie sur `up`, en erreur sur `down`), `static` réussit
partout (URL fixe), `skip` est ignorée partout.
