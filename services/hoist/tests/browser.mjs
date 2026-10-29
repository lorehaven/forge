// Drives headless Chromium over the DevTools protocol: the page is rendered, the browser takes it
// over, a click works.
//   quench test --browser          (or: node tests/browser.mjs http://127.0.0.1:3000)
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const base = (process.argv[2] ?? "http://127.0.0.1:3000").replace(/\/$/, "");
const port = 9400 + Math.floor(Math.random() * 400);
const chrome = spawn(process.env.CHROMIUM ?? "chromium", [
  "--headless=new", "--no-sandbox", "--disable-gpu", `--remote-debugging-port=${port}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), "quench-chrome-"))}`, "about:blank",
], { stdio: "ignore" });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let failures = 0;
const check = (name, ok) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}`);
  if (!ok) failures++;
};

let ws;
for (let i = 0; i < 100 && !ws; i++) {
  try {
    const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const page = pages.find((p) => p.type === "page");
    if (page) ws = new WebSocket(page.webSocketDebuggerUrl);
  } catch { /* chromium is still starting */ }
  await sleep(100);
}
if (!ws) { console.log("FAIL  could not reach chromium"); process.exit(1); }
if (ws.readyState !== 1) await new Promise((r) => (ws.onopen = r));

let next = 0;
const waiting = new Map();
const problems = [];
ws.onmessage = (m) => {
  const d = JSON.parse(m.data);
  if (d.id && waiting.has(d.id)) waiting.get(d.id)(d.result);
  if (d.method === "Runtime.exceptionThrown") problems.push(JSON.stringify(d.params).slice(0, 200));
};
const send = (method, params = {}) => new Promise((resolve) => {
  const id = ++next;
  waiting.set(id, resolve);
  ws.send(JSON.stringify({ id, method, params }));
});
const run = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result.value;
const until = async (expression, ms = 10000) => {
  for (let t = 0; t < ms; t += 100) {
    if (await run(expression)) return true;
    await sleep(100);
  }
  return false;
};

await send("Runtime.enable");
await send("Page.enable");

const html = await (await fetch(base + "/")).text();
check("the server renders the page", html.includes("<h1>hoist</h1>") && html.includes("id=\"count\""));

await send("Page.navigate", { url: base + "/" });
check("the page shows the counter", await until("!!document.querySelector('#up')"));
await sleep(500);
await run("document.querySelector('#up').click()");
check("a click is handled by the app", await until("document.querySelector('#count').textContent === '1'"));
await run("document.querySelector('#down').click()");
check("and so is the next one", await until("document.querySelector('#count').textContent === '0'"));

check("the page raised no exceptions", problems.length === 0);
if (problems.length) console.log(problems.join("\n"));

chrome.kill();
process.exit(failures === 0 ? 0 : 1);