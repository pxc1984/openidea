use crate::asm::{Instruction, LIMIT, Program, RETURN, Region, STACK, STACK_SIZE};
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::io::{BufRead, Read, Write};

const OPS: &str = "li la lla lui auipc mv nop ret j jr jal jalr call tail \
add sub addi addw subw addiw neg negw sext.b sext.h sext.w zext.b zext.h zext.w \
sll srl sra slli srli srai sllw srlw sraw slliw srliw sraiw \
and or xor andi ori xori not andn orn xnor \
slt sltu slti sltiu seqz snez sltz sgtz \
beq bne blt bge bltu bgeu beqz bnez blez bgez bltz bgtz bgt ble bgtu bleu \
lb lbu lh lhu lw lwu ld sb sh sw sd \
mul mulw mulh mulhu mulhsu div divu rem remu divw divuw remw remuw \
sh1add sh2add sh3add sh1add.uw sh2add.uw sh3add.uw add.uw slli.uw \
clz ctz cpop clzw ctzw cpopw min max minu maxu rev8 orc.b rol ror rori rolw rorw roriw \
bclr bclri bext bexti binv binvi bset bseti \
ecall ebreak fence fence.i pause \
csrrw csrrs csrrc csrrwi csrrsi csrrci csrr csrw csrs csrc csrwi csrsi csrci rdcycle rdtime rdinstret \
flw fld fsw fsd fmv.w.x fmv.x.w fmv.d.x fmv.x.d \
fadd.s fsub.s fmul.s fdiv.s fsqrt.s fmin.s fmax.s fsgnj.s fsgnjn.s fsgnjx.s feq.s flt.s fle.s fclass.s \
fadd.d fsub.d fmul.d fdiv.d fsqrt.d fmin.d fmax.d fsgnj.d fsgnjn.d fsgnjx.d feq.d flt.d fle.d fclass.d \
fmadd.s fmsub.s fnmadd.s fnmsub.s fmadd.d fmsub.d fnmadd.d fnmsub.d \
fcvt.s.d fcvt.d.s fcvt.w.s fcvt.wu.s fcvt.l.s fcvt.lu.s fcvt.w.d fcvt.wu.d fcvt.l.d fcvt.lu.d \
fcvt.s.w fcvt.s.wu fcvt.s.l fcvt.s.lu fcvt.d.w fcvt.d.wu fcvt.d.l fcvt.d.lu \
fmv.s fmv.d fneg.s fneg.d fabs.s fabs.d";

pub(crate) fn supported(op: &str) -> bool {
    OPS.split_whitespace().any(|s| s == op) || atomic_op(op).is_some()
}

pub(crate) fn validate(instruction: &Instruction) -> Result<()> {
    let op = instruction.op.as_str();
    let count = instruction.args.len();
    let valid = match op {
        "nop" | "ret" | "ecall" | "ebreak" | "fence.i" | "pause" => count == 0,
        "fence" => count == 0 || count == 2,
        "j" | "jr" | "call" | "tail" => count == 1,
        "jal" => (1..=2).contains(&count),
        "jalr" => (1..=3).contains(&count),
        "li" | "la" | "lla" | "lui" | "auipc" | "mv" | "neg" | "negw" | "not" | "sext.b" | "sext.h" | "sext.w"
        | "zext.b" | "zext.h" | "zext.w" | "seqz" | "snez" | "sltz" | "sgtz" | "clz" | "ctz" | "cpop" | "clzw"
        | "ctzw" | "cpopw" | "rev8" | "orc.b" | "beqz" | "bnez" | "blez" | "bgez" | "bltz" | "bgtz" | "lb" | "lbu"
        | "lh" | "lhu" | "lw" | "lwu" | "ld" | "sb" | "sh" | "sw" | "sd" | "csrr" | "csrw" | "csrs" | "csrc"
        | "csrwi" | "csrsi" | "csrci" | "flw" | "fld" | "fsw" | "fsd" | "fmv.w.x" | "fmv.x.w" | "fmv.d.x"
        | "fmv.x.d" | "fclass.s" | "fclass.d" | "fmv.s" | "fmv.d" | "fneg.s" | "fneg.d" | "fabs.s" | "fabs.d" => {
            count == 2
        }
        "rdcycle" | "rdtime" | "rdinstret" => count == 1,
        _ if op.starts_with("fcvt.") || op.starts_with("fsqrt.") => (2..=3).contains(&count),
        _ if op.starts_with("fmadd.")
            || op.starts_with("fmsub.")
            || op.starts_with("fnmadd.")
            || op.starts_with("fnmsub.") =>
        {
            (4..=5).contains(&count)
        }
        _ if atomic_op(op).is_some_and(|(name, _)| name == "lr") => count == 2,
        _ if op.starts_with('f')
            && !op.starts_with("fsgnj")
            && !op.starts_with("fmin")
            && !op.starts_with("fmax")
            && !op.starts_with("feq")
            && !op.starts_with("flt")
            && !op.starts_with("fle") =>
        {
            (3..=4).contains(&count)
        }
        _ => count == 3,
    };
    if !valid {
        return Err(Error::new(format!("wrong number of operands for {op}")));
    }
    Ok(())
}

fn atomic_op(op: &str) -> Option<(&str, usize)> {
    let op = op.trim_end_matches(".aqrl").trim_end_matches(".aq").trim_end_matches(".rl");
    let (name, size) = op.rsplit_once('.')?;
    let width = match size {
        "w" => 4,
        "d" => 8,
        _ => return None,
    };
    ["lr", "sc", "amoswap", "amoadd", "amoxor", "amoand", "amoor", "amomin", "amomax", "amominu", "amomaxu"]
        .contains(&name)
        .then_some((name, width))
}

pub struct Machine {
    program: Program,
    pub registers: [u64; 32],
    floating: [u64; 32],
    csr: BTreeMap<u64, u64>,
    pc: u64,
    steps: u64,
    entry: String,
    reservation: Option<(u64, usize)>,
}

#[derive(Debug)]
pub struct Outcome {
    pub value: u64,
    pub exit_code: i32,
    pub returned: bool,
}

impl Machine {
    pub fn new(mut program: Program, function: Option<&str>, values: &[u64]) -> Result<Self> {
        let (entry, pc) = if let Some(name) = function {
            (name.to_owned(), program.symbol(name)?)
        } else if let Ok(address) = program.symbol("_start") {
            ("_start".to_owned(), address)
        } else if let Ok(address) = program.symbol("main") {
            ("main".to_owned(), address)
        } else {
            program
                .functions()
                .into_iter()
                .next()
                .unwrap_or_else(|| ("<entry>".to_owned(), *program.code.keys().next().unwrap()))
        };
        if !program.code.contains_key(&pc) {
            return Err(Error::new(format!("'{entry}' is not an executable function")));
        }
        program.regions.push(Region { start: STACK - STACK_SIZE as u64, data: vec![0; STACK_SIZE], writable: true });
        program.regions.push(Region { start: 0x4000_0000, data: Vec::new(), writable: true });
        let mut machine = Self {
            program,
            registers: [0; 32],
            floating: [u64::MAX; 32],
            csr: BTreeMap::new(),
            pc,
            steps: 0,
            entry,
            reservation: None,
        };
        machine.registers[1] = RETURN;
        let extra = values
            .len()
            .saturating_sub(8)
            .checked_mul(8)
            .filter(|n| *n <= STACK_SIZE - 16)
            .ok_or_else(|| Error::new("too many function arguments"))?;
        machine.registers[2] = STACK - (extra as u64).div_ceil(16) * 16;
        machine.registers[3] = 0x1100_0800;
        for (i, value) in values.iter().enumerate() {
            if i < 8 {
                machine.registers[10 + i] = *value;
            } else {
                machine.store(machine.registers[2] + ((i - 8) * 8) as u64, 8, *value)?;
            }
        }
        Ok(machine)
    }

    pub fn entry(&self) -> &str {
        &self.entry
    }

    pub fn run(
        &mut self,
        max_steps: u64,
        input: &mut dyn BufRead,
        output: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> Result<Outcome> {
        loop {
            if self.pc == RETURN {
                return Ok(Outcome { value: self.registers[10], exit_code: 0, returned: true });
            }
            if self.steps >= max_steps {
                return Err(Error::new(format!("instruction limit ({max_steps}) exceeded at 0x{:x}", self.pc)));
            }
            let instruction = self
                .program
                .code
                .get(&self.pc)
                .cloned()
                .ok_or_else(|| Error::new(format!("no instruction at 0x{:x}", self.pc)))?;
            let line = instruction.line;
            let outcome = self.step(&instruction, input, output, stderr).map_err(|e| e.at(line))?;
            self.registers[0] = 0;
            self.steps += 1;
            if let Some(code) = outcome {
                return Ok(Outcome { value: self.registers[10], exit_code: code, returned: false });
            }
        }
    }

    fn load(&self, address: u64, width: usize) -> Result<u64> {
        let bytes = self.bytes(address, width)?;
        let mut result = [0; 8];
        result[..width].copy_from_slice(bytes);
        Ok(u64::from_le_bytes(result))
    }

    fn bytes(&self, address: u64, width: usize) -> Result<&[u8]> {
        for region in &self.program.regions {
            if let Some(offset) = address.checked_sub(region.start).and_then(|n| usize::try_from(n).ok())
                && let Some(bytes) = offset.checked_add(width).and_then(|end| region.data.get(offset..end))
            {
                return Ok(bytes);
            }
        }
        Err(Error::new(format!("invalid memory read at 0x{address:x}")))
    }

    fn store(&mut self, address: u64, width: usize, value: u64) -> Result<()> {
        if self.reservation.is_some_and(|(a, _)| a == address) {
            self.reservation = None;
        }
        self.write_bytes(address, &value.to_le_bytes()[..width])
    }

    fn write_bytes(&mut self, address: u64, bytes: &[u8]) -> Result<()> {
        for region in &mut self.program.regions {
            if let Some(offset) = address.checked_sub(region.start).and_then(|n| usize::try_from(n).ok())
                && let Some(dest) = offset.checked_add(bytes.len()).and_then(|end| region.data.get_mut(offset..end))
            {
                if !region.writable {
                    return Err(Error::new(format!("write to read-only memory at 0x{address:x}")));
                }
                dest.copy_from_slice(bytes);
                self.reservation = None;
                return Ok(());
            }
        }
        Err(Error::new(format!("invalid memory write at 0x{address:x}")))
    }

    fn value(&self, text: &str, line: usize) -> Result<u64> {
        if let Some(inner) = text.strip_prefix("%hi(").and_then(|s| s.strip_suffix(')')) {
            return Ok(self.program.eval(inner, self.pc, line)?.wrapping_add(0x800) >> 12);
        }
        if let Some(inner) = text.strip_prefix("%lo(").and_then(|s| s.strip_suffix(')')) {
            return Ok(sign(self.program.eval(inner, self.pc, line)? & 0xfff, 12));
        }
        self.program.eval(text, self.pc, line)
    }

    fn address(&self, text: &str, line: usize) -> Result<u64> {
        if text.ends_with(')')
            && let Some(open) = text.rfind('(')
            && let Ok(register) = reg(&text[open + 1..text.len() - 1])
        {
            let offset = text[..open].trim();
            return Ok(self.registers[register].wrapping_add(if offset.is_empty() {
                0
            } else {
                self.value(offset, line)?
            }));
        }
        self.value(text, line)
    }

    fn step(
        &mut self,
        instruction: &Instruction,
        input: &mut dyn BufRead,
        output: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> Result<Option<i32>> {
        let op = instruction.op.as_str();
        let args = &instruction.args;
        let arg = |i: usize| {
            args.get(i).map(String::as_str).ok_or_else(|| Error::new(format!("missing operand {} for {op}", i + 1)))
        };
        let r = |i| reg(arg(i)?);
        let x = |i| -> Result<u64> { Ok(self.registers[r(i)?]) };
        let imm = |i| self.value(arg(i)?, instruction.line);
        let target = |i| -> Result<u64> {
            let text = arg(i)?;
            match crate::expr::eval(text, &|_| Err(Error::new("symbolic target"))) {
                Ok(offset) => Ok(self.pc.wrapping_add(offset)),
                Err(_) => self.value(text, instruction.line),
            }
        };
        let mut next = self.pc.wrapping_add(4);
        match op {
            "nop" | "fence" | "fence.i" | "pause" => {}
            "ebreak" => return Err(Error::new("breakpoint instruction reached")),
            "li" | "la" | "lla" => {
                self.registers[r(0)?] = imm(1)?;
            }
            "lui" => {
                self.registers[r(0)?] = sign(imm(1)? << 12, 32);
            }
            "auipc" => {
                self.registers[r(0)?] = self.pc.wrapping_add(sign(imm(1)? << 12, 32));
            }
            "mv" => {
                self.registers[r(0)?] = x(1)?;
            }
            "ret" => next = self.registers[1] & !1,
            "j" | "tail" | "call" => {
                next = target(0)?;
                if op == "call" {
                    self.registers[1] = self.pc + 4;
                }
            }
            "jr" => next = x(0)? & !1,
            "jal" => {
                let (dest, target) = if args.len() == 1 { (1, target(0)?) } else { (r(0)?, target(1)?) };
                self.registers[dest] = self.pc + 4;
                next = target;
            }
            "jalr" => {
                let (dest, target) = match args.len() {
                    1 => (1, x(0)?),
                    2 => (r(0)?, self.address(arg(1)?, instruction.line)?),
                    3 => (r(0)?, x(1)?.wrapping_add(imm(2)?)),
                    _ => return Err(Error::new("invalid jalr operands")),
                };
                self.registers[dest] = self.pc + 4;
                next = target & !1;
            }
            "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" | "bgt" | "ble" | "bgtu" | "bleu" => {
                let (a, b) = (x(0)?, x(1)?);
                let branch = match op {
                    "beq" => a == b,
                    "bne" => a != b,
                    "blt" => (a as i64) < (b as i64),
                    "bge" => (a as i64) >= (b as i64),
                    "bltu" => a < b,
                    "bgeu" => a >= b,
                    "bgt" => (a as i64) > (b as i64),
                    "ble" => (a as i64) <= (b as i64),
                    "bgtu" => a > b,
                    _ => a <= b,
                };
                if branch {
                    next = target(2)?;
                }
            }
            "beqz" | "bnez" | "blez" | "bgez" | "bltz" | "bgtz" => {
                let a = x(0)? as i64;
                if match op {
                    "beqz" => a == 0,
                    "bnez" => a != 0,
                    "blez" => a <= 0,
                    "bgez" => a >= 0,
                    "bltz" => a < 0,
                    _ => a > 0,
                } {
                    next = target(1)?;
                }
            }
            "lb" | "lbu" | "lh" | "lhu" | "lw" | "lwu" | "ld" => {
                let width = match op {
                    "lb" | "lbu" => 1,
                    "lh" | "lhu" => 2,
                    "lw" | "lwu" => 4,
                    _ => 8,
                };
                let address = self.address(arg(1)?, instruction.line)?;
                aligned(address, width)?;
                let value = self.load(address, width)?;
                self.registers[r(0)?] =
                    if op.ends_with('u') || width == 8 { value } else { sign(value, width as u32 * 8) };
            }
            "sb" | "sh" | "sw" | "sd" => {
                let width = match op {
                    "sb" => 1,
                    "sh" => 2,
                    "sw" => 4,
                    _ => 8,
                };
                let address = self.address(arg(1)?, instruction.line)?;
                aligned(address, width)?;
                self.store(address, width, x(0)?)?;
            }
            "neg" | "negw" | "not" | "sext.b" | "sext.h" | "sext.w" | "zext.b" | "zext.h" | "zext.w" | "seqz"
            | "snez" | "sltz" | "sgtz" | "clz" | "ctz" | "cpop" | "clzw" | "ctzw" | "cpopw" | "rev8" | "orc.b" => {
                let a = x(1)?;
                self.registers[r(0)?] = match op {
                    "neg" => a.wrapping_neg(),
                    "negw" => sign(a.wrapping_neg(), 32),
                    "not" => !a,
                    "sext.b" => sign(a, 8),
                    "sext.h" => sign(a, 16),
                    "sext.w" => sign(a, 32),
                    "zext.b" => a & 255,
                    "zext.h" => a & 65535,
                    "zext.w" => a & 0xffff_ffff,
                    "seqz" => (a == 0) as u64,
                    "snez" => (a != 0) as u64,
                    "sltz" => ((a as i64) < 0) as u64,
                    "sgtz" => ((a as i64) > 0) as u64,
                    "clz" => a.leading_zeros().into(),
                    "ctz" => a.trailing_zeros().into(),
                    "cpop" => a.count_ones().into(),
                    "clzw" => (a as u32).leading_zeros().into(),
                    "ctzw" => (a as u32).trailing_zeros().into(),
                    "cpopw" => (a as u32).count_ones().into(),
                    "rev8" => a.swap_bytes(),
                    _ => a
                        .to_le_bytes()
                        .iter()
                        .enumerate()
                        .fold(0, |v, (i, b)| v | (if *b == 0 { 0 } else { 255 } << (i * 8))),
                };
            }
            "ecall" => {
                if let Some(code) = self.syscall(input, output, stderr)? {
                    return Ok(Some(code));
                }
            }
            _ if atomic_op(op).is_some() => {
                let (name, width) = atomic_op(op).unwrap();
                let address_index = if name == "lr" { 1 } else { 2 };
                let address = self.address(arg(address_index)?, instruction.line)?;
                aligned(address, width)?;
                let old = self.load(address, width)?;
                let old = if width == 4 { sign(old, 32) } else { old };
                let dest = r(0)?;
                if name == "lr" {
                    self.reservation = Some((address, width));
                    self.registers[dest] = old;
                } else if name == "sc" {
                    let valid = self.reservation == Some((address, width));
                    let value = x(1)?;
                    self.reservation = None;
                    if valid {
                        self.store(address, width, value)?;
                    }
                    self.registers[dest] = u64::from(!valid);
                } else {
                    let a = if width == 4 { sign(x(1)?, 32) } else { x(1)? };
                    let value = match name {
                        "amoswap" => a,
                        "amoadd" => old.wrapping_add(a),
                        "amoxor" => old ^ a,
                        "amoand" => old & a,
                        "amoor" => old | a,
                        "amomin" => (old as i64).min(a as i64) as u64,
                        "amomax" => (old as i64).max(a as i64) as u64,
                        "amominu" => old.min(a),
                        _ => old.max(a),
                    };
                    self.store(address, width, value)?;
                    self.registers[dest] = old;
                }
            }
            _ if op.starts_with('f') => self.fp(instruction)?,
            _ if op.starts_with("csr") || matches!(op, "rdcycle" | "rdtime" | "rdinstret") => {
                let dest = if op.starts_with("rd")
                    || matches!(op, "csrr" | "csrrw" | "csrrs" | "csrrc" | "csrrwi" | "csrrsi" | "csrrci")
                {
                    Some(r(0)?)
                } else {
                    None
                };
                let key = if op.starts_with("rd") {
                    match op {
                        "rdcycle" => 0xc00,
                        "rdtime" => 0xc01,
                        _ => 0xc02,
                    }
                } else {
                    csr_number(arg(if dest.is_some() { 1 } else { 0 })?)?
                };
                let old = if (0xc00..=0xc02).contains(&key) { self.steps } else { *self.csr.get(&key).unwrap_or(&0) };
                if !op.starts_with("rd") && op != "csrr" {
                    let index = if dest.is_some() { 2 } else { 1 };
                    let value = if op.ends_with('i') { imm(index)? } else { x(index)? };
                    let value = if op.contains("csrrs") || op.starts_with("csrs") {
                        old | value
                    } else if op.contains("csrrc") || op.starts_with("csrc") {
                        old & !value
                    } else {
                        value
                    };
                    self.csr.insert(key, value);
                }
                if let Some(dest) = dest {
                    self.registers[dest] = old;
                }
            }
            _ => {
                let (a, b) = (x(1)?, if is_immediate(op) { imm(2)? } else { x(2)? });
                if matches!(op, "addi" | "addiw" | "andi" | "ori" | "xori" | "slti" | "sltiu")
                    && !(-2048..=2047).contains(&(b as i64))
                {
                    return Err(Error::new("immediate must fit a signed 12-bit integer"));
                }
                if is_immediate(op)
                    && !matches!(op, "addi" | "addiw" | "andi" | "ori" | "xori" | "slti" | "sltiu")
                    && b >= if matches!(op, "slliw" | "srliw" | "sraiw" | "roriw") { 32 } else { 64 }
                {
                    return Err(Error::new("shift immediate is out of range"));
                }
                let word = matches!(
                    op,
                    "addw"
                        | "subw"
                        | "addiw"
                        | "sllw"
                        | "srlw"
                        | "sraw"
                        | "slliw"
                        | "srliw"
                        | "sraiw"
                        | "mulw"
                        | "divw"
                        | "divuw"
                        | "remw"
                        | "remuw"
                        | "rolw"
                        | "rorw"
                        | "roriw"
                );
                let (a, b) = if word { (sign(a, 32), sign(b, 32)) } else { (a, b) };
                let shift = b as u32 & if word { 31 } else { 63 };
                let value = match op {
                    "add" | "addi" | "addw" | "addiw" => a.wrapping_add(b),
                    "sub" | "subw" => a.wrapping_sub(b),
                    "sll" | "slli" | "sllw" | "slliw" => a << shift,
                    "srl" | "srli" => a >> shift,
                    "srlw" | "srliw" => (a as u32 >> shift) as u64,
                    "sra" | "srai" | "sraw" | "sraiw" => ((a as i64) >> shift) as u64,
                    "and" | "andi" => a & b,
                    "or" | "ori" => a | b,
                    "xor" | "xori" => a ^ b,
                    "andn" => a & !b,
                    "orn" => a | !b,
                    "xnor" => !(a ^ b),
                    "slt" | "slti" => ((a as i64) < (b as i64)) as u64,
                    "sltu" | "sltiu" => (a < b) as u64,
                    "mul" | "mulw" => a.wrapping_mul(b),
                    "mulh" => (((a as i64 as i128) * (b as i64 as i128)) >> 64) as u64,
                    "mulhu" => (((a as u128) * (b as u128)) >> 64) as u64,
                    "mulhsu" => (((a as i64 as i128) * (b as u128 as i128)) >> 64) as u64,
                    "div" | "divw" => {
                        if b == 0 {
                            u64::MAX
                        } else {
                            (a as i64).wrapping_div(b as i64) as u64
                        }
                    }
                    "rem" | "remw" => {
                        if b == 0 {
                            a
                        } else {
                            (a as i64).wrapping_rem(b as i64) as u64
                        }
                    }
                    "divu" => a.checked_div(b).unwrap_or(u64::MAX),
                    "remu" => {
                        if b == 0 {
                            a
                        } else {
                            a % b
                        }
                    }
                    "divuw" => u64::from((a as u32).checked_div(b as u32).unwrap_or(u32::MAX)),
                    "remuw" => {
                        if b as u32 == 0 {
                            a
                        } else {
                            ((a as u32) % (b as u32)) as u64
                        }
                    }
                    "min" => (a as i64).min(b as i64) as u64,
                    "max" => (a as i64).max(b as i64) as u64,
                    "minu" => a.min(b),
                    "maxu" => a.max(b),
                    "add.uw" => (a as u32 as u64).wrapping_add(b),
                    "slli.uw" => (a as u32 as u64) << shift,
                    "sh1add" | "sh2add" | "sh3add" | "sh1add.uw" | "sh2add.uw" | "sh3add.uw" => {
                        let a = if op.ends_with(".uw") { a as u32 as u64 } else { a };
                        (a << (op.as_bytes()[2] - b'0')).wrapping_add(b)
                    }
                    "bclr" | "bclri" => a & !(1 << shift),
                    "bset" | "bseti" => a | (1 << shift),
                    "binv" | "binvi" => a ^ (1 << shift),
                    "bext" | "bexti" => (a >> shift) & 1,
                    "rol" => a.rotate_left(shift),
                    "ror" | "rori" => a.rotate_right(shift),
                    "rolw" => (a as u32).rotate_left(shift) as u64,
                    "rorw" | "roriw" => (a as u32).rotate_right(shift) as u64,
                    _ => return Err(Error::new(format!("unsupported instruction: {op}"))),
                };
                self.registers[r(0)?] = if word { sign(value, 32) } else { value };
            }
        }
        self.pc = next;
        Ok(None)
    }

    fn syscall(
        &mut self,
        input: &mut dyn BufRead,
        output: &mut dyn Write,
        stderr: &mut dyn Write,
    ) -> Result<Option<i32>> {
        let a = self.registers[10];
        let b = self.registers[11];
        let c = self.registers[12];
        let io = |e: std::io::Error| Error::new(e.to_string());
        match self.registers[17] {
            1 => write!(output, "{}", a as i32).map_err(io)?,
            2 => write!(output, "{}", f32::from_bits(self.floating[10] as u32)).map_err(io)?,
            3 => write!(output, "{}", f64::from_bits(self.floating[10])).map_err(io)?,
            4 => {
                let mut address = a;
                loop {
                    let byte = self.load(address, 1)? as u8;
                    if byte == 0 {
                        break;
                    }
                    output.write_all(&[byte]).map_err(io)?;
                    address += 1;
                }
            }
            5..=7 => {
                output.flush().map_err(io)?;
                let mut line = String::new();
                input.read_line(&mut line).map_err(io)?;
                match self.registers[17] {
                    5 => self.registers[10] = crate::integer(line.trim())?,
                    6 => {
                        self.floating[10] = 0xffff_ffff_0000_0000
                            | u64::from(
                                line.trim().parse::<f32>().map_err(|_| Error::new("expected float input"))?.to_bits(),
                            )
                    }
                    _ => {
                        self.floating[10] =
                            line.trim().parse::<f64>().map_err(|_| Error::new("expected double input"))?.to_bits()
                    }
                }
            }
            8 => {
                let count = usize::try_from(b)
                    .ok()
                    .filter(|n| *n <= LIMIT)
                    .ok_or_else(|| Error::new("invalid input buffer size"))?;
                if count > 0 {
                    output.flush().map_err(io)?;
                    let mut line = String::new();
                    input.take((count - 1) as u64).read_line(&mut line).map_err(io)?;
                    let mut bytes = line.into_bytes();
                    bytes.truncate(count - 1);
                    bytes.push(0);
                    self.write_bytes(a, &bytes)?;
                }
            }
            9 => {
                let heap = self.program.regions.last_mut().unwrap();
                let old = heap.data.len();
                let amount = usize::try_from(a)
                    .ok()
                    .filter(|n| *n <= LIMIT.saturating_sub(old))
                    .ok_or_else(|| Error::new("invalid heap allocation"))?;
                heap.data.resize(old + amount, 0);
                self.registers[10] = heap.start + old as u64;
            }
            10 => return Ok(Some(0)),
            93 | 94 => return Ok(Some(a as i32)),
            11 => output.write_all(&[a as u8]).map_err(io)?,
            12 => {
                output.flush().map_err(io)?;
                let mut byte = [0];
                self.registers[10] =
                    if input.read(&mut byte).map_err(io)? == 0 { u64::MAX } else { u64::from(byte[0]) };
            }
            34 => write!(output, "{:08x}", a as u32).map_err(io)?,
            35 => write!(output, "{:032b}", a as u32).map_err(io)?,
            36 => write!(output, "{}", a as u32).map_err(io)?,
            64 => {
                let count =
                    usize::try_from(c).ok().filter(|n| *n <= LIMIT).ok_or_else(|| Error::new("invalid write size"))?;
                let bytes = self.bytes(b, count)?;
                match a {
                    1 => output.write_all(bytes).map_err(io)?,
                    2 => stderr.write_all(bytes).map_err(io)?,
                    _ => {
                        self.registers[10] = (-9i64) as u64;
                        return Ok(None);
                    }
                }
                self.registers[10] = count as u64;
            }
            63 => {
                if a != 0 {
                    self.registers[10] = (-9i64) as u64;
                    return Ok(None);
                }
                let count =
                    usize::try_from(c).ok().filter(|n| *n <= LIMIT).ok_or_else(|| Error::new("invalid read size"))?;
                self.bytes(b, count)?;
                output.flush().map_err(io)?;
                let mut bytes = vec![0; count];
                let read = input.read(&mut bytes).map_err(io)?;
                self.write_bytes(b, &bytes[..read])?;
                self.registers[10] = read as u64;
            }
            number => return Err(Error::new(format!("unsupported environment call: {number}"))),
        }
        Ok(None)
    }

    fn fp(&mut self, instruction: &Instruction) -> Result<()> {
        let op = instruction.op.as_str();
        let args = &instruction.args;
        let arg =
            |i: usize| args.get(i).map(String::as_str).ok_or_else(|| Error::new("missing floating-point operand"));
        let fr = |i| freg(arg(i)?);
        let bits = |i| -> Result<u64> { Ok(self.floating[fr(i)?]) };
        let single = op.ends_with(".s");
        let value = |i| -> Result<f64> {
            Ok(if single { f64::from(f32::from_bits(bits(i)? as u32)) } else { f64::from_bits(bits(i)?) })
        };
        let result =
            |v: f64| if single { 0xffff_ffff_0000_0000 | u64::from((v as f32).to_bits()) } else { v.to_bits() };
        if matches!(op, "flw" | "fld" | "fsw" | "fsd") {
            let address = self.address(arg(1)?, instruction.line)?;
            let width = if op.ends_with('w') { 4 } else { 8 };
            aligned(address, width)?;
            let dest = fr(0)?;
            if op.starts_with("fl") {
                let loaded = self.load(address, width)?;
                self.floating[dest] = if width == 4 { loaded | 0xffff_ffff_0000_0000 } else { loaded };
            } else {
                self.store(address, width, self.floating[dest])?;
            }
            return Ok(());
        }
        match op {
            "fmv.w.x" | "fmv.d.x" => {
                self.floating[fr(0)?] = if op == "fmv.w.x" {
                    self.registers[reg(arg(1)?)?] | 0xffff_ffff_0000_0000
                } else {
                    self.registers[reg(arg(1)?)?]
                };
            }
            "fmv.x.w" | "fmv.x.d" => {
                self.registers[reg(arg(0)?)?] = if op == "fmv.x.w" { sign(bits(1)?, 32) } else { bits(1)? };
            }
            "fmv.s" | "fmv.d" => self.floating[fr(0)?] = bits(1)?,
            "feq.s" | "feq.d" | "flt.s" | "flt.d" | "fle.s" | "fle.d" => {
                let (a, b) = (value(1)?, value(2)?);
                self.registers[reg(arg(0)?)?] = u64::from(if op.starts_with("feq") {
                    a == b
                } else if op.starts_with("flt") {
                    a < b
                } else {
                    a <= b
                });
            }
            "fclass.s" | "fclass.d" => {
                let v = value(1)?;
                let class = if v.is_infinite() {
                    if v.is_sign_negative() { 0 } else { 7 }
                } else if v == 0.0 {
                    if v.is_sign_negative() { 3 } else { 4 }
                } else if v.is_nan() {
                    let quiet = if single { bits(1)? & (1 << 22) != 0 } else { bits(1)? & (1 << 51) != 0 };
                    if quiet { 9 } else { 8 }
                } else if if single { (v as f32).is_subnormal() } else { v.is_subnormal() } {
                    if v.is_sign_negative() { 2 } else { 5 }
                } else if v.is_sign_negative() {
                    1
                } else {
                    6
                };
                self.registers[reg(arg(0)?)?] = 1 << class;
            }
            _ if op.starts_with("fsgnj") || op.starts_with("fabs") || op.starts_with("fneg") => {
                let a = bits(1)?;
                let mask = if single { 1 << 31 } else { 1 << 63 };
                let b = if op.starts_with("fabs") {
                    0
                } else if op.starts_with("fneg") {
                    !a
                } else {
                    bits(2)?
                };
                let sign = if op.starts_with("fsgnjn") {
                    !b & mask
                } else if op.starts_with("fsgnjx") {
                    (a ^ b) & mask
                } else {
                    b & mask
                };
                self.floating[fr(0)?] = (a & !mask) | sign;
            }
            _ if op.starts_with("fcvt.") => {
                let parts: Vec<_> = op.split('.').collect();
                let dest = parts[1];
                let source = parts[2];
                if matches!(source, "s" | "d") {
                    let a = if source == "s" {
                        f64::from(f32::from_bits(bits(1)? as u32))
                    } else {
                        f64::from_bits(bits(1)?)
                    };
                    if matches!(dest, "s" | "d") {
                        self.floating[fr(0)?] = if dest == "s" {
                            0xffff_ffff_0000_0000 | u64::from((a as f32).to_bits())
                        } else {
                            a.to_bits()
                        };
                    } else {
                        let rounded = match args.get(2).map(String::as_str).unwrap_or("dyn") {
                            "rtz" => a.trunc(),
                            "rdn" => a.floor(),
                            "rup" => a.ceil(),
                            "rne" | "dyn" => a.round_ties_even(),
                            "rmm" => a.round(),
                            _ => return Err(Error::new("unknown rounding mode")),
                        };
                        self.registers[reg(arg(0)?)?] = match dest {
                            "w" => (rounded as i32 as i64) as u64,
                            "wu" => sign(rounded as u32 as u64, 32),
                            "l" => (rounded as i64) as u64,
                            "lu" => rounded as u64,
                            _ => return Err(Error::new("invalid conversion")),
                        };
                    }
                } else {
                    let a = self.registers[reg(arg(1)?)?];
                    let v = match source {
                        "w" => (a as i32) as f64,
                        "wu" => (a as u32) as f64,
                        "l" => (a as i64) as f64,
                        "lu" => a as f64,
                        _ => return Err(Error::new("invalid conversion")),
                    };
                    self.floating[fr(0)?] =
                        if dest == "s" { 0xffff_ffff_0000_0000 | u64::from((v as f32).to_bits()) } else { v.to_bits() };
                }
            }
            _ => {
                let a = value(1)?;
                let base = op.split('.').next().unwrap();
                let v = if base == "fsqrt" {
                    a.sqrt()
                } else {
                    let b = value(2)?;
                    match base {
                        "fadd" => a + b,
                        "fsub" => a - b,
                        "fmul" => a * b,
                        "fdiv" => a / b,
                        "fmin" => a.min(b),
                        "fmax" => a.max(b),
                        "fmadd" | "fmsub" | "fnmadd" | "fnmsub" => {
                            let c = value(3)?;
                            let (a, c) = match base {
                                "fmsub" => (a, -c),
                                "fnmadd" => (-a, -c),
                                "fnmsub" => (-a, c),
                                _ => (a, c),
                            };
                            if single { f64::from((a as f32).mul_add(b as f32, c as f32)) } else { a.mul_add(b, c) }
                        }
                        _ => return Err(Error::new(format!("unsupported instruction: {op}"))),
                    }
                };
                self.floating[fr(0)?] = result(v);
            }
        }
        Ok(())
    }
}

fn sign(value: u64, bits: u32) -> u64 {
    (((value << (64 - bits)) as i64) >> (64 - bits)) as u64
}
fn aligned(address: u64, width: usize) -> Result<()> {
    if !address.is_multiple_of(width as u64) {
        Err(Error::new(format!("unaligned {width}-byte access at 0x{address:x}")))
    } else {
        Ok(())
    }
}
fn is_immediate(op: &str) -> bool {
    matches!(
        op,
        "addi"
            | "addiw"
            | "andi"
            | "ori"
            | "xori"
            | "slti"
            | "sltiu"
            | "slli"
            | "srli"
            | "srai"
            | "slliw"
            | "srliw"
            | "sraiw"
            | "rori"
            | "roriw"
            | "slli.uw"
            | "bclri"
            | "bseti"
            | "binvi"
            | "bexti"
    )
}
fn reg(text: &str) -> Result<usize> {
    const NAMES: [&str; 32] = [
        "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7",
        "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
    ];
    if text == "fp" {
        return Ok(8);
    }
    NAMES
        .iter()
        .position(|name| *name == text)
        .or_else(|| text.strip_prefix('x').and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < 32))
        .ok_or_else(|| Error::new(format!("invalid integer register: {text}")))
}
fn freg(text: &str) -> Result<usize> {
    const NAMES: [&str; 32] = [
        "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1", "fa2", "fa3", "fa4", "fa5",
        "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7", "fs8", "fs9", "fs10", "fs11", "ft8", "ft9", "ft10",
        "ft11",
    ];
    NAMES
        .iter()
        .position(|name| *name == text)
        .or_else(|| text.strip_prefix('f').and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < 32))
        .ok_or_else(|| Error::new(format!("invalid floating-point register: {text}")))
}
fn csr_number(text: &str) -> Result<u64> {
    match text {
        "fflags" => Ok(1),
        "frm" => Ok(2),
        "fcsr" => Ok(3),
        "cycle" => Ok(0xc00),
        "time" => Ok(0xc01),
        "instret" => Ok(0xc02),
        _ => crate::integer(text),
    }
}
