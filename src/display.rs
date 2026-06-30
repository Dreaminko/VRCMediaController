pub fn compose_chatbox(
    media: Option<&str>,
    heart_rate: Option<u16>,
    heart_rate_format: &str,
) -> Option<String> {
    let media = media.filter(|value| !value.trim().is_empty());
    let heart = heart_rate.map(|bpm| heart_rate_format.replace("{heartrate}", &bpm.to_string()));

    match (media, heart) {
        (Some(media), Some(heart)) if !heart.trim().is_empty() => {
            Some(format!("{} | {}", media, heart))
        }
        (Some(media), _) => Some(media.to_string()),
        (None, Some(heart)) if !heart.trim().is_empty() => Some(heart),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::compose_chatbox;

    #[test]
    fn combines_media_and_heart_rate() {
        assert_eq!(
            compose_chatbox(Some("🎵 Song"), Some(82), "❤️ {heartrate} bpm"),
            Some("🎵 Song | ❤️ 82 bpm".to_string())
        );
    }

    #[test]
    fn supports_each_source_independently() {
        assert_eq!(
            compose_chatbox(Some("🎵 Song"), None, "❤️ {heartrate} bpm"),
            Some("🎵 Song".to_string())
        );
        assert_eq!(
            compose_chatbox(None, Some(70), "❤️ {heartrate} bpm"),
            Some("❤️ 70 bpm".to_string())
        );
        assert_eq!(compose_chatbox(None, None, "❤️ {heartrate} bpm"), None);
    }
}
