use std::collections::HashMap;

use super::super::page::{page_setup, Format, Margin, PageSetup};
use super::super::{parse, stylesheet, Stylesheet};
use super::boxes::{edges, Size};
use super::font::{COURIER, TIMES};
use super::*;

fn t(text: &str) -> Value {
    text.into()
}
fn n(number: f64) -> Value {
    number.into()
}
/// A style object's entries, unknown names as `Key::Other`.
fn object(entries: &[(&str, Value)]) -> Cascaded {
    let key = |name: &str| Prop::named(name).map_or_else(|| Key::Other(name.into()), Key::Prop);
    Cascaded(
        entries
            .iter()
            .map(|(name, value)| (key(name), value.clone()))
            .collect(),
    )
}
fn normalized(entries: &[(&str, Value)]) -> Result<Style, String> {
    style(&object(entries), &Style::default()).map_err(String::from)
}
fn ok(entries: &[(&str, Value)]) -> Style {
    normalized(entries).unwrap()
}
fn rejects(entries: &[(&str, Value)], part: &str) {
    let message = normalized(entries).expect_err(part);
    assert!(message.contains(part), "{message} lacks {part}");
}
fn get(s: &Style, name: &str) -> Option<Value> {
    s.get(Prop::named(name).unwrap()).cloned()
}

#[test]
fn given_pt_px_em_and_rem_lengths_when_resolving_a_point_value_then_converts_to_points() {
    assert_eq!(point(&t("12pt"), 12.0, false), Ok(12.0));
    assert_eq!(point(&t("16px"), 12.0, false), Ok(12.0));
    assert_eq!(point(&t("2em"), 10.0, false), Ok(20.0));
    assert_eq!(point(&t("2rem"), 10.0, false), Ok(24.0));
    assert_eq!(point(&t("0"), 12.0, false), Ok(0.0));
}

#[test]
fn given_mm_cm_in_pc_and_q_lengths_when_resolving_a_length_or_width_then_converts_at_72pt_per_inch()
{
    for length in ["25.4mm", "2.54cm", "1in", "6pc", "101.6Q"] {
        assert_eq!(point(&t(length), 12.0, false), Ok(72.0), "{length}");
        assert_eq!(
            dimension(&t(length), "width", false).unwrap().value,
            72.0,
            "{length}"
        );
    }
}

#[test]
fn given_an_unrecognised_length_when_resolving_a_point_value_then_rejects_it() {
    assert_eq!(
        point(&t("2vh"), 12.0, false),
        Err("invalid length: 2vh".into())
    );
    assert_eq!(
        point(&t("abc"), 12.0, false),
        Err("invalid length: abc".into())
    );
}

#[test]
fn given_1_2_3_or_4_shorthand_values_when_expanding_margin_edges_then_mirrors_css_shorthand_order()
{
    for (margin, expected) in [
        ("4pt", [4.0, 4.0, 4.0, 4.0]),
        ("4pt 8pt", [4.0, 8.0, 4.0, 8.0]),
        ("4pt 8pt 12pt", [4.0, 8.0, 12.0, 8.0]),
        ("1pt 2pt 3pt 4pt", [1.0, 2.0, 3.0, 4.0]),
    ] {
        assert_eq!(
            edges(&ok(&[("margin", t(margin))]), "margin", 12.0),
            Ok(expected)
        );
    }
}

#[test]
fn given_a_per_side_override_when_expanding_margin_edges_then_it_wins_over_the_shorthand() {
    let s = ok(&[("margin", t("4pt")), ("marginTop", t("9pt"))]);
    assert_eq!(edges(&s, "margin", 12.0), Ok([9.0, 4.0, 4.0, 4.0]));
}

#[test]
fn given_rgb_hsl_named_and_hex_colors_when_resolving_a_color_then_converges_on_the_same_rgb_triple()
{
    for value in [
        "red",
        "#f00",
        "#ff0000",
        "rgb(255, 0, 0)",
        "rgb(100% 0% 0%)",
        "rgba(255 0 0 / 1)",
        "rgba(255, 0, 0, 100%)",
        "hsl(0, 100%, 50%)",
        "hsl(360deg 100% 50%)",
        "#f00f",
        "#ff0000ff",
        "Red",
    ] {
        assert_eq!(color(&t(value)), Ok([255, 0, 0]), "{value}");
    }
    assert_eq!(color(&t("rebeccapurple")), Ok([0x66, 0x33, 0x99]));
    assert_eq!(color(&t("hsl(120 100% 25%)")), Ok([0, 128, 0]));
}

#[test]
fn given_a_translucent_color_when_resolving_it_then_rejects_with_the_opacity_reason() {
    for value in [
        "rgba(0, 0, 0, 0.5)",
        "#0008",
        "#00000080",
        "hsl(0 0% 0% / 50%)",
        "oklch(50% 0.1 20 / 50%)",
    ] {
        assert_eq!(
            color(&t(value)),
            Err("a colour with alpha is not supported; use the opacity property".into()),
            "{value}"
        );
    }
    assert_eq!(
        color(&t("rgb(from red r g b)")),
        Err("unsupported color: rgb(from red r g b)".into())
    );
}

#[test]
fn given_oklch_and_oklab_colours_when_resolving_them_then_converts_to_clipped_srgb() {
    assert_eq!(
        color(&t("oklch(63.7% 0.237 25.331)")),
        Ok([0xfb, 0x2c, 0x36])
    );
    assert_eq!(color(&t("oklab(0.62796 0.22486 0.12585)")), Ok([255, 0, 0]));
    assert_eq!(color(&t("oklch(70% 0.4 150 / 1)")), Ok([0, 0xd6, 0]));
}

#[test]
fn given_a_border_shorthand_with_rgb_spaces_when_normalizing_then_keeps_the_function_as_one_token()
{
    let s = ok(&[("border", t("1pt solid rgb(0, 0, 255)"))]);
    assert_eq!(get(&s, "borderTopColor"), Some(t("rgb(0, 0, 255)")));
    assert_eq!(color(&t("rgb(0, 0, 255)")), Ok([0, 0, 255]));
}

#[test]
fn given_each_flex_shorthand_form_when_normalizing_then_expands_it_to_grow_shrink_and_basis_as_css_does(
) {
    let longhands = |flex: Value| {
        let s = ok(&[("flex", flex)]);
        [
            get(&s, "flexGrow"),
            get(&s, "flexShrink"),
            get(&s, "flexBasis"),
        ]
        .map(Option::unwrap)
    };
    assert_eq!(longhands(n(1.0)), [n(1.0), n(1.0), n(0.0)]);
    assert_eq!(longhands(t("2 0")), [n(2.0), n(0.0), n(0.0)]);
    assert_eq!(longhands(t("1 1 0")), [n(1.0), n(1.0), t("0")]);
    assert_eq!(longhands(t("auto")), [n(1.0), n(1.0), t("auto")]);
    assert_eq!(longhands(t("none")), [n(0.0), n(0.0), t("auto")]);
    assert_eq!(longhands(t("initial")), [n(0.0), n(1.0), t("auto")]);
    assert_eq!(longhands(t("100px")), [n(1.0), n(1.0), t("100px")]);
    assert_eq!(longhands(t("50% 2")), [n(2.0), n(1.0), t("50%")]);
    rejects(&[("flex", t("1 2 3"))], "invalid flex-basis: 3");
    rejects(&[("flex", t("1 auto 2"))], "invalid flex: 1 auto 2");
}

#[test]
fn given_a_percent_height_when_sizing_a_box_then_resolves_it_against_the_parents_fixed_height_or_rejects_without_one(
) {
    let s = ok(&[
        ("height", t("50%")),
        ("width", t("auto")),
        ("maxWidth", t("none")),
        ("margin", t("0 auto")),
    ]);
    let size = Size {
        border_box: false,
        auto_margin: 2 | 8,
        width: None,
        min_width: None,
        max_width: None,
        height: Some(100.0),
        min_height: None,
        max_height: None,
    };
    assert_eq!(box_size(&s, Some(200.0), true), Ok(Some(size)));
    assert_eq!(
        box_size(&s, None, true),
        Err("a percent height needs a parent with a fixed height; use pt or px".into())
    );
}

#[test]
fn given_a_font_family_list_when_selecting_a_font_then_takes_the_first_registered_family_or_built_in_match_in_order(
) {
    let inter = FontSlots {
        regular: 2,
        bold: Some(3),
        italic: None,
        bold_italic: None,
    };
    let fonts = HashMap::from([
        ("Helvetica".to_string(), HELVETICA),
        ("Inter".to_string(), inter),
    ]);
    let pick = |family: &str| {
        let s = Style(vec![(Key::Prop(prop!("fontFamily")), t(family))]);
        font(&s, &Font::new(HELVETICA), &fonts).unwrap().family
    };
    assert_eq!(pick("'Missing', \"Inter\", sans-serif"), inter);
    assert_eq!(pick("Missing, system-ui, sans-serif"), HELVETICA);
    assert_eq!(pick("Arial"), HELVETICA);
    assert_eq!(pick("Georgia, 'Times New Roman', serif"), TIMES);
    assert_eq!(pick("Missing, monospace"), COURIER);
    assert_eq!(pick("Courier New, serif"), COURIER);
    assert_eq!(pick("Inter, serif"), inter);
    // CSS reads a quoted generic keyword as a family name, so it matches nothing here.
    assert_eq!(pick("'serif', monospace"), COURIER);
}

#[test]
fn given_a_font_family_list_with_no_match_when_selecting_a_font_then_rejects_naming_the_families_and_inherits_without_a_list(
) {
    let fonts = HashMap::from([("Helvetica".to_string(), HELVETICA)]);
    let pick = |family: &str| {
        let s = Style(vec![(Key::Prop(prop!("fontFamily")), t(family))]);
        font(&s, &Font::new(TIMES), &fonts).unwrap_err()
    };
    assert!(pick("Missing, 'Also Missing', cursive").starts_with(
        "unregistered font family: Missing, 'Also Missing', cursive (register it in fonts"
    ));
    assert!(pick("'serif'").starts_with("unregistered font family: 'serif'"));
    assert_eq!(
        font(&Style::default(), &Font::new(COURIER), &fonts)
            .unwrap()
            .family,
        COURIER
    );
}

#[test]
fn given_a_font_weight_outside_normal_bold_and_1_to_1000_when_normalizing_then_rejects_it() {
    for weight in [t("normal"), t("bold"), n(700.0), t("700")] {
        ok(&[("fontWeight", weight)]);
    }
    for weight in [t("bolder"), t("boldd"), n(0.0), t("1001")] {
        rejects(&[("fontWeight", weight)], "invalid fontWeight");
    }
}

#[test]
fn given_a_rem_width_as_minified_tailwind_emits_when_parsing_it_then_converts_at_12pt_per_rem() {
    assert_eq!(
        dimension(&t("8rem"), "width", false),
        Ok(Dimension {
            kind: 1,
            value: 96.0
        })
    );
}

#[test]
fn given_a_width_with_more_digits_than_a_float_holds_when_normalizing_then_rejects_it() {
    let width = format!("{}pt", "9".repeat(45));
    rejects(&[("width", t(&width))], &format!("invalid width: {width}"));
}

#[test]
fn given_an_oblique_angle_past_90deg_a_percent_spacing_a_wavy_line_or_another_transform_when_normalizing_then_rejects_naming_the_value(
) {
    for (name, value, message) in [
        (
            "fontStyle",
            "oblique 100deg",
            "invalid fontStyle: oblique 100deg",
        ),
        ("letterSpacing", "10%", "invalid letterSpacing: 10%"),
        ("wordSpacing", "20000pt", "invalid wordSpacing: 20000pt"),
        (
            "textDecoration",
            "underline wavy",
            "text-decoration: wavy is not supported",
        ),
        (
            "textTransform",
            "full-width",
            "invalid textTransform: full-width",
        ),
    ] {
        rejects(&[(name, t(value))], message);
    }
}

#[test]
fn given_list_style_when_normalizing_then_omitted_parts_reset_and_unsupported_parts_reject() {
    let s = ok(&[("listStyle", t("inside"))]);
    assert_eq!(get(&s, "listStyleType"), Some(t("disc")));
    assert_eq!(get(&s, "listStylePosition"), Some(t("inside")));
    let s = ok(&[("listStyleType", t("square")), ("listStyle", t("none"))]);
    assert_eq!(get(&s, "listStyleType"), Some(t("none")));
    let s = ok(&[("listStyle", t("none upper-roman"))]);
    assert_eq!(get(&s, "listStyleType"), Some(t("upper-roman")));
    rejects(
        &[("listStyle", t("url(x.png)"))],
        "list-style-image is not supported",
    );
    rejects(
        &[("listStyleType", t("lower-greek"))],
        "invalid listStyleType: lower-greek",
    );
    rejects(
        &[("listStylePosition", t("middle"))],
        "invalid listStylePosition",
    );
    rejects(
        &[("listStyleImage", t("none"))],
        "unsupported style property: listStyleImage (",
    );
    rejects(
        &[("toString", t("x"))],
        "unsupported style property: toString",
    );
}

#[test]
fn given_ordinals_outside_a_counters_range_when_numbering_then_letters_and_numerals_count_and_the_rest_fall_back_to_decimal(
) {
    let numbers = |kind: &str, ns: &[i64]| ns.iter().map(|&n| counter(kind, n)).collect::<Vec<_>>();
    assert_eq!(
        numbers("lower-alpha", &[1, 26, 27, 703]),
        ["a", "z", "aa", "aaa"]
    );
    assert_eq!(
        numbers("upper-roman", &[4, 9, 1994, 3999]),
        ["IV", "IX", "MCMXCIV", "MMMCMXCIX"]
    );
    assert_eq!(numbers("lower-alpha", &[0, -3, 4000]), ["0", "-3", "ewv"]);
    assert_eq!(numbers("lower-roman", &[0, -3, 4000]), ["0", "-3", "4000"]);
    assert_eq!(
        numbers("decimal-leading-zero", &[7, 0, -5, 123]),
        ["07", "00", "-05", "123"]
    );
}

#[test]
fn given_white_space_and_vertical_align_values_when_normalizing_then_only_the_supported_keywords_pass(
) {
    for value in ["normal", "nowrap", "pre", "pre-wrap", "pre-line"] {
        ok(&[("whiteSpace", t(value))]);
    }
    rejects(
        &[("whiteSpace", t("break-spaces"))],
        "invalid whiteSpace: break-spaces",
    );
    for value in ["baseline", "sub", "super", "top", "middle", "bottom"] {
        ok(&[("verticalAlign", t(value))]);
    }
    rejects(&[("verticalAlign", t("toString"))], "invalid verticalAlign");
}

#[test]
fn given_inherit_initial_and_currentcolor_when_normalizing_then_they_take_the_parents_value_the_initial_value_and_the_elements_colour(
) {
    let parent = ok(&[
        ("color", t("red")),
        ("backgroundColor", t("blue")),
        ("margin", t("4pt")),
    ]);
    let child = style(
        &object(&[
            ("backgroundColor", t("inherit")),
            ("margin", t("inherit")),
            ("color", t("initial")),
        ]),
        &parent,
    )
    .unwrap();
    assert_eq!(get(&child, "color"), Some(t("black")));
    let boxed = box_style(&child, 12.0, 1.0).unwrap().unwrap();
    assert_eq!(boxed.background, Some([0, 0, 255]));
    assert_eq!(boxed.margin, [4.0; 4]);
    // The cascade hands the child its inherited colour.
    let current = style(
        &object(&[
            ("color", t("red")),
            ("borderBottom", t("1pt solid currentcolor")),
        ]),
        &parent,
    )
    .unwrap();
    assert_eq!(
        box_style(&current, 12.0, 1.0)
            .unwrap()
            .unwrap()
            .border_color[2],
        [255, 0, 0]
    );
}

fn pages(sheets: &[&str]) -> Stylesheet {
    let mut sheet = Stylesheet::default();
    for source in sheets {
        parse(source, None, &mut sheet).unwrap();
    }
    sheet
}
fn css(sheets: &[&str]) -> PageSetup {
    page_setup(None, None, None, &pages(sheets).pages).unwrap()
}
fn pt(length: &str) -> f64 {
    point(&t(length), 12.0, false).unwrap()
}

#[test]
fn given_each_css_size_form_when_resolving_at_page_then_sizes_the_page_in_points() {
    for (size, expected) in [
        ("auto", (595.0, 842.0)),
        ("landscape", (842.0, 595.0)),
        ("portrait LEGAL", (612.0, 1008.0)),
        ("A3 landscape", (1191.0, 842.0)),
        ("148mm 210mm", (pt("148mm"), pt("210mm"))),
        ("5in", (360.0, 360.0)),
    ] {
        let page = css(&[&format!("@page {{ size: {size} }}")]);
        assert_eq!((page.width, page.height), expected, "{size}");
    }
}

#[test]
fn given_at_page_rules_across_stylesheets_when_resolving_the_page_then_later_declarations_win_and_important_beats_both(
) {
    let page = css(&[
        "@page { size: A5; margin: 10mm }",
        "@page { margin-top: 1in !important } @PAGE { margin-top: 0; margin-left: 2cm }",
    ]);
    assert_eq!(
        page,
        PageSetup {
            width: 420.0,
            height: 595.0,
            margin: [72.0, pt("10mm"), pt("10mm"), pt("2cm")],
        }
    );
}

#[test]
fn given_render_options_and_at_page_when_resolving_the_page_then_each_option_replaces_its_at_page_value_and_orientation_turns_either_size(
) {
    let sheet = pages(&["@page { size: A3; margin: 1in }"]);
    let landscape = t("landscape");
    assert_eq!(
        page_setup(None, Some(&landscape), None, &sheet.pages),
        Ok(PageSetup {
            width: 1191.0,
            height: 842.0,
            margin: [72.0; 4],
        })
    );
    let format = Format::Size {
        width: Some(t("100mm")),
        height: Some(n(200.0)),
    };
    let margin = Margin::All(t("20mm 15mm"));
    assert_eq!(
        page_setup(Some(&format), Some(&landscape), Some(&margin), &sheet.pages)
            .unwrap()
            .margin,
        [pt("20mm"), pt("15mm"), pt("20mm"), pt("15mm")]
    );
    let margin = Margin::Sides([Some(n(10.0)), Some(t("1cm")), Some(n(0.0)), Some(t("2pc"))]);
    assert_eq!(
        page_setup(
            Some(&Format::Named(t("Letter"))),
            None,
            Some(&margin),
            &sheet.pages
        ),
        Ok(PageSetup {
            width: 612.0,
            height: 792.0,
            margin: [10.0, pt("1cm"), 0.0, 24.0],
        })
    );
}

#[test]
fn given_page_setup_outside_the_supported_subset_when_validating_then_rejects_with_a_reason() {
    for (source, message) in [
        (
            "@page :first { margin: 0 }",
            "unsupported @page selector: :first",
        ),
        (
            "@page { @top-center { content: 'x' } }",
            "unsupported @page margin box: @top-center",
        ),
        ("@page { marks: crop }", "unsupported @page property: marks"),
        ("@page { size: B5 }", "unsupported page size: B5"),
        ("@page { size: A4 10mm }", "invalid page size: A4 10mm"),
        ("@page { size: 15000pt 10in }", "invalid page size: 15000pt"),
        (
            "@page { margin: 10% }",
            "invalid margin: 10% in @page at 1:1",
        ),
    ] {
        let message_seen = stylesheet(source).expect_err(source);
        assert!(
            message_seen.contains(message),
            "{message_seen} lacks {message}"
        );
    }
    let a4 = Format::Named(t("A4"));
    let tiny = Format::Size {
        width: Some(t("0mm")),
        height: Some(n(100.0)),
    };
    let top = Margin::Sides([Some(n(10.0)), None, None, None]);
    let wide = Margin::All(t("0 300pt"));
    let (sideways, landscape) = (t("sideways"), t("landscape"));
    let a5 = Format::Named(t("A5"));
    for (format, orientation, margin, message) in [
        (&a4, Some(&sideways), None, "invalid orientation: sideways"),
        (&tiny, None, None, "invalid page width: 0mm"),
        (&a4, None, Some(&top), "invalid margin right: undefined"),
        (
            &a5,
            Some(&landscape),
            Some(&wide),
            "the margins leave no room for content",
        ),
    ] {
        let seen = page_setup(Some(format), orientation, margin, &[]).unwrap_err();
        assert!(seen.contains(message), "{seen} lacks {message}");
    }
}

#[test]
fn given_font_style_stretch_and_variation_values_when_parsing_then_reads_angles_percents_and_the_last_of_a_repeated_tag(
) {
    assert_eq!(slant(&t("oblique -12.5deg")), Ok(Slant::Oblique(-12.5)));
    assert_eq!(slant(&t("oblique")), Ok(Slant::Oblique(14.0)));
    assert_eq!(stretch(&t("semi-condensed")), Ok(87.5));
    assert_eq!(stretch(&t("62.5%")), Ok(62.5));
    assert_eq!(
        variations(&t("'wght' 300, \"wdth\" 80, \"wght\" 650")).unwrap()[..],
        [(*b"wdth", 80.0), (*b"wght", 650.0)]
    );
    assert!(variations(&t("normal")).unwrap().is_empty());
    for (name, value) in [
        ("fontVariationSettings", "wght 300"),
        ("fontVariationSettings", "'wgh' 300"),
        ("fontStretch", "wide"),
        ("fontStyle", "oblique 10"),
    ] {
        rejects(&[(name, t(value))], &format!("invalid {name}: {value}"));
    }
}
