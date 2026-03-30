# VRChat Media Controller

![banner](images/banner.png)

[English](#english) | [中文](#中文) | [日本語](#日本語)

---

## English

A lightweight utility to bridge Windows media playback with VRChat. 

This tool displays your currently playing track in the VRChat Chatbox and allows you to control your media (Play/Pause, Previous, Next) directly from your VRChat Avatar's radial menu using OSC.

### Features
- **Chatbox Synchronization**: Automatically updates your VRChat Chatbox with the current song's `Title - Artist`. You can customize the format in the application!
- **Headset Media Control**: Use your Avatar's radial menu to Play, Pause, or skip tracks.
- **Auto-Clear**: The chatbox automatically clears when media is stopped or the feature is toggled off.
- **Multilingual UI**: Fully supports English, Simplified Chinese, and Japanese.

### Usage
1. **Avatar Setup**: You need to add specific menus to your avatar. Import `MediaController.unitypackage` into your Unity project, and drag the `MediaController` prefab into your Avatar's root node.
2. **Run the App**: Launch the compiled `VRCMediaController.exe` program (or run from source).
3. **VRChat Settings**: In VRChat, open your Action Menu -> Options -> OSC, and make sure **OSC is Enabled**.

---

## 中文

一个轻量级的实用工具，用于连接 Windows 系统媒体播放和 VRChat。

此工具可以将您当前正在播放的歌曲同步显示在 VRChat 聊天框（头顶字）中，并通过 OSC 协议，让您直接在 VRChat 内部的虚拟化身环形菜单里控制媒体播放（播放/暂停、上一首、下一首）。

### 功能特点
- **聊天框同步**：自动将当前歌曲的 `歌名 - 歌手` 更新到 VRChat 聊天框。您可以在软件界面中自定义显示格式！
- **VR内媒体控制**：直接使用 Avatar 的环形菜单来进行播放、暂停或切歌操作。
- **自动清空**：当媒体停止或关闭聊天框输出时，VRChat 的聊天框内容会自动清空。
- **多语言界面**：原生支持简体中文、英语和日语。

### 使用方法
1. **Avatar 设置**：您需要为您的模型添加特定的菜单，将`MediaController.unitypackage`导入到Unity工程中，并将预制体`MediaController`拖拽到Avatar根节点下。
2. **运行程序**：启动打包好的 `VRCMediaController.exe` 程序（或从源码运行）。
3. **VRChat 设置**：在 VRChat 中，打开您的环形菜单（Action Menu） -> Options -> OSC，确保 **OSC 处于开启 (Enabled) 状态**。
---

## 日本語

Windowsのメディア再生とVRChatを連携させる軽量ユーティリティです。

現在再生中の曲目をVRChatのチャットボックス上に表示し、OSCを利用してVRChatのアバターのラジアルメニューから直接メディア（再生/一時停止、前へ、次へ）をコントロールできるようにします。

### 特徴
- **チャットボックス同期**：現在再生中の曲の「曲名 - アーティスト」をVRChatのチャットボックスに自動更新します。アプリケーション内でフォーマットを自由にカスタマイズ可能です！
- **VR内メディアコントロール**：アバターのラジアルメニューを使って、再生、一時停止、曲送りを操作できます。
- **自動クリア機能**：メディアが停止された時、または機能をオフにした時にチャットボックスが自動的にクリアされます。
- **多言語UI**：日本語、英語、簡体字中国語を完全サポートしています。

### 使い方
1. **アバターのセットアップ**：アバターに特定のメニューを追加する必要があります。`MediaController.unitypackage` をUnityプロジェクトにインポートし、プレハブ `MediaController` をアバターのルート配下にドラッグ＆ドロップしてください。
2. **アプリの起動**：パッケージ化された `VRCMediaController.exe` プログラムを起動します（またはソースコードを実行）。
3. **VRChatの設定**：VRChat内で、アクションメニュー -> Options -> OSC を開き、**OSCが有効 (Enabled)** になっていることを確認します。
