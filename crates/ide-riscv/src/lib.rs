//! RV64 source simulator for standalone teaching programs and integer ABI function calls.
mod asm;
mod cpu;
mod expr;

use std::fmt;
use std::io::{self, Write};

pub use asm::Program;
pub use cpu::{Machine, Outcome};
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Error {
    pub line: Option<usize>,
    pub message: String,
}
impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { line: None, message: message.into() }
    }
    pub(crate) fn at(mut self, line: usize) -> Self {
        if self.line.is_none() {
            self.line = Some(line);
        }
        self
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(f, "{line}: ")?;
        }
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

pub fn integer(text: &str) -> Result<u64> {
    expr::eval(text, &|name| Err(Error::new(format!("expected integer, found '{name}'"))))
}

/// Shared by the desktop and remote server; executes before either starts its UI/protocol.
pub fn cli(args: Vec<String>) -> i32 {
    let mut file = None;
    let mut function = None;
    let mut values = Vec::new();
    let mut max_steps = 10_000_000;
    let mut iter = args.into_iter();
    let parse = (|| -> Result<()> {
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--function" => function = Some(iter.next().ok_or_else(|| Error::new("--function requires a label"))?),
                "--max-steps" => {
                    max_steps = iter
                        .next()
                        .and_then(|s| s.parse().ok())
                        .filter(|n| *n > 0)
                        .ok_or_else(|| Error::new("--max-steps requires a positive integer"))?
                }
                "--" => {
                    for value in iter.by_ref() {
                        values.push(integer(&value)?);
                    }
                    break;
                }
                _ if file.is_none() && !arg.starts_with('-') => file = Some(arg),
                _ => return Err(Error::new(format!("unknown option: {arg}"))),
            }
        }
        if file.is_none() {
            return Err(Error::new(
                "usage: --riscv-run <file.asm> [--function label] [--max-steps N] [-- integer arguments]",
            ));
        }
        Ok(())
    })();
    if let Err(error) = parse {
        eprintln!("RISC-V: {error}");
        return 2;
    }
    let file = file.unwrap();
    let run = (|| -> Result<i32> {
        let text = std::fs::read_to_string(&file).map_err(|e| Error::new(e.to_string()))?;
        let program = Program::assemble(&text)?;
        let mut machine = Machine::new(program, function.as_deref(), &values)?;
        let outcome =
            machine.run(max_steps, &mut io::stdin().lock(), &mut io::stdout().lock(), &mut io::stderr().lock())?;
        if outcome.returned {
            println!(
                "Execution Done: {}({}) = {}",
                machine.entry(),
                values.iter().map(|v| (*v as i64).to_string()).collect::<Vec<_>>().join(", "),
                outcome.value as i64
            );
        }
        io::stdout().flush().ok();
        Ok(outcome.exit_code)
    })();
    match run {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{file}:{error}");
            1
        }
    }
}
