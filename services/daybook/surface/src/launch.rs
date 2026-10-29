//! What a tap outside the app asks it to show. The Kotlin host writes a small file (`launch`) and the program reads it:
//! `add` and a list (the widget's **+**), `open`, a list and a task (a task of the widget or a reminder).

use daybook_core::widget::parse_list_id;
use daybook_core::{ListRef, Msg};

/// The message for the text of a launch file; `None` for anything that is not one.
#[must_use]
pub fn parse(text: &str) -> Option<Msg> {
    let mut lines = text.lines().map(str::trim);
    let action = lines.next()?;
    let list = lines
        .next()
        .and_then(parse_list_id)
        .unwrap_or(ListRef::Inbox);
    let task = lines.next().filter(|t| !t.is_empty()).map(str::to_string);
    match action {
        "add" => Some(Msg::Show {
            list,
            add: true,
            task: None,
        }),
        "open" => Some(Msg::Show {
            list,
            add: false,
            task,
        }),
        _ => None,
    }
}
