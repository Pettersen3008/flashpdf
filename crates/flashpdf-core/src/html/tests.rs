use super::*;

/// A tree as `tag.class(children)`, text quoted and whitespace-only text left out.
fn show(nodes: &[Node]) -> String {
    let parts: Vec<String> = nodes
        .iter()
        .filter_map(|node| match node {
            Node::Text(text) if text.trim().is_empty() => None,
            Node::Text(text) => Some(format!("{text:?}")),
            Node::Element(element) => Some(format!(
                "{}{}({})",
                element.tag,
                element
                    .classes
                    .iter()
                    .map(|class| format!(".{class}"))
                    .collect::<String>(),
                show(&element.children)
            )),
        })
        .collect();
    parts.join(" ")
}

fn tree(source: &str) -> Result<String, String> {
    parse(source, &HashMap::new()).map(|document| show(&document.nodes))
}

#[test]
fn given_a_document_with_optional_end_tags_when_parsing_then_builds_the_nested_tree_and_collects_style(
) {
    let document = parse(
        "<!DOCTYPE html>\n<html><head><meta charset=utf-8><style>p { color: red }</style>\n<body>\n  <main class='a  b'>\n    <p>One\n    <p>Two<br>\n    <ul><li>x<li>y</ul>\n    <table><tr><td>1<td>2<tr><td colspan=2>3</table>\n  </main>\n",
        &HashMap::new(),
    )
    .unwrap();
    assert_eq!(
        show(&document.nodes),
        r#"main.a.b(p("One\n    ") p("Two" br()) ul(li("x") li("y")) table(tbody(tr(td("1") td("2")) tr(td("3")))))"#
    );
    assert_eq!(document.styles, ["p { color: red }"]);
}

#[test]
fn given_markup_a_browser_would_repair_when_parsing_then_rejects_with_line_and_column() {
    let cases = [
        ("<div><span></div>", "</div> while <span> from 1:6 is open; close it first at 1:12"),
        ("<main>\n  <div>", "<div> is never closed; add </div> at 2:3"),
        ("<div/>", "<div /> does not close in HTML; write <div></div> at 1:1"),
        ("<main></p></main>", "</p> closes no open element at 1:7"),
        ("<p><b>x<div>", "<div> would end <p> from 1:1 while <b> is open; close <b> first at 1:8"),
        ("<main>a < b</main>", "< starts no tag; write &lt; for a literal < at 1:9"),
        ("hello", "text outside an element; wrap it in one at 1:1"),
        ("<main><marquee></marquee></main>", "unsupported element <marquee> at 1:7"),
        ("<main><svg></svg></main>", "inline <svg> is not supported in HTML; pass the SVG bytes in the images option and use <img src> at 1:7"),
        ("<main onclick=x></main>", "unsupported attribute onclick on <main> at 1:1"),
        ("<main id=a id=b></main>", "duplicate attribute id on <main> at 1:1"),
    ];
    for (source, message) in cases {
        assert_eq!(tree(source).unwrap_err(), message, "{source}");
    }
}

#[test]
fn given_character_references_when_parsing_then_decodes_named_and_numeric_ones_and_rejects_a_bare_ampersand(
) {
    assert_eq!(
        tree("<p>&lt;&amp;&#8364;&#x20AC;&euro;</p>").unwrap(),
        "p(\"<&€€€\")"
    );
    assert_eq!(
        tree("<p>Fish & chips</p>").unwrap_err(),
        "& starts no character reference; write &amp; at 1:9"
    );
    assert_eq!(
        tree("<p>&bogus;</p>").unwrap_err(),
        "unknown character reference &bogus; at 1:4"
    );
}

#[test]
fn given_element_attributes_when_parsing_then_maps_them_like_element_records() {
    let images = HashMap::from([("logo".to_owned(), Picture::Image(3))]);
    let document = parse(
        r#"<a href="https://example.com/pay"><img src=logo alt="Acme"></a><ol start=3 reversed><li value="7">x</li></ol><h2 data-bookmark="">T</h2><td rowspan=2>"#,
        &images,
    )
    .unwrap();
    let Node::Element(link) = &document.nodes[0] else {
        panic!()
    };
    let Node::Element(image) = &link.children[0] else {
        panic!()
    };
    assert_eq!(link.href.as_deref(), Some("https://example.com/pay"));
    assert_eq!(image.picture, Some(Picture::Image(3)));
    assert_eq!(image.alt.as_deref(), Some("Acme"));
    let Node::Element(list) = &document.nodes[1] else {
        panic!()
    };
    assert_eq!((list.start, list.reversed), (Some(3), true));
    let Node::Element(heading) = &document.nodes[2] else {
        panic!()
    };
    assert_eq!(heading.bookmark, Bookmark::Off);
    let Node::Element(cell) = &document.nodes[3] else {
        panic!()
    };
    assert_eq!(cell.row_span, Some(2));
    assert_eq!(
        parse("<img src=chart>", &images).err().unwrap(),
        "src \"chart\" names no entry in the images option on <img> at 1:1"
    );
    assert_eq!(
        tree("<a href='javascript:x'></a>").unwrap_err(),
        "href must be an absolute http, https, or mailto URL in printable ASCII, not \"javascript:x\" on <a> at 1:1"
    );
}
