use std::collections::HashMap;

type LangMap = HashMap<&'static str, &'static str>;

fn build_translations() -> HashMap<&'static str, LangMap> {
    let mut map = HashMap::new();

    map.insert(
        "en",
        LangMap::from([
            ("title", "VRCMediaController"),
            ("osc_online", "OSC Server: Online (9001)"),
            ("osc_error", "OSC Server: Error"),
            ("no_media", "No Media Playing"),
            ("enable_chatbox", "Enable Chatbox Output"),
            (
                "format_label",
                "Chatbox Format String:\nUse {name} and {artist} as variables.",
            ),
            ("language", "Language"),
            ("unknown", "Unknown"),
            ("unknown_artist", "Unknown Artist"),
            ("tray_show", "Show"),
            ("tray_quit", "Quit"),
            ("tray_tooltip", "VRCMediaController"),
            ("display_mode_label", "Chatbox Display Mode:"),
            ("display_mode_persistent", "Always On"),
            ("display_mode_timed", "Timed"),
            ("display_duration_label", "Duration: {n}s"),
        ]),
    );

    map.insert(
        "zh",
        LangMap::from([
            ("title", "VRChat 媒体控制器 (VRCMediaController)"),
            ("osc_online", "OSC 服务器: 在线 (9001)"),
            ("osc_error", "OSC 服务器: 错误"),
            ("no_media", "当前无媒体播放"),
            ("enable_chatbox", "启用聊天框文字输出"),
            (
                "format_label",
                "聊天框格式字符串：\n使用 {name} 和 {artist} 作为变量。",
            ),
            ("language", "语言 / Language"),
            ("unknown", "未知"),
            ("unknown_artist", "未知艺术家"),
            ("tray_show", "显示窗口"),
            ("tray_quit", "退出"),
            ("tray_tooltip", "VRChat 媒体控制器"),
            ("display_mode_label", "聊天框显示方式："),
            ("display_mode_persistent", "持续显示"),
            ("display_mode_timed", "定时显示"),
            ("display_duration_label", "显示时长：{n} 秒"),
        ]),
    );

    map.insert(
        "ja",
        LangMap::from([
            ("title", "VRChat メディアコントローラー"),
            ("osc_online", "OSC サーバー: オンライン (9001)"),
            ("osc_error", "OSC サーバー: エラー"),
            ("no_media", "再生中のメディアはありません"),
            ("enable_chatbox", "チャットボックス出力を有効にする"),
            (
                "format_label",
                "チャットボックスのフォーマット文字列：\n{name} または {artist} を変数として使用します。",
            ),
            ("language", "言語 / Language"),
            ("unknown", "不明"),
            ("unknown_artist", "不明なアーティスト"),
            ("tray_show", "ウィンドウを表示"),
            ("tray_quit", "終了"),
            ("tray_tooltip", "VRChat メディアコントローラー"),
            ("display_mode_label", "表示モード："),
            ("display_mode_persistent", "常時表示"),
            ("display_mode_timed", "タイマー表示"),
            ("display_duration_label", "表示時間：{n}秒"),
        ]),
    );

    map
}

pub struct I18n {
    translations: HashMap<&'static str, LangMap>,
}

impl I18n {
    pub fn new() -> Self {
        Self {
            translations: build_translations(),
        }
    }

    pub fn get(&self, lang: &str, key: &str) -> String {
        let en = self.translations.get("en").unwrap();
        let lang_map = self.translations.get(lang).unwrap_or(en);
        lang_map
            .get(key)
            .unwrap_or_else(|| en.get(key).unwrap_or(&key))
            .to_string()
    }
}
