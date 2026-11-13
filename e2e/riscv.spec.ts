import { expect, ipcCalls, stubDesktop, test } from "./fixtures";

test("RISC-V function arguments validate before launch and reach the Run console", async ({ workbench: page }) => {
  await stubDesktop(page);
  await page.evaluate(async () => {
    const module = "/src/features/run/riscv.ts";
    const { riscvConfig } = await import(/* @vite-ignore */ module);
    const config = riscvConfig(
      "/dev/acme-api/sum.asm",
      ".globl sum\n# int64_t sum(int64_t a, int64_t b)\nsum: add a0, a0, a1\nret",
      "/dev/acme-api",
      "sum",
    );
    const h = window.__ipc.handlers;
    h.run_configs = () => [config];
    h.run_apply_settings = (args) => args.configs;
    h.run_start = (args) => {
      setTimeout(() => {
        window.__ipc.send(args.onOutput, { kind: "stdout", text: "Execution Done: sum(40, 2) = 42\n" });
        setTimeout(() => window.__ipc.emit({ type: "runExited", run: 77, code: 0 }), 50);
      }, 50);
      return { id: 77, configId: config.id, name: config.name, pid: 4242, running: true, pty: false };
    };
    const store = "/src/features/run/store.ts";
    const { loadConfigs, selectConfig } = await import(/* @vite-ignore */ store);
    await loadConfigs();
    selectConfig(config.id);
  });
  await page.keyboard.press("Control+r");
  const dialog = page.getByRole("dialog", { name: "Arguments for 'sum'" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByText("int64_t a, int64_t b")).toBeVisible();
  await expect(dialog.getByRole("button", { name: "OK", exact: true })).toBeDisabled();
  await dialog.getByRole("textbox", { name: "Value" }).fill("40");
  await expect(dialog.getByRole("button", { name: "OK", exact: true })).toBeDisabled();
  expect(await ipcCalls(page, "run_start")).toEqual([]);
  await dialog.getByRole("textbox", { name: "Value" }).fill("40 2");
  await dialog.getByRole("button", { name: "OK", exact: true }).click();
  await expect(
    page.getByRole("region", { name: "Run", exact: true }).getByText("Execution Done: sum(40, 2) = 42"),
  ).toBeVisible();
  expect((await ipcCalls(page, "run_start"))[0]).toMatchObject({
    config: {
      command: "openidea-riscv",
      args: ["/dev/acme-api/sum.asm", "--function", "sum", "--max-steps", "10000000", "--", "40", "2"],
    },
  });
  await page.keyboard.press("Control+r");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("textbox", { name: "Value" })).toHaveValue("40 2");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(dialog).toBeHidden();
  expect(await ipcCalls(page, "run_start")).toHaveLength(1);
});
