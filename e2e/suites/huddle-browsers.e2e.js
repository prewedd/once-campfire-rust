// Huddles across browsers: Ana in Chromium, Ben in BROWSER (webkit | firefox).
//   node huddle-browsers.e2e.js <dir> <base> <join code> <out> [browser]  (default: firefox, then webkit)
const path = require("path")
const [, , PW, BASE, CODE, OUT, ONLY] = process.argv
if (!ONLY) {
  const { execFileSync } = require("child_process")
  let failed = false
  for (const browser of [ "firefox", "webkit" ]) {
    try { process.stdout.write(execFileSync(process.execPath, [ __filename, PW, BASE, CODE, OUT, browser ], { encoding: "utf8" })) }
    catch (error) { process.stdout.write(error.stdout ?? ""); failed = true }
  }
  process.exit(failed ? 1 : 0)
}
const BROWSER = ONLY
const pw = require("playwright")
const { privateRoom } = require("../support")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = [], errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} [${BROWSER}] ${name}${detail ? " — " + detail : ""}`) }
let ben
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]); console.log("  ben status:", await ben?.evaluate(() => [document.querySelector("#huddle .huddle__status")?.textContent, document.querySelector("#huddle")?.hidden, document.querySelectorAll("#huddle .huddle__person").length]).catch(() => "?")) } }
async function join(b, name, options) {
  const context = await b.newContext({ viewport: { width: 1280, height: 860 }, ...options })
  const p = await context.newPage()
  p.on("pageerror", e => e.name !== "AbortError" && errors.push(`${name}: ${e.message}`))
  // WebKit reports a fetch cancelled by navigating away as an "access control" error.
  p.on("console", m => m.type() === "error" && !/access control checks|Failed to load resource|interactive-widget/.test(m.text()) && errors.push(`${name}: ${m.text()}`))
  await p.goto(`${BASE}/join/${CODE}`)
  await p.fill('input[name="user[name]"]', name)
  await p.fill('input[name="user[email_address]"]', `${name.toLowerCase()}${t}@x.test`)
  await p.fill('input[name="user[password]"]', "secret-password-" + t)
  await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
  if (!p.url().endsWith("/rooms/1")) await p.goto(`${BASE}/rooms/1`)
  await p.waitForLoadState("networkidle")
  return p
}

;(async () => {
  const chrome = await pw.chromium.launch({ args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--autoplay-policy=no-user-gesture-required"] })
  const other = BROWSER === "firefox"
    ? await pw.firefox.launch({ firefoxUserPrefs: { "media.navigator.streams.fake": true, "media.navigator.permission.disabled": true, "media.autoplay.default": 0 } })
    : await pw.webkit.launch()
  const ana = await join(chrome, `Ana${t}`, { permissions: ["microphone", "camera"] })
  ben = await join(other, `Ben${t}`, {})
  await privateRoom(BASE, ana, ben)

  await step("join", async () => {
    await ana.locator(".huddle-btn").click()
    await ana.locator("#huddle .huddle__timer").filter({ hasText: /\d:\d\d/ }).waitFor({ timeout: 15000 })
    await ben.waitForFunction(() => document.querySelector(".huddle-btn")?.innerText.includes("Join"), null, { timeout: 10000 })
    await ben.locator(".huddle-btn").click()
    await ben.waitForFunction(() => document.querySelectorAll("#huddle .huddle__person").length === 2, null, { timeout: 20000 })
    check("joins Chromium's huddle", true)
    if (BROWSER === "webkit") {
      // Playwright's WebKit can't grant the microphone: the huddle says so instead of failing.
      await ben.locator("#huddle .huddle__status", { hasText: "microphone is blocked" }).waitFor({ timeout: 10000 })
      check("says when the microphone is blocked", true)
    } else {
      await ben.waitForFunction(() => document.querySelector('[data-huddle-control="mic"]')?.getAttribute("aria-pressed") === "true", null, { timeout: 10000 })
      check("publishes its microphone", true)
    }
    await ben.waitForFunction(() => document.querySelectorAll("#huddle .huddle__audio audio").length >= 1, null, { timeout: 10000 })
    check("receives Chromium's audio", true)
    if (BROWSER !== "webkit") {
      await ana.waitForFunction(() => document.querySelectorAll("#huddle .huddle__audio audio").length >= 1, null, { timeout: 10000 })
      check("Chromium receives its audio", true)
      const flowing = await ana.waitForFunction(async () => {
        const audio = document.querySelector("#huddle .huddle__audio audio")
        return audio?.srcObject?.getAudioTracks()[0]?.readyState === "live"
      }, null, { timeout: 10000 }).then(() => true, () => false)
      check("…as a live track", flowing)
    }
  })

  await step("controls", async () => {
    await ben.locator('[data-huddle-control="mic"]').click()
    await ana.locator("#huddle .huddle__muted").first().waitFor({ timeout: 8000 })
    check("mute reaches Chromium", true)
    await ben.locator('[data-huddle-control="mic"]').click()
    await ben.locator('[data-huddle-control="expand"]').click()
    const slider = ben.locator("#huddle .huddle__tile input[type=range]")
    await slider.evaluate(input => { input.value = "150"; input.dispatchEvent(new Event("input", { bubbles: true })) })
    check("volume works", (await ben.locator("#huddle .huddle__volume output").innerText()) === "150%")
    await ben.locator('[data-huddle-open="settings"]').click()
    await ben.locator('#huddle select[data-device="audioinput"]').waitFor({ timeout: 5000 })
    check("settings open", true)
    await ben.screenshot({ path: `${OUT}/huddle-${BROWSER}.png` })
    await ben.keyboard.press("Escape")
    await ben.locator("#huddle [data-huddle-leave]").click()
    await ana.waitForFunction(() => document.querySelectorAll("#huddle .huddle__person").length === 1, null, { timeout: 10000 })
    check("leaves cleanly", true)
    await ana.locator("#huddle [data-huddle-leave]").click()
  })

  check("no JavaScript errors", errors.length === 0, errors.slice(0, 3).join(" | "))
  await chrome.close(); await other.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
