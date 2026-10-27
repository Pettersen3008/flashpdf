use super::writer::Run;
use crate::css::style::{Transform, WhiteSpace};

const NBSP: char = '\u{a0}';

/// CSS collapsible whitespace only; U+00A0 is content.
pub(super) fn blank(text: &str) -> bool {
    text.chars()
        .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}'))
}

/// Applies `white-space` to a paragraph's runs. In `pre`, `pre-wrap`, and `pre-line` a newline ends
/// the line. The engine collapses every ASCII space, so `pre` and `pre-wrap` keep spaces as U+00A0,
/// but `pre-wrap` leaves the last of a run of spaces as a space, where the line may wrap. `nowrap`
/// collapses to one U+00A0 so its words never break. A tab reaches the next multiple of 8 columns.
pub(super) fn preserve(runs: Vec<Run>) -> Vec<Run> {
    let mut result = Vec::with_capacity(runs.len());
    let mut column = 0;
    // The line has text, and it ends in a space that may wrap, or in a collapsed one.
    let mut started = false;
    let mut loose = false;
    let last = runs.len().wrapping_sub(1);
    for (index, item) in runs.into_iter().enumerate() {
        let mode = item.white_space;
        if mode == WhiteSpace::Normal {
            result.push(item);
            continue;
        }
        let mut piece = String::new();
        let mut spaces = 0usize;
        let flush = |piece: &mut String,
                     spaces: &mut usize,
                     column: &mut usize,
                     started: bool,
                     loose: &mut bool| {
            if *spaces == 0 {
                return;
            }
            let kept = if mode == WhiteSpace::PreWrap && started && !*loose {
                *spaces - 1
            } else {
                *spaces
            };
            piece.extend(std::iter::repeat_n(NBSP, kept));
            piece.extend(std::iter::repeat_n(' ', *spaces - kept));
            *loose = kept < *spaces;
            *column += *spaces;
            *spaces = 0;
        };
        let text = item.text.replace("\r\n", "\n").replace('\r', "\n");
        for c in text.chars() {
            if c == '\n' && mode != WhiteSpace::Nowrap {
                flush(&mut piece, &mut spaces, &mut column, started, &mut loose);
                result.push(Run {
                    text: std::mem::take(&mut piece),
                    hard_break: true,
                    ..item.clone()
                });
                (column, started, loose) = (0, false, false);
            } else if mode == WhiteSpace::PreLine {
                piece.push(c);
            } else if mode == WhiteSpace::Nowrap {
                if matches!(c, ' ' | '\t' | '\n' | '\u{c}') {
                    if started && !loose {
                        piece.push(NBSP);
                    }
                    loose = started;
                } else {
                    piece.push(c);
                    (started, loose) = (true, false);
                }
            } else if c == ' ' || c == '\t' {
                spaces += if c == ' ' {
                    1
                } else {
                    8 - (column + spaces) % 8
                };
            } else {
                flush(&mut piece, &mut spaces, &mut column, started, &mut loose);
                piece.push(c);
                (column, started, loose) = (column + 1, true, false);
            }
        }
        flush(&mut piece, &mut spaces, &mut column, started, &mut loose);
        // A collapsed space at the end of a line goes.
        if mode == WhiteSpace::Nowrap && loose && (item.hard_break || index == last) {
            piece.pop();
            loose = false;
        }
        let hard_break = item.hard_break;
        if !piece.is_empty() || hard_break {
            result.push(Run {
                text: piece,
                ..item
            });
        }
        if hard_break {
            (column, started, loose) = (0, false, false);
        }
    }
    result
}

/// `\p{M}` for the combining blocks; std has no general category, and other marks are rare in text.
fn mark(c: char) -> bool {
    matches!(c, '\u{300}'..='\u{36f}' | '\u{1ab0}'..='\u{1aff}' | '\u{1dc0}'..='\u{1dff}' | '\u{20d0}'..='\u{20ff}' | '\u{fe20}'..='\u{fe2f}')
}

/// `\p{Pc}`.
fn connector(c: char) -> bool {
    matches!(
        c,
        '_' | '\u{203f}' | '\u{2040}' | '\u{2054}' | '\u{fe33}' | '\u{fe34}' | '\u{fe4d}'
            ..='\u{fe4f}' | '\u{ff3f}'
    )
}

/// Locale-independent, so output does not depend on the runtime's default language. For
/// `capitalize` a letter starts a word unless a letter, digit, or mark precedes it, or an
/// apostrophe, period, colon, or middle dot inside a word, as in `don't`.
pub(super) fn transform_text(text: &str, value: Transform, before: &str) -> String {
    match value {
        Transform::Uppercase => return text.to_uppercase(),
        Transform::Lowercase => return text.to_lowercase(),
        Transform::None => return text.into(),
        Transform::Capitalize => {}
    }
    // ponytail: char::is_alphabetic stands in for \p{L}; it also counts Nl and some marks.
    let word = |c: char| c.is_alphabetic() || c.is_numeric() || mark(c) || connector(c);
    let mut previous: [Option<char>; 2] = [None, None];
    for c in before.chars() {
        previous = [previous[1], Some(c)];
    }
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        let [two, one] = previous;
        let joined = matches!(one, Some('\'' | '’' | '.' | ':' | '·'))
            && two.is_some_and(|c| c.is_alphabetic() || c.is_numeric());
        if c.is_alphabetic() && !one.is_some_and(word) && !joined {
            result.extend(c.to_uppercase());
        } else {
            result.push(c);
        }
        previous = [one, Some(c)];
    }
    result
}

/// The core reports a glyph it cannot show as `UnsupportedCharacter('x')` (a built-in font, WinAnsi
/// only) or `MissingGlyph('x')` (the embedded font lacks it), with the character in Rust's
/// debug escaping; this names the character and code point and says which font to blame.
pub(super) fn glyph_error(message: &str) -> Option<String> {
    let (missing, rest) = if let Some(rest) = message.strip_prefix("MissingGlyph('") {
        (true, rest)
    } else {
        (false, message.strip_prefix("UnsupportedCharacter('")?)
    };
    let escaped = rest
        .strip_suffix("')")
        .filter(|escaped| !escaped.is_empty())?;
    let character: String = if let Some(hex) = escaped.strip_prefix("\\u{") {
        let code = u32::from_str_radix(hex.strip_suffix('}').unwrap_or(hex), 16).ok()?;
        char::from_u32(code)?.into()
    } else if escaped.starts_with('\\') && escaped.chars().count() == 2 {
        escaped[1..].into()
    } else {
        escaped.into()
    };
    let code = format!("U+{:04X}", u32::from(character.chars().next()?));
    Some(if missing {
        format!("the font has no glyph for \"{character}\" ({code})")
    } else {
        format!("the built-in font has no glyph for \"{character}\" ({code}); register a font that has it")
    })
}

/// Explains the engine's bare error codes; `locate` then names the element.
pub(super) fn engine_error(message: &str) -> Option<&'static str> {
    Some(match message {
        "InvalidLayout" => "InvalidLayout: the content does not fit its container; reduce fixed widths, margins, padding, or borders",
        "TextTooWide" => "TextTooWide: a character is wider than its column; reduce the font size or widen the column",
        "ContentOverflow" => "ContentOverflow: the content is taller than its box's height or max-height, or than its flex container; raise the height or shorten the content",
        "LineTooWide" => "LineTooWide: a line of white-space: pre or nowrap text is wider than its container; shorten it, reduce the font size, or use white-space: pre-wrap",
        _ => return None,
    })
}
