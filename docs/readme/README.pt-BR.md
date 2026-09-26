<p align="center">
  <img src="../../src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>Seu celular Android como webcam no Windows.</b><br>
  Um cabo, um botão. Sem app no celular, sem marca d’água, grátis e de código aberto.
</p>

<p align="center">
  <a href="../../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.fr.md">Français</a> ·
  <b>Português</b> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <img src="../screenshots/main-light-en.png" width="860" alt="Plugcam mostrando a câmera do celular, com os controles à direita">
</p>

## Por que o Plugcam

- **Nada para instalar no celular.** O Plugcam conversa com o celular pela depuração USB e roda nele a parte de câmera do [scrcpy](https://github.com/Genymobile/scrcpy) enquanto você transmite.
- **Uma webcam de verdade no Windows.** A “Plugcam Camera” aparece junto das outras câmeras nos programas e navegadores que usam DirectShow: Zoom, Discord, Telegram Desktop, OBS, Chrome, Edge, Firefox e, portanto, também em chamadas pela web como o Google Meet.
- **Imagem boa.** Até 1080p a 30 fps, ou 60 fps em celulares compatíveis. O celular codifica H.264 por hardware, e o PC quase não sente.
- **Todos os controles que você espera.** Câmera traseira ou frontal e cada lente, zoom com 1×/2×/5×, lanterna, rotação para celular em pé, imagem espelhada.
- **Grátis para sempre.** Sem marca d’água, sem limite de tempo, sem conta, sem telemetria. Apache-2.0.
- **Fala a sua língua.** 13 idiomas, incluindo o português.

## Como começar

1. **Instale.** Baixe `Plugcam_x.y.z_x64-setup.exe` na [versão mais recente](https://github.com/Qwinty/plugcam/releases/latest) e execute. O Windows pede permissão de administrador uma vez, para registrar a câmera.
2. **Ative a depuração USB** no celular: *Configurações → Sobre o telefone*, toque sete vezes em *Número da versão* e depois *Configurações → Sistema → Opções do desenvolvedor → Depuração USB*. O guia inicial do Plugcam mostra o caminho.
3. **Conecte o celular**, toque em *Permitir* nele e clique em **Ligar câmera**. No seu app de vídeo, escolha **Plugcam Camera**.

O instalador ainda não é assinado, então o SmartScreen pode dizer “O Windows protegeu o computador”. Clique em *Mais informações → Executar assim mesmo*, ou confira o arquivo com o `.sha256` que acompanha a versão.

## Capturas de tela

<table>
  <tr>
    <td><img src="../screenshots/main-dark-ru.png" alt=""></td>
    <td><img src="../screenshots/settings-de.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Tema escuro igual ao Windows · Русский</td>
    <td align="center">Configurações · Deutsch</td>
  </tr>
  <tr>
    <td><img src="../screenshots/wizard-ja.png" alt=""></td>
    <td><img src="../screenshots/main-light-es.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Guia inicial · 日本語</td>
    <td align="center">Pronto para transmitir · Español</td>
  </tr>
  <tr>
    <td><img src="../screenshots/settings-dark-zh-CN.png" alt=""></td>
    <td><img src="../screenshots/wizard-dark-fr.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Configurações, tema escuro · 简体中文</td>
    <td align="center">Guia inicial · Français</td>
  </tr>
</table>

## Requisitos

- Windows 10 ou 11, 64 bits.
- Um celular com Android 12 ou mais novo (necessário para capturar a câmera) e um cabo USB que transfira dados.
- O driver USB costuma vir pelo Windows Update. Se o celular não for encontrado, instale o [driver USB do Google](https://developer.android.com/studio/run/win-usb) ou o do fabricante.

## Bom saber

- **Apps da Microsoft Store**, como o app Câmera do Windows, não enxergam a Plugcam Camera: ela é uma câmera DirectShow, usada por programas clássicos e navegadores. Uma câmera Media Foundation para apps da Store está planejada.
- **Wi-Fi** ainda não existe; é o próximo passo.
- **Algumas lentes não dão imagem.** Os celulares listam lentes que não transmitem para apps de terceiros. O Plugcam percebe, oculta a lente e troca para a câmera principal.
- **Sem som.** Use o microfone do PC ou um headset.
- O Plugcam foi feito e testado com um OnePlus 11R (Android 15) e o Chrome. Outros celulares e apps devem funcionar do mesmo jeito; se não funcionarem, [abra uma issue](https://github.com/Qwinty/plugcam/issues) com o modelo do celular e o app.

Como funciona, como compilar e licenças: veja o [README em inglês](../../README.md).

## Privacidade

O Plugcam não tem servidores e não envia nada para lugar nenhum. O vídeo vai do celular para o PC pelo cabo e fica lá.

Licença: [Apache-2.0](../../LICENSE). Componentes de terceiros: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md).
