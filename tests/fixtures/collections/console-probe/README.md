# Fixture console-probe

Collection minimale servant à vérifier et documenter le comportement de la sortie console de `bru` 4.2.1 lors de l'exécution de scripts (`console.log`).

## Comportement observé avec `bru run` (version 4.2.1)

Lors de l'exécution d'une requête comportant un script avec `console.log("fixture-log")` :
- `console.log(...)` apparaît sur **stdout** (sortie standard), sans aucun préfixe, suivi d'un saut de ligne.
- `console.error(...)` apparaît sur **stderr** (sortie d'erreur).
- `bru run` écrit également sur stdout son tableau récapitulatif d'exécution (`📊 Execution Summary`), ainsi que le statut de chaque requête.
- Lorsque stdout est redirigé vers un tube (pipe anonyme), `bru` ne produit généralement pas de codes d'échappement ANSI, mais ceux-ci sont tout de même nettoyés par précaution dans `bruno-tui`.

Exemple de sortie brute observée sur stdout :
```text
fixture-log
log (200 OK) - 7 ms

📊 Execution Summary
┌───────────────┬──────────────┐
│ Metric        │    Result    │
├───────────────┼──────────────┤
│ Status        │    ✓ PASS    │
├───────────────┼──────────────┤
│ Requests      │ 1 (1 Passed) │
├───────────────┼──────────────┤
│ Tests         │     0/0      │
├───────────────┼──────────────┤
│ Assertions    │     0/0      │
├───────────────┼──────────────┤
│ Duration (ms) │     192      │
└───────────────┴──────────────┘
```

## Sécurité

La console de `bru` peut contenir des secrets ou données sensibles affichées par les scripts utilisateur ou le CLI.
Elle reste **en mémoire vive uniquement** pour la session en cours :
- Jamais écrite sur le disque ;
- Jamais enregistrée dans l'historique persistant (`$XDG_STATE_HOME/bruno-tui/history.json`) ;
- Jamais logguée.
