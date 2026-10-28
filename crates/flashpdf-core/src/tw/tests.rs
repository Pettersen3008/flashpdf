use super::*;

#[test]
fn given_utilities_when_translating_then_matches_tailwind_values_and_arbitrary_type_inference() {
    assert_eq!(
        declarations("[box-shadow:none]"),
        Ok("box-shadow:none;".into())
    );
    assert_eq!(declarations("[display:grid]"), Ok("display:grid;".into()));
    let css = declarations("p-4 px-2.5 -mt-1 mx-auto w-1/2 max-w-md text-sm font-bold bg-gray-100 border-b border-gray-200 rounded-full gap-x-2 flex justify-between text-[min(9pt,12pt)] font-[Arial]").unwrap();
    for value in [
        "padding:calc(0.25rem * 4);",
        "padding-inline:calc(0.25rem * 2.5);",
        "margin-top:calc(calc(0.25rem * 1) * -1);",
        "margin-inline:auto;",
        "width:calc(1 / 2 * 100%);",
        "max-width:28rem;",
        "font-size:min(9pt,12pt);",
        "font-weight:700;",
        "font-family:Arial;",
        "background-color:oklch(96.7% 0.003 264.542);",
        "border-bottom-style:solid;",
        "border-bottom-width:1px;",
        "border-color:oklch(92.8% 0.006 264.531);",
        "border-radius:3.40282e38px;",
        "column-gap:calc(0.25rem * 2);",
        "display:flex;",
        "justify-content:space-between;",
    ] {
        assert!(css.contains(value), "missing {value} in {css}");
    }
    assert_eq!(
        declarations("text-[length:min(9pt,12pt)]").unwrap(),
        "font-size:min(9pt,12pt);"
    );
    assert_eq!(
        declarations("text-[color:rgb(0_128_0)]").unwrap(),
        "color:rgb(0 128 0);"
    );
    assert_eq!(
        declarations("[font-family:Open\\_Sans]").unwrap(),
        "font-family:Open_Sans;"
    );
    assert_eq!(
        declarations("text-[percentage:120%]").unwrap(),
        "font-size:120%;"
    );
    assert_eq!(
        declarations("font-[calc(600+100)]").unwrap(),
        "font-weight:calc(600 + 100);"
    );
    assert_eq!(
        declarations("text-[var(--my_color)]").unwrap(),
        "color:var(--my_color);"
    );
    assert_eq!(
        declarations("p-[calc(1rem-var(--size))]").unwrap(),
        "padding:calc(1rem - var(--size));"
    );
    assert_eq!(
        declarations("p-[calc(1rem--2px)]").unwrap(),
        "padding:calc(1rem - -2px);"
    );
    assert_eq!(
        declarations("font-[\"foo:bar\",Arial]").unwrap(),
        "font-family:\"foo:bar\",Arial;"
    );
}

#[test]
fn given_conflicting_utilities_when_reordered_then_follows_tailwind_property_and_numeric_name_order(
) {
    let names = "p-10 p-2 px-8 pl-1 size-4 w-8 gap-2 gap-x-4 text-sm text-lg leading-6 rounded-lg rounded-t-sm rounded-tl-none border-4 border-2 border-dashed font-bold font-normal";
    let reversed = names
        .split_ascii_whitespace()
        .rev()
        .collect::<Vec<_>>()
        .join(" ");
    let css = declarations(names).unwrap();
    assert_eq!(declarations(&reversed).unwrap(), css);
    assert!(css.find("padding:calc(0.25rem * 2)") < css.find("padding:calc(0.25rem * 10)"));
    assert!(css.find("padding:calc(0.25rem * 10)") < css.find("padding-inline"));
    assert!(css.find("padding-inline") < css.find("padding-left"));
    assert!(css.find("font-size:1.125rem") < css.find("font-size:0.875rem"));
    assert!(css.find("line-height:calc(1.25 / 0.875)") < css.find("line-height:calc(0.25rem * 6)"));
}

#[test]
fn given_unknown_or_unsupported_utilities_when_translating_then_rejects_each_token_by_name() {
    for name in [
        "border-",
        "border-x-",
        "rounded-",
        "rounded-t-",
        "shadow-lg",
        "p-1.3",
        "p-01",
        "p-1.00",
        "text-gray-250",
        "h-sm",
        "size-sm",
        "w-1/00",
        "max-w-auto",
        "-m-auto",
        "-p-4",
        "flex-row-reverse",
        "flex-wrap-reverse",
        "hidden",
        "items-baseline",
        "[filter:none]",
        "[--tw-sort:padding]",
    ] {
        assert_eq!(
            declarations(name),
            Err(format!("unknown tw utility: {name}"))
        );
    }
    for name in ["md:p-4", "hover:bg-gray-100"] {
        assert_eq!(
            declarations(name),
            Err(format!("tw takes no variants: {name}"))
        );
    }
}

#[test]
fn given_malformed_or_oversized_arbitrary_input_when_translating_then_rejects_before_css_or_allocation_growth(
) {
    for name in [
        "[color:red;display:flex]",
        "[padding:1px]junk",
        "p-[1px",
        "p-1px]",
        "text-[rgb(1_2_3]]",
        "[color:]",
    ] {
        assert!(declarations(name).unwrap_err().contains(name));
    }
    assert_eq!(
        declarations(&" ".repeat(16385)),
        Err("tw exceeds 16384 bytes".into())
    );
    assert_eq!(
        declarations(&"p-0 ".repeat(257)),
        Err("tw exceeds 256 utilities".into())
    );
    assert_eq!(
        declarations(&format!("p-[{}]", "0".repeat(1024))),
        Err("tw utility exceeds 1024 bytes".into())
    );
    assert!(
        declarations(&format!("p-[{}0{}]", "(".repeat(16), ")".repeat(16)))
            .unwrap_err()
            .contains("nesting exceeds 16")
    );
}

#[test]
fn given_the_default_palette_when_parsing_every_shade_then_accepts_all_286_colours_including_missing_achromatic_hues(
) {
    for (family, shades) in PALETTE {
        for (shade, value) in SHADES.into_iter().zip(shades) {
            let css = color(&format!("{family}-{shade}")).unwrap();
            assert_eq!(css, format!("oklch({value})"));
            assert!(
                crate::css::style::color(&Value::from(css.as_str())).is_ok(),
                "{family}-{shade}: {css}"
            );
        }
    }
    assert_eq!(
        crate::css::style::color(&Value::from("oklch(97% 0 none)")),
        crate::css::style::color(&Value::from("oklch(97% 0 0)"))
    );
}
