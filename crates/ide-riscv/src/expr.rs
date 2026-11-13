use crate::{Error, Result};

pub(crate) fn eval(text: &str, lookup: &dyn Fn(&str) -> Result<u64>) -> Result<u64> {
    let mut parser = Parser { text: text.trim(), pos: 0, lookup };
    let value = parser.expression(0)?;
    parser.space();
    if parser.pos != parser.text.len() {
        return Err(Error::new(format!("invalid expression: {text}")));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
    lookup: &'a dyn Fn(&str) -> Result<u64>,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.text.as_bytes().get(self.pos).is_some_and(u8::is_ascii_whitespace) {
            self.pos += 1;
        }
    }

    fn expression(&mut self, min: u8) -> Result<u64> {
        self.space();
        let mut left = self.atom()?;
        loop {
            self.space();
            let rest = &self.text[self.pos..];
            let op = ["<<", ">>", "+", "-", "*", "/", "%", "&", "^", "|"].into_iter().find(|op| rest.starts_with(op));
            let Some(op) = op else { break };
            let precedence = match op {
                "|" => 1,
                "^" => 2,
                "&" => 3,
                "<<" | ">>" => 4,
                "+" | "-" => 5,
                _ => 6,
            };
            if precedence < min {
                break;
            }
            self.pos += op.len();
            let right = self.expression(precedence + 1)?;
            left = match op {
                "+" => left.wrapping_add(right),
                "-" => left.wrapping_sub(right),
                "*" => left.wrapping_mul(right),
                "/" | "%" if right == 0 => return Err(Error::new("division by zero in expression")),
                "/" => (left as i64).wrapping_div(right as i64) as u64,
                "%" => (left as i64).wrapping_rem(right as i64) as u64,
                "<<" => left.wrapping_shl(right as u32),
                ">>" => left.wrapping_shr(right as u32),
                "&" => left & right,
                "^" => left ^ right,
                "|" => left | right,
                _ => unreachable!(),
            };
        }
        Ok(left)
    }

    fn atom(&mut self) -> Result<u64> {
        self.space();
        let rest = &self.text[self.pos..];
        let ch = rest.chars().next().ok_or_else(|| Error::new("expected expression"))?;
        if matches!(ch, '+' | '-' | '~') {
            self.pos += 1;
            let value = self.expression(7)?;
            return Ok(match ch {
                '-' => value.wrapping_neg(),
                '~' => !value,
                _ => value,
            });
        }
        if ch == '(' {
            self.pos += 1;
            let value = self.expression(0)?;
            self.space();
            if !self.text[self.pos..].starts_with(')') {
                return Err(Error::new("missing ')'"));
            }
            self.pos += 1;
            return Ok(value);
        }
        if ch == '\'' {
            self.pos += 1;
            let end = quoted_end(&self.text[self.pos..], '\'').ok_or_else(|| Error::new("unterminated character"))?;
            let bytes = unescape(&self.text[self.pos..self.pos + end])?;
            self.pos += end + 1;
            if bytes.len() != 1 {
                return Err(Error::new("character literal must contain one byte"));
            }
            return Ok(u64::from(bytes[0]));
        }
        let start = self.pos;
        while self.text.as_bytes().get(self.pos).is_some_and(|c| c.is_ascii_alphanumeric() || b"_.$".contains(c)) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(Error::new(format!("invalid expression: {}", self.text)));
        }
        let word = &self.text[start..self.pos];
        if word.as_bytes()[0].is_ascii_digit() {
            if word.len() > 1
                && word[..word.len() - 1].bytes().all(|c| c.is_ascii_digit())
                && (word.ends_with('f') || word.ends_with('b'))
            {
                return (self.lookup)(word);
            }
            let (digits, radix) = if word.starts_with("0x") || word.starts_with("0X") {
                (&word[2..], 16)
            } else if word.starts_with("0b") || word.starts_with("0B") {
                (&word[2..], 2)
            } else if word.len() > 1 && word.starts_with('0') {
                (&word[1..], 8)
            } else {
                (word, 10)
            };
            return u64::from_str_radix(digits, radix).map_err(|_| Error::new(format!("invalid integer: {word}")));
        }
        (self.lookup)(word)
    }
}

pub(crate) fn quoted_end(text: &str, quote: char) -> Option<usize> {
    let mut escape = false;
    for (i, ch) in text.char_indices() {
        if escape {
            escape = false;
        } else if ch == '\\' {
            escape = true;
        } else if ch == quote {
            return Some(i);
        }
    }
    None
}

pub(crate) fn unescape(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut bytes = [0; 4];
            out.extend_from_slice(ch.encode_utf8(&mut bytes).as_bytes());
            continue;
        }
        let ch = chars.next().ok_or_else(|| Error::new("incomplete escape"))?;
        match ch {
            'n' => out.push(b'\n'),
            'r' => out.push(b'\r'),
            't' => out.push(b'\t'),
            'b' => out.push(8),
            'f' => out.push(12),
            '\\' => out.push(b'\\'),
            '\'' => out.push(b'\''),
            '"' => out.push(b'"'),
            'x' => {
                let mut hex = String::new();
                while chars.peek().is_some_and(char::is_ascii_hexdigit) && hex.len() < 2 {
                    hex.push(chars.next().unwrap());
                }
                out.push(u8::from_str_radix(&hex, 16).map_err(|_| Error::new("invalid hex escape"))?);
            }
            '0'..='7' => {
                let mut octal = ch.to_string();
                while chars.peek().is_some_and(|c| matches!(c, '0'..='7')) && octal.len() < 3 {
                    octal.push(chars.next().unwrap());
                }
                out.push(u16::from_str_radix(&octal, 8).map_err(|_| Error::new("invalid octal escape"))? as u8);
            }
            _ => return Err(Error::new(format!("unknown escape: \\{ch}"))),
        }
    }
    Ok(out)
}
