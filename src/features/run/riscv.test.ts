import { beforeEach, describe, expect, it, vi } from "vitest";
import { riscvArguments, riscvFunctions } from "@/platform/riscv";
import { OPTIMISTIC, fileTargets, produceContext, produceFile, toolsFor } from "./producers";
import { prepareRiscv, riscvConfig, riscvType } from "./riscv";
import { promptValue } from "./dialogs";

vi.mock("./dialogs", () => ({ promptValue: vi.fn() }));
const source = `.data\nmsg: .string "#sum:"\n.text\n.globl sum, poly\n# int64_t sum(int64_t a, int64_t b)\nsum: add a0, a0, a1\nret\n// int64_t poly(int64_t x)\npoly: mul a0, a0, a0\nret\n`;
const input = { path: "/course/math.asm", root: "/course", configs: [], content: source };
beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
});

describe("RISC-V run support", () => {
  it("detects exported functions and signatures without data or comment labels", () => {
    expect(riscvFunctions(source)).toEqual([
      { name: "sum", line: 5, parameters: ["int64_t a", "int64_t b"] },
      { name: "poly", line: 8, parameters: ["int64_t x"] },
    ]);
    expect(riscvFunctions("/*\n.globl fake\nfake: ret\n*/\n.text\nactual: ret")[0]?.name).toBe("actual");
    expect(fileTargets(input, source).map((f) => f.name)).toEqual(["sum", "poly"]);
  });

  it("runs a file or the function at the caret without external tools", () => {
    expect(toolsFor("riscv")).toEqual([]);
    expect(produceFile(input, OPTIMISTIC)?.config?.params?.function).toBe("sum");
    const selected = produceContext({ ...input, line: 9 }, OPTIMISTIC)!;
    expect(selected.scope).toBe("element");
    expect(selected.config?.params?.function).toBe("poly");
    expect(selected.config?.args).toEqual([input.path, "--function", "poly", "--max-steps", "10000000"]);
    const main = produceFile({ ...input, content: "main: li a7, 10\necall" }, OPTIMISTIC)!;
    expect(main.config?.params?.function).toBe("");
    expect(main.config?.debugAdapter).toBeNull();
    const loop = `${source}\n.globl loop\nloop: li t0, 1\nagain: addi t0, t0, -1\nbnez t0, again\nret`;
    expect(
      produceContext({ ...input, content: loop, line: loop.split("\n").length - 1 }, OPTIMISTIC)?.config?.params
        ?.function,
    ).toBe("loop");
  });

  it("preserves paths containing spaces and metacharacters as argv", () => {
    const config = riscvConfig("/course/a b; c.asm", source, "/course", "poly");
    expect(config.args[0]).toBe("/course/a b; c.asm");
    expect(
      riscvType.commandLine!({ path: config.args[0]!, function: "poly", arguments: "-1, 0xffffffffffffffff" })!.args,
    ).toEqual([config.args[0], "--function", "poly", "--max-steps", "10000000", "--", "-1", "0xffffffffffffffff"]);
  });

  it("validates 64-bit integer arguments and counts", () => {
    expect(riscvArguments("-9223372036854775808, 0xffffffffffffffff 0b10 046", 4)).toHaveLength(4);
    for (const value of ["18446744073709551616", "-9223372036854775809", "1.5", "1; echo hi", "08"]) {
      expect(() => riscvArguments(value)).toThrow();
    }
    expect(() => riscvArguments("1", 2)).toThrow();
    expect(riscvArguments("", 0)).toEqual([]);
  });

  it("prompts with signature arity, remembers arguments, and cancels without launching", async () => {
    const config = riscvConfig(input.path, source, input.root, "sum");
    vi.mocked(promptValue).mockResolvedValueOnce("40 2");
    const prepared = await prepareRiscv(config);
    expect(prepared?.args.slice(-3)).toEqual(["--", "40", "2"]);
    const options = vi.mocked(promptValue).mock.calls[0]![1]!;
    expect(options.message).toBe("int64_t a, int64_t b");
    expect(options.validate!("1")).toBeTruthy();
    expect(options.validate!("1 2")).toBeNull();
    vi.mocked(promptValue).mockResolvedValueOnce(null);
    expect(await prepareRiscv(config)).toBeNull();
    expect(vi.mocked(promptValue).mock.calls[1]![1]?.initial).toBe("40 2");
  });

  it("runs saved arguments without prompting when disabled", async () => {
    const config = riscvConfig(input.path, source, input.root, "sum");
    config.params = { ...config.params, prompt: "false", arguments: "20 22" };
    expect((await prepareRiscv(config))?.args.slice(-3)).toEqual(["--", "20", "22"]);
    expect(promptValue).not.toHaveBeenCalled();
  });
});
