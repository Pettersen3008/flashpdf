use super::cascade::{Inline, Resolver};
use super::selector::ElemInput;
use super::*;

enum N {
    E(El),
    T(&'static str),
}

struct El {
    tag: &'static str,
    id: Option<&'static str>,
    classes: Vec<&'static str>,
    style: Option<&'static str>,
    children: Vec<N>,
}

fn e(tag: &'static str, id: &'static str, class: &'static str, children: Vec<N>) -> N {
    let id = (!id.is_empty()).then_some(id);
    N::E(El {
        tag,
        id,
        classes: class.split_whitespace().collect(),
        style: None,
        children,
    })
}
fn styled(tag: &'static str, class: &'static str, style: &'static str, children: Vec<N>) -> N {
    let N::E(mut el) = e(tag, "", class, children) else {
        unreachable!()
    };
    el.style = Some(style);
    N::E(el)
}
fn item(tag: &'static str, id: &'static str) -> N {
    e(tag, id, "", vec![N::T("x")])
}
fn list(count: usize) -> Vec<N> {
    const IDS: [&str; 7] = ["n1", "n2", "n3", "n4", "n5", "n6", "n7"];
    vec![e(
        "div",
        "",
        "",
        IDS[..count].iter().map(|id| item("p", id)).collect(),
    )]
}

type Styled = Vec<(Option<&'static str>, Cascaded)>;

fn walk(
    resolver: &Resolver,
    tree: &mut Tree,
    nodes: &[N],
    parent: Option<u32>,
    inherited: &Cascaded,
    size: Option<f64>,
    out: &mut Styled,
) -> Result<(), String> {
    let elements: Vec<&El> = nodes
        .iter()
        .filter_map(|node| if let N::E(el) = node { Some(el) } else { None })
        .collect();
    let inputs: Vec<ElemInput> = elements
        .iter()
        .map(|el| ElemInput {
            tag: el.tag,
            id: el.id,
            classes: el.classes.clone(),
            empty: el.children.iter().all(|child| matches!(child, N::T(""))),
        })
        .collect();
    let first = tree.add_siblings(parent, &inputs)?;
    for (offset, el) in elements.into_iter().enumerate() {
        let node = first + offset as u32;
        let inline = el.style.map_or(Inline::None, Inline::Text);
        let cascaded = resolver.cascade(tree, node, &inline, inherited, size, false)?;
        let own = match cascaded.get(&Key::Prop(Prop::of("fontSize"))) {
            Some(&Value::Number(own)) => Some(own),
            _ => size,
        };
        out.push((el.id, cascaded.clone()));
        walk(
            resolver,
            tree,
            &el.children,
            Some(node),
            &cascaded,
            own,
            out,
        )?;
    }
    Ok(())
}

fn resolve_at(roots: &[N], sheets: &[&str], width: f64) -> Result<Styled, String> {
    let resolver = Resolver::new(sheets, width)?;
    let mut out = Vec::new();
    walk(
        &resolver,
        &mut Tree::default(),
        roots,
        None,
        &Cascaded::default(),
        None,
        &mut out,
    )?;
    Ok(out)
}
fn resolve(roots: &[N], sheets: &[&str]) -> Result<Styled, String> {
    resolve_at(roots, sheets, 523.0)
}

fn text<'a>(styled: &'a Cascaded, name: &str) -> Option<&'a str> {
    styled.get(&Key::Prop(Prop::named(name)?))?.text()
}
fn custom<'a>(styled: &'a Cascaded, name: &str) -> Option<&'a str> {
    styled.get(&Key::Custom(name.into()))?.text()
}

/// The ids, in document order, of the elements `selector` matches in `tree`.
fn matched(selector: &str, tree: &[N]) -> String {
    let css = format!("{selector} {{ background-color: red }}");
    let styled = resolve(tree, &[&css]).unwrap();
    let ids: Vec<&str> = styled
        .iter()
        .filter(|(_, style)| text(style, "backgroundColor") == Some("red"))
        .filter_map(|(id, _)| *id)
        .collect();
    ids.join(" ")
}

fn validate(css: &str) -> Result<(), String> {
    stylesheet(css)
}
fn rejects(css: &str, parts: &[&str]) {
    let message = validate(css).expect_err(css);
    for part in parts {
        assert!(message.contains(part), "{css}: {message} lacks {part}");
    }
}
fn one(css: &str, class: &'static str) -> Result<Cascaded, String> {
    Ok(resolve(&[e("main", "", class, vec![N::T("x")])], &[css])?
        .remove(0)
        .1)
}

#[test]
fn given_text_between_siblings_when_matching_child_positions_then_only_elements_count() {
    let tree = [e(
        "main",
        "",
        "",
        vec![
            N::T("intro"),
            item("p", "a"),
            item("p", "b"),
            N::T("between"),
            item("p", "c"),
            N::T("outro"),
        ],
    )];
    assert_eq!(matched("p:first-child", &tree), "a");
    assert_eq!(matched("p:last-child", &tree), "c");
    assert_eq!(matched("p:only-child", &tree), "");
    assert_eq!(matched("p:nth-child(2)", &tree), "b");
    let alone = [e(
        "main",
        "",
        "",
        vec![N::T("a"), item("p", "x"), N::T("b")],
    )];
    assert_eq!(matched("p:only-child", &alone), "x");
}

#[test]
fn given_an_b_odd_and_even_when_matching_nth_child_then_follows_the_formula_from_either_end() {
    let tree = list(7);
    for (selector, expected) in [
        (":nth-child(odd)", "n1 n3 n5 n7"),
        (":nth-child(EVEN)", "n2 n4 n6"),
        (":nth-child(3)", "n3"),
        (":nth-child(2n+1)", "n1 n3 n5 n7"),
        (":nth-child( 3n - 1 )", "n2 n5"),
        (":nth-child(-n+3)", "n1 n2 n3"),
        (":nth-child(n+5)", "n5 n6 n7"),
        (":nth-child(0n+2)", "n2"),
        (":nth-child(0)", ""),
        (":nth-last-child(2)", "n6"),
        (":nth-last-child(-n+2)", "n6 n7"),
        (":nth-last-child(odd)", "n1 n3 n5 n7"),
        (":nth-last-child(2n)", "n2 n4 n6"),
    ] {
        assert_eq!(
            matched(&format!("p{selector}"), &tree),
            expected,
            "{selector}"
        );
    }
}

#[test]
fn given_mixed_tags_when_matching_of_type_then_each_counts_only_its_tag() {
    let tree = [e(
        "div",
        "",
        "",
        vec![
            item("h1", "a"),
            item("p", "b"),
            item("p", "c"),
            item("h1", "d"),
            item("p", "e"),
        ],
    )];
    assert_eq!(matched("p:first-of-type", &tree), "b");
    assert_eq!(matched("p:last-of-type", &tree), "e");
    assert_eq!(matched("p:nth-of-type(2)", &tree), "c");
    assert_eq!(matched("p:nth-last-of-type(odd)", &tree), "b e");
    assert_eq!(matched("div > :first-of-type", &tree), "a b");
    assert_eq!(matched("h1:only-of-type", &tree), "");
    let two = [e("div", "", "", vec![item("h1", "a"), item("p", "b")])];
    assert_eq!(matched("div > :only-of-type", &two), "a b");
}

#[test]
fn given_elements_with_and_without_children_when_matching_empty_then_only_zero_length_text_matches()
{
    let tree = [e(
        "div",
        "",
        "",
        vec![
            e("p", "a", "", vec![]),
            e("p", "b", "", vec![N::T("")]),
            e("p", "c", "", vec![N::T(" ")]),
            e("p", "d", "", vec![item("span", "e")]),
        ],
    )];
    assert_eq!(matched("p:empty", &tree), "a b");
}

#[test]
fn given_one_root_or_several_when_matching_root_then_each_root_matches() {
    assert_eq!(
        matched(":root", &[e("main", "a", "", vec![item("div", "b")])]),
        "a"
    );
    assert_eq!(
        matched(":root", &[item("main", "a"), item("main", "b")]),
        "a b"
    );
    let styled = resolve(
        &[e("main", "", "", vec![item("p", "b")])],
        &[":root { --gap: 4pt } p { padding-top: var(--gap) }"],
    )
    .unwrap();
    assert_eq!(text(&styled[1].1, "paddingTop"), Some("4pt"));
}

#[test]
fn given_combinators_and_universal_when_matching_then_each_follows_css() {
    let tree = [e(
        "div",
        "d",
        "",
        vec![
            item("h1", "h1"),
            item("p", "p1"),
            item("p", "p2"),
            e("section", "s", "", vec![item("p", "sp")]),
            item("h1", "h2"),
            item("p", "p3"),
        ],
    )];
    for (selector, expected) in [
        ("h1 + p", "p1 p3"),
        ("h1 ~ p", "p1 p2 p3"),
        ("h1~p", "p1 p2 p3"),
        ("p + p", "p2"),
        ("h1 + p + p", "p2"),
        ("h1 ~ p + section", "s"),
        ("h1 ~ h1", "h2"),
        ("div > p", "p1 p2 p3"),
        ("div p", "p1 p2 sp p3"),
        ("section > *", "sp"),
        ("* > p", "p1 p2 sp p3"),
        ("div > * + *", "p1 p2 s h2 p3"),
        ("*", "d h1 p1 p2 s sp h2 p3"),
        ("* + *", "p1 p2 s h2 p3"),
        ("section ~ p", "p3"),
        ("div > section p", "sp"),
    ] {
        assert_eq!(matched(selector, &tree), expected, "{selector}");
    }
}

#[test]
fn given_selector_lists_in_is_where_not_when_matching_then_follows_selectors_4() {
    let tree = [e(
        "div",
        "d",
        "box",
        vec![
            e("h1", "h", "a", vec![N::T("x")]),
            e("p", "p1", "a b", vec![N::T("x")]),
            e("section", "s", "", vec![e("p", "sp", "b", vec![N::T("x")])]),
        ],
    )];
    for (selector, expected) in [
        (":is(h1, section)", "h s"),
        (":where(h1, section)", "h s"),
        (":not(h1, section)", "d p1 sp"),
        (":not(.a)", "d s sp"),
        (".a:not(.b)", "h"),
        (":is(.a, .b):not(p)", "h"),
        (":where(div > p)", "p1"),
        (":not(div > p)", "d h s sp"),
        (":is(.box p, section > p)", "p1 sp"),
        (":not(:is(h1, p))", "d s"),
        (":not(:not(h1))", "h"),
        (":is(h1:first-child, p:last-child)", "h sp"),
        (":not(:first-child)", "p1 s"),
        (":is()", ""),
        (":not(:hover)", "d h p1 s sp"),
        (":is(:hover, h1)", "h"),
        (".box :where(.b, h1) + *", "p1 s"),
    ] {
        assert_eq!(matched(selector, &tree), expected, "{selector}");
    }
}

#[test]
fn given_pseudo_classes_when_cascading_then_is_and_not_count_their_most_specific_argument() {
    let color = |css: &str| {
        let styled = resolve(&[e("p", "x", "a", vec![N::T("y")])], &[css]).unwrap();
        text(&styled[0].1, "color").map(str::to_owned)
    };
    assert_eq!(
        color(":is(#x, .a) { color: red } .a.a.a { color: blue }").as_deref(),
        Some("red")
    );
    assert_eq!(
        color("p:not(#nope) { color: red } .a.a.a { color: blue }").as_deref(),
        Some("red")
    );
    assert_eq!(
        color(".a:where(#x) { color: red } .a { color: blue }").as_deref(),
        Some("blue")
    );
    assert_eq!(
        color("p:first-child { color: red } .a { color: blue }").as_deref(),
        Some("red")
    );
    assert_eq!(
        color(":is(p, #nope) { color: red } #x { color: blue }").as_deref(),
        Some("blue")
    );
    assert_eq!(
        color(":not(.b, p) { color: red } :not(.b, .c, .d) { color: blue }").as_deref(),
        Some("blue")
    );
    let classes = ".c0.c1.c2.c3.c4.c5.c6.c7.c8.c9.c10";
    let styled = resolve(
        &[e("main", "x", "c0 c1 c2 c3 c4 c5 c6 c7 c8 c9 c10", vec![])],
        &[&format!("#x {{ color: red }} {classes} {{ color: blue }}")],
    )
    .unwrap();
    assert_eq!(text(&styled[0].1, "color"), Some("red"));
}

#[test]
fn given_selectors_that_match_no_rendered_element_when_validating_then_skips_even_invalid_declarations(
) {
    let css = ".a:hover, .a:focus-visible, .a:active, a:visited, button:disabled, .a:checked { letter-spacing: 1pt }
		.a::before, .a::after, .a::placeholder, .a::selection, ::backdrop, .a:before, p::first-line { letter-spacing: 1pt }
		a[href]::after, input[type=text]:focus, .a:hover:has(.b), :is(:hover, :focus) { letter-spacing: 1pt }
		:host, :host(.a), html, body, button, .a:not(:is(div)) > svg { letter-spacing: 1pt }
		.a:-moz-focusring, .a:-webkit-autofill, .a:-ms-input-placeholder { letter-spacing: 1pt }";
    validate(css).unwrap();
    assert_eq!(matched(":not(:hover):not(:focus)", &[item("p", "x")]), "x");
}

#[test]
fn given_selectors_outside_the_subset_when_validating_then_rejects_each_with_reason_and_position() {
    for (css, parts) in [
        (
            "a[href] { color: red }",
            &["attribute selectors", "in selector \"a[href]\" at 1:1"][..],
        ),
        (
            ".a[data-x='a,b'], .c { color: red }",
            &["attribute selectors", "in selector \".a[data-x='a,b']\""],
        ),
        (
            ".a:has(.b) { color: red }",
            &[":has() (not implemented) in selector \".a:has(.b)\" at 1:1"],
        ),
        (
            "\n  p:lang(en) { color: red }",
            &[":lang() (not implemented) in selector \"p:lang(en)\" at 2:3"],
        ),
        (".a:dir(rtl), .b { color: red }", &[":dir()"]),
        (
            ":scope, :defined { color: red }",
            &[":scope (not implemented)"],
        ),
        (".a:unknown { color: red }", &[":unknown (not implemented)"]),
        ("*|p { color: red }", &["namespaces"]),
        ("|p { color: red }", &["namespaces"]),
        (
            "p:nth-child(2 of .a) { color: red }",
            &["(use An+B, odd, or even; \"of <selector>\" is not implemented)"],
        ),
        ("p:nth-child(n+) { color: red }", &[":nth-child(n+)"]),
        ("p:nth-of-type() { color: red }", &[":nth-of-type()"]),
        (":is(.a, [x]) { color: red }", &["attribute selectors"]),
        (":not(.a:has(.b)) { color: red }", &[":has()"]),
        (
            "@media print { .a[x] { color: red } }",
            &["attribute selectors"],
        ),
        (
            "@media (prefers-color-scheme: dark) { .a:has(.b) { color: red } }",
            &[":has()"],
        ),
    ] {
        rejects(css, parts);
    }
}

#[test]
fn given_malformed_selectors_when_validating_then_rejects_instead_of_skipping() {
    for css in [
        ". { color: red }",
        ".a > { color: red }",
        "> .a { color: red }",
        ".a >> .b { color: red }",
        ".a, { color: red }",
        "..a { color: red }",
        "#.a { color: red }",
        ".a:: { color: red }",
        ".a$ { color: red }",
        "*a { color: red }",
        ":is(.a { color: red }",
        ".a[x { color: red }",
        ":not(.a,) { color: red }",
    ] {
        rejects(css, &["invalid CSS selector in selector"]);
    }
    rejects(
        &format!("{}.a{} {{ color: red }}", ":is(".repeat(17), ")".repeat(17)),
        &[":is() nests deeper than 16 levels"],
    );
    validate(&format!(
        "{}.a{} {{ color: red }}",
        ":is(".repeat(16),
        ")".repeat(16)
    ))
    .unwrap();
}

#[test]
fn given_a_long_sibling_chain_on_a_wide_list_when_matching_then_stays_linear() {
    let mut children: Vec<N> = (0..20_000)
        .map(|_| e("span", "", "", vec![N::T("x")]))
        .collect();
    children.push(item("p", "p"));
    let css = format!("p ~ {}span {{ color: red }}", "span ~ ".repeat(7));
    let started = std::time::Instant::now();
    let styled = resolve(&[e("div", "", "", children)], &[&css]).unwrap();
    assert!(started.elapsed().as_secs() < 2);
    assert!(styled
        .iter()
        .all(|(_, style)| text(style, "color").is_none()));
}

#[test]
fn given_long_descendant_selectors_in_is_and_not_on_a_deep_tree_when_matching_then_stays_polynomial(
) {
    let mut element = N::T("x");
    for _ in 0..40 {
        element = e("div", "", "", vec![element]);
    }
    let chain = format!("section {}", "div ".repeat(8));
    let sheets = [
        format!(":not({chain}) {{ color: red }}"),
        format!(":is({chain}, {chain}p, {chain}div ~ div) {{ color: red }}"),
        format!(":where({chain}) > :not(:is({chain}, {chain})) {{ color: red }}"),
        format!("{chain}{{ color: red }}"),
    ];
    let sheets: Vec<&str> = sheets.iter().map(String::as_str).collect();
    let started = std::time::Instant::now();
    resolve(&[element], &sheets).unwrap();
    assert!(started.elapsed().as_secs() < 2);
}

#[test]
fn given_css_outside_the_subset_when_validating_then_rejects_with_its_source_position() {
    for (css, parts) in [
        (".a { box-shadow: 1pt }", &["box-shadow", "\".a\" at 1:1"][..]),
        (".a { grid-template-columns: 1fr }", &["unsupported CSS property: grid-template-columns", "\".a\" at 1:1"]),
        (".a { color }", &["invalid CSS declaration: color", "at 1:1"]),
        (".a { color: #000 } oops", &["invalid CSS stylesheet at 1:20"]),
        ("{bad}p { color: red }", &["invalid CSS stylesheet"]),
        ("@layer x { .a { color: red }", &["invalid CSS stylesheet at 1:1"]),
        ("/* one\n\n\n*/\n.a { box-shadow: 1pt }", &["\".a\" at 5:1"]),
        ("/* ✓ 😀 */ .a { box-shadow: 1pt }", &["\".a\" at 1:12"]),
        (&format!("/*{}", "a/*".repeat(10_000)), &["invalid CSS stylesheet"]),
        ("p { fontSize: 20pt }", &["unsupported CSS property: fontSize (use kebab-case: font-size) in selector \"p\" at 1:1"]),
    ] {
        rejects(css, parts);
    }
}

#[test]
fn given_at_rules_and_selectors_that_match_nothing_when_validating_then_skips_them() {
    validate(
        r#"@import "x.css"; @layer a, b; @media print { .a { line-height: 1 } }
		.a:hover, a[href]::after, .a::before, input[type=text]:focus, :host { line-height: 1; content: "}" }
		html, body button { -webkit-text-size-adjust: 100%; color: notacolor }
		@layer utilities { .\[class\*\=\'size-\'\] { width: 1rem } }
		.a { color: red }"#,
    )
    .unwrap();
}

#[test]
fn given_important_declarations_when_cascading_then_they_beat_specificity_and_inline_styles() {
    let red = resolve(
        &[e("main", "x", "a", vec![])],
        &["#x { color: blue } .a { color: red !important }"],
    )
    .unwrap();
    assert_eq!(text(&red[0].1, "color"), Some("red"));
    let green = resolve(
        &[styled("main", "a", "color: green !important", vec![])],
        &[".a { color: red !important }"],
    )
    .unwrap();
    assert_eq!(text(&green[0].1, "color"), Some("green"));
}

const TAILWIND: &str = r#"/*! tailwindcss v4.1.0 | MIT License | https://tailwindcss.com */
@layer properties {
  @supports ((-webkit-hyphens: none) and (not (margin-trim: inline))) or ((-moz-orient: inline) and (not (color:rgb(from red r g b)))) {
    *, :before, :after, ::backdrop { --tw-font-weight: initial; }
  }
}
@layer theme, base, components, utilities;
@layer theme { :root, :host { --spacing: .25rem; --color-red-500: oklch(63.7% 0.237 25.331); } }
@layer base { *, ::before, ::after { box-sizing: border-box; border: 0 solid; } }
.p-4 { padding: 1rem; }
.p-5 { padding: calc(var(--spacing) * 5); }
.md\:flex { display: flex; }
@property --tw-font-weight { syntax: "*"; inherits: false; initial-value: 0; }
@media (width >= 48rem) { .md\:flex { display: flex; } }
"#;

#[test]
fn given_a_tailwind_sheet_when_resolving_then_uses_only_matching_rules_and_names_undefined_variables(
) {
    validate(TAILWIND).unwrap();
    assert_eq!(
        text(&one(TAILWIND, "p-4").unwrap(), "padding"),
        Some("1rem")
    );
    let message = one(TAILWIND, "p-5").unwrap_err();
    assert!(
        message.starts_with("undefined CSS variable: --spacing (rules inside an @layer block"),
        "{message}"
    );
    assert!(message.ends_with(" on <main.p-5>"), "{message}");
    assert_eq!(
        custom(&one(TAILWIND, "md:flex").unwrap(), "--tw-font-weight"),
        Some("0")
    );
}

#[test]
fn given_bounded_variables_when_resolving_then_rejects_cycles_and_oversized_expansion() {
    let cyclic = styled("main", "", "--x: var(--x)var(--x); color: var(--x)", vec![]);
    assert!(resolve(&[cyclic], &[])
        .unwrap_err()
        .contains("cyclic CSS variable: --x"));
    let large: &'static str =
        Box::leak(format!("--x: {}; color: var(--x)", "x".repeat(64 * 1024 + 1)).into_boxed_str());
    assert!(resolve(&[styled("main", "", large, vec![])], &[])
        .unwrap_err()
        .contains("CSS variable output is too large"));
    let wide: &'static str = Box::leak(
        format!("--a: x; --b: {}; color: var(--b)", "var(--a)".repeat(1025)).into_boxed_str(),
    );
    assert!(resolve(&[styled("main", "", wide, vec![])], &[])
        .unwrap_err()
        .contains("CSS variable expansion is too large"));
}

#[test]
fn given_a_var_fallback_with_parentheses_or_a_nested_var_when_resolving_then_substitutes_it() {
    let styled = resolve(
        &[e("main", "", "a", vec![e("p", "", "b", vec![])])],
        &[
            ".a { color: var(--missing, rgb(255, 0, 0)) }",
            ".b { --blue: blue; color: var(--missing, var(--blue)) }",
        ],
    )
    .unwrap();
    assert_eq!(text(&styled[0].1, "color"), Some("rgb(255, 0, 0)"));
    assert_eq!(text(&styled[1].1, "color"), Some("blue"));
}

#[test]
fn given_an_uppercase_type_selector_and_mixed_case_names_when_cascading_then_matches_case_insensitively(
) {
    let styled = resolve(
        &[e("p", "", "u", vec![])],
        &["P { color: red } .u { TEXT-DECORATION-LINE: underline }"],
    )
    .unwrap();
    assert_eq!(text(&styled[0].1, "color"), Some("red"));
    assert_eq!(text(&styled[0].1, "textDecoration"), Some("underline"));
}

#[test]
fn given_escaped_class_selectors_when_matching_then_they_match_the_unescaped_name() {
    let css = r".w-1\/2 { width: 50% } .w-\[120px\] { width: 120px } .\32xl\:p-4 { padding: 4pt }
		.a\,b { color: red } .md\:\31 0 { margin: 1pt }";
    assert_eq!(text(&one(css, "w-1/2").unwrap(), "width"), Some("50%"));
    assert_eq!(
        text(&one(css, "w-[120px]").unwrap(), "width"),
        Some("120px")
    );
    assert_eq!(text(&one(css, "2xl:p-4").unwrap(), "padding"), Some("4pt"));
    assert_eq!(text(&one(css, "a,b").unwrap(), "color"), Some("red"));
    assert_eq!(text(&one(css, "md:10").unwrap(), "margin"), Some("1pt"));
    assert_eq!(text(&one(css, "w-1").unwrap(), "width"), None);
}

#[test]
fn given_user_agent_defaults_when_cascading_then_lists_nest_and_author_rules_override() {
    let lists = resolve(
        &[e(
            "ul",
            "a",
            "",
            vec![e(
                "li",
                "",
                "",
                vec![e(
                    "ul",
                    "b",
                    "",
                    vec![e("li", "", "", vec![e("ol", "c", "", vec![])])],
                )],
            )],
        )],
        &[],
    )
    .unwrap();
    let get = |id: &str, name: &str| {
        lists
            .iter()
            .find(|(own, _)| *own == Some(id))
            .and_then(|(_, style)| text(style, name))
            .map(str::to_owned)
    };
    assert_eq!(get("a", "margin").as_deref(), Some("1em 0"));
    assert_eq!(get("a", "listStyleType").as_deref(), Some("disc"));
    assert_eq!(get("b", "margin").as_deref(), Some("0"));
    assert_eq!(get("b", "listStyleType").as_deref(), Some("circle"));
    assert_eq!(get("c", "listStyleType").as_deref(), Some("decimal"));
    let heading = resolve(
        &[styled("h1", "", "margin-bottom: 3pt", vec![])],
        &["h1 { margin: 0 }"],
    )
    .unwrap();
    let keys: Vec<&str> = heading[0].1 .0.iter().map(|(key, _)| key.name()).collect();
    assert_eq!(
        keys,
        [
            "fontSize",
            "fontWeight",
            "breakAfter",
            "margin",
            "marginBottom"
        ]
    );
    assert_eq!(
        heading[0].1.get(&Key::Prop(Prop::of("fontSize"))),
        Some(&Value::Number(24.0))
    );
}

#[test]
fn given_math_over_lengths_and_numbers_when_resolving_then_evaluates_with_css_precedence() {
    let style = one(
        ".a {
		--s: 0.25rem;
		padding-top: calc(var(--s) * 4 + 2pt * (1 + 2));
		padding-right: max(4pt, calc(1rem - 3px));
		padding-bottom: clamp(2pt, 1px * 10, 6pt);
		padding-left: min(10px, 1rem);
		margin: calc(2px + 1pt) 0;
		line-height: calc(1.75 / 1.125);
		width: calc(1 / 2 * 100%);
		height: calc(0.0078125 * 1pt);
	}",
        "a",
    )
    .unwrap();
    for (name, expected) in [
        ("paddingTop", "18pt"),
        ("paddingRight", "9.75pt"),
        ("paddingBottom", "6pt"),
        ("paddingLeft", "7.5pt"),
        ("margin", "2.5pt 0"),
        ("lineHeight", "1.555556"),
        ("width", "50%"),
        ("height", "0.007813pt"),
    ] {
        assert_eq!(text(&style, name), Some(expected), "{name}");
    }
}

#[test]
fn given_em_inside_calc_when_resolving_then_font_size_uses_the_parent_and_others_the_element() {
    let styled = resolve(
        &[styled(
            "main",
            "",
            "font-size: 20pt",
            vec![e("p", "", "a", vec![])],
        )],
        &[".a { font-size: calc(1em + 2pt); padding-top: calc(1em + 2pt) }"],
    )
    .unwrap();
    assert_eq!(
        styled[1].1.get(&Key::Prop(Prop::of("fontSize"))),
        Some(&Value::Number(22.0))
    );
    assert_eq!(text(&styled[1].1, "paddingTop"), Some("24pt"));
}

#[test]
fn given_invalid_math_when_resolving_then_rejects_naming_the_expression_and_reason() {
    let deep = format!(
        ".a {{ padding: calc({}1{}) }}",
        "(".repeat(40),
        ")".repeat(40)
    );
    let long = format!(".a {{ padding: calc({}1pt) }}", "1pt + ".repeat(300));
    for (css, expected) in [
        (
            ".a { width: calc(100% - 4pt) }",
            "a percentage cannot combine with a length",
        ),
        (
            ".a { width: min(100%, 200pt) }",
            "a percentage cannot combine with a length",
        ),
        (
            ".a { padding: calc(50%) }",
            "padding does not take a percentage",
        ),
        (
            ".a { padding: calc(1 + 2pt) }",
            "invalid calc(1 + 2pt) in padding: cannot add a number and a length",
        ),
        (
            ".a { padding: calc(2pt * 2pt) }",
            "cannot multiply two lengths",
        ),
        (
            ".a { padding: calc(4pt / 2pt) }",
            "the divisor must be a number",
        ),
        (".a { padding: calc(4pt / 0) }", "division by zero"),
        (".a { padding: calc(1pt +2pt) }", "unexpected \"+2pt\""),
        (".a { padding: calc(3vh) }", "invalid length: 3vh"),
        (".a { padding: clamp(1pt, 2pt) }", "clamp() takes 3 values"),
        (&deep, "nested too deeply"),
        (&long, "longer than 1024 characters"),
    ] {
        let message = one(css, "a").unwrap_err();
        assert!(
            message.contains(expected) && message.ends_with(" on <main.a>"),
            "{css}: {message}"
        );
    }
}

#[test]
fn given_property_registrations_when_cascading_then_initial_values_apply_and_inherits_false_stops_inheritance(
) {
    let css = r#"@property --edge { syntax: "*"; inherits: false; initial-value: solid; }
		@property --tone { syntax: "*"; inherits: true; initial-value: green; }
		@property --bare { syntax: "*"; inherits: false; }
		.p { --edge: none; --tone: red; --bare: black }
		.c { border-style: var(--edge); color: var(--tone, blue); background-color: var(--bare, white) }"#;
    let styled = resolve(&[e("main", "", "p", vec![e("p", "", "c", vec![])])], &[css]).unwrap();
    assert_eq!(text(&styled[1].1, "borderStyle"), Some("solid"));
    assert_eq!(text(&styled[1].1, "color"), Some("red"));
    assert_eq!(text(&styled[1].1, "backgroundColor"), Some("white"));
    assert_eq!(text(&one(css, "c").unwrap(), "color"), Some("green"));
    rejects(
        "@property --x { syntax: '*'; initial-value: 0 }",
        &["invalid @property --x: it needs syntax and inherits"],
    );
}

#[test]
fn given_media_blocks_when_resolving_then_print_applies_screen_skips_and_width_compares() {
    let css = ".a { color: red }
		@media print { .a { color: blue } }
		@media screen { .a { color: green } }
		@media (width >= 48rem) { .m { padding: 24pt } }
		@media (min-width: 30rem) { .n { margin: 1pt } }
		@media (max-width: 30rem) { .o { margin: 2pt } }
		@media (width < 48rem) { .p { margin: 3pt } }
		@media screen and (width >= 1rem) { .q { margin: 4pt } }
		@media print and (width >= 1rem) { .r { margin: 5pt } }
		@media not print, (width > 1000pt) { .s { margin: 6pt } }
		@media print { @media (width >= 48rem) { .t { margin: 7pt } } }";
    assert_eq!(text(&one(css, "a").unwrap(), "color"), Some("blue"));
    let at = |class: &'static str, width: f64, name: &str| {
        let styled = resolve_at(&[e("main", "", class, vec![])], &[css], width).unwrap();
        text(&styled[0].1, name).map(str::to_owned)
    };
    assert_eq!(at("m", 523.0, "padding"), None);
    assert_eq!(at("m", 595.0, "padding").as_deref(), Some("24pt"));
    assert_eq!(at("n", 523.0, "margin").as_deref(), Some("1pt"));
    assert_eq!(at("o", 523.0, "margin"), None);
    assert_eq!(at("p", 523.0, "margin").as_deref(), Some("3pt"));
    assert_eq!(at("p", 595.0, "margin"), None);
    assert_eq!(at("q", 523.0, "margin"), None);
    assert_eq!(at("r", 523.0, "margin").as_deref(), Some("5pt"));
    assert_eq!(at("s", 523.0, "margin"), None);
    assert_eq!(at("t", 523.0, "margin"), None);
    assert_eq!(at("t", 595.0, "margin").as_deref(), Some("7pt"));
}

#[test]
fn given_a_media_query_outside_the_subset_when_resolving_then_rejects_only_where_a_rule_matches() {
    let dark = "@media (prefers-color-scheme: dark) { .d { color: white } }";
    validate(dark).unwrap();
    rejects(
        &format!("{dark} @media (width >= 48rem) {{ .e {{ box-shadow: 1pt }} }}"),
        &["box-shadow"],
    );
    assert_eq!(text(&one(dark, "other").unwrap(), "color"), None);
    assert_eq!(
        one(dark, "d").unwrap_err(),
        "unsupported media query \"(prefers-color-scheme: dark)\" (only all, print, screen, and width conditions are supported) in selector \".d\" at 1:39 on <main.d>"
    );
    let empty = "@media (width >=) { .d { color: white } }";
    assert!(one(empty, "d").unwrap_err().contains("(invalid length: =)"));
    rejects(
        &format!(
            "{}.a {{ color: red }}{}",
            "@media print {".repeat(17),
            "}".repeat(17)
        ),
        &["@media nests deeper than 16 levels"],
    );
}

#[test]
fn given_page_rules_when_parsing_then_collects_declarations_in_order_and_rejects_the_rest() {
    let mut sheet = Stylesheet::default();
    parse("@page { size: A5; margin: 10mm }", None, &mut sheet).unwrap();
    parse(
        "@page { margin-top: 1in !important } @PAGE { margin-top: 0 }",
        None,
        &mut sheet,
    )
    .unwrap();
    let pages: Vec<(&str, &str, bool)> = sheet
        .pages
        .iter()
        .map(|page| (page.name.as_str(), page.value.as_str(), page.important))
        .collect();
    assert_eq!(
        pages,
        [
            ("size", "A5", false),
            ("margin", "10mm", false),
            ("margin-top", "1in", true),
            ("margin-top", "0", false)
        ]
    );
    assert_eq!(sheet.pages[3].at, " in @page at 1:38");
    for (css, expected) in [
        ("@page :first { margin: 0 }", "unsupported @page selector: :first (one size and margin apply to every page) in @page at 1:1"),
        ("@page { @top-center { content: 'x' } }", "unsupported @page margin box: @top-center"),
        ("@page { marks: crop }", "unsupported @page property: marks"),
    ] {
        rejects(css, &[expected]);
    }
}

#[test]
fn given_numbers_in_css_syntax_when_converting_then_matches_javascript() {
    assert_eq!(point("1.e1pt", 12.0, false), Ok(10.0));
    assert_eq!(point(".5em", 10.0, false), Ok(5.0));
    assert_eq!(point("1.pt", 12.0, false), Ok(1.0));
    assert!(point("3.40282e38px", 12.0, false).unwrap() > 1e38);
    assert_eq!(point("1q", 12.0, false), Err("invalid length: 1q".into()));
    assert_eq!(js_number(1.5e21), "1.5e+21");
    assert_eq!(js_number(1e-7), "1e-7");
    assert_eq!(js_number(-0.0), "0");
    assert_eq!(json_string("a\"b\\\n"), r#""a\"b\\\n""#);
}

#[test]
fn given_an_invalid_value_without_var_when_validating_a_stylesheet_then_rejects_naming_the_rule() {
    rejects(
        ".a { color: notacolor }",
        &["unsupported color: notacolor in selector \".a\" at 1:1"],
    );
    rejects(
        "\n.b { margin: banana }",
        &["invalid length: banana", "\".b\" at 2:1"],
    );
    validate(".c { color: var(--brand) }").unwrap();
}

#[test]
fn given_calc_color_mix_custom_properties_and_css_wide_keywords_when_validating_a_stylesheet_then_accepts_them(
) {
    validate(
        ".a { --brand: notacolor; margin: calc(0.25rem * 2); background-color: color-mix(in oklab, #000 50%, transparent) }",
    )
    .unwrap();
    validate(".t { color: currentcolor; background-color: inherit; margin: initial }").unwrap();
}
