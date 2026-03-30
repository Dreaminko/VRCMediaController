# VRChat Media Controller

![banner](images/banner.png)

[English](#english) | [中文](#中文) | [日本語](#日本語)

---

## English

A lightweight utility to bridge Windows media playback with VRChat.

This tool syncs your current media info to the VRChat Chatbox and allows you to control playback (Play/Pause, Previous, Next) directly from your Avatar's Action Menu via the OSC protocol.

### Features
- **Chatbox Synchronization**: Automatically updates the VRChat Chatbox with the current `Title - Artist`. The display format is fully customizable within the app.
- **In-VR Media Control**: Use your Avatar's radial menu to Play, Pause, or skip tracks without leaving VR.
- **Auto-Clear**: The chatbox automatically clears when media is stopped or the output is toggled off.
- **Multilingual UI**: Native support for English, Simplified Chinese, and Japanese.

### Usage
1. **Avatar Setup**: You need to add specific menus to your avatar. Ensure you have **Modular Avatar** installed, import `MediaController.unitypackage` into your Unity project, and drag the `MediaController` prefab onto your Avatar's root node.
2. **Run the App**: Launch the compiled `VRCMediaController.exe` (or run from source).
3. **VRChat Settings**: In VRChat, open your Action Menu -> Options -> OSC, and ensure **OSC is Enabled**.

---

## 中文

一个轻量级的实用工具，用于连接 Windows 系统媒体播放和 VRChat。

此工具可以将您当前正在播放的歌曲同步显示在 VRChat 聊天框中，并通过 OSC 协议，让您直接在 VRChat 内部的虚拟化身环形菜单里控制媒体播放（播放/暂停、上一首、下一首）。

### 功能特点
- **聊天框同步**：自动将当前歌曲的 `歌名 - 歌手` 更新到 VRChat 聊天框。也可以在软件界面中自定义显示格式。
- **VR内媒体控制**：直接使用 Avatar 的环形菜单来进行播放、暂停或切歌操作。
- **自动清空**：当媒体停止或关闭聊天框输出时，VRChat 的聊天框内容会自动清空。
- **多语言界面**：原生支持简体中文、英语和日语。

### 使用方法
1. **Avatar 设置**：您需要为您的模型添加特定的菜单，确保您已经安装Modular Avatar，将`MediaController.unitypackage`导入到Unity工程中，并将预制体`MediaController`拖拽到Avatar根节点下。
2. **运行程序**：启动打包好的 `VRCMediaController.exe` 程序（或从源码运行）。
3. **VRChat 设置**：在 VRChat 中，打开您的环形菜单（Action Menu） -> Options -> OSC，确保 **OSC 处于开启 (Enabled) 状态**。
---

## 日本語

Windowsのメディア再生とVRChatを連携させる軽量なユーティリティです。

現在再生中の曲情報をVRChatのチャットボックスに同期し、OSCプロトコルを通じてアバターのアクションメニューから直接メディア操作（再生/一時停止、曲送り/戻し）を可能にします。

### 特徴
- **チャットボックス同期**：再生中の「曲名 - アーティスト」をチャットボックスに自動表示します。表示形式はアプリ内で自由にカスタマイズ可能です。
- **VR内メディアコントロール**：アバターのラジアルメニュー（アクションメニュー）から、再生、一時停止、曲送りなどの操作が直接行えます。
- **自動クリア機能**：メディアが停止した際や、出力をオフにした際に、チャットボックスの内容が自動的に消去されます。
- **多言語UI**：日本語、英語、簡体字中国語に標準対応しています。

### 使い方
1. **アバターの設定**：アバターに専用のメニューを追加する必要があります。**Modular Avatar**がインストールされていることを確認し、`MediaController.unitypackage` をUnityにインポートして、`MediaController` プレハブをアバターのルート直下にドラッグ＆ドロップしてください。
2. **アプリの起動**：ビルド済みの `VRCMediaController.exe` を起動します（またはソースから実行）。
3. **VRChatの設定**：VRChat内で、アクションメニュー -> Options -> OSC を開き、**OSCが有効 (Enabled)** になっていることを確認してください。
