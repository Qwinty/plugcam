<p align="center">
  <img src="../../src-tauri/icons/128x128@2x.png" width="112" height="112" alt="">
</p>

<h1 align="center">Plugcam</h1>

<p align="center">
  <b>把安卓手机变成 Windows 摄像头。</b><br>
  一根数据线，一个按钮。手机上无需安装应用，没有水印，免费开源。
</p>

<p align="center">
  <a href="../../README.md">English</a> ·
  <a href="README.ru.md">Русский</a> ·
  <a href="README.de.md">Deutsch</a> ·
  <a href="README.es.md">Español</a> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.pt-BR.md">Português</a> ·
  <b>简体中文</b> ·
  <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <img src="../screenshots/main-light-en.png" width="860" alt="Plugcam 正在显示手机摄像头画面，右侧是控制面板">
</p>

## 为什么选择 Plugcam

- **手机上什么都不用装。** Plugcam 通过 USB 调试与手机通信，只在传输期间在手机上运行 [scrcpy](https://github.com/Genymobile/scrcpy) 的摄像头部分。
- **真正的 Windows 摄像头。** “Plugcam Camera”会和其他摄像头一起出现在使用 DirectShow 的软件和浏览器中：Zoom、Discord、Telegram Desktop、OBS、Chrome、Edge、Firefox，因此也能用于 Google Meet 等网页通话。
- **画质好。** 最高 1080p 30 帧/秒，支持的手机可达 60 帧/秒。手机用硬件编码 H.264，电脑几乎没有负担。
- **该有的控制都有。** 后置或前置摄像头及各个镜头、1×/2×/5× 变焦、手电筒、竖放手机时的画面旋转、镜像。
- **永久免费。** 没有水印、没有时长限制、无需账号、没有遥测。Apache-2.0 许可。
- **支持你的语言。** 13 种语言，包括简体中文。

## 快速开始

1. **安装。** 从[最新版本](https://github.com/Qwinty/plugcam/releases/latest)下载 `Plugcam_x.y.z_x64-setup.exe` 并运行。Windows 会请求一次管理员权限，用于注册摄像头。
2. **在手机上开启 USB 调试**：*设置 → 关于手机*，连续点按 *版本号* 七次，然后 *设置 → 系统 → 开发者选项 → USB 调试*。Plugcam 的初始设置向导会一步步引导你。
3. **连接手机**，在手机上点按 *允许*，然后点击 **打开摄像头**。在视频软件中选择 **Plugcam Camera**。

安装程序暂未签名，SmartScreen 可能会提示“Windows 已保护你的电脑”。点击 *更多信息 → 仍要运行*，或用发布页中附带的 `.sha256` 校验文件。

## 截图

<table>
  <tr>
    <td><img src="../screenshots/main-dark-ru.png" alt=""></td>
    <td><img src="../screenshots/settings-de.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">深色主题跟随 Windows · Русский</td>
    <td align="center">设置 · Deutsch</td>
  </tr>
  <tr>
    <td><img src="../screenshots/wizard-ja.png" alt=""></td>
    <td><img src="../screenshots/main-light-es.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">初始设置向导 · 日本語</td>
    <td align="center">准备就绪 · Español</td>
  </tr>
  <tr>
    <td><img src="../screenshots/settings-dark-zh-CN.png" alt=""></td>
    <td><img src="../screenshots/wizard-dark-fr.png" alt=""></td>
  </tr>
  <tr>
    <td align="center">设置，深色主题 · 简体中文</td>
    <td align="center">初始设置向导 · Français</td>
  </tr>
</table>

## 系统要求

- Windows 10 或 11，64 位。
- Android 12 或更高版本的手机（摄像头采集需要），以及能传输数据的 USB 数据线。
- 手机的 USB 驱动通常会通过 Windows 更新自动安装。如果找不到手机，请安装 [Google USB 驱动](https://developer.android.com/studio/run/win-usb) 或手机厂商的驱动。

## 须知

- **Microsoft Store 应用**（例如 Windows 相机应用）看不到 Plugcam Camera：它是 DirectShow 摄像头，供传统桌面软件和浏览器使用。面向 Store 应用的 Media Foundation 摄像头已在计划中。
- **Wi-Fi 连接** 暂不支持，这是下一步计划。
- **有些镜头没有画面。** 手机会列出一些不向第三方应用输出画面的镜头。Plugcam 会发现这种情况，隐藏该镜头并切换到主摄。
- **没有声音。** 请使用电脑的麦克风或耳机。
- Plugcam 基于 OnePlus 11R（Android 15）和 Chrome 开发并测试。其他手机和软件应该也能正常工作；如果不行，请[提交 issue](https://github.com/Qwinty/plugcam/issues)，并注明手机型号和软件。

工作原理、从源码构建和许可证信息，请参阅[英文 README](../../README.md)。

## 隐私

Plugcam 没有服务器，不会向任何地方发送数据。视频通过数据线从手机传到电脑，只留在你的电脑上。

许可证：[Apache-2.0](../../LICENSE)。第三方组件见 [THIRD_PARTY_NOTICES.md](../../THIRD_PARTY_NOTICES.md)。
