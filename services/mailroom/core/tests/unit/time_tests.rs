use mailroom_core::time::{forward_subject, quote, reply_subject, short, size, snippet, stamp};

const NOW: i64 = 1_790_683_200; // 2026-09-29 12:00 UTC

#[quench_test::test]
fn a_time_is_shown_in_the_readers_zone() {
    assert_eq!(stamp(NOW, 0), "2026-09-29 12:00");
    assert_eq!(stamp(NOW, 120), "2026-09-29 14:00");
    assert_eq!(stamp(NOW, -750), "2026-09-28 23:30");
    assert_eq!(stamp(0, 0), "1970-01-01 00:00");
    assert_eq!(stamp(-86_400, 0), "1969-12-31 00:00");
}

#[quench_test::test]
fn lists_show_the_time_today_the_day_this_year_and_the_date_before() {
    assert_eq!(short(NOW - 3600, NOW, 0), "11:00");
    assert_eq!(short(NOW - 86_400 * 3, NOW, 0), "Sep 26");
    assert_eq!(short(NOW - 86_400 * 400, NOW, 0), "2025-08-25");
    // Yesterday late at night is not "today" for a reader in Warsaw.
    assert_eq!(short(NOW - 15 * 3600, NOW, 120), "Sep 28");
    assert_eq!(short(NOW + 3600, NOW, 0), "13:00");
}

#[quench_test::test]
fn sizes_are_short() {
    assert_eq!(size(12), "12 B");
    assert_eq!(size(1536), "2 KB");
    assert_eq!(size(340 * 1024), "340 KB");
    assert_eq!(size(1_258_291), "1.2 MB");
}

#[quench_test::test]
fn snippets_are_one_line_and_cut_with_an_ellipsis() {
    assert_eq!(snippet("a  b\n c", 20), "a b c");
    assert_eq!(snippet("abcdefghij", 5), "abcd…");
    assert_eq!(snippet("", 5), "");
    assert_eq!(snippet("żółć żółć", 6), "żółć …");
}

#[quench_test::test]
fn replies_quote_and_subjects_do_not_stack() {
    assert_eq!(
        quote("one\ntwo", "Ann", NOW, 0),
        "\n\nOn 2026-09-29 12:00, Ann wrote:\n> one\n> two\n"
    );
    assert_eq!(reply_subject("Hello"), "Re: Hello");
    assert_eq!(reply_subject(" re: Hello"), "re: Hello");
    assert_eq!(forward_subject("Hello"), "Fwd: Hello");
    assert_eq!(forward_subject("FWD: Hello"), "FWD: Hello");
    assert_eq!(reply_subject(""), "Re: ");
}
