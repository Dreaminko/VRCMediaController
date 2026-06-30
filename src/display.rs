/// Composes the chatbox output by substituting placeholders in the
/// unified format string. {name}, {artist}, and {heartrate} are always
/// replaced — pass empty strings when a source is unavailable.
pub fn compose_chatbox(
    format: &str,
    name: &str,
    artist: &str,
    heart_rate: Option<u16>,
) -> Option<String> {
    let hr = heart_rate.map(|b| b.to_string()).unwrap_or_default();
    let text = format
        .replace("{name}", name)
        .replace("{artist}", artist)
        .replace("{heartrate}", &hr)
        .trim()
        .to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::compose_chatbox;

    #[test]
    fn combines_media_and_heart_rate() {
        assert_eq!(
            compose_chatbox(
                "🎵 {name} - {artist} | ❤️ {heartrate} bpm",
                "Song",
                "Artist",
                Some(82),
            ),
            Some("🎵 Song - Artist | ❤️ 82 bpm".to_string())
        );
    }

    #[test]
    fn supports_each_source_independently() {
        assert_eq!(
            compose_chatbox(
                "🎵 {name} - {artist} | ❤️ {heartrate} bpm",
                "Song",
                "Artist",
                None,
            ),
            Some("🎵 Song - Artist | ❤️  bpm".to_string())
        );
        assert_eq!(
            compose_chatbox(
                "🎵 {name} - {artist} | ❤️ {heartrate} bpm",
                "",
                "",
                Some(70),
            ),
            Some("🎵  -  | ❤️ 70 bpm".to_string())
        );
        assert_eq!(
            compose_chatbox("🎵 {name} - {artist} | ❤️ {heartrate} bpm", "", "", None,),
            Some("🎵  -  | ❤️  bpm".to_string())
        );
    }
}
