//! The unified diff `riveter diff` prints: the format of `diff -u`, so what reads one reads this.

use riveter::diff::unified;

fn lines(count: usize) -> String {
    (1..=count).fold(String::new(), |text, n| text + &format!("line {n}\n"))
}

#[quench_test::test]
fn the_same_text_has_no_diff() {
    assert_eq!(unified("a\nb\n", "a\nb\n", "old", "new"), None);
    assert_eq!(unified("", "", "old", "new"), None);
}

#[quench_test::test]
fn a_changed_line_is_shown_removed_and_added_with_its_context() {
    let old = lines(10);
    let new = old.replace("line 5\n", "line five\n");
    let diff = unified(&old, &new, "live/x", "merged/x").unwrap();
    assert_eq!(
        diff,
        "--- live/x\n+++ merged/x\n@@ -2,7 +2,7 @@\n line 2\n line 3\n line 4\n-line 5\n+line five\n line 6\n line 7\n line 8\n"
    );
}

#[quench_test::test]
fn changes_far_apart_are_separate_hunks_and_close_ones_are_one() {
    let old = lines(40);
    let far = old
        .replace("line 3\n", "three\n")
        .replace("line 38\n", "thirty-eight\n");
    let diff = unified(&old, &far, "a", "b").unwrap();
    assert_eq!(diff.matches("@@ -").count(), 2, "{diff}");

    let near = old
        .replace("line 3\n", "three\n")
        .replace("line 6\n", "six\n");
    let diff = unified(&old, &near, "a", "b").unwrap();
    assert_eq!(diff.matches("@@ -").count(), 1, "{diff}");
}

#[quench_test::test]
fn additions_and_removals_at_the_ends() {
    let diff = unified("b\n", "a\nb\nc\n", "o", "n").unwrap();
    assert_eq!(diff, "--- o\n+++ n\n@@ -1,1 +1,3 @@\n+a\n b\n+c\n");

    let diff = unified("a\nb\nc\n", "b\n", "o", "n").unwrap();
    assert_eq!(diff, "--- o\n+++ n\n@@ -1,3 +1,1 @@\n-a\n b\n-c\n");
}

#[quench_test::test]
fn a_new_object_is_all_additions() {
    let diff = unified("", "kind: A\nname: b\n", "live", "merged").unwrap();
    assert_eq!(
        diff,
        "--- live\n+++ merged\n@@ -0,0 +1,2 @@\n+kind: A\n+name: b\n"
    );
}

#[quench_test::test]
fn the_script_applies_back_to_the_new_text() {
    // Whatever the diff says to remove and add must turn the old text into the new one.
    let old: String = (0..60).fold(String::new(), |text, n| {
        text + &format!("k{}: {}\n", n % 7, n)
    });
    let new: String = (0..60)
        .filter(|n| n % 5 != 0)
        .fold(String::new(), |text, n| {
            text + &format!("k{}: {}\n", n % 7, if n % 3 == 0 { n + 100 } else { n })
        })
        + "tail: 1\n";
    let diff = unified(&old, &new, "o", "n").unwrap();

    let removed: Vec<&str> = diff
        .lines()
        .filter_map(|l| l.strip_prefix('-'))
        .filter(|l| !l.starts_with("--"))
        .collect();
    let added: Vec<&str> = diff
        .lines()
        .filter_map(|l| l.strip_prefix('+'))
        .filter(|l| !l.starts_with("++"))
        .collect();
    let kept = |text: &str, skip: &[&str]| -> Vec<String> {
        text.lines()
            .filter(|l| !skip.contains(l))
            .map(str::to_string)
            .collect()
    };
    // Lines not removed are in the new text, and the new text's other lines are the added ones.
    let survivors = kept(&old, &removed);
    let new_lines: Vec<&str> = new.lines().collect();
    for line in &survivors {
        assert!(new_lines.contains(&line.as_str()), "{line} was lost");
    }
    for line in &added {
        assert!(new_lines.contains(line), "{line} is not in the new text");
    }
    assert_eq!(survivors.len() + added.len(), new_lines.len());
}

#[quench_test::test]
fn very_long_texts_still_diff() {
    let old = lines(30_000);
    let new = old.replace("line 15000\n", "changed\n");
    let diff = unified(&old, &new, "o", "n").unwrap();
    assert!(diff.contains("-line 15000") && diff.contains("+changed"));
    // The common start and end are cut off first, so a single change is not a whole-file replacement.
    assert!(diff.lines().count() < 20, "{}", diff.lines().count());
}
