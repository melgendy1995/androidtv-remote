pub fn command_to_keyevent(command: &str) -> Option<(&'static str, bool)> {
    match command {
        "up" => Some(("KEYCODE_DPAD_UP", false)),
        "down" => Some(("KEYCODE_DPAD_DOWN", false)),
        "left" => Some(("KEYCODE_DPAD_LEFT", false)),
        "right" => Some(("KEYCODE_DPAD_RIGHT", false)),
        "select" => Some(("KEYCODE_DPAD_CENTER", false)),
        "select_hold" => Some(("KEYCODE_DPAD_CENTER", true)),
        "menu" => Some(("KEYCODE_BACK", false)),
        "play_pause" => Some(("KEYCODE_MEDIA_PLAY_PAUSE", false)),
        "previous" => Some(("KEYCODE_MEDIA_PREVIOUS", false)),
        "next" => Some(("KEYCODE_MEDIA_NEXT", false)),
        "home" => Some(("KEYCODE_HOME", false)),
        "home_double" => Some(("KEYCODE_APP_SWITCH", false)),
        "home_hold" => Some(("KEYCODE_HOME", true)),
        "vol_up" => Some(("KEYCODE_VOLUME_UP", false)),
        "vol_down" => Some(("KEYCODE_VOLUME_DOWN", false)),
        "vol_mute" => Some(("KEYCODE_VOLUME_MUTE", false)),
        "power" => Some(("KEYCODE_POWER", false)),
        "netflix" => Some(("KEYCODE_BUTTON_3", false)),
        "youtube" => Some(("KEYCODE_BUTTON_2", false)),
        _ => None,
    }
}

pub fn escape_input_text(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            ' ' => "%s".to_string(),
            '\'' | '"' | '\\' | '&' | '<' | '>' | '|' | ';' | '(' | ')' => {
                format!("\\{c}")
            }
            c if c.is_ascii_graphic() => c.to_string(),
            _ => String::new(),
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum InputAction {
    Keyevents(Vec<&'static str>),
    Text(String),
}

pub fn field_update_actions(previous: &str, next: &str) -> Vec<InputAction> {
    if previous == next {
        return Vec::new();
    }
    if let Some(suffix) = next.strip_prefix(previous) {
        let escaped = escape_input_text(suffix);
        if escaped.is_empty() {
            return Vec::new();
        }
        return vec![
            InputAction::Keyevents(vec!["KEYCODE_MOVE_END"]),
            InputAction::Text(escaped),
        ];
    }
    if previous.starts_with(next) {
        return vec![backspace_from_end(
            previous.chars().count() - next.chars().count(),
        )];
    }
    let mut actions = vec![backspace_from_end(previous.chars().count())];
    let escaped = escape_input_text(next);
    if !escaped.is_empty() {
        actions.push(InputAction::Text(escaped));
    }
    actions
}

fn backspace_from_end(count: usize) -> InputAction {
    let mut keys = Vec::with_capacity(count + 1);
    keys.push("KEYCODE_MOVE_END");
    keys.extend(std::iter::repeat("KEYCODE_DEL").take(count));
    InputAction::Keyevents(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_only_the_new_suffix() {
        assert_eq!(
            field_update_actions("he", "hel"),
            vec![
                InputAction::Keyevents(vec!["KEYCODE_MOVE_END"]),
                InputAction::Text("l".into()),
            ]
        );
    }

    #[test]
    fn does_not_resend_identical_text() {
        assert!(field_update_actions("hello", "hello").is_empty());
    }

    #[test]
    fn backspaces_deleted_suffix() {
        assert_eq!(
            field_update_actions("hello", "hel"),
            vec![InputAction::Keyevents(vec![
                "KEYCODE_MOVE_END",
                "KEYCODE_DEL",
                "KEYCODE_DEL",
            ])]
        );
    }
}
