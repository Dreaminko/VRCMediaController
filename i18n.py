TRANSLATIONS = {
    "en": {
        "title": "VRCMediaController",
        "osc_starting": "OSC Server: Starting...",
        "osc_online": "OSC Server: Online (9001)",
        "osc_error": "OSC Server: Error",
        "no_media": "No Media Playing",
        "enable_chatbox": "Enable Chatbox Output",
        "format_label": "Chatbox Format String:\nUse {name} and {artist} as variables.",
        "language": "Language",
        "unknown": "Unknown",
        "unknown_artist": "Unknown Artist",
        "tray_show": "Show",
        "tray_quit": "Quit",
        "tray_tooltip": "VRCMediaController",
    },
    "zh": {
        "title": "VRChat 媒体控制器 (VRCMediaController)",
        "osc_starting": "OSC 服务器: 启动中...",
        "osc_online": "OSC 服务器: 在线 (9001)",
        "osc_error": "OSC 服务器: 错误",
        "no_media": "当前无媒体播放",
        "enable_chatbox": "启用聊天框文字输出",
        "format_label": "聊天框格式字符串：\n使用 {name} 和 {artist} 作为变量。",
        "language": "语言 / Language",
        "unknown": "未知",
        "unknown_artist": "未知艺术家",
        "tray_show": "显示窗口",
        "tray_quit": "退出",
        "tray_tooltip": "VRChat 媒体控制器",
    },
    "ja": {
        "title": "VRChat メディアコントローラー",
        "osc_starting": "OSC サーバー: 起動中...",
        "osc_online": "OSC サーバー: オンライン (9001)",
        "osc_error": "OSC サーバー: エラー",
        "no_media": "再生中のメディアはありません",
        "enable_chatbox": "チャットボックス出力を有効にする",
        "format_label": "チャットボックスのフォーマット文字列：\n{name} または {artist} を変数として使用します。",
        "language": "言語 / Language",
        "unknown": "不明",
        "unknown_artist": "不明なアーティスト",
        "tray_show": "ウィンドウを表示",
        "tray_quit": "終了",
        "tray_tooltip": "VRChat メディアコントローラー",
    },
}


def get_text(lang_code, key):
    """Fallback to english if key or lang is missing."""
    lang_dict = TRANSLATIONS.get(lang_code, TRANSLATIONS["en"])
    return lang_dict.get(key, TRANSLATIONS["en"].get(key, key))
