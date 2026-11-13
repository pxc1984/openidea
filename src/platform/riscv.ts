/** RISC-V source conventions shared by the editor and run producers. */
export const RISCV_INSTRUCTIONS = new Set(
  `li la lla lui auipc mv nop ret j jr jal jalr call tail
add sub addi addw subw addiw neg negw sext.b sext.h sext.w zext.b zext.h zext.w
sll srl sra slli srli srai sllw srlw sraw slliw srliw sraiw
and or xor andi ori xori not andn orn xnor slt sltu slti sltiu seqz snez sltz sgtz
beq bne blt bge bltu bgeu beqz bnez blez bgez bltz bgtz bgt ble bgtu bleu
lb lbu lh lhu lw lwu ld sb sh sw sd
mul mulw mulh mulhu mulhsu div divu rem remu divw divuw remw remuw
sh1add sh2add sh3add sh1add.uw sh2add.uw sh3add.uw add.uw slli.uw
clz ctz cpop clzw ctzw cpopw min max minu maxu rev8 orc.b rol ror rori rolw rorw roriw
bclr bclri bext bexti binv binvi bset bseti ecall ebreak fence fence.i pause
csrrw csrrs csrrc csrrwi csrrsi csrrci csrr csrw csrs csrc csrwi csrsi csrci rdcycle rdtime rdinstret
flw fld fsw fsd fmv.w.x fmv.x.w fmv.d.x fmv.x.d
fadd.s fsub.s fmul.s fdiv.s fsqrt.s fmin.s fmax.s fsgnj.s fsgnjn.s fsgnjx.s feq.s flt.s fle.s fclass.s
fadd.d fsub.d fmul.d fdiv.d fsqrt.d fmin.d fmax.d fsgnj.d fsgnjn.d fsgnjx.d feq.d flt.d fle.d fclass.d
fmadd.s fmsub.s fnmadd.s fnmsub.s fmadd.d fmsub.d fnmadd.d fnmsub.d
fcvt.s.d fcvt.d.s fcvt.w.s fcvt.wu.s fcvt.l.s fcvt.lu.s fcvt.w.d fcvt.wu.d fcvt.l.d fcvt.lu.d
fcvt.s.w fcvt.s.wu fcvt.s.l fcvt.s.lu fcvt.d.w fcvt.d.wu fcvt.d.l fcvt.d.lu
fmv.s fmv.d fneg.s fneg.d fabs.s fabs.d`.split(/\s+/),
);

export const RISCV_REGISTERS = new Set(
  `zero ra sp gp tp fp t0 t1 t2 s0 s1 a0 a1 a2 a3 a4 a5 a6 a7 s2 s3 s4 s5 s6 s7 s8 s9 s10 s11 t3 t4 t5 t6
ft0 ft1 ft2 ft3 ft4 ft5 ft6 ft7 fs0 fs1 fa0 fa1 fa2 fa3 fa4 fa5 fa6 fa7 fs2 fs3 fs4 fs5 fs6 fs7 fs8 fs9 fs10 fs11 ft8 ft9 ft10 ft11`
    .split(/\s+/)
    .concat(
      Array.from({ length: 32 }, (_, i) => `x${i}`),
      Array.from({ length: 32 }, (_, i) => `f${i}`),
    ),
);

export interface RiscvFunction {
  name: string;
  line: number;
  /** Null means no signature; an empty array means an explicit void parameter list. */
  parameters: string[] | null;
}

export const isRiscvPath = (path: string) => /\.(asm|s)$/i.test(path);

export function riscvCode(source: string): string {
  return source.replace(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|\/\*[\s\S]*?\*\/|\/\/[^\n]*|#[^\n]*/g, (part) =>
    part.startsWith('"') || part.startsWith("'") ? part : part.replace(/[^\n]/g, " "),
  );
}

export function riscvFunctions(source: string): RiscvFunction[] {
  const lines = source.split("\n");
  const code = riscvCode(source).split("\n");
  const exports = new Set<string>();
  for (const line of code) {
    const global = /^\s*\.(?:globl|global)\s+(.+)/.exec(line);
    if (global) for (const name of global[1]!.split(/[\s,]+/)) exports.add(name);
    const typed = /^\s*\.type\s+([\w.$]+)\s*,\s*[@%]?function\b/.exec(line);
    if (typed) exports.add(typed[1]!);
  }
  const candidates: RiscvFunction[] = [];
  let text = true;
  for (let line = 0; line < code.length; line++) {
    const raw = code[line]!;
    if (/^\s*\.(?:data|bss|rodata)\b/.test(raw)) text = false;
    const section = /^\s*\.section\s+([^,\s]+)/.exec(raw);
    if (section) text = /^\.text(?:\.|$)/.test(section[1]!);
    if (/^\s*\.text\b/.test(raw)) text = true;
    if (!text) continue;
    const match = /^\s*([A-Za-z_$][\w.$]*)\s*:/.exec(raw);
    if (!match) continue;
    const name = match[1]!;
    const comment = lines[line - 1]?.trim().replace(/^(?:#|\/\/)\s*/, "");
    const signature = comment && /\b([A-Za-z_$][\w.$]*)\s*\(([^)]*)\)\s*;?$/.exec(comment);
    const parameters =
      signature?.[1] === name
        ? signature[2]!.trim() === "void" || !signature[2]!.trim()
          ? []
          : signature[2]!.split(",").map((p) => p.trim())
        : null;
    candidates.push({ name, line, parameters });
  }
  const marked = candidates.filter(
    (f) => exports.has(f.name) || f.parameters !== null || /^(main|_start)$/.test(f.name),
  );
  return marked.length ? marked : candidates.slice(0, 1);
}

export function riscvArguments(value: string, count?: number): string[] {
  const args = value.trim() ? value.trim().split(/[\s,]+/) : [];
  if (count !== undefined && args.length !== count) throw new Error(`Expected ${count} integer arguments`);
  for (const arg of args) {
    if (!/^[+-]?(?:0[xX][\da-fA-F]+|0[bB][01]+|0[0-7]*|[1-9]\d*)$/.test(arg))
      throw new Error("Expected integer arguments");
    const sign = arg.startsWith("-") ? -1n : 1n;
    const raw = arg.replace(/^[+-]/, "");
    const number = sign * BigInt(/^0[0-7]+$/.test(raw) ? `0o${raw.slice(1)}` : raw);
    if (number < -(1n << 63n) || number > (1n << 64n) - 1n)
      throw new Error("Integer argument is outside the 64-bit range");
  }
  return args;
}
