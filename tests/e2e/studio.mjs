// End-to-end test of `acu studio`. It copies the Go adapter's invented
// fixture into a work folder, starts the studio there, and checks the API's
// guards. Then it drives the page in Chromium as a person would: pick the
// folder, write the proposed atlas.yaml, extract, record two tour steps,
// reorder them by keyboard, save, suggest regions, mark one as checked,
// change the description, run the adapters again, and see a save over a
// newer file refused. It checks atlas.yaml after each save and fails on any
// console error. It also writes screenshots of the setup screen and the
// editor in both themes.
//
//   node tests/e2e/studio.mjs <acu program> <work folder>
//
// The fixture needs the Go adapter (acu-adapter-go on the PATH, or the
// source in this repository). The exit code is 1 when a check fails.

import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const acu = resolve(process.argv[2] ?? join(repo, "target/release/acu"));
const work = resolve(process.argv[3] ?? join(repo, "out/studio"));
const port = 4391;
const base = `http://127.0.0.1:${port}/`;
const failures = [];
const check = (ok, what, detail = "") => {
  if (!ok) failures.push(`${what}${detail ? "\n    " + String(detail).replace(/\n/g, "\n    ") : ""}`);
};

rmSync(work, { recursive: true, force: true });
const code = join(work, "code");
const project = join(code, "lending");
mkdirSync(code, { recursive: true });
cpSync(join(repo, "adapters/go/testdata/fixture"), project, { recursive: true });
const configPath = join(project, "atlas.yaml");
const yaml = () => readFileSync(configPath, "utf8");

/** Sends one raw HTTP request, so the test can set any Host or Origin. */
function raw(method, path, headers = {}, body = "") {
  return new Promise((ok, fail) => {
    const req = request({ host: "127.0.0.1", port, method, path, headers }, res => {
      let data = "";
      res.on("data", d => (data += d));
      res.on("end", () => ok({ status: res.statusCode, headers: res.headers, body: data }));
    });
    req.on("error", fail);
    req.end(body);
  });
}

/** The headers of a changing call from the studio's own page. */
const own = {
  Host: `127.0.0.1:${port}`,
  Origin: `http://127.0.0.1:${port}`,
  "Content-Type": "application/json",
  "X-Acu-Studio": "1",
};

const child = spawn(acu, ["studio", "--addr", `127.0.0.1:${port}`, "--root", code, "--workspace", join(work, "ws")], {
  stdio: ["ignore", "ignore", "pipe"],
});
let serverLog = "";
child.stderr.on("data", d => (serverLog += d));
const stop = () => child.kill();
process.on("exit", stop);

async function main() {
  for (let i = 0; i < 100 && !serverLog.includes("serving"); i++) await new Promise(r => setTimeout(r, 100));
  check(serverLog.includes("serving the studio"), "acu studio starts", serverLog);

  // The guards.
  const st = await raw("GET", "/api/state", { Host: `127.0.0.1:${port}` });
  check(st.status === 200 && JSON.parse(st.body).setup === true, "GET /api/state answers", st.body);
  check(st.headers["x-content-type-options"] === "nosniff", "answers send nosniff");
  const evilHost = await raw("GET", "/api/state", { Host: "evil.example" });
  check(evilHost.status === 403, "a foreign Host is refused", evilHost.status);
  const evilOrigin = await raw("POST", "/api/folders", { ...own, Origin: "http://evil.example" }, "{}");
  check(evilOrigin.status === 403, "a foreign Origin is refused", evilOrigin.status);
  const noGuard = await raw("POST", "/api/folders", { ...own, "X-Acu-Studio": "" }, "{}");
  check(noGuard.status === 403, "a call without the guard header is refused", noGuard.status);
  const crossSite = await raw("POST", "/api/folders", { ...own, "Sec-Fetch-Site": "cross-site" }, "{}");
  check(crossSite.status === 403, "a cross-site call is refused", crossSite.status);
  const textBody = await raw("POST", "/api/folders", { ...own, "Content-Type": "text/plain" }, "{}");
  check(textBody.status === 415, "a call without a JSON body is refused", textBody.status);
  const preflight = await raw("OPTIONS", "/api/folders", { Host: own.Host, Origin: "http://evil.example" });
  check(
    preflight.status === 403 && !preflight.headers["access-control-allow-origin"],
    "a preflight gets no CORS grant",
  );
  const outside = await raw("POST", "/api/folders", own, JSON.stringify({ path: "/" }));
  check(outside.status === 403, "the picker refuses a folder outside its root", outside.body);
  const badClone = await raw("POST", "/api/clone", own, JSON.stringify({ url: "ext::sh -c touch% /tmp/x" }));
  check(badClone.status === 400, "a clone of a non-HTTPS address is refused", badClone.body);
  const noTrust = await raw("POST", "/api/open", own, JSON.stringify({ path: project, config: "version: 1\n" }));
  check(noTrust.status === 428, "opening without trust is refused", noTrust.body);

  const browser = await chromium.launch();
  const errors = [];
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 }, colorScheme: "dark" });
  page.on("pageerror", e => errors.push("page error: " + e.message));
  // The stale save below answers 409 on purpose; the browser logs that one.
  page.on(
    "console",
    m => m.type() === "error" && !/status of 409/.test(m.text()) && errors.push("console: " + m.text()),
  );

  // Set up the project.
  await page.goto(base);
  await page.waitForSelector("#setup:not([hidden]) .su-folder");
  await page.screenshot({ path: join(work, "shot-setup.png") });
  await page.click(".su-folder:has-text('lending')");
  await page.click("[data-act='choose']");
  await page.waitForSelector("#suText");
  const proposal = await page.inputValue("#suText");
  check(/- adapter: go/.test(proposal), "the proposal names the Go adapter", proposal);
  check(/- plugin: go-chi/.test(proposal), "the proposal finds the chi plugin", proposal);
  check(await page.isDisabled("[data-act='go']"), "extraction waits for trust");
  await page.check("#suTrust");
  await page.screenshot({ path: join(work, "shot-setup-proposal.png") });
  await Promise.all([page.waitForURL(/.*/, { waitUntil: "load" }), page.click("[data-act='go']")]);
  await page.waitForSelector("#editBtn:not([hidden])", { timeout: 180000 });
  check(yaml().startsWith("# atlas.yaml"), "the studio wrote atlas.yaml", yaml());

  // Record two steps, then put the second first by keyboard.
  await page.click("#editBtn");
  await page.waitForSelector("#editor:not([hidden]) [data-act='record']");
  const unit = await page.evaluate(() => {
    const ids = [...window.atlas.model.units.keys()].sort();
    const id = ids.find(i => i.includes("store")) ?? ids[0];
    window.atlas.go({ v: "unit", id });
    return id;
  });
  await page.click("[data-act='record']");
  const hint = await page.getAttribute("input[data-field='title']", "placeholder");
  check(
    !!hint && (await page.inputValue("input[data-field='title']")) === "",
    "a recorded step leaves its title to the person",
    hint,
  );
  await page.fill("input[data-field='title']", "The store");
  await page.fill("textarea[data-field='text']", "The store keeps the `Loan` records.");
  await page.evaluate(() => window.atlas.go({ v: "overview" }));
  await page.click("[data-act='toggle']");
  await page.click("[data-act='record']");
  await page.fill("input[data-field='title']:last-of-type", "Start here");
  const texts = page.locator("textarea[data-field='text']");
  await texts.last().fill("The overview shows each region.");
  const heads = page.locator(".ed-head");
  check((await heads.count()) === 2, "two steps show", await heads.count());
  await heads.nth(1).focus();
  await page.keyboard.press("Alt+ArrowUp");
  const first = await page.locator(".ed-head").first().innerText();
  check(first.includes("Start here"), "Alt+ArrowUp moves the step up", first);
  await page.screenshot({ path: join(work, "shot-editor-tour.png") });
  await Promise.all([page.waitForEvent("load"), page.click("[data-act='save']")]);
  await page.waitForSelector("#editor:not([hidden]) .ed-head");
  let y = yaml();
  check(
    /tour:\n {2}- title: "?Start here"?\n {4}open: \{ view: overview \}/.test(y),
    "the tour starts with the overview step",
    y,
  );
  check(y.includes(`id: "${unit}"`) || y.includes(`id: ${unit}`), "the second step opens the unit by ID", y);
  check(!/drafted/.test(y.split("tour:")[1] ?? ""), "steps that a person wrote carry no drafted mark", y);
  const tourBtn = await page.isVisible("#tourBtn");
  check(tourBtn, "the Tour button shows after the save");

  // Suggest regions, then check one and rename it.
  await page.click("[data-tab='regions']");
  await page.click("[data-act='suggest']");
  await page.waitForSelector(".ed-head");
  await Promise.all([page.waitForEvent("load"), page.click("[data-act='save']")]);
  await page.waitForSelector("#editor:not([hidden]) [data-tab='regions'][aria-selected='true']");
  y = yaml();
  check(/regions:\n/.test(y) && y.includes('drafted: { by: "acu studio"'), "suggested regions carry a drafted mark", y);
  await page.locator("[data-act='toggle']").first().click();
  await page.waitForSelector(".ed-form [data-field='paths']");
  await page.waitForFunction(() => /match \d+ unit/.test(document.querySelector("[data-match]")?.textContent ?? ""));
  await page.click("[data-act='checked-region']");
  await page.fill(".ed-form input[data-field='name']", "Commands");
  await page.screenshot({ path: join(work, "shot-editor-regions.png") });
  await Promise.all([page.waitForEvent("load"), page.click("[data-act='save']")]);
  await page.waitForSelector("#editor:not([hidden]) .ed-head");
  y = yaml();
  const firstRegion = y.split("regions:\n")[1].split("\n  - ")[0];
  check(
    firstRegion.includes("Commands") && !firstRegion.includes("drafted"),
    "the checked region lost its mark",
    firstRegion,
  );

  // The description.
  await page.click("[data-tab='project']");
  await page.fill("[data-field='pdesc']", "A lending service. It lends books to members.");
  await Promise.all([page.waitForEvent("load"), page.click("[data-act='save']")]);
  await page.waitForSelector("#editor:not([hidden]) [data-field='pdesc']");
  y = yaml();
  check(y.includes("A lending service."), "the description saved", y);

  // A save over a newer file fails and says why.
  writeFileSync(configPath, yaml() + "# A person edited this file meanwhile.\n");
  await page.fill("[data-field='pname']", "Lending, renamed");
  await page.click("[data-act='save']");
  await page.waitForSelector(".ed-msg.bad");
  const msg = await page.innerText(".ed-msg.bad");
  check(msg.includes("changed on disk"), "a stale save is refused", msg);
  check(yaml().endsWith("# A person edited this file meanwhile.\n"), "the stale save wrote nothing");
  await page.click("[data-act='discard']");

  // Run the adapters again.
  await page.click("[data-tab='extract']");
  await Promise.all([page.waitForEvent("load", { timeout: 180000 }), page.click("[data-act='extract']")]);
  await page.waitForSelector("#editor:not([hidden]) [data-tab='extract'][aria-selected='true']");

  // Screenshots in the light theme.
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await page.click("[data-tab='tour']");
  await page.locator("[data-act='toggle']").first().click();
  await page.screenshot({ path: join(work, "shot-editor-tour-light.png") });
  await page.goto(base);
  await page.click("#projectsBtn");
  await page.waitForSelector("#setup:not([hidden]) .su-folder");
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await page.screenshot({ path: join(work, "shot-setup-light.png") });

  check(errors.length === 0, "no console errors", errors.join("\n"));
  await browser.close();
}

main()
  .catch(e => failures.push("the test stopped: " + (e.stack ?? e)))
  .finally(() => {
    stop();
    if (failures.length) {
      console.error(`studio.mjs: ${failures.length} checks failed:\n- ` + failures.join("\n- "));
      console.error("server log:\n" + serverLog.slice(-4000));
      process.exit(1);
    }
    console.error("studio.mjs: every check passed");
  });
