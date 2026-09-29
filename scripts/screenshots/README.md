# Screenshots of the app

How to take screenshots of Plugcam, for the README and site (`docs/screenshots/`) or to check a UI change.
The window is a WebView2 page, so the reliable way is the Chrome DevTools Protocol (CDP), not screen capture.

| File | What it does |
|---|---|
| `start.ps1` | Starts a build portable, with WebView2's remote debugging on port 9222 |
| `cdp.mjs` | Runs JS in the page and saves screenshots (`eval`, `shot`, `shotdark`, `console`) |
| `shoot.sh` | One README shot: language, theme, screen |
| `frame.py` | Rounded corners, border and shadow, as in `docs/screenshots/` (needs Pillow) |

## 1. Build

Build into a separate target folder, with the frontend embedded and a test identifier:

```bash
pnpm build
cd src-tauri
CARGO_TARGET_DIR="$TEMP/plugcam-shots" TAURI_CONFIG='{"identifier":"io.github.plugcam.test"}' \
  cargo build --features tauri/custom-protocol
```

- `--features tauri/custom-protocol` embeds `dist/`. Without it a debug build loads `http://127.0.0.1:1420` and shows an empty window unless `pnpm dev` runs.
- The test identifier matters. Plugcam is single-instance per identifier: if the installed app or a dev build from another worktree is running, the new copy hands over to it and exits at once.
- The separate target folder keeps the main `src-tauri/target` cache intact. A first build takes about 7 minutes.

A release build (`pnpm tauri build`) works too: point `start.ps1` at `src-tauri/target/release/plugcam.exe`.

## 2. Start

```bash
pwsh scripts/screenshots/start.ps1 -Exe "$TEMP/plugcam-shots/debug/plugcam.exe"
```

- It puts `portable.txt` next to the exe, so settings, logs and the WebView cache go to `data\` there. Without it the build reads and rewrites `%APPDATA%\io.github.plugcam\settings.json`, the real one, and drops any field it doesn't know, for example a setting from another branch.
- `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` replaces the WebView2 flags from `tauri.conf.json` (`--disable-gpu` among them), so the script passes them along with `--remote-debugging-port`.
- If Chrome already uses port 9222, pass `-Port 9333` and set `CDP_PORT=9333` for `cdp.mjs`.
- A fresh `data\` starts in the first-run guide. `shoot.sh` skips it; by hand, click "Skip" (see below).

## 3. Shoot

For the README set: 5 screens in each of the 8 README languages, 980×668 at 2x.

```bash
S=scripts/screenshots; OUT="$TEMP/plugcam-shots/shots"; mkdir -p "$OUT"
for L in en ru de es fr pt-BR zh-CN ja; do
  bash $S/shoot.sh $L light main     "$OUT/main-light-$L.png"
  bash $S/shoot.sh $L dark  main     "$OUT/main-dark-$L.png" color
  bash $S/shoot.sh $L light settings "$OUT/settings-$L.png"
  bash $S/shoot.sh $L dark  settings "$OUT/settings-dark-$L.png"
  bash $S/shoot.sh $L light wizard   "$OUT/wizard-$L.png"
done
python $S/frame.py docs/screenshots "$OUT"/*.png
```

- The main screens need a phone connected and streaming; the picture is whatever its camera sees, so point it at something you're fine publishing. A fresh `data\` doesn't start the camera by itself: `node cdp.mjs eval "window.__TAURI_INTERNALS__.invoke('set_camera', {on: true})"`, and `{on: false}` afterwards. Only one Plugcam can stream from a phone at a time.
- The theme comes from `prefers-color-scheme` emulation, not from Windows. The Mica class is removed before each shot, since Mica shows whatever is behind the window.
- Check the set on one contact sheet before committing: text cut off in German or Japanese, a stray dialog, a wrong theme.

For a quick look while working on the UI:

```bash
cd scripts/screenshots
node cdp.mjs eval "[...document.querySelectorAll('button')].map(b => b.getAttribute('aria-label') || b.innerText.trim())"
node cdp.mjs eval "[...document.querySelectorAll('button')].find(b => b.innerText.trim() === 'Skip').click()"
node cdp.mjs eval "window.__TAURI_INTERNALS__.invoke('update_settings', {patch: {detailedLog: true}})"
node cdp.mjs shot "$TEMP/look.png"
node cdp.mjs console 4
```

`eval` clicks the real buttons and `__TAURI_INTERNALS__.invoke` calls the same commands the UI calls, so this checks the whole path, backend included.

## What doesn't work

- **Screen-capture and computer-use tools.** They look for apps in the Start menu, and a dev build isn't there. Tools that do find the window (cua-driver) returned blank captures: the window is transparent (Mica) and WebView2 draws without the GPU.
- **Your everyday Plugcam.** Don't point these scripts at the installed app: `shoot.sh` changes the language setting, and clicks change whatever they touch.

## Clean up

- Stop the copy: `Get-Process plugcam | Where-Object Path -like "$env:TEMP\plugcam-shots\*" | Stop-Process`.
- Delete `$TEMP/plugcam-shots` (a few GB).
- "Save report" in Settings writes a file to Downloads and opens Explorer and a GitHub page. Delete that report if you pressed it while testing.
