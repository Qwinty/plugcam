<p align="center">
  <img src="../../src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>Android スマートフォンを Windows の Web カメラに。</b><br>
  ケーブル 1 本、ボタン 1 つ。スマートフォンにアプリは不要、透かしなし、無料のオープンソースです。
</p>

<p align="center">
  <a href="../../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.pt-BR.md">Português</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <b>日本語</b>
</p>

<p align="center">
  <img src="../screenshots/main-light-en.png" width="860" alt="スマートフォンのカメラ映像を表示する Plugcam。右側に操作パネル">
</p>

## Plugcam の特長

- **スマートフォンに何もインストールしません。** Plugcam は USB デバッグでスマートフォンとやり取りし、配信中だけ [scrcpy](https://github.com/Genymobile/scrcpy) のカメラ部分をスマートフォンで動かします。
- **Windows の本物の Web カメラ。** 「Plugcam Camera」は、DirectShow を使うアプリやブラウザで他のカメラと並んで表示されます。Zoom、Discord、Telegram Desktop、OBS、Chrome、Edge、Firefox、そして Google Meet などの Web 通話でも使えます。
- **きれいな映像。** 最大 1080p・30 fps、対応機種なら 60 fps。H.264 のエンコードはスマートフォンのハードウェアが行うので、PC の負荷はわずかです。
- **必要な操作はそろっています。** 背面・前面カメラと各レンズの切り替え、1×/2×/5× のズーム、ライト、縦置き用の回転、左右反転。
- **ずっと無料。** 透かし、時間制限、アカウント、テレメトリはありません。Apache-2.0 ライセンス。
- **多言語対応。** 日本語を含む 13 言語。

## 使い方

1. **インストール。** [最新リリース](https://github.com/Qwinty/plugcam/releases/latest)から `Plugcam_x.y.z_x64-setup.exe` をダウンロードして実行します。カメラを登録するため、Windows が一度だけ管理者権限を求めます。
2. **スマートフォンで USB デバッグをオン**にします。*設定 → デバイス情報* で *ビルド番号* を 7 回タップし、*設定 → システム → 開発者向けオプション → USB デバッグ* をオンにします。Plugcam の初期設定ガイドが順に案内します。
3. **スマートフォンを接続**し、スマートフォンで *許可* をタップしてから **カメラをオン** を押します。ビデオ通話アプリで **Plugcam Camera** を選びます。

インストーラーはまだコード署名されていないため、SmartScreen が「Windows によって PC が保護されました」と表示することがあります。*詳細情報 → 実行* をクリックするか、リリースに添付された `.sha256` でファイルを確認してください。

## スクリーンショット

<table>
  <tr>
    <td><img src="../screenshots/main-dark-ru.png" alt=""></td>
    <td><img src="../screenshots/settings-de.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">Windows に合わせたダークテーマ · Русский</td>
    <td align="center">設定 · Deutsch</td>
  </tr>
  <tr>
    <td><img src="../screenshots/wizard-ja.png" alt=""></td>
    <td><img src="../screenshots/main-light-es.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">初期設定ガイド · 日本語</td>
    <td align="center">配信の準備完了 · Español</td>
  </tr>
  <tr>
    <td><img src="../screenshots/settings-dark-zh-CN.png" alt=""></td>
    <td><img src="../screenshots/wizard-dark-fr.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">設定（ダークテーマ） · 简体中文</td>
    <td align="center">初期設定ガイド · Français</td>
  </tr>
</table>

## 動作環境

- Windows 10 または 11（64 ビット）。
- Android 12 以降のスマートフォン（カメラの取り込みに必要）と、データ通信ができる USB ケーブル。
- スマートフォンの USB ドライバは通常 Windows Update で入ります。スマートフォンが見つからない場合は、[Google USB ドライバ](https://developer.android.com/studio/run/win-usb) かメーカーのドライバをインストールしてください。

## 知っておきたいこと

- **Microsoft Store のアプリ**（Windows のカメラ アプリなど）からは Plugcam Camera が見えません。これは従来のデスクトップ アプリやブラウザが使う DirectShow カメラだからです。Store アプリ向けの Media Foundation カメラを予定しています。
- **Wi-Fi 接続** はまだありません。次に取り組む予定です。
- **映像が出ないレンズがあります。** スマートフォンは、他社アプリには映像を出さないレンズも一覧に含めます。Plugcam はそれを検出してそのレンズを隠し、メインカメラに切り替えます。
- **音声はありません。** PC のマイクやヘッドセットをお使いください。
- Plugcam は OnePlus 11R（Android 15）と Chrome で開発・テストしました。他のスマートフォンやアプリでも同じように動くはずです。動かない場合は、機種名とアプリ名を添えて [issue を作成](https://github.com/Qwinty/plugcam/issues)してください。

仕組み、ソースからのビルド、ライセンスについては[英語の README](../../README.md) をご覧ください。

## プライバシー

Plugcam にはサーバーがなく、どこにも何も送信しません。映像はケーブルでスマートフォンから PC に届き、PC の中だけにとどまります。

ライセンス: [Apache-2.0](../../LICENSE)。サードパーティ製コンポーネント: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)。
