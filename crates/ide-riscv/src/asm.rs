use crate::{Error, Result, expr};
use std::collections::BTreeMap;

pub(crate) const LIMIT: usize = 64 * 1024 * 1024;
pub(crate) const TEXT: u64 = 0x10000;
pub(crate) const STACK: u64 = 0x7ff0_0000;
pub(crate) const STACK_SIZE: usize = 1024 * 1024;
pub(crate) const RETURN: u64 = 0xffff_ffff_ffff_fffc;

#[derive(Clone)]
pub(crate) struct Instruction {
    pub op: String,
    pub args: Vec<String>,
    pub line: usize,
}
#[derive(Clone)]
pub(crate) struct Region {
    pub start: u64,
    pub data: Vec<u8>,
    pub writable: bool,
}
#[derive(Clone)]
enum Symbol {
    Address(u64),
    Expression(String, u64, usize),
}

pub struct Program {
    pub(crate) code: BTreeMap<u64, Instruction>,
    pub(crate) regions: Vec<Region>,
    symbols: BTreeMap<String, Symbol>,
    numeric: Vec<(String, usize, u64)>,
}

impl Program {
    pub fn assemble(source: &str) -> Result<Self> {
        let mut program = Self {
            code: BTreeMap::new(),
            symbols: BTreeMap::new(),
            numeric: Vec::new(),
            regions: vec![
                Region { start: TEXT, data: Vec::new(), writable: false },
                Region { start: 0x10000000, data: Vec::new(), writable: false },
                Region { start: 0x11000000, data: Vec::new(), writable: true },
                Region { start: 0x12000000, data: Vec::new(), writable: true },
            ],
        };
        let lines = strip_comments(source)?;
        let mut section = 0;
        let mut data = Vec::new();
        for (index, raw) in lines.iter().enumerate() {
            let line = index + 1;
            let process = (|| -> Result<()> {
                let mut text = raw.trim();
                while let Some(colon) = text.find(':') {
                    let name = text[..colon].trim();
                    if !valid_name(name) {
                        break;
                    }
                    let address = program.regions[section].start + program.regions[section].data.len() as u64;
                    if name.bytes().all(|c| c.is_ascii_digit()) {
                        program.numeric.push((name.to_owned(), line, address));
                    } else if program.symbols.insert(name.to_owned(), Symbol::Address(address)).is_some() {
                        return Err(Error::new(format!("duplicate symbol: {name}")));
                    }
                    text = text[colon + 1..].trim_start();
                }
                if text.is_empty() {
                    return Ok(());
                }
                let split = text.find(char::is_whitespace).unwrap_or(text.len());
                let op = &text[..split];
                let operands = text[split..].trim();
                let args = arguments(operands)?;
                let address = program.regions[section].start + program.regions[section].data.len() as u64;
                let constant = |value: &str| program.eval(value, address, line);
                if op.starts_with('.') {
                    let size = match op {
                        ".text" => {
                            section = 0;
                            return Ok(());
                        }
                        ".data" => {
                            section = 2;
                            return Ok(());
                        }
                        ".bss" => {
                            section = 3;
                            return Ok(());
                        }
                        ".rodata" => {
                            section = 1;
                            return Ok(());
                        }
                        ".section" => {
                            section = match args.first().map(String::as_str) {
                                Some(s) if s == ".text" || s.starts_with(".text.") => 0,
                                Some(s) if s == ".rodata" || s.starts_with(".rodata.") => 1,
                                Some(s) if s == ".data" || s.starts_with(".data.") => 2,
                                Some(s) if s == ".bss" || s.starts_with(".bss.") => 3,
                                _ => return Err(Error::new("supported sections: .text, .rodata, .data, .bss")),
                            };
                            return Ok(());
                        }
                        ".equ" | ".equiv" | ".eqv" => {
                            let pair = if args.len() == 2 {
                                args.clone()
                            } else {
                                operands.splitn(2, char::is_whitespace).map(|s| s.trim().to_owned()).collect()
                            };
                            if pair.len() != 2 || !valid_name(&pair[0]) {
                                return Err(Error::new("expected .equ NAME, expression"));
                            }
                            if program
                                .symbols
                                .insert(pair[0].clone(), Symbol::Expression(pair[1].clone(), address, line))
                                .is_some()
                            {
                                return Err(Error::new(format!("duplicate symbol: {}", pair[0])));
                            }
                            return Ok(());
                        }
                        ".globl" | ".global" | ".type" | ".size" | ".option" | ".ident" | ".attribute" => return Ok(()),
                        ".align" | ".p2align" | ".balign" => {
                            let n = constant(args.first().ok_or_else(|| Error::new("missing alignment"))?)?;
                            let alignment = if op == ".balign" {
                                n
                            } else {
                                1u64.checked_shl(u32::try_from(n).unwrap_or(u32::MAX)).unwrap_or(0)
                            };
                            if alignment == 0 || !alignment.is_power_of_two() || alignment > LIMIT as u64 {
                                return Err(Error::new("invalid alignment"));
                            }
                            ((alignment - address % alignment) % alignment) as usize
                        }
                        ".zero" | ".skip" | ".space" => {
                            let count = constant(args.first().ok_or_else(|| Error::new("missing size"))?)?;
                            usize::try_from(count).map_err(|_| Error::new("allocation too large"))?
                        }
                        ".ascii" | ".asciz" | ".asciiz" | ".string" => {
                            let mut bytes = Vec::new();
                            for arg in &args {
                                if !arg.starts_with('"') || !arg.ends_with('"') || arg.len() < 2 {
                                    return Err(Error::new("expected quoted string"));
                                }
                                bytes.extend(expr::unescape(&arg[1..arg.len() - 1])?);
                                if op != ".ascii" {
                                    bytes.push(0);
                                }
                            }
                            data.push((section, program.regions[section].data.len(), line, Data::Bytes(bytes.clone())));
                            bytes.len()
                        }
                        ".byte" | ".half" | ".short" | ".2byte" | ".word" | ".long" | ".4byte" | ".dword" | ".quad"
                        | ".8byte" | ".float" | ".double" => {
                            let width = match op {
                                ".byte" => 1,
                                ".half" | ".short" | ".2byte" => 2,
                                ".word" | ".long" | ".4byte" | ".float" => 4,
                                _ => 8,
                            };
                            data.push((
                                section,
                                program.regions[section].data.len(),
                                line,
                                Data::Values(args.clone(), width, op.to_owned()),
                            ));
                            width * args.len()
                        }
                        _ => return Err(Error::new(format!("unsupported directive: {op}"))),
                    };
                    let allocated: usize = program.regions.iter().map(|r| r.data.len()).sum();
                    if size > LIMIT.saturating_sub(allocated) {
                        return Err(Error::new("data exceeds 64 MiB simulator limit"));
                    }
                    let region = &mut program.regions[section];
                    region.data.resize(region.data.len() + size, 0);
                    if matches!(op, ".skip" | ".space") && args.len() > 1 {
                        let value = program.eval(&args[1], address, line)? as u8;
                        let region = &mut program.regions[section];
                        let start = region.data.len() - size;
                        region.data[start..].fill(value);
                    }
                } else {
                    if section != 0 {
                        return Err(Error::new("instructions must be in .text"));
                    }
                    if !crate::cpu::supported(op) {
                        return Err(Error::new(format!("unsupported instruction: {op}")));
                    }
                    if !address.is_multiple_of(4) {
                        return Err(Error::new("instruction address is not aligned to 4 bytes"));
                    }
                    let instruction = Instruction { op: op.to_owned(), args, line };
                    crate::cpu::validate(&instruction)?;
                    program.code.insert(address, instruction);
                    program.regions[section].data.extend_from_slice(&[0; 4]);
                }
                Ok(())
            })();
            process.map_err(|error| error.at(line))?;
        }
        for (section, offset, line, item) in data {
            let address = program.regions[section].start + offset as u64;
            let bytes = match item {
                Data::Bytes(bytes) => bytes,
                Data::Values(args, width, op) => {
                    let mut bytes = Vec::new();
                    for arg in args {
                        let value = match op.as_str() {
                            ".float" => u64::from(
                                arg.parse::<f32>().map_err(|_| Error::new("invalid float").at(line))?.to_bits(),
                            ),
                            ".double" => {
                                arg.parse::<f64>().map_err(|_| Error::new("invalid double").at(line))?.to_bits()
                            }
                            _ => program.eval(&arg, address + bytes.len() as u64, line).map_err(|e| e.at(line))?,
                        };
                        bytes.extend_from_slice(&value.to_le_bytes()[..width]);
                    }
                    bytes
                }
            };
            program.regions[section].data[offset..offset + bytes.len()].copy_from_slice(&bytes);
        }
        // Resolve constants even when unused, so cycles and unknown symbols are diagnostics.
        for name in program.symbols.keys() {
            program.lookup(name, TEXT, 0, 0)?;
        }
        if program.code.is_empty() {
            return Err(Error::new("no executable instructions"));
        }
        Ok(program)
    }

    pub(crate) fn eval(&self, text: &str, address: u64, line: usize) -> Result<u64> {
        expr::eval(text, &|name| self.lookup(name, address, line, 0))
    }

    fn lookup(&self, name: &str, address: u64, line: usize, depth: usize) -> Result<u64> {
        if depth > 64 {
            return Err(Error::new("cyclic constant definition"));
        }
        if name == "." {
            return Ok(address);
        }
        if name.len() > 1 && name[..name.len() - 1].bytes().all(|c| c.is_ascii_digit()) {
            let number = &name[..name.len() - 1];
            let found = if name.ends_with('f') {
                self.numeric.iter().find(|(n, l, _)| n == number && *l > line)
            } else if name.ends_with('b') {
                self.numeric.iter().rev().find(|(n, l, _)| n == number && *l <= line)
            } else {
                None
            };
            return found.map(|(_, _, a)| *a).ok_or_else(|| Error::new(format!("unknown local label: {name}")));
        }
        match self.symbols.get(name) {
            Some(Symbol::Address(address)) => Ok(*address),
            Some(Symbol::Expression(text, dot, source_line)) => {
                expr::eval(text, &|n| self.lookup(n, *dot, *source_line, depth + 1)).map_err(|e| e.at(*source_line))
            }
            None => Err(Error::new(format!("unknown symbol: {name}"))),
        }
    }

    pub fn symbol(&self, name: &str) -> Result<u64> {
        self.lookup(name, TEXT, 0, 0)
    }
    pub fn functions(&self) -> Vec<(String, u64)> {
        let mut names: Vec<_> = self
            .symbols
            .iter()
            .filter_map(|(name, value)| match value {
                Symbol::Address(address) if !name.starts_with('.') && self.code.contains_key(address) => {
                    Some((name.clone(), *address))
                }
                _ => None,
            })
            .collect();
        names.sort_by_key(|(_, address)| *address);
        names
    }
}

enum Data {
    Bytes(Vec<u8>),
    Values(Vec<String>, usize, String),
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.$".contains(&c))
}

pub(crate) fn arguments(text: &str) -> Result<Vec<String>> {
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escape = false;
    let mut depth = 0;
    for (i, ch) in text.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if let Some(q) = quote {
            if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
        } else if ch == ',' && depth == 0 {
            args.push(text[start..i].trim().to_owned());
            start = i + 1;
        }
    }
    if quote.is_some() || depth != 0 {
        return Err(Error::new("unbalanced operands"));
    }
    args.push(text[start..].trim().to_owned());
    if args.iter().any(String::is_empty) {
        return Err(Error::new("missing operand"));
    }
    Ok(args)
}

pub(crate) fn strip_comments(source: &str) -> Result<Vec<String>> {
    let mut out = String::new();
    let mut chars = source.chars().peekable();
    let mut block = false;
    let mut line_comment = false;
    let mut quote = None;
    let mut escape = false;
    while let Some(ch) = chars.next() {
        if ch == '\n' {
            out.push(ch);
            line_comment = false;
            escape = false;
            continue;
        }
        if line_comment {
            out.push(' ');
            continue;
        }
        if block {
            if ch == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block = false;
                out.push_str("  ");
            } else {
                out.push(' ');
            }
        } else if let Some(q) = quote {
            out.push(ch);
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            out.push(ch);
        } else if ch == '#' {
            line_comment = true;
            out.push(' ');
        } else if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            line_comment = true;
            out.push_str("  ");
        } else if ch == '/' && chars.peek() == Some(&'*') {
            chars.next();
            block = true;
            out.push_str("  ");
        } else {
            out.push(ch);
        }
    }
    if block {
        return Err(Error::new("unterminated block comment"));
    }
    Ok(out.lines().map(str::to_owned).collect())
}
