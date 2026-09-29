<p align="center">
  <img src="src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>Your Android phone as a Windows webcam.</b><br>
  One cable, one button. No app on the phone, no OBS, no watermarks.<br>
  A free, open-source alternative to DroidCam, Iriun and iVCam.
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
- **Good picture.** Up to 4K if the phone's camera captures it, at 30 fps, or 60 fps on phones
  that support it. The phone encodes H.264 in hardware and the PC's graphics chip decodes it, so
  your PC barely notices.
- **All the controls you'd expect.** Back or front camera and each lens, zoom with 1×/2×/5×
  presets, flashlight, rotation for a phone standing upright, mirror, and brightness, contrast,
  saturation and warmth.
- **Free for good.** No watermarks, no time limits, no account, no telemetry. Apache-2.0.
- **Light.** A 7.4 MB download. Waiting in the tray it takes 7 MB of memory; streaming from
  there, under 1% of the CPU. Updates install themselves with one click.
- **Speaks your language.** English, Deutsch, Español, Français, Italiano, Polski, Português,
  Türkçe, Українська, Русский, 日本語, 한국어, 简体中文.

## How it compares

The apps people usually try first, as of September 2026. Free tiers change often, so each name
links to the vendor's own page.

| | Plugcam | [DroidCam](https://droidcam.app/) | [Iriun](https://iriun.com/) | [iVCam](https://www.e2esoft.com/ivcam/) | [Camo](https://camo.com/pricing) | [Phone Link](https://support.microsoft.com/en-us/windows/apps/phonelink/use-your-mobile-device-s-camera) |
|---|---|---|---|---|---|---|
| App on the phone | **none** | yes | yes | yes | yes | Link to Windows |
| Free picture | **up to 4K, 30 or 60 fps** | 640×480; HD has a watermark | up to 4K, with a watermark | watermark; 640×480 after the trial | up to 720p | 720p |
| Ads | **none** | yes | yes | yes | none | none |
| Connection | USB | USB, Wi-Fi | USB, Wi-Fi | USB, Wi-Fi | USB, Wi-Fi | Wi-Fi + Bluetooth |
| Open source | **yes, Apache-2.0** | PC client only | no | no | no | no |
| Windows download | **7.4 MB** | 98 MB | 8.8 MB, needs .NET Desktop Runtime | about 43 MB | about 475 MB | built into Windows 11 |

Two app-free options you may already have: phones with Android 14 or newer can act as a USB
webcam on their own if the maker turned that mode on (Pixels do), and
[scrcpy](https://github.com/Genymobile/scrcpy), which Plugcam builds on, shows the camera in a
window (on Windows you need OBS to turn that window into a webcam). What Plugcam still lacks next
to the paid apps: Wi-Fi and sound.

### Other open-source projects

Of these, only Plugcam gives Windows a webcam with nothing installed on the phone and no OBS in
between. The others have things Plugcam doesn't: Wi-Fi, older Android versions, or (BestCam)
a camera that Microsoft Store apps can see. scrcpy, which Plugcam builds on, is a command-line
tool; Plugcam puts its camera mode into an app with a preview and controls. Checked against each
project's README in September 2026.

| | Plugcam | [VCamdroid](https://github.com/darusc/VCamdroid) | [Android Webcam Project](https://github.com/soubhagyajit/Android-Webcam-Project) | [BestCam](https://github.com/OneLimeStudio/BestCam) (alpha) | [scrcpy](https://github.com/Genymobile/scrcpy) |
|---|---|---|---|---|---|
| App on the phone | none | yes | yes | yes | none |
| Webcam on Windows | yes, DirectShow | yes, DirectShow | yes | yes, Media Foundation (Windows 11 22H2+) | through OBS or another window-capture tool; a webcam on Linux |
| Connection | USB | USB, Wi-Fi | USB, Wi-Fi | USB | USB, Wi-Fi |
| Android | 12+ | 7.0+ | 8.0+ | 8.0+ | 12+ for the camera |
| On the PC | app with a preview, controls and a first-run guide | app | app | Python script; app announced | command line; the window shows only the video |
| Install | installer or portable zip; updates itself | zip, then install.bat as administrator | installer | zip, no installer | zip or winget |
| License | Apache-2.0 | MIT | GPL-3.0 | GPL-2.0 | Apache-2.0 |

### Light on your PC

Plugcam is a native Rust app with a small C++ virtual camera. The window is drawn by the WebView2
that comes with Windows (through [Tauri](https://tauri.app)), so there is no bundled Chromium as
in Electron apps. The video itself never goes through the web page: decoding (Windows' own H.264
decoder, on the graphics chip), rotating, scaling, color adjustments and handing frames to the
camera all happen in Rust, and the window only gets a small preview while it is open. The
WebView draws without the graphics chip, which saves about 80 MB for a window this simple. Closed
to the tray, Plugcam shuts the WebView down completely, so a camera streaming in the background
takes under 1% of the CPU.

Measured on a Ryzen 7 8845HS laptop with Windows 11, counting Plugcam and all its WebView2
processes, averaged over a minute with [`scripts/measure.ps1`](scripts/measure.ps1):

| | Memory | CPU |
|---|---|---|
| In the tray | **7 MB** | 0% |
| Window open, camera off | 83 MB | 0% |
| Streaming 1080p30, in the tray | 99 MB | **0.6%** |
| Streaming 1080p30, window open with preview | 204 MB | 4.4% |

Memory is what the *Memory* column of Task Manager shows (the private working set), so you can
compare with any other app there. CPU is the share of all 16 threads of the 8-core processor, as
in Task Manager too. Streaming was measured with a
OnePlus 11R at 1080p, 30 fps. The picture is decoded on the Radeon 780M built into the processor;
its video memory is ordinary RAM, so the decoder's frame buffers count in the memory column.

adb, which Plugcam talks to the phone through, adds about 2 MB; it is shared with any other
Android tool you run.

## Get started

1. **Install.** Download `Plugcam_x.y.z_x64-setup.exe` from the
   [latest release](https://github.com/Qwinty/plugcam/releases/latest) and run it. Windows asks for
   administrator rights once, to register the camera. Plugcam then shows up in the Start menu.
   Prefer not to install? Take `Plugcam_x.y.z_x64-portable.zip`, extract it anywhere and run
   `Plugcam.exe`; it asks for administrator rights once on the first start, to add the camera.
2. **Turn on USB debugging** on the phone: *Settings → About phone*, tap *Build number* seven
   times, then *Settings → System → Developer options → USB debugging*. Plugcam's first-run guide
   walks you through it.
3. **Plug in the phone**, tap *Allow* on it, and press **Turn camera on**. In your video app, pick
   **Plugcam Camera**.

The installer is not code-signed yet, so SmartScreen may say "Windows protected your PC". Click
*More info → Run anyway*, or check the file against the `.sha256` next to it in the release.

**Updates come by themselves.** Plugcam looks for a new version once a day; when there is one, an
*Update* button appears. One click downloads it, checks its signature, installs it and restarts
Plugcam, which then shows what changed. Both the installed and the portable version update this
way, and you can turn the daily check off in Settings.

## Screenshots

<table>
  <tr>
    <td><img src="docs/screenshots/main-dark-en.png" alt="Main window, dark theme, with the color controls open"></td>
    <td><img src="docs/screenshots/settings-en.png" alt="Settings"></td>
  </tr>
  <tr>
    <td align="center">Dark theme follows Windows</td>
    <td align="center">Settings</td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/wizard-en.png" alt="First-run guide"></td>
    <td><img src="docs/screenshots/settings-dark-en.png" alt="Settings, dark theme"></td>
  </tr>
  <tr>
    <td align="center">First-run guide</td>
    <td align="center">Settings, dark theme</td>
  </tr>
</table>

## Requirements

- Windows 10 or 11, 64-bit.
- An Android phone with Android 12 or newer (camera capture needs it) and a USB cable that
  carries data.
- On most PCs the phone's USB driver comes with Windows Update. If the phone is not found, install
  the [Google USB driver](https://developer.android.com/studio/run/win-usb) or your phone maker's
  driver.

## FAQ

### Can I use my Android phone as a webcam on Windows without installing an app on the phone?

Yes, that is what Plugcam does. It starts the camera part of scrcpy on the phone over USB
debugging while you stream, and removes it when you stop. No APK is installed, and the phone
doesn't need root, only Android 12 or newer. Phones with Android 14 or newer may also have a
built-in USB webcam mode, if the maker turned it on (Pixels do).

### Is there a free, open-source alternative to DroidCam, Iriun or iVCam?

Plugcam is one. It is open source under Apache-2.0 and gives up to 4K at 30 fps, or 60 fps on phones
that support it, with no watermark, ads, time limit or account. What those apps have and Plugcam doesn't yet: Wi-Fi and sound. See
[How it compares](#how-it-compares).

### Which apps can use Plugcam Camera?

Desktop apps and browsers that use DirectShow cameras: Zoom, Discord, Telegram Desktop, OBS,
Chrome, Edge and Firefox, so browser calls such as Google Meet work too. Apps from the Microsoft
Store, like the Windows Camera app, don't see it; a Media Foundation camera for them is planned.
Microsoft Teams hasn't been checked yet.

### Do I need OBS?

No. Plugcam registers its own camera, so desktop video apps see it directly. With plain scrcpy on Windows
you would capture its window in OBS and start OBS's virtual camera.

### Does Plugcam work over Wi-Fi?

Not yet, only over a USB cable. Wi-Fi is next on the list.

### Does Plugcam send sound?

No, Plugcam sends only the picture. Use your PC's microphone or headset.

### Does Plugcam work with an iPhone, or on macOS or Linux?

No, Plugcam needs an Android phone and Windows. On Linux, scrcpy itself can turn the phone into a
webcam: load the `v4l2loopback` module and run
`scrcpy --video-source=camera --v4l2-sink=/dev/videoN --no-video-playback`. On a Mac with an
iPhone, Continuity Camera is built in.

### Which Android phones work with Plugcam?

Plugcam needs Android 12 or newer, because scrcpy can capture the camera only from Android 12 on.
It was built and tested with a OnePlus 11R (Android 15) and Chrome; other phones should work the
same way. Some phones list lenses they won't stream from a third-party app; Plugcam notices, hides that
lens and switches to the main camera. If your phone or app doesn't work, please
[open an issue](https://github.com/Qwinty/plugcam/issues) with the phone model and the app.
In Plugcam, **Settings → Diagnostics → Save report** makes a file to attach to it.

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
`src-tauri\resources`), then `pnpm tauri build`; `.\scripts\portable.ps1` packs the portable zip
from that build. Tests: `cargo test` in `src-tauri`. Releases are built by
[.github/workflows/release.yml](.github/workflows/release.yml) when a `v*` tag is pushed.

The design notes (in Russian) are in [docs/SPEC.md](docs/SPEC.md).

## Privacy

Plugcam has no servers and sends nothing anywhere. The video goes from the phone to your PC over
the cable and stays there. The only thing it fetches from the internet is the list of releases on
GitHub, once a day, to see whether there is an update (turn it off in Settings). Settings are a
JSON file in `%APPDATA%\io.github.plugcam`, or in the `data` folder of the portable version.

## License

[Apache-2.0](LICENSE). Plugcam ships scrcpy-server (Apache-2.0), adb from Android platform-tools,
and a fork of Softcam (MIT); see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
