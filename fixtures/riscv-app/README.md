# RISC-V assembly

Open this folder in OpenIDEA. `.asm`, `.s`, and `.S` files use the built-in RISC-V editor and source simulator. No Java, cross compiler, or QEMU installation is needed.

- Open `hello.asm` and use **Run Current File** (or the gutter beside `main`). The Run console prints `Hello, RISC-V!`.
- Open `functions.asm` and click the gutter beside `sum`, `sum32`, or `poly`, then choose Run. Enter integer arguments in the dialog. For `poly`, `2 3 4 5 6` returns `569`.
- **Run Context** inside a function selects that function. Running a file selects `_start`, then `main`, or its first function when neither exists.
- A preceding `#` or `//` C signature supplies the parameter names and required argument count. Without a signature, enter any number of integers. Decimal, `0x` hexadecimal, `0b` binary, and leading-zero octal are accepted, including signed 64-bit values and unsigned 64-bit bit patterns.
- Arguments go to `a0`–`a7`; remaining arguments go to the stack, whose initial address is aligned to 16 bytes. `ret` from the selected entry completes the run and prints its signed `a0` result. Arguments are remembered per file and function.
- **Edit Run Configurations → RISC-V Assembly** lets you select a file, entry label, arguments, whether to prompt, and the instruction limit (10 million by default). Stop terminates the runner process, including a run waiting for input.

## Source dialect

The integer runner supports RV64I and M arithmetic, loads/stores, branches, calls, the ABI register names and `x0`–`x31`, and common pseudoinstructions (`li`, `la`, `mv`, `ret`, sign/zero extensions). It also supports the Zba, Zbb, and Zbs operations used in the course, including `sh1add.uw`, `rev8`, `orc.b`, rotations, counts, and single-bit operations. `andn`, `orn`, and `xnor` follow the RISC-V ISA: they complement the second source operand.

Floating-point arithmetic, conversions, loads/stores and fused operations accept `f0`–`f31` and ABI names; atomic word/doubleword operations work in the single simulated thread. This is an educational source simulator, with host floating-point arithmetic, not an IEEE exception/rounding conformance or cycle-accurate hardware model.

Directives include `.text`, `.data`, `.bss`, `.section .rodata`, `.equ`, `.globl`/`.global`, `.byte`, `.half`, `.word`, `.dword`, `.float`, `.double`, `.ascii`, `.asciz`/`.string`, `.zero`, `.skip`, and `.align`. GNU `.type`, `.size`, `.option`, and `.attribute` metadata are accepted. Comments use `#`, `//`, or `/* … */`. Expressions support constants, label addresses/differences, `.`, arithmetic and bit operators, and repeated numeric labels with `f`/`b` references. Section positions resume after switching sections. Data is little-endian, with no implicit data alignment; `.align n` aligns to `2**n` bytes. Misaligned loads/stores produce a source-line diagnostic.

RARS environment calls include print/read integer, string, character and floating point, heap allocation, exit, and print integer in hex/binary/unsigned. Linux-style `read` (63), `write` (64), and `exit` (93/94) work on the Run console. Other calls produce diagnostics. The initial stack is 1 MiB; static data and heap allocations are each capped at 64 MiB.

The runner executes one source file directly: it does not compile C/C++, link ELF objects or libc, load existing ELF executables, or implement an operating system. Each source instruction (including a pseudo-instruction) occupies one simulated 4-byte slot, so this mode is for source-level exercises rather than inspecting real instruction encodings. Use the GNU RISC-V toolchain and QEMU separately for Linux binaries or linked C/assembly projects. Compressed/vector instructions, assembler macros and `.include` are not supported.

## Backend checks

```
cargo test -p ide-riscv
cargo run -p ide-riscv --bin openidea-riscv -- fixtures/riscv-app/functions.asm --function poly -- 2 3 4 5 6
```

The same runner is embedded in the desktop and `openidea-server`, so local and remote run configurations share the implementation and process lifecycle.
