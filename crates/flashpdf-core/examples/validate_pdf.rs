/// `validate_pdf <path> [pages] [--operators]`
///
/// Without `--operators` it prints the text of each page. With it, it prints
/// each page's raw content stream: the exact drawing program, which is what a
/// layout regression changes even when the text is unmoved.
fn main() {
    let path = std::env::args().nth(1).expect("PDF path");
    let operators = std::env::args().any(|value| value == "--operators");
    let expected_pages = std::env::args()
        .nth(2)
        .filter(|value| value != "--operators")
        .map(|value| value.parse().expect("page count"))
        .unwrap_or(1);
    let pdf = lopdf::Document::load(path).expect("valid PDF");
    let pages = pdf.get_pages();
    assert_eq!(pages.len(), expected_pages);
    // One trimmed block per page, form-feed separated, so a caller can assert
    // which page each string landed on.
    let blocks: Vec<String> = pages
        .iter()
        .map(|(page, id)| {
            if operators {
                let content = pdf.get_page_content(*id);
                String::from_utf8_lossy(&content).trim().to_owned()
            } else {
                // lopdf adds a space after each kerned `TJ` array.
                let text = pdf.extract_text(&[*page]).expect("extractable text");
                let lines: Vec<&str> = text.trim().lines().map(str::trim_end).collect();
                lines.join("\n")
            }
        })
        .collect();
    print!("{}", blocks.join("\u{c}"));
}
