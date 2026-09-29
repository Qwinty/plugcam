#!/usr/bin/env bash
# One README screenshot of a running Plugcam started with remote debugging (see README.md here).
#   bash shoot.sh <lang> <light|dark> <main|settings|wizard> <out.png> [color]
# `color` opens the colour controls on the main screen. Shots are 980x668 CSS px at 2x.
# Note: switching the language saves it in that Plugcam's settings.
set -euo pipefail
cd "$(dirname "$0")"
LANG_=$1 THEME=$2 SCREEN=$3 OUT=$4 EXTRA=${5:-}
case "$OUT" in /* | [A-Za-z]:*) ;; *) OUT="$OLDPWD/$OUT" ;; esac

node cdp.mjs eval "window.__TAURI_INTERNALS__.invoke('update_settings', {patch: {language: '$LANG_'}}).then(() => 'ok')" >/dev/null
sleep 0.8

# From wherever the app is: back to the main screen, past the first-run guide, then to $SCREEN.
node cdp.mjs eval "(async () => {
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const back = [...document.querySelectorAll('header button')].find((b) => b.querySelector('.lucide-arrow-left'));
  if (back) { back.click(); await wait(200); }
  const skip = document.querySelector('.top .text-btn');
  if (skip) { skip.click(); await wait(200); }
  if ('$SCREEN' !== 'main') { [...document.querySelectorAll('header .icon-btn')].pop().click(); await wait(300); }
  if ('$SCREEN' === 'wizard') {
    const rows = [...document.querySelectorAll('.link')];
    rows[rows.length - 1].click(); // Run the first-run setup again
    await wait(300);
    for (let i = 0; i < 2; i++) { document.querySelector('.nav .btn.accent').click(); await wait(200); }
  }
  if ('$EXTRA' === 'color') { document.querySelector('.head[aria-expanded]').click(); await wait(200); }
  // Mica is see-through: it would show whatever is behind the window.
  document.documentElement.classList.remove('mica');
  document.querySelector('aside')?.scrollTo(0, 0);
  return 'ok';
})()" >/dev/null
sleep 0.5

if [ "$THEME" = dark ]; then SHOT=shotdark; else SHOT=shot; fi
SIZE=980x668 DPR=2 node cdp.mjs "$SHOT" "$OUT"

if [ "$EXTRA" = color ]; then
  node cdp.mjs eval "document.querySelector('.head[aria-expanded]').click()" >/dev/null
fi
