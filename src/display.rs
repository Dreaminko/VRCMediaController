pub fn compose_chatbox(format: &str, name: &str, artist: &str) -> Option<String> {
    let text = format
        .replace("{name}", name)
        .replace("{artist}", artist)
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
    fn combines_title_and_artist() {
        assert_eq!(
            compose_chatbox("🎵 {name} - {artist}", "Song", "Artist"),
            Some("🎵 Song - Artist".to_string())
        );
    }

    #[test]
    fn returns_none_for_empty_text() {
        assert_eq!(compose_chatbox("", "", ""), None);
        assert_eq!(compose_chatbox("   ", "", ""), None);
    }
}
