#!/usr/bin/env bash
# Answers the app's native dialogs on the virtual display by CLICKING the button: the left one
# accepts ("Abrir", "Clicar", ...), the right one refuses, as /modo says. It also keeps a
# screenshot of each dialog's text, so the wording can be read as the person would read it.
# Usage: robo-dialogos.sh <work folder>   (DISPLAY must be the virtual one)
T=$1; n=0
while [ ! -e "$T/parar" ]; do
  for id in $(xdotool search --name '^Navegador do agente$' 2>/dev/null); do
    modo=$(cat "$T/modo.txt" 2>/dev/null || echo aceitar)
    eval "$(xdotool getwindowgeometry --shell "$id" 2>/dev/null)" || continue
    [ -n "${WIDTH:-}" ] || continue
    n=$((n+1)); import -window "$id" "$T/dialogo-$n-$modo.png" 2>/dev/null
    if [ "$modo" = recusar ]; then fx=$((WIDTH * 3 / 4)); else fx=$((WIDTH / 4)); fi
    xdotool mousemove --window "$id" "$fx" $((HEIGHT - 18)) click 1 2>>"$T/robo.log"
    echo "dialogo $n ($modo) id=$id ${WIDTH}x${HEIGHT} clique em $fx,$((HEIGHT - 18))" >> "$T/robo.log"
    sleep 1.5
  done
  sleep 0.4
done
