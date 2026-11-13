import { call, commands, type RunConfig } from "@/platform/ipc";
import { editorsFor } from "@/editor/api";
import { tx } from "@/platform/i18n";
import { isRiscvPath, riscvArguments, riscvFunctions } from "@/platform/riscv";
import type { RunConfigTypeContribution } from "@/workbench/contributions";
import { promptValue } from "./dialogs";

const argumentError = (value: string, count?: number): string | null => {
  try {
    riscvArguments(value, count);
    return null;
  } catch {
    return tx`Enter the required number of 64-bit integer arguments (decimal, hexadecimal, binary, or octal).`;
  }
};

function commandLine(p: Record<string, string>) {
  const args = [p.path ?? ""];
  if (p.function?.trim()) args.push("--function", p.function.trim());
  args.push("--max-steps", p.maxSteps || "10000000");
  if (p.arguments?.trim()) args.push("--", ...p.arguments.trim().split(/[\s,]+/));
  return { command: "openidea-riscv", args };
}

export const riscvType: RunConfigTypeContribution = {
  id: "riscv",
  label: "RISC-V Assembly",
  icon: "file-binary",
  description: "Run an RV64 assembly program or an individual function",
  defaults: { maxSteps: "10000000", prompt: "true" },
  fields: [
    { key: "path", label: "Assembly file", required: true, mono: true, browse: "file" },
    { key: "function", label: "Function label", mono: true, placeholder: "Automatic entry point" },
    { key: "arguments", label: "Integer arguments", mono: true, validate: (value) => argumentError(value) },
    {
      key: "prompt",
      label: "Ask for function arguments before running",
      control: "checkbox",
      visible: (p) => !!p.function,
    },
    {
      key: "maxSteps",
      label: "Instruction limit",
      mono: true,
      validate: (value) =>
        /^[1-9]\d*$/.test(value) && Number.isSafeInteger(Number(value)) ? null : tx`Enter a positive integer`,
    },
  ],
  commandLine,
  paramsFrom: (config) => (config.kind === "riscv" ? { ...config.params } : null),
};

export function riscvConfig(path: string, content: string, cwd: string | null, target?: string): RunConfig {
  const functions = riscvFunctions(content);
  const automatic = functions.find((f) => f.name === "_start") ?? functions.find((f) => f.name === "main");
  const functionName = target ?? (!automatic ? functions[0]?.name : undefined);
  const selected = functions.find((f) => f.name === functionName);
  const name = functionName ? `${functionName}()` : path.slice(path.lastIndexOf("/") + 1);
  const params = {
    path,
    function: functionName ?? "",
    arguments: "",
    prompt: "true",
    maxSteps: "10000000",
    ...(selected?.parameters != null
      ? { parameters: JSON.stringify(selected.parameters), parametersFunction: functionName! }
      : {}),
  };
  return {
    id: `context:file:${cwd && path.startsWith(cwd + "/") ? path.slice(cwd.length + 1) : path}#riscv:${functionName ?? "program"}`,
    name,
    kind: "riscv",
    ...commandLine(params),
    cwd,
    env: {},
    detected: true,
    isTest: false,
    debugAdapter: null,
    params,
  };
}

/** All launch paths (gutter, Run, rerun) use the same argument prompt. */
export async function prepareRiscv(config: RunConfig): Promise<RunConfig | null> {
  if (config.kind !== "riscv" || !config.params) return config;
  const p = config.params;
  let args = p.arguments ?? "";
  if (p.function && p.prompt !== "false") {
    const key = `riscv.arguments:${p.path}:${p.function}`;
    let remembered: string | null = null;
    try {
      remembered = localStorage.getItem(key);
    } catch {
      /* Storage is optional. */
    }
    let parameters: string[] | null = null;
    try {
      const parsed: unknown = JSON.parse(p.parametersFunction === p.function ? (p.parameters ?? "null") : "null");
      if (Array.isArray(parsed) && parsed.every((s) => typeof s === "string")) parameters = parsed;
    } catch {
      /* No signature. */
    }
    try {
      const editor = editorsFor(p.path ?? "")[0];
      const source = editor ? editor.view.state.doc.toString() : await call(commands.fsReadText(p.path ?? ""));
      parameters = riscvFunctions(source).find((f) => f.name === p.function)?.parameters ?? null;
    } catch {
      /* Keep detected metadata if the source is temporarily unavailable. */
    }
    const name = p.function;
    const value = await promptValue(tx`Arguments for '${name}'`, {
      initial: args || remembered || "",
      message: parameters?.length
        ? parameters.join(", ")
        : tx`Integer arguments are passed in a0–a7, then on the stack.`,
      validate: (value) => argumentError(value, parameters?.length),
    });
    if (value === null) return null;
    args = value;
    try {
      localStorage.setItem(key, value);
    } catch {
      /* Storage is optional. */
    }
  }
  return { ...config, ...commandLine({ ...p, arguments: args }) };
}

export { isRiscvPath };
