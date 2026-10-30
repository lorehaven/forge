use sage_core::markdown::{Block, Inline, inline, parse, plain_text};

fn text(s: &str) -> Inline {
    Inline::Text(s.into())
}

#[quench_test::test]
fn paragraph_lines_join_and_blank_lines_separate() {
    assert_eq!(
        parse("one\ntwo\n\nthree"),
        vec![
            Block::Paragraph(vec![text("one two")]),
            Block::Paragraph(vec![text("three")])
        ]
    );
}

#[quench_test::test]
fn headings_from_one_to_six() {
    for level in 1..=6_u8 {
        let blocks = parse(&format!("{} Title", "#".repeat(level as usize)));
        assert_eq!(blocks, vec![Block::Heading(level, vec![text("Title")])]);
    }
    assert!(matches!(parse("####### no")[0], Block::Paragraph(_)));
    assert!(matches!(parse("#nospace")[0], Block::Paragraph(_)));
}

#[quench_test::test]
fn lists_group_even_across_blank_lines() {
    let blocks = parse("- **a**: x\n\n- b\n1. one\n2. two");
    assert_eq!(
        blocks,
        vec![
            Block::List {
                ordered: false,
                items: vec![
                    vec![Inline::Strong(vec![text("a")]), text(": x")],
                    vec![text("b")]
                ]
            },
            Block::List {
                ordered: true,
                items: vec![vec![text("one")], vec![text("two")]]
            },
        ]
    );
}

#[quench_test::test]
fn a_rule_is_three_or_more_of_one_mark() {
    assert_eq!(parse("---"), vec![Block::Rule]);
    assert_eq!(parse("*****"), vec![Block::Rule]);
    assert!(matches!(parse("--")[0], Block::Paragraph(_)));
    assert!(matches!(parse("-*-")[0], Block::Paragraph(_)));
}

#[quench_test::test]
fn a_table_has_a_head_and_rows_without_the_divider() {
    let blocks = parse("| a | **b** |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |");
    assert_eq!(
        blocks,
        vec![Block::Table {
            head: vec![vec![text("a")], vec![Inline::Strong(vec![text("b")])]],
            rows: vec![
                vec![vec![text("1")], vec![text("2")]],
                vec![vec![text("3")], vec![text("4")]]
            ],
        }]
    );
    // A single row is only text.
    assert!(matches!(parse("| a |")[0], Block::Paragraph(_)));
}

#[quench_test::test]
fn fences_carry_their_language_and_code_is_kept_as_written() {
    let blocks = parse("before\n```rust\nfn main() {}\n  <b>x</b>\n```\nafter");
    assert_eq!(
        blocks,
        vec![
            Block::Paragraph(vec![text("before")]),
            Block::Code {
                lang: "rust".into(),
                code: "fn main() {}\n  <b>x</b>".into()
            },
            Block::Paragraph(vec![text("after")]),
        ]
    );
    // No language: the header says "code"; a first line with spaces is code, not a language.
    assert_eq!(
        parse("```\nplain\n```"),
        vec![Block::Code {
            lang: "code".into(),
            code: "plain".into()
        }]
    );
    assert_eq!(
        parse("```let a = 1\nb```"),
        vec![Block::Code {
            lang: "code".into(),
            code: "let a = 1\nb".into()
        }]
    );
}

#[quench_test::test]
fn a_fence_still_arriving_is_a_code_block_so_far() {
    let blocks = parse("text\n```py\nprint(1)");
    assert_eq!(blocks.len(), 2);
    assert_eq!(
        blocks[1],
        Block::Code {
            lang: "py".into(),
            code: "print(1)".into()
        }
    );
}

#[quench_test::test]
fn inline_marks() {
    assert_eq!(
        inline("a **b** and *c* and `d` e"),
        vec![
            text("a "),
            Inline::Strong(vec![text("b")]),
            text(" and "),
            Inline::Em(vec![text("c")]),
            text(" and "),
            Inline::Code("d".into()),
            text(" e"),
        ]
    );
    assert_eq!(
        inline("__b__ and _i_"),
        vec![
            Inline::Strong(vec![text("b")]),
            text(" and "),
            Inline::Em(vec![text("i")])
        ]
    );
    // Code keeps its marks.
    assert_eq!(inline("`**x**`"), vec![Inline::Code("**x**".into())]);
}

#[quench_test::test]
fn underscores_inside_words_are_not_emphasis() {
    assert_eq!(inline("snake_case_name"), vec![text("snake_case_name")]);
    assert_eq!(inline("a * b"), vec![text("a * b")]);
    assert_eq!(inline("unclosed **bold"), vec![text("unclosed **bold")]);
    assert_eq!(inline("2 * 3 * 4"), vec![text("2 * 3 * 4")]);
}

#[quench_test::test]
fn links_are_clickable_only_for_safe_addresses() {
    assert_eq!(
        inline("see [the docs](https://example.com/a?b=1) now"),
        vec![
            text("see "),
            Inline::Link {
                href: "https://example.com/a?b=1".into(),
                text: vec![text("the docs")]
            },
            text(" now"),
        ]
    );
    assert_eq!(inline("[x](javascript:alert(1))"), vec![text("x)")]);
    assert_eq!(
        inline("[mail](mailto:a@b.c)"),
        vec![Inline::Link {
            href: "mailto:a@b.c".into(),
            text: vec![text("mail")]
        }]
    );
}

#[quench_test::test]
fn markup_in_text_stays_text() {
    let blocks = parse("<script>alert(1)</script> & <b>x</b>");
    assert_eq!(
        blocks,
        vec![Block::Paragraph(vec![text(
            "<script>alert(1)</script> & <b>x</b>"
        )])]
    );
}

#[quench_test::test]
fn a_tool_result_card_becomes_a_tool_block() {
    let card = "<div class=\"tool-result tool-success\"><div class=\"tool-header\"><span class=\"tool-icon\">🔍</span>\
<span class=\"tool-name\">web_search \"rust &amp; wasm\"</span></div><div class=\"tool-content\"><p>First &lt;result&gt;</p>\n<ul><li>one</li><li>two</li></ul></div></div>";
    let blocks = parse(&format!("Intro\n\n{card}\n\nOutro"));
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0], Block::Paragraph(vec![text("Intro")]));
    let Block::Tool {
        name,
        error,
        paragraphs,
    } = &blocks[1]
    else {
        panic!("{blocks:?}");
    };
    assert_eq!(name, "web_search \"rust & wasm\"");
    assert!(!error);
    assert_eq!(paragraphs.len(), 2, "{paragraphs:?}");
    assert_eq!(paragraphs[0], "First <result>");
    assert!(paragraphs[1].contains("- one") && paragraphs[1].contains("- two"));
    assert_eq!(blocks[2], Block::Paragraph(vec![text("Outro")]));
}

#[quench_test::test]
fn a_failed_tool_is_marked_and_an_open_card_is_text() {
    let failed = "<div class=\"tool-result tool-error\"><div class=\"tool-header\"><span class=\"tool-name\">calculator</span></div><div class=\"tool-content\"><p>division by zero</p></div></div>";
    let Block::Tool { error, name, .. } = &parse(failed)[0] else {
        panic!()
    };
    assert!(error);
    assert_eq!(name, "calculator");
    let open = "<div class=\"tool-result tool-success\"><div class=\"tool-header\">";
    assert!(matches!(parse(open)[0], Block::Paragraph(_)));
}

#[quench_test::test]
fn plain_text_drops_the_marks() {
    assert_eq!(
        plain_text(&inline("a **b** [c](https://x.y) `d`")),
        "a b c d"
    );
}
