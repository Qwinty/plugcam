# Recorded scrcpy streams

Raw bytes of the video socket after the device name and codec id (session packet first),
recorded with `plugcam-cli --dump FILE`. Used by `src-tauri/tests/fixtures.rs`.

| File | Phone | Command |
| --- | --- | --- |
| `oneplus11r-front-640x480.scrcpy` | OnePlus 11R (PHK110), Android 15, scrcpy-server 4.1 | `plugcam-cli --camera-id 1 --size 640x480 --bitrate 0.5 --dump FILE`, ~2.7 s |

Re-record them whenever `SERVER_VERSION` changes.
