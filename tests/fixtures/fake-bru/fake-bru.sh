#!/bin/sh
# Faux `bru` pour tester le runner sans Node ni réseau.
#
# Appelé comme `bru run <mode> [paramètre] ... --reporter-json /dev/fd/3` :
# le mode et son paramètre sont passés comme cibles de la requête, ce qui
# évite de modifier l'environnement du processus de test.
#
#   report <fichier>       écrit <fichier> sur le fd 3, sort en 1
#   slow-report <fichier>  attend 1 s puis agit comme `report`
#   none                   n'écrit aucun rapport, sort en 0 (hors collection)
#   invalid                écrit du JSON non conforme sur le fd 3, sort en 0
#   path-not-found         écrit « Path not found » sur stdout, sort en 5
#   sleep <fichier-pid>    écrit son PID dans <fichier-pid> puis dort ;
#                          `exec` garde le PID du processus lancé
#   ok, green, json, skip, folder/down (avec ou sans extension `.bru`)
#                          cibles réelles de `tests/fixtures/collections/
#                          runner-probe/` : sert `tests/fixtures/reports/
#                          mixed.json`, sort en 1. `tests/app_run.rs` ne
#                          fournit qu'une seule cible (le nœud sélectionné
#                          dans l'arbre), sans second argument possible.
#   secret, secret.bru     cible de `tests/fixtures/collections/secret-probe/` :
#                          sert `mixed.json` et sort en 1 si les arguments
#                          contiennent `--env-var oktaClientSecret=fixture-value`,
#                          sinon sort en 5 sans rapport. N'affiche jamais les
#                          arguments reçus.

[ "$1" = "run" ] && shift
MODE="$1"
PARAM="$2"

case "$MODE" in
  report)
    cat "$PARAM" >&3
    exit 1
    ;;
  slow-report)
    sleep 1
    cat "$PARAM" >&3
    exit 1
    ;;
  none)
    echo "You can run only at the root of a collection"
    exit 0
    ;;
  invalid)
    printf '[{"iterationIndex": "zero"}]' >&3
    exit 0
    ;;
  path-not-found)
    echo "Path not found: $MODE"
    exit 5
    ;;
  sleep)
    echo $$ > "$PARAM"
    exec sleep 60
    ;;
  ok|ok.bru|green|green.bru|json|json.bru|skip|skip.bru|folder/down|folder/down.bru)
    cat "$(dirname "$0")/../reports/mixed.json" >&3
    exit 1
    ;;
  secret|secret.bru)
    previous=""
    for arg in "$@"; do
      if [ "$previous" = "--env-var" ] && [ "$arg" = "oktaClientSecret=fixture-value" ]; then
        cat "$(dirname "$0")/../reports/mixed.json" >&3
        exit 1
      fi
      previous="$arg"
    done
    echo "variable secrète absente"
    exit 5
    ;;
  *)
    echo "mode inconnu : $MODE" >&2
    exit 99
    ;;
esac
