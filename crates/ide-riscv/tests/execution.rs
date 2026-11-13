use ide_riscv::{Machine, Program};
use std::io::Cursor;

fn execute(source: &str, function: Option<&str>, args: &[u64]) -> (ide_riscv::Outcome, String) {
    let mut machine = Machine::new(Program::assemble(source).unwrap(), function, args).unwrap();
    let mut out = Vec::new();
    let result = machine.run(10000, &mut Cursor::new(Vec::new()), &mut out, &mut Vec::new()).unwrap();
    (result, String::from_utf8(out).unwrap())
}

#[test]
fn course_functions() {
    let source = include_str!("../../../fixtures/riscv-app/functions.asm");
    assert_eq!(execute(source, Some("sum"), &[41, 1]).0.value, 42);
    assert_eq!(execute(source, Some("sum"), &[u64::MAX, 2]).0.value, 1);
    assert_eq!(execute(source, Some("sum32"), &[0x7fff_ffff, 1]).0.value, 0xffff_ffff_8000_0000);
    assert_eq!(execute(source, Some("poly"), &[2, 3, 4, 5, 6]).0.value, 569);
}

#[test]
fn stack_arguments_and_nested_calls() {
    let source = ".text\n.globl sum\nsum: addi sp, sp, -16\nsd ra, 0(sp)\ncall helper\nld ra, 0(sp)\naddi sp, sp, 16\nld t0, 0(sp)\nadd a0, a0, t0\nret\nhelper: add a0, a0, a1\nret";
    assert_eq!(execute(source, Some("sum"), &[1, 2, 3, 4, 5, 6, 7, 8, 9]).0.value, 12);
}

#[test]
fn directives_expressions_sections_and_local_labels() {
    let source = r##"
        /* A multiline comment:
            unsupported fake instructions
        */
        .equ CHAR_BIT, 8
        .data
    data: .word 0x4433
        .align 3
        .dword 0xffeeddccbbaa9988
    cur:
    size: .dword cur-data
        .byte 'x', 046, 0b101, -6
        .section .rodata
    text: .string "# not a comment", "// nor this"
        .bss
    zero: .zero 1024*1024
        .data
        .align 3
    ptr: .dword size
        .text
        .globl run
    run: la t0, ptr
         ld t0, 0(t0)
         ld a0, 0(t0)
         li t1, 3
    1:   addi a0, a0, CHAR_BIT
         addi t1, t1, -1
         bnez t1, 1b
         j 2f
         li a0, 999
    2:   ret // returns 16+3*8
    "##;
    assert_eq!(execute(source, Some("run"), &[]).0.value, 40);
}

#[test]
fn syscall_output_and_program_exit() {
    let (result, out) = execute(include_str!("../../../fixtures/riscv-app/hello.asm"), None, &[]);
    assert_eq!(out, "Hello, RISC-V!\n");
    assert_eq!(result.exit_code, 0);
    assert!(!result.returned);
    let linux = ".data\nmsg: .ascii \"hi\\n\"\n.text\n_start: li a0, 1\nla a1, msg\nli a2, 3\nli a7, 64\necall\nli a0, 7\nli a7, 93\necall";
    let (result, out) = execute(linux, None, &[]);
    assert_eq!(out, "hi\n");
    assert_eq!(result.exit_code, 7);
}

#[test]
fn isa_integer_edges_and_bit_extensions() {
    let cases = [
        ("div", i64::MIN as u64, u64::MAX, i64::MIN as u64),
        ("div", 100, 0, u64::MAX),
        ("rem", 100, 0, 100),
        ("divu", u64::MAX, 2, u64::MAX / 2),
        ("divw", 0x80000000, u64::MAX, 0xffffffff80000000),
        ("divuw", 0xffffffff, 1, u64::MAX),
        ("sllw", 0x40000000, 1, 0xffffffff80000000),
        ("srlw", 0x80000000, 1, 0x40000000),
        ("sraw", 0x80000000, 1, 0xffffffffc0000000),
        ("mulh", u64::MAX, 2, u64::MAX),
        ("mulhu", u64::MAX, u64::MAX, u64::MAX - 1),
        ("mulhsu", u64::MAX, u64::MAX, u64::MAX),
        ("andn", 15, 3, 12),
        ("orn", 0, 3, !3),
        ("xnor", 3, 3, u64::MAX),
        ("sh3add.uw", u64::MAX, 1, 0x7fffffff9),
        ("add.uw", u64::MAX, 1, 0x100000000),
        ("bext", 1 << 63, 63, 1),
        ("bset", 0, 63, 1 << 63),
        ("bclr", u64::MAX, 63, 0x7fffffffffffffff),
        ("binv", 0, 64, 1),
        ("ror", 1, 1, 1 << 63),
        ("min", u64::MAX, 0, u64::MAX),
        ("minu", u64::MAX, 0, 0),
    ];
    for (op, a, b, expected) in cases {
        let source = format!("run: {op} a0, a0, a1\nret");
        assert_eq!(execute(&source, Some("run"), &[a, b]).0.value, expected, "{op}");
    }
    for (op, a, expected) in [
        ("clz", 0, 64),
        ("ctzw", 0, 32),
        ("cpop", u64::MAX, 64),
        ("rev8", 0x1122334455667788, 0x8877665544332211),
        ("orc.b", 0x10001, 0xff00ff),
        ("sext.b", 255, u64::MAX),
        ("zext.h", u64::MAX, 65535),
    ] {
        assert_eq!(execute(&format!("run: {op} a0, a0\nret"), None, &[a]).0.value, expected, "{op}");
    }
}

#[test]
fn floating_point_and_atomic_memory() {
    let source = ".data\nx: .double 1.5\ny: .double 2.5\n.text\nrun: la t0, x\nfld ft0, 0(t0)\nfld ft1, 8(t0)\nfadd.d ft0, ft0, ft1\nfcvt.l.d a0, ft0\nret";
    assert_eq!(execute(source, None, &[]).0.value, 4);
    let atomic = ".data\nx: .word 5\n.text\nrun: la t0, x\nli t1, 2\namoadd.w a0, t1, (t0)\nlr.w t1, (t0)\naddi t1, t1, 3\nsc.w t2, t1, (t0)\nlw a0, (t0)\nret";
    assert_eq!(execute(atomic, None, &[]).0.value, 10);
}

#[test]
fn diagnostics_and_limits() {
    for source in [
        "run: badop a0, a0",
        "run: nop\n.equ A, B\n.equ B, A",
        "run: nop\n.equ A, 1\n.equ A, 2",
        ".data\n.zero 99999999999\n.text\nrun: ret",
        "/* no close",
    ] {
        assert!(Program::assemble(source).is_err(), "{source}");
    }
    let mut machine = Machine::new(Program::assemble("run: j run").unwrap(), None, &[]).unwrap();
    assert!(
        machine
            .run(10, &mut Cursor::new([]), &mut Vec::new(), &mut Vec::new())
            .unwrap_err()
            .message
            .contains("instruction limit")
    );
    let mut machine = Machine::new(
        Program::assemble(".section .rodata\nx: .dword 1\n.text\nrun: la t0, x\nsd a0, (t0)\nret").unwrap(),
        None,
        &[],
    )
    .unwrap();
    let error = machine.run(10, &mut Cursor::new([]), &mut Vec::new(), &mut Vec::new()).unwrap_err();
    assert_eq!(error.line, Some(5));
    assert!(error.message.contains("read-only"));
}

#[test]
fn signed_expressions_and_relative_numeric_branches() {
    assert_eq!(execute("run: li a0, -8/2 + -7%3\nret", None, &[]).0.value, (-5i64) as u64);
    assert_eq!(execute("run: li a0, 3\naddi a0, a0, -1\nbnez a0, -4\nj 8\nli a0, 99\nret", None, &[]).0.value, 0);
}
