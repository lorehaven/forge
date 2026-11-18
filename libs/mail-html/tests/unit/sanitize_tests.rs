use mail_html::sanitize::{Options, safe_link, sanitize, to_text};

fn clean(html: &str) -> String {
    sanitize(html, &Options::default(), |_| None).html
}

fn clean_with(html: &str, remote: bool) -> mail_html::sanitize::Sanitized {
    sanitize(
        html,
        &Options {
            allow_remote_images: remote,
            ..Options::default()
        },
        |id| (id == "pic").then(|| "/api/cid/pic".to_string()),
    )
}

#[quench_test::test]
fn ordinary_formatting_survives() {
    let out = clean(
        "<p>Hello <b>bold</b> and <i>italic</i></p><ul><li>one</li><li>two</li></ul><table><tr><td>cell</td></tr></table>",
    );
    assert_eq!(
        out,
        "<p>Hello <b>bold</b> and <i>italic</i></p><ul><li>one</li><li>two</li></ul><table><tbody><tr><td>cell</td></tr></tbody></table>"
    );
}

#[quench_test::test]
fn scripts_styles_and_active_content_are_gone_with_their_content() {
    for html in [
        "<script>alert(1)</script>x",
        "<SCRIPT SRC=//evil/x.js></SCRIPT>x",
        "<style>body{background:url(//evil)}</style>x",
        "<iframe src=//evil></iframe>x",
        "<object data=x></object>x",
        "<embed src=x>x",
        "<svg onload=alert(1)><circle/></svg>x",
        "<math><mi xlink:href=javascript:alert(1)>y</mi></math>x",
        "<noscript><img src=x onerror=alert(1)></noscript>x",
        "<template><script>alert(1)</script></template>x",
        "<form action=//evil><input name=p><button>go</button></form>x",
        "<textarea>secret</textarea>x",
        "<select><option>a</option></select>x",
        "<meta http-equiv=refresh content=0;url=//evil>x",
        "<link rel=stylesheet href=//evil>x",
        "<base href=//evil>x",
        "<audio src=x></audio><video src=x></video>x",
    ] {
        let out = clean(html);
        assert_eq!(out, "x", "{html} -> {out}");
    }
}

#[quench_test::test]
fn event_handlers_and_unknown_attributes_are_never_written() {
    let out =
        clean("<p onclick=\"alert(1)\" onmouseover=alert(2) id=x class=y data-a=b name=n>t</p>");
    assert_eq!(out, "<p>t</p>");
    let out = clean("<div ONCLICK=x style=\"color:red\" title=\"hi\">t</div>");
    assert_eq!(out, "<div style=\"color:red\" title=\"hi\">t</div>");
}

#[quench_test::test]
fn javascript_and_data_links_lose_their_href_but_keep_the_text() {
    for href in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "java\tscript:alert(1)",
        "java\nscript:alert(1)",
        " javascript:alert(1)",
        "\u{1}javascript:alert(1)",
        "vbscript:x",
        "data:text/html,<script>alert(1)</script>",
        "file:///etc/passwd",
        "//evil.example/x",
        "/relative",
        "#frag",
        "ftp://x",
        "about:blank",
        "blob:x",
    ] {
        let out = clean(&format!("<a href=\"{href}\">text</a>"));
        assert_eq!(out, "<a>text</a>", "{href:?} -> {out}");
    }
}

#[quench_test::test]
fn safe_links_are_kept_and_hardened() {
    let out = clean("<a href=\"https://example.com/a?b=1&c=2\">x</a>");
    assert_eq!(
        out,
        "<a href=\"https://example.com/a?b=1&amp;c=2\" target=\"_blank\" rel=\"noopener noreferrer nofollow\">x</a>"
    );
    for ok in ["http://a.b", "mailto:a@b.c", "tel:+48123"] {
        assert!(safe_link(ok).is_some(), "{ok}");
    }
    assert!(safe_link("").is_none());
    assert!(safe_link(&format!("https://x/{}", "a".repeat(3000))).is_none());
    assert!(safe_link("https://x/ y").is_none());
}

#[quench_test::test]
fn attribute_values_are_escaped_so_they_cannot_break_out() {
    let out = clean("<a href=\"https://x/\" title='\"><script>alert(1)</script>'>t</a>");
    assert!(!out.contains("<script"), "{out}");
    assert!(out.contains("title=\"&quot;&gt;&lt;script&gt;"), "{out}");
}

#[quench_test::test]
fn text_is_escaped() {
    let out = clean("<p>1 &lt; 2 &amp;&amp; 3 &gt; 2 &lt;script&gt;</p>");
    assert_eq!(out, "<p>1 &lt; 2 &amp;&amp; 3 &gt; 2 &lt;script&gt;</p>");
}

#[quench_test::test]
fn remote_images_are_blocked_and_counted_unless_allowed() {
    let html = "<img src=\"https://tracker.example/p.gif\" width=1 height=1><img src=\"http://a/b.png\" alt=\"logo\">";
    let blocked = clean_with(html, false);
    assert_eq!(blocked.remote_blocked, 2);
    assert!(
        !blocked.html.contains("tracker.example"),
        "{}",
        blocked.html
    );
    assert!(blocked.html.contains("data-blocked=\"remote\""));
    assert!(blocked.html.contains("alt=\"logo\""));
    let allowed = clean_with(html, true);
    assert_eq!(allowed.remote_blocked, 0);
    assert!(
        allowed
            .html
            .contains("src=\"https://tracker.example/p.gif\"")
    );
}

#[quench_test::test]
fn cid_and_data_images_resolve_or_vanish() {
    let out = clean_with("<img src=\"cid:pic\"><img src=\"cid:other\">", false).html;
    assert!(out.contains("src=\"/api/cid/pic\""), "{out}");
    assert_eq!(out.matches("src=").count(), 1, "{out}");
    let out = clean("<img src=\"data:image/png;base64,iVBORw0KGgo=\">");
    assert!(out.contains("data:image/png;base64,iVBORw0KGgo="), "{out}");
    for bad in [
        "data:image/svg+xml;base64,PHN2Zz4=",
        "data:text/html;base64,PHNjcmlwdD4=",
        "data:image/png;base64,\"onerror=alert(1)",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ftp://x/y.png",
    ] {
        let out = clean(&format!("<img src='{bad}'>"));
        assert!(!out.contains("src="), "{bad} -> {out}");
    }
    let huge = format!("data:image/png;base64,{}", "A".repeat(1_100_000));
    assert!(!clean(&format!("<img src=\"{huge}\">")).contains("data:"));
}

#[quench_test::test]
fn inline_styles_are_filtered_property_by_property() {
    let out = clean(
        "<p style=\"color:red;position:fixed;top:0;background:url(http://evil/x);font-size:12px;behavior:url(x.htc);width:expression(alert(1))\">t</p>",
    );
    assert_eq!(out, "<p style=\"color:red;font-size:12px\">t</p>");
    for bad in [
        r"color:red\;x",
        "background:URL( http://evil )",
        "background-image:url(x)",
        "color:var(--x)",
        "background:image-set(x)",
        "margin:calc(1px + attr(x))",
        "color:red /* c */",
        "z-index:9999",
        "position:absolute",
        "-moz-binding:url(x)",
        "display:flex",
        "content:\"x\"",
    ] {
        let out = clean(&format!("<p style=\"{bad}\">t</p>"));
        assert_eq!(out, "<p>t</p>", "{bad} -> {out}");
    }
    assert_eq!(
        clean("<p style=\"display:none\">t</p>"),
        "<p style=\"display:none\">t</p>"
    );
}

#[quench_test::test]
fn presentational_attributes_are_validated() {
    let out = clean(
        "<table width=\"100%\" border=1 cellpadding=\"x\" bgcolor=\"#fff\" align=\"center\"><tr><td colspan=2 bgcolor=\"red;x\">c</td></tr></table>",
    );
    assert_eq!(
        out,
        "<table width=\"100%\" border=\"1\" bgcolor=\"#fff\" align=\"center\"><tbody><tr><td colspan=\"2\">c</td></tr></tbody></table>"
    );
    assert_eq!(
        clean("<font color=\"rgb(1,2,3)\" size=3>t</font>"),
        "<font color=\"rgb(1,2,3)\" size=\"3\">t</font>"
    );
    assert_eq!(
        clean("<font color=\"javascript:x\">t</font>"),
        "<font>t</font>"
    );
    assert_eq!(
        clean("<p dir=\"rtl\" lang=\"pl-PL\">t</p>"),
        "<p dir=\"rtl\" lang=\"pl-PL\">t</p>"
    );
    assert_eq!(clean("<p dir=\"x\" lang=\"<x>\">t</p>"), "<p>t</p>");
}

#[quench_test::test]
fn unknown_wrapper_tags_are_unwrapped() {
    assert_eq!(clean("<custom-tag><p>kept</p></custom-tag>"), "<p>kept</p>");
    assert_eq!(clean("<label>x</label><span>y</span>"), "x<span>y</span>");
}

#[quench_test::test]
fn mutation_xss_payloads_are_neutralised() {
    for payload in [
        "<img src=x onerror=alert(1)>",
        "<svg/onload=alert(1)>",
        "<body onload=alert(1)>",
        "<a href=\"jav&#x09;ascript:alert(1)\">x</a>",
        "<a href=\"&#106;avascript:alert(1)\">x</a>",
        "<<script>alert(1)//<</script>",
        "<img src=\"x\" onerror=\"alert(1)\"//>",
        "<div style=\"width:expression(alert(1))\">",
        "<math><mglyph><style><!--</style><img title=\"--&gt;&lt;img src=1 onerror=alert(1)&gt;\">",
        "<table background=\"javascript:alert(1)\"><tr><td>x",
        "<p title=\"\" onmouseover=\"alert(1)\">x",
        "<isindex action=javascript:alert(1)>",
        "<details open ontoggle=alert(1)>",
        "<a href=\"x\" onfocus=alert(1) autofocus>",
        "<xmp><script>alert(1)</script></xmp>",
        "<noembed><img src=x onerror=alert(1)></noembed>",
        "<plaintext><script>alert(1)</script>",
        "<!--[if gte IE 4]><script>alert(1)</script><![endif]-->",
        "<![CDATA[<script>alert(1)</script>]]>",
        "<?php echo 1 ?><img src=x onerror=alert(1)>",
    ] {
        let out = clean(payload).to_ascii_lowercase();
        for needle in [
            "<script",
            "onerror",
            "onload",
            "onclick",
            "onmouseover",
            "onfocus",
            "ontoggle",
            "javascript:",
            "expression(",
        ] {
            assert!(
                !out.contains(needle),
                "{payload} -> {out} contains {needle}"
            );
        }
    }
}

#[quench_test::test]
fn depth_and_size_are_bounded() {
    let deep = format!("{}x{}", "<div>".repeat(5000), "</div>".repeat(5000));
    let out = clean(&deep);
    assert!(out.contains('x'));
    assert!(out.matches("<div>").count() <= 70);

    let big = "<p>hello</p>".repeat(400_000);
    let result = sanitize(
        &big,
        &Options {
            max_output: 10_000,
            ..Options::default()
        },
        |_| None,
    );
    assert!(result.truncated);
    assert!(result.html.len() < 20_000);
}

#[quench_test::test]
fn empty_and_garbage_input_are_fine() {
    assert_eq!(clean(""), "");
    assert_eq!(clean("just text"), "just text");
    let _ = clean("<<<>>>&&&;;;<");
    let _ = clean("\u{0}\u{1}<p\u{0}>");
}

#[quench_test::test]
fn html_to_text_keeps_structure_and_links() {
    let text = to_text(
        "<html><head><title>T</title><style>p{}</style></head><body><h1>Title</h1><p>Hello <a href=\"https://x.example/a\">link</a>.</p><ul><li>one</li><li>two</li></ul><p>a<br>b</p><script>x()</script><img alt=\"logo\" src=x></body></html>",
    );
    assert!(text.contains("Title\n"), "{text}");
    assert!(text.contains("Hello link (https://x.example/a)."), "{text}");
    assert!(text.contains("- one\n- two"), "{text}");
    assert!(text.contains("a\nb"), "{text}");
    assert!(text.contains("logo"), "{text}");
    assert!(!text.contains("x()") && !text.contains("p{}"), "{text}");
}

#[quench_test::test]
fn text_conversion_collapses_whitespace_and_keeps_preformatted() {
    assert_eq!(to_text("<p>a   \n\n   b</p>"), "a b");
    assert_eq!(to_text("<pre>a  b\n c</pre>"), "a  b\n c");
    assert_eq!(
        to_text("<p>x</p><p></p><p></p><p></p><p>y</p>"),
        "x\n\ny".replace("\n\ny", "\ny")
    );
    assert_eq!(
        to_text("<a href=\"https://x.example\">https://x.example</a>"),
        "https://x.example"
    );
    assert_eq!(to_text("<a href=\"javascript:x\">t</a>"), "t");
    assert_eq!(
        to_text("<table><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></table>"),
        "a b\nc"
    );
    assert_eq!(to_text(""), "");
    let deep = format!("{}x{}", "<div>".repeat(3000), "</div>".repeat(3000));
    assert!(to_text(&deep).contains('x'));
}
