use crate::css::Message;
use std::borrow::Cow;

use super::{js_number, number_end, parse_number, point};

const MAX_EXPRESSION: usize = 1024;
const MAX_DEPTH: usize = 16;
const FUNCTIONS: [&str; 4] = ["calc", "min", "max", "clamp"];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Number,
    Length,
    Percent,
}

#[derive(Clone, Copy)]
struct Quantity {
    value: f64,
    kind: Kind,
}

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Length of `calc(`, `min(`, `max(`, `clamp(`, or `(` at the start of `text`, any case.
fn function(text: &[u8], bare: bool) -> Option<usize> {
    if bare && text.first() == Some(&b'(') {
        return Some(1);
    }
    FUNCTIONS.iter().find_map(|name| {
        let end = name.len();
        (text.len() > end && text[..end].eq_ignore_ascii_case(name.as_bytes()) && text[end] == b'(')
            .then_some(end + 1)
    })
}

/// Length of a signed number with an optional unit or `%` at the start of `text`. A number takes
/// its sign before `+` and `-` match, since CSS requires whitespace around those operators.
fn quantity_end(text: &[u8]) -> Option<usize> {
    let sign = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let digits = number_end(&text[sign..], false);
    if digits == 0 {
        return None;
    }
    let number = sign + digits;
    let free = |at: usize| {
        !text
            .get(at)
            .is_some_and(|&byte| word(byte) || byte == b'.' || byte == b'-')
    };
    let letters = text[number..]
        .iter()
        .take_while(|b| b.is_ascii_alphabetic())
        .count();
    if letters > 0 {
        return free(number + letters).then_some(number + letters);
    }
    if text.get(number) == Some(&b'%') && free(number + 1) {
        return Some(number + 1);
    }
    free(number).then_some(number)
}

fn lex(expression: &str) -> Result<Vec<String>, Message> {
    let bytes = expression.as_bytes();
    let mut tokens = Vec::new();
    let mut end = 0;
    loop {
        let start = end + expression[end..].len() - expression[end..].trim_start().len();
        let rest = &bytes[start..];
        let length = quantity_end(rest)
            .or_else(|| function(rest, true))
            .or_else(|| {
                rest.first()
                    .is_some_and(|b| b"*/+,)-".contains(b))
                    .then_some(1)
            });
        let Some(length) = length else { break };
        tokens.push(expression[start..start + length].to_ascii_lowercase());
        end = start + length;
    }
    let rest = expression[end..].trim();
    if !rest.is_empty() {
        return Err(format!("unexpected \"{rest}\"").into());
    }
    Ok(tokens)
}

fn quantity(token: &str, em: f64) -> Result<Quantity, Message> {
    let (negative, unsigned) = match token.as_bytes()[0] {
        b'-' => (true, &token[1..]),
        b'+' => (false, &token[1..]),
        _ => (false, token),
    };
    let factor = if negative { -1.0 } else { 1.0 };
    let digits = number_end(unsigned.as_bytes(), false);
    let (value, kind) = match &unsigned[digits..] {
        "" => (parse_number(&unsigned[..digits]), Kind::Number),
        "%" => (parse_number(&unsigned[..digits]), Kind::Percent),
        _ => (point(unsigned, em, false)?, Kind::Length),
    };
    Ok(Quantity {
        value: factor * value,
        kind,
    })
}

/// `+`, `-`, `min()`, `max()`, and `clamp()` take operands of one kind.
fn same(left: Quantity, right: Quantity, verb: &str) -> Result<(), Message> {
    if left.kind == right.kind {
        return Ok(());
    }
    Err(if left.kind == Kind::Number || right.kind == Kind::Number {
        format!("cannot {verb} a number and a length or percentage").into()
    } else {
        "a percentage cannot combine with a length, because the engine resolves percentages when it lays the box out".into()
    })
}

struct Evaluator<'a> {
    tokens: &'a [String],
    at: usize,
    em: f64,
}

impl Evaluator<'_> {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn sum(&mut self, depth: usize) -> Result<Quantity, Message> {
        let mut left = self.product(depth)?;
        while let Some(operator @ ("+" | "-")) = self.peek() {
            let add = operator == "+";
            self.at += 1;
            let right = self.product(depth)?;
            same(left, right, if add { "add" } else { "subtract" })?;
            left.value = if add {
                left.value + right.value
            } else {
                left.value - right.value
            };
        }
        Ok(left)
    }

    fn product(&mut self, depth: usize) -> Result<Quantity, Message> {
        let mut left = self.operand(depth)?;
        while let Some(operator @ ("*" | "/")) = self.peek() {
            let multiply = operator == "*";
            self.at += 1;
            let right = self.operand(depth)?;
            if !multiply {
                if right.kind != Kind::Number {
                    return Err("the divisor must be a number".into());
                }
                if right.value == 0.0 {
                    return Err("division by zero".into());
                }
                left.value /= right.value;
            } else if left.kind != Kind::Number && right.kind != Kind::Number {
                return Err("cannot multiply two lengths or percentages".into());
            } else {
                let kind = if left.kind == Kind::Number {
                    right.kind
                } else {
                    left.kind
                };
                left = Quantity {
                    kind,
                    value: left.value * right.value,
                };
            }
        }
        Ok(left)
    }

    fn operand(&mut self, depth: usize) -> Result<Quantity, Message> {
        let token = self.tokens.get(self.at).ok_or("unexpected end")?;
        self.at += 1;
        if depth > MAX_DEPTH {
            return Err("nested too deeply".into());
        }
        let Some(name) = token.strip_suffix('(') else {
            let bytes = token.as_bytes();
            let digit = |at: usize| {
                bytes
                    .get(at)
                    .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
            };
            if digit(0) || (matches!(bytes[0], b'+' | b'-') && digit(1)) {
                return quantity(token, self.em);
            }
            return Err(format!("unexpected \"{token}\"").into());
        };
        let mut args = vec![self.sum(depth + 1)?];
        while !name.is_empty() && self.peek() == Some(",") {
            self.at += 1;
            args.push(self.sum(depth + 1)?);
        }
        match self.peek() {
            Some(")") => self.at += 1,
            Some(other) => return Err(format!("unexpected \"{other}\"").into()),
            None => return Err("expected \")\"".into()),
        }
        if name.is_empty() || name == "calc" {
            if args.len() > 1 {
                return Err("calc() takes one expression".into());
            }
            return Ok(args[0]);
        }
        if name == "clamp" && args.len() != 3 {
            return Err("clamp() takes 3 values".into());
        }
        for &arg in &args {
            same(args[0], arg, "compare")?;
        }
        // Math.min and Math.max: NaN wins.
        let pick = |values: &[Quantity], larger: bool| {
            values.iter().map(|arg| arg.value).fold(
                if larger {
                    f64::NEG_INFINITY
                } else {
                    f64::INFINITY
                },
                |a, b| {
                    if a.is_nan() || b.is_nan() {
                        f64::NAN
                    } else if larger {
                        a.max(b)
                    } else {
                        a.min(b)
                    }
                },
            )
        };
        let value = match name {
            "min" => pick(&args, false),
            "max" => pick(&args, true),
            _ => {
                let low = pick(&args[1..], false);
                pick(
                    &[
                        args[0],
                        Quantity {
                            value: low,
                            kind: args[0].kind,
                        },
                    ],
                    true,
                )
            }
        };
        Ok(Quantity {
            kind: args[0].kind,
            value,
        })
    }
}

fn evaluate(expression: &str, em: f64) -> Result<Quantity, Message> {
    let tokens = lex(expression)?;
    let mut evaluator = Evaluator {
        tokens: &tokens,
        at: 0,
        em,
    };
    let result = evaluator.sum(0)?;
    if let Some(token) = evaluator.peek() {
        return Err(format!("unexpected \"{token}\"").into());
    }
    Ok(result)
}

/// `Number(value.toFixed(6))`: rounds to six decimals, a tie away from zero.
fn fixed6(value: f64) -> f64 {
    let magnitude = value.abs();
    let seven = format!("{magnitude:.7}");
    let scaled: u64 = seven.replace('.', "").parse().unwrap_or(0);
    let tie = seven.ends_with('5')
        && scaled.is_multiple_of(78_125)
        && seven.parse::<f64>() == Ok(magnitude);
    let rounded: f64 = if tie {
        let up = scaled / 10 + 1;
        format!("{}.{:06}", up / 1_000_000, up % 1_000_000)
            .parse()
            .unwrap_or(0.0)
    } else {
        format!("{magnitude:.6}").parse().unwrap_or(0.0)
    };
    if value < 0.0 {
        -rounded
    } else {
        rounded
    }
}

fn format(result: Quantity, property: &str) -> Result<String, Message> {
    if result.value.is_nan() || result.value.abs() >= 1e9 {
        return Err("the result is out of range".into());
    }
    if result.kind == Kind::Percent && !matches!(property, "width" | "height" | "lineHeight") {
        return Err(format!("{property} does not take a percentage").into());
    }
    let text = js_number(fixed6(result.value));
    Ok(match result.kind {
        Kind::Length => format!("{text}pt"),
        Kind::Percent => format!("{text}%"),
        Kind::Number => text,
    })
}

/// Index of the `)` closing the `(` at `open`, searching at most MAX_EXPRESSION characters.
fn closing(value: &[u8], open: usize) -> Result<usize, Message> {
    let mut depth = 0usize;
    for (index, &byte) in value
        .iter()
        .enumerate()
        .take(open + MAX_EXPRESSION)
        .skip(open)
    {
        if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth = depth.wrapping_sub(1);
            if depth == 0 {
                return Ok(index);
            }
        }
    }
    Err(format!("unbalanced parentheses or longer than {MAX_EXPRESSION} characters").into())
}

/// Replaces each top-level `calc()`, `min()`, `max()`, and `clamp()` in `value` with the plain
/// number, `pt` length, or percentage it computes to, so every later parser sees plain values.
/// `em` is the font size an `em` unit resolves against.
pub fn evaluate_math<'a>(property: &str, value: &'a str, em: f64) -> Result<Cow<'a, str>, Message> {
    let bytes = value.as_bytes();
    let found = (0..bytes.len()).any(|at| function(&bytes[at..], false).is_some());
    if !found {
        return Ok(Cow::Borrowed(value));
    }
    let mut result = String::new();
    let mut copied = 0;
    let mut depth = 0isize;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let boundary = index == 0 || !(word(bytes[index - 1]) || bytes[index - 1] == b'-');
        if byte == b'"' || byte == b'\'' {
            match bytes[index + 1..].iter().position(|&other| other == byte) {
                Some(offset) => index += offset + 1,
                None => break,
            }
        } else if let Some(length) = (depth == 0 && boundary)
            .then(|| function(&bytes[index..], false))
            .flatten()
        {
            let open = index + length - 1;
            let mut end = open;
            let computed = closing(bytes, open).and_then(|close| {
                end = close;
                format(evaluate(&value[index..=close], em)?, property)
            });
            match computed {
                Ok(text) => {
                    result.push_str(&value[copied..index]);
                    result.push_str(&text);
                }
                Err(message) => {
                    return Err(format!(
                        "invalid {} in {property}: {message}",
                        &value[index..=end]
                    )
                    .into());
                }
            }
            copied = end + 1;
            index = end;
        } else if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth -= 1;
        }
        index += 1;
    }
    result.push_str(&value[copied..]);
    Ok(Cow::Owned(result))
}
