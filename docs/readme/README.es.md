<p align="center">
  <img src="../../src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>Tu teléfono Android como webcam para Windows.</b><br>
  Un cable, un botón. Sin app en el teléfono, sin marcas de agua, gratis y de código abierto.
</p>

<p align="center">
  <a href="../../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <b>Español</b> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.pt-BR.md">Português</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <img src="../screenshots/main-light-es.png" width="860" alt="Plugcam mostrando la cámara del teléfono, con los controles a la derecha">
</p>

## Por qué Plugcam

- **No hay que instalar nada en el teléfono.** Plugcam habla con el teléfono mediante la depuración USB y ejecuta en él la parte de cámara de [scrcpy](https://github.com/Genymobile/scrcpy) mientras transmites.
- **Una webcam de verdad para Windows.** «Plugcam Camera» aparece junto a tus otras cámaras en los programas y navegadores que usan DirectShow: Zoom, Discord, Telegram Desktop, OBS, Chrome, Edge, Firefox y, por tanto, también en llamadas web como Google Meet.
- **Buena imagen.** Hasta 1080p a 30 fps, o 60 fps en teléfonos compatibles. El teléfono codifica H.264 por hardware, así que el PC apenas lo nota.
- **Todos los controles que esperas.** Cámara trasera o frontal y cada lente, zoom con 1×/2×/5×, linterna, rotación para un teléfono en vertical, imagen en espejo.
- **Gratis para siempre.** Sin marcas de agua, sin límite de tiempo, sin cuenta, sin telemetría. Apache-2.0.
- **Habla tu idioma.** 13 idiomas, incluido el español.

## Primeros pasos

1. **Instala.** Descarga `Plugcam_x.y.z_x64-setup.exe` de la [última versión](https://github.com/Qwinty/plugcam/releases/latest) y ejecútalo. Windows pide permisos de administrador una vez, para registrar la cámara.
2. **Activa la depuración USB** en el teléfono: *Ajustes → Información del teléfono*, toca siete veces *Número de compilación* y luego *Ajustes → Sistema → Opciones para desarrolladores → Depuración USB*. La guía inicial de Plugcam te acompaña.
3. **Conecta el teléfono**, toca *Permitir* en él y pulsa **Encender cámara**. En tu app de vídeo, elige **Plugcam Camera**.

El instalador aún no está firmado, así que SmartScreen puede decir «Windows protegió su PC». Haz clic en *Más información → Ejecutar de todas formas*, o comprueba el archivo con el `.sha256` que lo acompaña en la versión.

## Capturas

<table>
  <tr>
    <td><img src="../screenshots/main-dark-ru.png" alt=""></td>
    <td><img src="../screenshots/settings-de.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Tema oscuro como Windows · Русский</td>
    <td align="center">Ajustes · Deutsch</td>
  </tr>
  <tr>
    <td><img src="../screenshots/wizard-ja.png" alt=""></td>
    <td><img src="../screenshots/main-light-es.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Guía inicial · 日本語</td>
    <td align="center">Listo para transmitir · Español</td>
  </tr>
  <tr>
    <td><img src="../screenshots/settings-dark-zh-CN.png" alt=""></td>
    <td><img src="../screenshots/wizard-dark-fr.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Ajustes, tema oscuro · 简体中文</td>
    <td align="center">Guía inicial · Français</td>
  </tr>
</table>

## Requisitos

- Windows 10 u 11 de 64 bits.
- Un teléfono con Android 12 o posterior (necesario para capturar la cámara) y un cable USB que transmita datos.
- El controlador USB suele llegar por Windows Update. Si no se encuentra el teléfono, instala el [controlador USB de Google](https://developer.android.com/studio/run/win-usb) o el del fabricante.

## Conviene saber

- **Las apps de Microsoft Store**, como la app Cámara de Windows, no ven Plugcam Camera: es una cámara DirectShow, la que usan los programas clásicos y los navegadores. Está prevista una cámara Media Foundation para apps de la Store.
- **Wi-Fi** todavía no está; es lo siguiente.
- **Algunas lentes no dan imagen.** Los teléfonos listan lentes que no transmiten a apps de terceros. Plugcam lo detecta, oculta esa lente y cambia a la cámara principal.
- **Sin sonido.** Usa el micrófono del PC o unos auriculares.
- Plugcam se hizo y se probó con un OnePlus 11R (Android 15) y Chrome. Otros teléfonos y apps deberían funcionar igual; si no es así, [abre un issue](https://github.com/Qwinty/plugcam/issues) con el modelo del teléfono y la app.

Cómo funciona, cómo compilarlo y las licencias: en el [README en inglés](../../README.md).

## Privacidad

Plugcam no tiene servidores y no envía nada a ninguna parte. El vídeo va del teléfono al PC por el cable y se queda ahí.

Licencia: [Apache-2.0](../../LICENSE). Componentes de terceros: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md).
