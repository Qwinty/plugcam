<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>Your Android phone as a Windows webcam.</b><br>
  One cable, one button. No app on the phone, no watermarks, free and open source.
</p>

<p align="center">
  <a href="https://github.com/Qwinty/plugcam/releases/latest"><img src="https://img.shields.io/github/v/release/Qwinty/plugcam?include_prereleases&label=download&color=0a64c2" alt="Download"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0a64c2" alt="Windows 10 and 11">
  <img src="https://img.shields.io/badge/Android-12%2B-3ddc84" alt="Android 12+">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Qwinty/plugcam?color=555" alt="License"></a>
</p>

<p align="center">
  <b>English</b> ·
  <a href="docs/readme/README.ru.md">Русский</a> ·
  <a href="docs/readme/README.de.md">Deutsch</a> ·
  <a href="docs/readme/README.es.md">Español</a> ·
  <a href="docs/readme/README.fr.md">Français</a> ·
  <a href="docs/readme/README.pt-BR.md">Português</a> ·
  <a href="docs/readme/README.zh-CN.md">简体中文</a> ·
  <a href="docs/readme/README.ja.md">日本語</a>
</p>

<p align="center">
  <img src="docs/screenshots/main-light-en.png" width="860" alt="Plugcam streaming the phone's camera, with controls on the right">
</p>

## Why Plugcam

- **Nothing to install on the phone.** Plugcam talks to the phone over USB debugging and runs the
  camera part of [scrcpy](https://github.com/Genymobile/scrcpy) on it for as long as you stream.
- **A real webcam for Windows.** "Plugcam Camera" shows up next to your other cameras in desktop
  apps and browsers that use DirectShow: Zoom, Discord, Telegram Desktop, OBS, Chrome, Edge,
  Firefox, and so web calls like Google Meet too.
- **Good picture.** Up to 1080p at 30 fps, or 60 fps on phones that support it. The phone encodes
  H.264 in hardware, so your PC barely notices.
- **All the controls you'd expect.** Back or front camera and each lens, zoom with 1×/2×/5×
  presets, flashlight, rotation for a phone standing upright, mirror.
- **Free for good.** No watermarks, no time limits, no account, no telemetry. Apache-2.0.
- **Speaks your language.** English, Deutsch, Español, Français, Italiano, Polski, Português,
  Türkçe, Українська, Русский, 日本語, 한국어, 简体中文.

## Get started

1. **Install.** Download `Plugcam_x.y.z_x64-setup.exe` from the
   [latest release](https://github.com/Qwinty/plugcam/releases/latest) and run it. Windows asks for
   administrator rights once, to register the camera.
2. **Turn on USB debugging** on the phone: *Settings → About phone*, tap *Build number* seven
   times, then *Settings → System → Developer options → USB debugging*. Plugcam's first-run guide
   walks you through it.
3. **Plug in the phone**, tap *Allow* on it, and press **Turn camera on**. In your video app, pick
   **Plugcam Camera**.

The installer is not code-signed yet, so SmartScreen may say "Windows protected your PC". Click
*More info → Run anyway*, or check the file against the `.sha256` next to it in the release.

## Screenshots

<table>
  <tr>
    <td><img src="docs/screenshots/main-dark-ru.png" alt="Main window, dark theme, in Russian"></td>
    <td><img src="docs/screenshots/settings-de.png" alt="Settings in German"></td>
  </tr>
  <tr>
    <td align="center">Dark theme follows Windows · Русский</td>
    <td align="center">Settings · Deutsch</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/wizard-ja.png" alt="First-run guide in Japanese"></td>
    <td><img src="docs/screenshots/main-light-es.png" alt="Main window in Spanish"></td>
  </tr>
  <tr>
    <td align="center">First-run guide · 日本語</td>
    <td align="center">Ready to stream · Español</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/settings-dark-zh-CN.png" alt="Settings, dark theme, in Chinese"></td>
    <td><img src="docs/screenshots/wizard-dark-fr.png" alt="First-run guide, dark theme, in French"></td>
  </tr>
  <tr>
    <td align="center">Settings, dark theme · 简体中文</td>
    <td align="center">First-run guide · Français</td>
  </tr>
</table>

## Requirements

- Windows 10 or 11, 64-bit.
- An Android phone with Android 12 or newer (camera capture needs it) and a USB cable that
  carries data.
- On most PCs the phone's USB driver comes with Windows Update. If the phone is not found, install
  the [Google USB driver](https://developer.android.com/studio/run/win-usb) or your phone maker's
  driver.

## Good to know

- **Apps from the Microsoft Store**, like the Windows Camera app, don't see Plugcam Camera: it is
  a DirectShow camera, which classic desktop apps and browsers use. A Media Foundation camera for
  Store apps is planned.
- **Wi-Fi** is not there yet; it is next on the list.
- **Some lenses give no picture.** Phones list lenses they won't stream from a third-party app.
  Plugcam notices, hides that lens and switches to the main camera.
- **No sound.** Use your PC's microphone or headset.
- Plugcam was built and tested with a OnePlus 11R (Android 15) and Chrome. Other phones and apps
  should work the same way; if yours don't, please
  [open an issue](https://github.com/Qwinty/plugcam/issues) with the phone model and the app.

## How it works

```
phone camera ─▶ scrcpy-server ─▶ adb over USB ─▶ Plugcam ─▶ "Plugcam Camera" ─▶ Zoom, OBS, …
                (H.264 in the      (TCP tunnel)    (Media Foundation     (DirectShow
                 phone's encoder)                    decoder → frames)     filter, Softcam)
```

Plugcam pushes `scrcpy-server` 4.1 (pinned by checksum) to the phone, starts it through adb and
reads its video stream. Windows' built-in H.264 decoder turns the stream into pictures, which are
rotated, scaled and mirrored as needed and handed to the virtual camera. When you stop, the server
exits and removes itself from the phone.

The app is written in Rust with [Tauri](https://tauri.app) and [Svelte](https://svelte.dev). The
virtual camera is a fork of [Softcam](https://github.com/tshino/softcam).

## Build from source

You need Rust (stable), Node.js 24 with pnpm, Visual Studio 2022 or newer with the C++ desktop
workload, and adb (Android platform-tools) on PATH.

```powershell
pnpm install
.\scripts\vcam-build.ps1          # builds plugcam_cam.dll (x64 and x86)
.\scripts\vcam-register.ps1       # registers the dev build as "Plugcam Camera" (asks for admin)
pnpm tauri dev                    # runs the app
```

To build the installer: `.\scripts\stage-resources.ps1` (copies adb and the camera DLL into
`src-tauri\resources`), then `pnpm tauri build`. Tests: `cargo test` in `src-tauri`.

The design notes (in Russian) are in [docs/SPEC.md](docs/SPEC.md).

## Privacy

Plugcam has no servers and sends nothing anywhere. The video goes from the phone to your PC over
the cable and stays there. Settings are a JSON file in `%APPDATA%\io.github.plugcam`.

## License

[Apache-2.0](LICENSE). Plugcam ships scrcpy-server (Apache-2.0), adb from Android platform-tools,
and a fork of Softcam (MIT); see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
