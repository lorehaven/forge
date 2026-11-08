use loupe::pager::Key;
use loupe::terminal::{key_of, osc52};
use quench_cli::keys::{KeyCode, KeyEvent, Modifiers, Parsed, parse_key};

fn parsed(bytes: &[u8]) -> Option<Key> {
    match parse_key(bytes) {
        Parsed::Key(event, _) => key_of(event),
        other => panic!("{other:?}"),
    }
}

#[quench_test::test]
fn terminal_bytes_become_pager_keys() {
    assert_eq!(parsed(b"j"), Some(Key::Char('j')));
    assert_eq!(parsed(b"G"), Some(Key::Char('G')));
    assert_eq!(parsed(b"\x1b[A"), Some(Key::Up));
    assert_eq!(parsed(b"\x1b[6~"), Some(Key::PageDown));
    assert_eq!(parsed(b"\t"), Some(Key::Tab));
    assert_eq!(parsed(b"\x1b[Z"), Some(Key::BackTab));
    assert_eq!(parsed(b"\x7f"), Some(Key::Backspace));
    assert_eq!(parsed(b"\r"), Some(Key::Enter));
    assert_eq!(parsed(&[3]), Some(Key::Ctrl('c')));
    assert_eq!(parsed(&[26]), Some(Key::Ctrl('z')));
    let f1 = KeyEvent {
        code: KeyCode::F(1),
        modifiers: Modifiers::NONE,
    };
    assert_eq!(key_of(f1), None);
}

#[quench_test::test]
fn the_clipboard_sequence_is_osc_52() {
    assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
}

#[quench_test::test]
fn raised_signals_become_pager_signals() {
    use loupe::pager::Signal;
    use loupe::terminal::{Raised, signal_of};
    assert_eq!(
        signal_of(Raised::Interrupt, || None),
        Some(Signal::Interrupt)
    );
    assert_eq!(
        signal_of(Raised::Terminate, || None),
        Some(Signal::Terminate)
    );
    assert_eq!(signal_of(Raised::Stop, || None), Some(Signal::Stop));
    assert_eq!(signal_of(Raised::Continue, || None), Some(Signal::Continue));
    assert_eq!(
        signal_of(Raised::WindowChange, || Some((100, 40))),
        Some(Signal::Resize(100, 40))
    );
    assert_eq!(
        signal_of(Raised::WindowChange, || None),
        None,
        "no size, nothing to do"
    );
}
