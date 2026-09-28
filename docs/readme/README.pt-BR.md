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
- **Imagem boa.** Até 1080p a 30 fps, ou 60 fps em celulares compatíveis. O celular codifica H.264 por hardware, o chip gráfico do PC decodifica, e o PC quase não sente.
- **Todos os controles que você espera.** Câmera traseira ou frontal e cada lente, zoom com 1×/2×/5×, lanterna, rotação para celular em pé, imagem espelhada, além de brilho, contraste, saturação e temperatura.
- **Grátis para sempre.** Sem marca d’água, sem limite de tempo, sem conta, sem telemetria. Apache-2.0.
- **Leve.** Um download de 7,4 MB. Na bandeja, ocupa 8 MB de memória; transmitindo de lá, menos de 1% da CPU. As atualizações se instalam sozinhas com um clique.
- **Fala a sua língua.** 13 idiomas, incluindo o português.

## Comparação

Os apps que as pessoas costumam testar primeiro, em setembro de 2026. Os planos gratuitos mudam com frequência, então cada nome leva à página do próprio fabricante.

| | Plugcam | [DroidCam](https://droidcam.app/) | [Iriun](https://iriun.com/) | [iVCam](https://www.e2esoft.com/ivcam/) | [Camo](https://camo.com/pricing) | [Phone Link](https://support.microsoft.com/en-us/windows/apps/phonelink/use-your-mobile-device-s-camera) |
|---|---|---|---|---|---|---|
| App no celular | **nenhum** | sim | sim | sim | sim | Link to Windows |
| Imagem grátis | **1080p, 30 ou 60 fps** | 640×480; HD com marca d’água | até 4K, com marca d’água | marca d’água; 640×480 após o teste | até 720p | 720p |
| Anúncios | **não** | sim | sim | sim | não | não |
| Conexão | USB | USB, Wi-Fi | USB, Wi-Fi | USB, Wi-Fi | USB, Wi-Fi | Wi-Fi + Bluetooth |
| Código aberto | **sim, Apache-2.0** | só o cliente para PC | não | não | não | não |
| Download para Windows | **7,4 MB** | 98 MB | 2,6 MB | cerca de 43 MB | cerca de 475 MB | já vem no Windows 11 |

Duas opções sem app que você talvez já tenha: celulares com Android 14 ou mais novo podem funcionar sozinhos como webcam USB se o fabricante ativou esse modo (os Pixel ativam), e o [scrcpy](https://github.com/Genymobile/scrcpy), no qual o Plugcam se baseia, mostra a câmera numa janela (no Windows, é preciso o OBS para transformar essa janela em webcam). O que ainda falta ao Plugcam em relação aos apps pagos: Wi-Fi, 4K e som.

### Leve para o seu PC

O Plugcam é um app nativo em Rust com uma pequena câmera virtual em C++. A janela é desenhada pelo WebView2 que já vem com o Windows (via [Tauri](https://tauri.app)), então não há um Chromium embutido como nos apps Electron. O vídeo em si nunca passa pela página web: a decodificação (com o decodificador H.264 do próprio Windows, no chip gráfico), a rotação, o redimensionamento, os ajustes de cor e a entrega dos quadros à câmera acontecem em Rust, e a janela só recebe uma pequena prévia enquanto está aberta. Fechado na bandeja, o Plugcam desliga o WebView por completo, então uma câmera rodando em segundo plano usa menos de 1% da CPU.

Medido num notebook com Ryzen 7 8845HS e Windows 11, contando o Plugcam e todos os seus processos do WebView2, com a média de um minuto feita pelo [`scripts/measure.ps1`](../../scripts/measure.ps1):

| | Memória (privada) | CPU |
|---|---|---|
| Na bandeja | **8 MB** | 0% |
| Janela aberta, câmera desligada | 211 MB | 0% |
| Transmitindo 1080p30, na bandeja | 126 MB | **0,7%** |
| Transmitindo 1080p30, janela aberta com prévia | 375 MB | 3,9% |

A CPU é a fatia dos 16 threads do processador de 8 núcleos. A transmissão foi medida com um OnePlus 11R em 1080p a 30 fps. A imagem é decodificada na Radeon 780M integrada ao processador; a memória de vídeo dela é a RAM comum, então os buffers de quadros do decodificador entram na coluna de memória.

O adb, pelo qual o Plugcam conversa com o celular, soma cerca de 9 MB; ele é compartilhado com qualquer outra ferramenta Android que você usar.

## Como começar

1. **Instale.** Baixe `Plugcam_x.y.z_x64-setup.exe` na [versão mais recente](https://github.com/Qwinty/plugcam/releases/latest) e execute. O Windows pede permissão de administrador uma vez, para registrar a câmera. Depois disso, o Plugcam aparece no menu Iniciar. Prefere não instalar? Pegue `Plugcam_x.y.z_x64-portable.zip`, extraia onde quiser e execute `Plugcam.exe`; na primeira execução ele pede permissão de administrador uma vez, para adicionar a câmera.
2. **Ative a depuração USB** no celular: *Configurações → Sobre o telefone*, toque sete vezes em *Número da versão* e depois *Configurações → Sistema → Opções do desenvolvedor → Depuração USB*. O guia inicial do Plugcam mostra o caminho.
3. **Conecte o celular**, toque em *Permitir* nele e clique em **Ligar câmera**. No seu app de vídeo, escolha **Plugcam Camera**.

O instalador ainda não é assinado, então o SmartScreen pode dizer “O Windows protegeu o computador”. Clique em *Mais informações → Executar assim mesmo*, ou confira o arquivo com o `.sha256` que acompanha a versão.

**As atualizações chegam sozinhas.** O Plugcam procura uma versão nova uma vez por dia; quando há uma, aparece o botão *Atualizar*. Um clique baixa a versão, confere a assinatura, instala e reinicia o Plugcam, que então mostra o que mudou. Tanto a versão instalada quanto a portátil se atualizam assim, e você pode desligar a verificação diária nas Configurações.

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

O Plugcam não tem servidores e não envia nada para lugar nenhum. O vídeo vai do celular para o PC pelo cabo e fica lá. A única coisa que ele busca na internet é a lista de versões no GitHub, uma vez por dia, para ver se há atualização (dá para desligar nas Configurações). As configurações ficam num arquivo JSON em `%APPDATA%\io.github.plugcam`, ou na pasta `data` da versão portátil.

Licença: [Apache-2.0](../../LICENSE). Componentes de terceiros: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md).
