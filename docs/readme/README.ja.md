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
- **きれいな映像。** 最大 1080p・30 fps、対応機種なら 60 fps。H.264 のエンコードはスマートフォンのハードウェアが、デコードは PC のグラフィックスチップが行うので、PC の負荷はわずかです。
- **必要な操作はそろっています。** 背面・前面カメラと各レンズの切り替え、1×/2×/5× のズーム、ライト、縦置き用の回転、左右反転、明るさ・コントラスト・彩度・暖かさの調整。
- **ずっと無料。** 透かし、時間制限、アカウント、テレメトリはありません。Apache-2.0 ライセンス。
- **軽量。** ダウンロードは 7.4 MB。タスクトレイで待機中のメモリ使用量は 8 MB、そのまま配信しても CPU 使用率は 1% 未満です。アップデートはワンクリックで自動的にインストールされます。
- **多言語対応。** 日本語を含む 13 言語。

## 他のアプリとの比較

多くの人が最初に試すアプリです（2026 年 9 月時点）。無料プランはよく変わるため、各名前は開発元のページにリンクしています。

| | Plugcam | [DroidCam](https://droidcam.app/) | [Iriun](https://iriun.com/) | [iVCam](https://www.e2esoft.com/ivcam/) | [Camo](https://camo.com/pricing) | [Phone Link](https://support.microsoft.com/en-us/windows/apps/phonelink/use-your-mobile-device-s-camera) |
|---|---|---|---|---|---|---|
| スマートフォンのアプリ | **不要** | 必要 | 必要 | 必要 | 必要 | Link to Windows |
| 無料版の映像 | **1080p、30 または 60 fps** | 640×480、HD は透かし入り | 最大 4K、透かし入り | 透かし入り、試用期間後は 640×480 | 最大 720p | 720p |
| 広告 | **なし** | あり | あり | あり | なし | なし |
| 接続 | USB | USB、Wi-Fi | USB、Wi-Fi | USB、Wi-Fi | USB、Wi-Fi | Wi-Fi + Bluetooth |
| オープンソース | **はい、Apache-2.0** | PC 版クライアントのみ | いいえ | いいえ | いいえ | いいえ |
| Windows 版のダウンロード | **7.4 MB** | 98 MB | 2.6 MB | 約 43 MB | 約 475 MB | Windows 11 に標準搭載 |

アプリなしで使える方法が、すでに手元にあるかもしれません。Android 14 以降のスマートフォンは、メーカーがそのモードを有効にしていれば単体で USB Web カメラとして使えます（Pixel は対応）。また、Plugcam のベースである [scrcpy](https://github.com/Genymobile/scrcpy) はカメラ映像をウィンドウに表示できます（Windows でそのウィンドウを Web カメラにするには OBS が必要です）。有料アプリと比べて Plugcam にまだ足りないのは、Wi-Fi、4K、音声です。

### PC に軽い

Plugcam は、小さな C++ 製の仮想カメラを備えたネイティブの Rust アプリです。ウィンドウは Windows に付属する WebView2 で描画するため（[Tauri](https://tauri.app) 経由）、Electron アプリのように Chromium を同梱していません。映像そのものは Web ページを通りません。デコード（Windows 標準の H.264 デコーダーで、グラフィックスチップ上で実行）、回転、拡大縮小、色の調整、カメラへのフレームの受け渡しはすべて Rust で行い、ウィンドウには開いている間だけ小さなプレビューを送ります。タスクトレイに閉じると Plugcam は WebView を完全に終了するので、バックグラウンドで動くカメラの CPU 使用率は 1% 未満です。

Ryzen 7 8845HS 搭載の Windows 11 ノート PC で、Plugcam とそのすべての WebView2 プロセスを合計し、[`scripts/measure.ps1`](../../scripts/measure.ps1) で 1 分間の平均を測定しました。

| | メモリ（プライベート） | CPU |
|---|---|---|
| タスクトレイ | **8 MB** | 0% |
| ウィンドウ表示中、カメラはオフ | 211 MB | 0% |
| 1080p30 で配信中、タスクトレイ | 126 MB | **0.7%** |
| 1080p30 で配信中、ウィンドウ表示・プレビューあり | 375 MB | 3.9% |

CPU は 8 コア CPU の全 16 スレッドに対する割合です。配信は OnePlus 11R で 1080p・30 fps で測定しました。映像のデコードは CPU 内蔵の Radeon 780M が行います。そのビデオメモリは通常の RAM なので、デコーダーのフレームバッファーもメモリの列に含まれます。

Plugcam がスマートフォンとの通信に使う adb がさらに約 9 MB 使いますが、これは他に実行している Android ツールと共有されます。

## 使い方

1. **インストール。** [最新リリース](https://github.com/Qwinty/plugcam/releases/latest)から `Plugcam_x.y.z_x64-setup.exe` をダウンロードして実行します。カメラを登録するため、Windows が一度だけ管理者権限を求めます。インストール後、Plugcam はスタート メニューに表示されます。インストールしたくない場合は、`Plugcam_x.y.z_x64-portable.zip` を好きな場所に展開して `Plugcam.exe` を実行してください。初回起動時に、カメラを追加するため一度だけ管理者権限を求めます。
2. **スマートフォンで USB デバッグをオン**にします。*設定 → デバイス情報* で *ビルド番号* を 7 回タップし、*設定 → システム → 開発者向けオプション → USB デバッグ* をオンにします。Plugcam の初期設定ガイドが順に案内します。
3. **スマートフォンを接続**し、スマートフォンで *許可* をタップしてから **カメラをオン** を押します。ビデオ通話アプリで **Plugcam Camera** を選びます。

インストーラーはまだコード署名されていないため、SmartScreen が「Windows によって PC が保護されました」と表示することがあります。*詳細情報 → 実行* をクリックするか、リリースに添付された `.sha256` でファイルを確認してください。

**アップデートは自動で届きます。** Plugcam は 1 日 1 回新しいバージョンを確認し、見つかると *更新* ボタンが表示されます。クリック 1 回でダウンロード、署名の確認、インストール、Plugcam の再起動まで行い、再起動後に変更点を表示します。インストール版もポータブル版も同じ方法でアップデートされ、毎日の確認は設定でオフにできます。

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

Plugcam にはサーバーがなく、どこにも何も送信しません。映像はケーブルでスマートフォンから PC に届き、PC の中だけにとどまります。インターネットから取得するのは、アップデートの有無を確かめるための GitHub のリリース一覧だけで、それも 1 日 1 回です（設定でオフにできます）。設定は `%APPDATA%\io.github.plugcam` の JSON ファイルに、ポータブル版では `data` フォルダーに保存されます。

ライセンス: [Apache-2.0](../../LICENSE)。サードパーティ製コンポーネント: [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)。
