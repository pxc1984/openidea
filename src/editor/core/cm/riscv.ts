import { StreamLanguage, type StreamParser } from "@codemirror/language";
import { completeFromList } from "@codemirror/autocomplete";
import { RISCV_INSTRUCTIONS, RISCV_REGISTERS } from "@/platform/riscv";

const parser: StreamParser<{ block: boolean }> = {
  startState: () => ({ block: false }),
  token(stream, state) {
    if (state.block) {
      const close = stream.string.indexOf("*/", stream.pos);
      if (close < 0) stream.skipToEnd();
      else {
        stream.pos = close + 2;
        state.block = false;
      }
      return "comment";
    }
    if (stream.eatSpace()) return null;
    if (stream.match("/*")) {
      state.block = true;
      return "comment";
    }
    if (stream.match("#") || stream.match("//")) {
      stream.skipToEnd();
      return "comment";
    }
    if (stream.match(/^"(?:[^"\\]|\\.)*(?:"|$)|^'(?:[^'\\]|\\.)*'/)) return "string";
    if (stream.match(/^(?:[A-Za-z_.$][\w.$]*|\d+)(?=\s*:)/)) return "labelName";
    if (stream.match(/^\d+[fb]\b/)) return "labelName";
    if (stream.match(/^(?:0[xX][\da-fA-F]+|0[bB][01]+|\d+(?:\.\d*)?(?:[eE][+-]?\d+)?)/)) return "number";
    const word = stream.match(/^[A-Za-z_.$][\w.$]*/);
    if (word && typeof word === "object") {
      const name = word[0]!;
      if (name.startsWith(".")) return "meta";
      if (RISCV_REGISTERS.has(name)) return "variableName";
      if (RISCV_INSTRUCTIONS.has(name) || /^(?:lr|sc|amo\w+)\.[wd](?:\.aq|\.rl|\.aqrl)?$/.test(name)) return "keyword";
      return "labelName";
    }
    stream.next();
    return null;
  },
  languageData: {
    commentTokens: { line: "#", block: { open: "/*", close: "*/" } },
    autocomplete: completeFromList([
      ...Array.from(RISCV_INSTRUCTIONS, (label) => ({ label, type: "keyword" })),
      ...Array.from(RISCV_REGISTERS, (label) => ({ label, type: "variable" })),
      ...".text .data .bss .section .globl .equ .byte .half .word .dword .float .double .ascii .asciz .string .zero .skip .align"
        .split(" ")
        .map((label) => ({ label, type: "keyword" })),
    ]),
  },
};

export const riscvLanguage = StreamLanguage.define(parser);
