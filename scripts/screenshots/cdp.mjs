// A tiny Chrome DevTools Protocol client for Plugcam's WebView2, started with
// WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222 ..." (see README.md here).
//   node cdp.mjs eval "<js expression>"     prints the value; promises are awaited
//   node cdp.mjs shot out.png               light theme
//   node cdp.mjs shotdark out.png           dark theme
//   node cdp.mjs console [seconds]          reloads and prints console messages and exceptions
// SIZE=980x668 DPR=2 renders the shot at that CSS size and pixel ratio, whatever the window size.
// CDP_PORT picks another port than 9222.
import { writeFileSync } from "node:fs";

const [action, arg] = process.argv.slice(2);
const port = process.env.CDP_PORT || 9222;
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) throw new Error("no page target: " + JSON.stringify(targets));

const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
const events = [];
ws.addEventListener("message", (m) => {
  const msg = JSON.parse(m.data);
  if (msg.id && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  } else if (msg.method) events.push(msg);
});
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const i = ++id;
    pending.set(i, resolve);
    ws.send(JSON.stringify({ id: i, method, params }));
  });

if (action === "eval") {
  const r = await send("Runtime.evaluate", { expression: arg, awaitPromise: true, returnByValue: true });
  console.log(JSON.stringify(r.result?.result?.value ?? r.result, null, 2));
} else if (action === "shot" || action === "shotdark") {
  if (process.env.SIZE) {
    const [width, height] = process.env.SIZE.split("x").map(Number);
    await send("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: Number(process.env.DPR || 1),
      mobile: false,
    });
    await new Promise((r) => setTimeout(r, 300));
  }
  const scheme = action === "shotdark" ? "dark" : "light";
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
  const r = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(arg, Buffer.from(r.result.data, "base64"));
  if (process.env.SIZE) await send("Emulation.clearDeviceMetricsOverride");
  await send("Emulation.setEmulatedMedia", { features: [] });
  console.log("saved", arg);
} else if (action === "console") {
  await send("Runtime.enable");
  await send("Log.enable");
  await send("Page.enable");
  await send("Page.reload");
  await new Promise((r) => setTimeout(r, (Number(arg) || 4) * 1000));
  for (const e of events) {
    if (e.method === "Runtime.consoleAPICalled")
      console.log(e.params.type, e.params.args.map((a) => a.value ?? a.description).join(" "));
    if (e.method === "Runtime.exceptionThrown")
      console.log("EXCEPTION", e.params.exceptionDetails.exception?.description ?? e.params.exceptionDetails.text);
    if (e.method === "Log.entryAdded") console.log("log", e.params.entry.level, e.params.entry.text, e.params.entry.url ?? "");
  }
} else {
  console.error("usage: node cdp.mjs eval <js> | shot <out.png> | shotdark <out.png> | console [seconds]");
  process.exit(2);
}
ws.close();
