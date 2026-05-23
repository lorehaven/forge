use riveter::render::strip_empty_lines;

#[test]
fn test_strip_empty_lines() {
    let input = "line1\n\nline2\n  \nline3\n";
    let expected = "line1\nline2\nline3\n";
    assert_eq!(strip_empty_lines(input), expected);

    let input = "\n\n  \n";
    let expected = "\n";
    assert_eq!(strip_empty_lines(input), expected);
}
