## Context

- `src/main.rs` initialise le terminal une seule fois (`ratatui::try_init`
  : mode brut + écran alternatif) et ne le restaure
  (`ratatui::try_restore`) qu'à la sortie du programme. La capture
  souris (`TerminalMouseCapture`) est gérée séparément, activée/désactivée
  indépendamment du mode brut.
- La boucle principale (`src/app/mod.rs::run`) est strictement pilotée
  par messages : `events.recv().await` bloque jusqu'au prochain
  événement, `update` produit un `Command`, la boucle l'exécute, puis
  appelle inconditionnellement `terminal.draw(...)` avant de reboucler.
  Les commandes existantes qui font de l'I/O (`CopyToClipboard`,
  `SaveEdit`, `ResolveSecrets`) passent par `tokio::task::spawn_blocking`
  et renvoient leur résultat plus tard via un `AppEvent` — la boucle,
  elle, continue immédiatement et redessine sans attendre.
- `src/app/event.rs::spawn_terminal_reader` lance un thread OS dédié qui
  boucle indéfiniment sur `crossterm::event::read()` (appel bloquant) et
  transmet chaque événement via un canal `mpsc`. Rien ne permet
  aujourd'hui de le mettre en pause : il lit en continu depuis l'entrée
  standard.
- `Environment.variables`/écriture `.bru` n'entrent pas en jeu ici : ce
  changement ne touche à aucun fichier de la collection, seulement à un
  fichier temporaire hors collection.

Voir `proposal.md` pour la motivation.

## Goals / Non-Goals

**Goals :**
- Céder tout le terminal (entrée et affichage) à un processus externe
  plein écran (`nano` ou équivalent) le temps qu'il tourne, sans
  corruption d'affichage ni vol de frappes clavier, puis rendre la main
  proprement à l'interface.

**Non-Goals :**
- Relire le contenu édité (voir proposal.md — consultation à sens
  unique).
- Généraliser un mécanisme de « suspension du terminal » réutilisable
  pour d'autres fonctionnalités futures : la solution ci-dessous reste
  scopée à ce seul raccourci ; une généralisation, si un futur besoin
  similaire apparaît, sera un changement à part.

## Decisions

### D1 — Exécution synchrone dans la boucle `run`, en dérogation ciblée au principe « la boucle ne bloque jamais »
`CLAUDE.md` pose que « la boucle d'événements ne bloque jamais », et
l'explique par l'exemple : « les exécutions `bru run` passent par un
`mpsc::channel` ». Cette règle protège contre le blocage de l'interface
par une opération lente que l'utilisateur n'a pas explicitement choisi
d'attendre plein écran (requête réseau, écriture disque, résolution de
secrets).

Ouvrir un éditeur externe plein écran est catégoriquement différent :
l'utilisateur demande explicitement que l'interface s'efface et que le
terminal soit entièrement occupé par l'éditeur jusqu'à ce qu'il le
ferme — exactement le même contrat que `git commit` ouvrant `$EDITOR`,
ou l'échappement shell (`:!`) d'un pager. Continuer à redessiner
`bruno-tui` pendant que l'éditeur est ouvert serait un bug (deux
programmes écrivant sur le même terminal), pas un respect renforcé du
principe.

Décision : ce `Command` est donc le seul à être exécuté **de façon
bloquante, directement dans la boucle `run`** (via
`tokio::task::spawn_blocking(...).await`, pas de `tokio::spawn` séparé ni
de retour immédiat), avant le `terminal.draw` de fin de tour. Aucun
autre message n'est traité pendant ce temps — c'est le comportement
voulu, pas un effet de bord toléré.

**Alternative écartée** : garder le style fire-and-forget
(`spawn_blocking` + `AppEvent` de retour, boucle continuant à redessiner
entre-temps) — rejetée, ferait dessiner `bruno-tui` par-dessus (ou
en-dessous de) l'éditeur externe sur le même terminal, un bug garanti
plutôt qu'une amélioration de réactivité (il n'y a rien d'utile à faire
tourner pendant que l'éditeur a le terminal).

### D2 — Mise en pause du thread de lecture du terminal
`spawn_terminal_reader` appelle `crossterm::event::read()` en boucle
bloquante sur l'entrée standard. Si ce thread continue de tourner
pendant que l'éditeur externe est aussi en train de lire l'entrée
standard (stdio hérité du processus enfant), les deux lecteurs se
disputent les mêmes octets : des frappes destinées à l'éditeur seraient
aléatoirement consommées par `bruno-tui`, et inversement.

Décision : remplacer la boucle par un `crossterm::event::poll(court
délai)` suivi d'un `event::read()` seulement si `poll` rapporte un
événement **et** qu'un indicateur partagé (`Arc<AtomicBool>`, ou
équivalent) signale que le terminal n'est pas suspendu ; suspendu, le
thread attend sans jamais appeler `read()`/`poll()` sur le descripteur,
le laissant entièrement disponible pour le processus enfant. La commande
d'ouverture d'éditeur positionne cet indicateur avant de restaurer le
terminal, spawn le processus enfant, attend sa fin, réinitialise le
terminal, puis efface l'indicateur.

**Alternative écartée** : tuer puis relancer le thread à chaque
ouverture d'éditeur — rejetée, plus complexe (gestion de `JoinHandle`,
re-création du canal) pour le même résultat qu'un simple indicateur
partagé, et plus risqué en cas de fermeture anormale de l'éditeur.

### D3 — Réutilisation de `ratatui::try_init`/`try_restore` pour la suspension
Ces deux fonctions font déjà exactement ce qu'il faut à l'ouverture et à
la fermeture du programme (mode brut, écran alternatif) et sont conçues
pour être rappelées en cours de session (usage documenté de ratatui pour
les échappements shell). Décision : les réutiliser telles quelles pour
suspendre puis reprendre, plutôt que dupliquer les appels bas niveau
`crossterm` (`enable_raw_mode`/`EnterAlternateScreen` etc.). La capture
souris (`TerminalMouseCapture`, gérée séparément) est désactivée avant
la suspension si elle était active, et réactivée après si elle l'était
— même lecture de `model.mouse.capture` que pour `Command::SetMouseCapture`
déjà existant. Si `try_init`/`try_restore` se révèlent, à l'implémentation,
mal adaptés à un second appel (comportement non documenté rencontré en
pratique), repli sur les appels `crossterm` directs — détail
d'implémentation, ne change ni la spec ni l'approche.

### D4 — Fichier temporaire : emplacement et cycle de vie
Le corps est écrit dans un fichier temporaire du système
(`std::env::temp_dir()`), nommé de façon non prévisible (pas un nom fixe
réutilisable entre deux ouvertures), supprimé dès la fermeture de
l'éditeur, réussite ou non. Ce n'est pas un secret au sens de
`CLAUDE.md`, mais le principe qui interdit d'écrire un secret sur disque
motive la même prudence par défaut : suppression systématique plutôt que
laissée à un nettoyage différé.

### D5 — Raccourci `Ctrl+E`, global, hors capture de texte
`Ctrl+C`/`Ctrl+X`/`Ctrl+S` sont déjà interceptés avant la répartition
par capture de texte (`key_message` dans `src/app/message.rs`) : un
`Ctrl+lettre` n'est jamais un caractère tapé dans un champ. `Ctrl+E` (
libre) suit le même chemin, donc reste actif même pendant une saisie de
champ de requête en cours — cohérent avec `Ctrl+S`, qui sauvegarde déjà
une requête en cours d'édition sans l'annuler. Aucune interaction avec
une session d'édition de requête (`EditSession`) : ce raccourci ne lit
que le résultat d'exécution déjà affiché par le panneau Réponse, jamais
l'état d'une saisie.

## Risks / Trade-offs

- [Un éditeur externe mal configuré (`$EDITOR` invalide, binaire
  absent) échoue au lancement] → Mitigation : l'échec du processus
  enfant (impossible à démarrer) est signalé par un message de statut,
  le terminal est immédiatement restauré (pas d'état intermédiaire
  bloqué), le fichier temporaire est tout de même supprimé.
- [L'éditeur externe se termine anormalement (tué, terminal fermé)] →
  Mitigation : le code qui restaure le terminal et efface l'indicateur
  de suspension s'exécute après l'attente du processus quel que soit son
  statut de sortie, jamais seulement sur un succès.
- [Dérogation au principe « la boucle ne bloque jamais »] → Documentée
  explicitement ci-dessus (D1) plutôt que silencieuse ; strictement
  scopée à ce `Command` unique, aucun autre traitement bloquant introduit
  ailleurs dans la boucle.
- [Redessin après un éditeur ayant laissé le terminal dans un état
  inattendu (taille changée pendant la suspension, par exemple)] →
  Mitigation : un redessin complet (pas un diff incrémental) est forcé
  après la reprise, sur le même principe qu'un redimensionnement de
  terminal déjà géré ailleurs dans l'application.
