# Plugcam fork of Softcam

Upstream: https://github.com/tshino/softcam (MIT), added with
`git subtree add --prefix vcam https://github.com/tshino/softcam.git main --squash`
at upstream commit `5113173`. Pull updates with `git subtree pull` and re-check the list below.

## Changes

| File | Change | Why |
| --- | --- | --- |
| `src/softcam/softcam.cpp` | camera name `Plugcam Camera`, CLSID `{518D4949-2206-4A92-B74A-6CF1C7348605}` | must not collide with other Softcam-based cameras |
| `src/softcamcore/FrameBuffer.cpp` | mutex / shared memory `Plugcam Camera/NamedMutex`, `Plugcam Camera/SharedMemory` | same |
| `src/softcamcore/DShowSoftcam.cpp` | debug filter and pin names | same |
| `src/softcamcore/DShowSoftcam.cpp` | with `framerate = 0`, samples are stamped with the real stream time | upstream advances timestamps by a fixed 1/60 s, so a 30 fps live source runs at 0.5x clock speed and renderers can pile up latency |
| `src/softcam/softcam.vcxproj` (+ `_vs2019`) | output `plugcam_cam.dll`; quoted paths in the post-build copy | the project folder has a space in its path |

## Build and register (development)

```powershell
scripts\vcam-build.ps1      # vcam\dist\bin\{x64,Win32}\plugcam_cam.dll
scripts\vcam-register.ps1   # UAC prompt; -Unregister to remove
```
