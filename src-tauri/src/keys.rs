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
    /// ASCII-safe payload for `adb shell input text`.
    Text(String),
    /// Payload containing non-ASCII characters (`input text` drops them);
    /// deliver via device clipboard + paste.
    PasteText(String),
}

pub fn field_update_actions(previous: &str, next: &str) -> Vec<InputAction> {
    if previous == next {
        return Vec::new();
    }
    if let Some(suffix) = next.strip_prefix(previous) {
        return append_text(Vec::new(), true, suffix);
    }
    if previous.starts_with(next) {
        return vec![backspace_from_end(
            previous.chars().count() - next.chars().count(),
        )];
    }
    let actions = vec![backspace_from_end(previous.chars().count())];
    append_text(actions, false, next)
}

fn append_text(mut actions: Vec<InputAction>, move_end_first: bool, payload: &str) -> Vec<InputAction> {
    if payload.is_empty() {
        return actions;
    }
    let ascii_safe = payload
        .chars()
        .all(|c| c.is_ascii_graphic() || c == ' ');
    if move_end_first {
        // Typing/pasting inserts at the cursor; ensure it sits at the end
        // since we don't track the device's actual cursor position.
        actions.push(InputAction::Keyevents(vec!["KEYCODE_MOVE_END"]));
    }
    actions.push(if ascii_safe {
        InputAction::Text(escape_input_text(payload))
    } else {
        InputAction::PasteText(payload.to_string())
    });
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

    #[test]
    fn non_ascii_text_uses_clipboard_paste_instead_of_input() {
        // `input text` silently drops every non-ASCII char; these must be
        // routed through clipboard+paste instead.
        assert_eq!(
            field_update_actions("", "مرحبا"),
            vec![
                InputAction::Keyevents(vec!["KEYCODE_MOVE_END"]),
                InputAction::PasteText("مرحبا".into()),
            ]
        );
        assert_eq!(
            field_update_actions("abc", "abcé"),
            vec![
                InputAction::Keyevents(vec!["KEYCODE_MOVE_END"]),
                InputAction::PasteText("é".into()),
            ]
        );
        assert_eq!(
            field_update_actions("abc", "abcx"),
            vec![
                InputAction::Keyevents(vec!["KEYCODE_MOVE_END"]),
                InputAction::Text("x".into()),
            ]
        );
    }
}
