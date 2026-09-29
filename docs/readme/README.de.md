<p align="center">
  <img src="../../src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>Dein Android-Smartphone als Webcam für Windows.</b><br>
  Ein Kabel, ein Knopf. Keine App auf dem Smartphone, kein OBS, keine Wasserzeichen.<br>
  Eine kostenlose Open-Source-Alternative zu DroidCam, Iriun und iVCam.
</p>

<p align="center">
  <a href="../../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <b>Deutsch</b> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.pt-BR.md">Português</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <img src="../screenshots/main-light-de.png" width="860" alt="Plugcam zeigt das Kamerabild des Smartphones, rechts die Bedienelemente">
</p>

## Warum Plugcam

- **Nichts auf dem Smartphone installieren.** Plugcam spricht per USB-Debugging mit dem Smartphone und startet dort für die Dauer der Übertragung den Kamerateil von [scrcpy](https://github.com/Genymobile/scrcpy).
- **Eine echte Webcam für Windows.** „Plugcam Camera“ erscheint neben deinen anderen Kameras in Programmen und Browsern, die DirectShow nutzen: Zoom, Discord, Telegram Desktop, OBS, Chrome, Edge, Firefox – und damit auch in Webanrufen wie Google Meet.
- **Gutes Bild.** Bis zu 4K, sofern die Kamera des Smartphones das aufnimmt, mit 30 fps, auf geeigneten Smartphones 60 fps. Das Smartphone kodiert H.264 per Hardware und der Grafikchip des PCs dekodiert es, der PC merkt kaum etwas.
- **Alle Regler, die man erwartet.** Haupt- oder Frontkamera und jedes Objektiv, Zoom mit 1×/2×/5×, Taschenlampe, Drehung für ein hochkant stehendes Smartphone, Spiegelbild sowie Helligkeit, Kontrast, Sättigung und Wärme.
- **Dauerhaft kostenlos.** Keine Wasserzeichen, kein Zeitlimit, kein Konto, keine Telemetrie. Apache-2.0.
- **Leicht.** 7,4 MB Download. Im Infobereich braucht Plugcam 7 MB Arbeitsspeicher, beim Streaming von dort aus unter 1 % der CPU. Updates installieren sich mit einem Klick von selbst.
- **Spricht deine Sprache.** 13 Sprachen, darunter Deutsch.

## Im Vergleich

Die Apps, die man meist zuerst ausprobiert, Stand September 2026. Kostenlose Varianten ändern sich oft, deshalb führt jeder Name zur Seite des Anbieters.

| | Plugcam | [DroidCam](https://droidcam.app/) | [Iriun](https://iriun.com/) | [iVCam](https://www.e2esoft.com/ivcam/) | [Camo](https://camo.com/pricing) | [Phone Link](https://support.microsoft.com/en-us/windows/apps/phonelink/use-your-mobile-device-s-camera) |
|---|---|---|---|---|---|---|
| App auf dem Smartphone | **keine** | ja | ja | ja | ja | Link to Windows |
| Kostenloses Bild | **bis 4K, 30 oder 60 fps** | 640×480; HD mit Wasserzeichen | bis 4K, mit Wasserzeichen | Wasserzeichen; 640×480 nach der Testphase | bis 720p | 720p |
| Werbung | **keine** | ja | ja | ja | keine | keine |
| Verbindung | USB | USB, WLAN | USB, WLAN | USB, WLAN | USB, WLAN | WLAN + Bluetooth |
| Open Source | **ja, Apache-2.0** | nur der PC-Client | nein | nein | nein | nein |
| Download für Windows | **7,4 MB** | 98 MB | 8,8 MB, braucht .NET Desktop Runtime | etwa 43 MB | etwa 475 MB | in Windows 11 enthalten |

Zwei Möglichkeiten ohne App, die du vielleicht schon hast: Smartphones mit Android 14 oder neuer können selbst als USB-Webcam arbeiten, wenn der Hersteller diesen Modus freigeschaltet hat (bei Pixel-Geräten ist das so), und [scrcpy](https://github.com/Genymobile/scrcpy), auf dem Plugcam aufbaut, zeigt die Kamera in einem Fenster (unter Windows brauchst du OBS, um aus diesem Fenster eine Webcam zu machen). Was Plugcam im Vergleich zu den kostenpflichtigen Apps noch fehlt: WLAN und Ton.

### Weitere Open-Source-Projekte

Von diesen Projekten gibt nur Plugcam Windows eine Webcam, ohne dass auf dem Smartphone etwas installiert wird und ohne OBS dazwischen. Die anderen können dafür Dinge, die Plugcam nicht kann: WLAN, ältere Android-Versionen oder (BestCam) eine Kamera, die auch Apps aus dem Microsoft Store sehen. Abgeglichen mit dem README des jeweiligen Projekts, Stand September 2026. scrcpy, auf dem Plugcam aufbaut, ist ein Kommandozeilenwerkzeug; Plugcam macht aus seinem Kameramodus eine App mit Vorschau und Reglern.

| | Plugcam | [VCamdroid](https://github.com/darusc/VCamdroid) | [Android Webcam Project](https://github.com/soubhagyajit/Android-Webcam-Project) | [BestCam](https://github.com/OneLimeStudio/BestCam) (Alpha) | [scrcpy](https://github.com/Genymobile/scrcpy) |
|---|---|---|---|---|---|
| App auf dem Smartphone | keine | ja | ja | ja | keine |
| Webcam unter Windows | ja, DirectShow | ja, DirectShow | ja | ja, Media Foundation (Windows 11 22H2+) | über OBS oder ein anderes Programm, das Fenster aufnimmt; unter Linux eine Webcam |
| Verbindung | USB | USB, WLAN | USB, WLAN | USB | USB, WLAN |
| Android | 12+ | 7.0+ | 8.0+ | 8.0+ | 12+ für die Kamera |
| Auf dem PC | App mit Vorschau, Reglern und Einrichtungsassistent | App | App | Python-Skript, App angekündigt | Kommandozeile, das Fenster zeigt nur das Video |
| Installation | Installer oder portables Zip, aktualisiert sich selbst | Zip, dann install.bat als Administrator | Installer | Zip, kein Installer | Zip oder winget |
| Lizenz | Apache-2.0 | MIT | GPL-3.0 | GPL-2.0 | Apache-2.0 |

### Leicht für deinen PC

Plugcam ist eine native Rust-App mit einer kleinen virtuellen Kamera in C++. Das Fenster zeichnet das WebView2, das mit Windows mitkommt (über [Tauri](https://tauri.app)), es wird also kein eigenes Chromium mitgeliefert wie bei Electron-Apps. Das Video selbst läuft nie durch die Webseite: Dekodieren (mit dem H.264-Decoder von Windows, auf dem Grafikchip), Drehen, Skalieren, Farbanpassungen und die Übergabe der Bilder an die Kamera passieren in Rust, und das Fenster bekommt nur eine kleine Vorschau, solange es offen ist. Das WebView zeichnet ohne den Grafikchip, was bei einem so einfachen Fenster etwa 80 MB spart. In den Infobereich geschlossen, beendet Plugcam das WebView vollständig, eine Kamera im Hintergrund braucht also unter 1 % der CPU.

Gemessen auf einem Laptop mit Ryzen 7 8845HS und Windows 11, Plugcam samt aller WebView2-Prozesse, gemittelt über eine Minute mit [`scripts/measure.ps1`](../../scripts/measure.ps1):

| | Arbeitsspeicher | CPU |
|---|---|---|
| Im Infobereich | **7 MB** | 0 % |
| Fenster offen, Kamera aus | 83 MB | 0 % |
| Streaming 1080p30, im Infobereich | 99 MB | **0,6 %** |
| Streaming 1080p30, Fenster offen mit Vorschau | 204 MB | 4,4 % |

Arbeitsspeicher ist der Wert, den der „Task-Manager“ in der Spalte „Arbeitsspeicher“ zeigt (der private Arbeitssatz), so lässt er sich dort mit jeder anderen App vergleichen. CPU ist der Anteil an allen 16 Threads des 8-Kern-Prozessors, ebenfalls wie im Task-Manager. Das Streaming wurde mit einem OnePlus 11R bei 1080p und 30 fps gemessen. Das Bild dekodiert die im Prozessor eingebaute Radeon 780M; ihr Videospeicher ist normaler Arbeitsspeicher, deshalb zählen die Bildpuffer des Decoders in der Speicherspalte mit.

adb, über das Plugcam mit dem Smartphone spricht, kommt mit etwa 2 MB dazu; es wird mit allen anderen Android-Tools geteilt, die du nutzt.

## Loslegen

1. **Installieren.** Lade `Plugcam_x.y.z_x64-setup.exe` aus dem [neuesten Release](https://github.com/Qwinty/plugcam/releases/latest) und starte sie. Windows fragt einmal nach Administratorrechten, um die Kamera zu registrieren. Danach findest du Plugcam im Startmenü. Lieber nicht installieren? Nimm `Plugcam_x.y.z_x64-portable.zip`, entpacke es an einen beliebigen Ort und starte `Plugcam.exe`; beim ersten Start fragt es einmal nach Administratorrechten, um die Kamera hinzuzufügen.
2. **USB-Debugging einschalten**: *Einstellungen → Über das Telefon*, siebenmal auf *Build-Nummer* tippen, dann *Einstellungen → System → Entwickleroptionen → USB-Debugging*. Die Ersteinrichtung von Plugcam führt dich hindurch.
3. **Smartphone anschließen**, dort auf *Zulassen* tippen und **Kamera einschalten** drücken. Wähle in deiner Video-App **Plugcam Camera**.

Der Installer ist noch nicht signiert, daher meldet SmartScreen eventuell „Der Computer wurde durch Windows geschützt“. Klicke auf *Weitere Informationen → Trotzdem ausführen* oder prüfe die Datei mit der `.sha256` daneben im Release.

**Updates kommen von selbst.** Plugcam sucht einmal am Tag nach einer neuen Version; gibt es eine, erscheint die Schaltfläche *Update*. Ein Klick lädt sie herunter, prüft ihre Signatur, installiert sie und startet Plugcam neu, das dann zeigt, was sich geändert hat. So aktualisieren sich sowohl die installierte als auch die portable Version, und die tägliche Suche lässt sich in den Einstellungen abschalten.

## Screenshots

<table>
  <tr>
    <td><img src="../screenshots/main-dark-de.png" alt=""></td>
    <td><img src="../screenshots/settings-de.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Dunkles Design wie Windows</td>
    <td align="center">Einstellungen</td>
  </tr>
  <tr>
    <td><img src="../screenshots/wizard-de.png" alt=""></td>
    <td><img src="../screenshots/settings-dark-de.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Ersteinrichtung</td>
    <td align="center">Einstellungen, dunkel</td>
  </tr>
</table>

## Voraussetzungen

- Windows 10 oder 11, 64-Bit.
- Ein Smartphone mit Android 12 oder neuer (für die Kameraaufnahme nötig) und ein USB-Kabel, das Daten überträgt.
- Den USB-Treiber liefert meist Windows Update. Wird das Smartphone nicht gefunden, installiere den [Google-USB-Treiber](https://developer.android.com/studio/run/win-usb) oder den Treiber des Herstellers.

## Häufige Fragen

### Kann ich mein Android-Handy als Webcam für Windows nutzen, ohne eine App zu installieren?

Ja, genau das macht Plugcam. Es startet per USB-Debugging für die Dauer der Übertragung den Kamerateil von scrcpy auf dem Smartphone und entfernt ihn wieder, wenn du aufhörst. Es wird keine APK installiert, und das Smartphone braucht kein Root, nur Android 12 oder neuer. Smartphones mit Android 14 oder neuer haben eventuell auch einen eingebauten USB-Webcam-Modus, wenn der Hersteller ihn freigeschaltet hat (bei Pixel-Geräten ist das so).

### Gibt es eine kostenlose Open-Source-Alternative zu DroidCam, Iriun oder iVCam?

Plugcam ist eine. Es ist Open Source unter Apache-2.0 und liefert bis zu 4K mit 30 fps, auf geeigneten Smartphones 60 fps, ohne Wasserzeichen, Werbung, Zeitlimit oder Konto. Was diese Apps haben und Plugcam noch nicht: WLAN und Ton. Siehe [Im Vergleich](#im-vergleich).

### In welchen Programmen funktioniert Plugcam Camera?

In Programmen und Browsern, die DirectShow-Kameras nutzen: Zoom, Discord, Telegram Desktop, OBS, Chrome, Edge und Firefox, also funktionieren auch Webanrufe wie Google Meet. Apps aus dem Microsoft Store, etwa die Windows-Kamera-App, sehen sie nicht; eine Media-Foundation-Kamera für sie ist geplant. Microsoft Teams wurde noch nicht getestet.

### Brauche ich dafür OBS?

Nein. Plugcam registriert eine eigene Kamera, Video-Programme sehen sie also direkt. Mit scrcpy allein müsstest du unter Windows dessen Fenster in OBS aufnehmen und die virtuelle Kamera von OBS starten.

### Funktioniert Plugcam auch über WLAN?

Noch nicht, nur per USB-Kabel. WLAN ist als Nächstes dran.

### Überträgt Plugcam auch den Ton?

Nein, Plugcam überträgt nur das Bild. Nutze das Mikrofon des PCs oder ein Headset.

### Funktioniert Plugcam mit dem iPhone oder unter macOS oder Linux?

Nein, Plugcam braucht ein Android-Smartphone und Windows. Unter Linux kann scrcpy selbst das Smartphone zur Webcam machen: Lade das Modul `v4l2loopback` und starte `scrcpy --video-source=camera --v4l2-sink=/dev/videoN --no-video-playback`. Auf dem Mac mit einem iPhone ist Continuity Camera schon eingebaut.

### Welche Android-Handys funktionieren mit Plugcam?

Plugcam braucht Android 12 oder neuer, weil scrcpy die Kamera erst ab Android 12 aufnehmen kann. Entwickelt und getestet wurde es mit einem OnePlus 11R (Android 15) und Chrome; andere Smartphones sollten genauso funktionieren. Manche Smartphones listen Objektive, die sie an Drittprogramme nicht streamen; Plugcam merkt das, blendet das Objektiv aus und wechselt zur Hauptkamera. Wenn dein Smartphone oder deine App nicht funktioniert, [eröffne bitte ein Issue](https://github.com/Qwinty/plugcam/issues) mit Smartphone-Modell und App. In Plugcam erstellt **Einstellungen → Diagnose → Bericht speichern** eine Datei, die du anhängen kannst.

Funktionsweise, Bauen aus dem Quellcode und Lizenzen: siehe [englisches README](../../README.md).

## Datenschutz

Plugcam hat keine Server und sendet nichts. Das Video geht per Kabel vom Smartphone zum PC und bleibt dort. Das Einzige, was es aus dem Internet abruft, ist einmal am Tag die Liste der Releases auf GitHub, um zu sehen, ob es ein Update gibt (abschaltbar in den Einstellungen). Die Einstellungen liegen als JSON-Datei in `%APPDATA%\io.github.plugcam` oder bei der portablen Version im Ordner `data`.

Lizenz: [Apache-2.0](../../LICENSE). Komponenten von Dritten: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md).
