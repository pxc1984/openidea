import { describe, expect, it } from "vitest";
import { highlightTree, tagHighlighter, tags } from "@lezer/highlight";
import { findLanguageDescription, loadSupport } from "./languages";

it("loads RISC-V for .asm, .s and .S files", async () => {
  for (const path of ["math.asm", "math.s", "math.S"]) {
    expect(findLanguageDescription("RISC-V Assembly", path)?.name).toBe("RISC-V Assembly");
    expect(await loadSupport(findLanguageDescription("RISC-V Assembly", path)!)).toBeTruthy();
  }
});

describe("RISC-V highlighting", () => {
  it("distinguishes comments, strings, labels, directives, registers and bit instructions", async () => {
    const support = await loadSupport(findLanguageDescription("RISC-V Assembly", "math.asm")!);
    const source =
      '.text\n/* block\n comment */\nsum: sh3add.uw a0, a1, x31 # end\n1: bnez a0, 1b\n.asciz "# text //"\n';
    const tokens: Array<[string, string]> = [];
    highlightTree(
      support!.language.parser.parse(source),
      tagHighlighter([
        { tag: tags.comment, class: "comment" },
        { tag: tags.keyword, class: "instruction" },
        { tag: tags.variableName, class: "register" },
        { tag: tags.labelName, class: "label" },
        { tag: tags.string, class: "string" },
        { tag: tags.meta, class: "directive" },
      ]),
      (from, to, style) => tokens.push([source.slice(from, to), style]),
    );
    expect(tokens).toContainEqual(["sh3add.uw", "instruction"]);
    expect(tokens).toContainEqual(["a0", "register"]);
    expect(tokens).toContainEqual(["x31", "register"]);
    expect(tokens).toContainEqual(["sum", "label"]);
    expect(tokens).toContainEqual(["1b", "label"]);
    expect(tokens).toContainEqual(['"# text //"', "string"]);
    expect(tokens).toContainEqual([".text", "directive"]);
    expect(
      tokens
        .filter(([, style]) => style === "comment")
        .map(([text]) => text)
        .join("\n"),
    ).toContain("comment */");
  });
});
