// Screen sharing: Ana shares, Ben sees it.  node screen.e2e.js <scratchpad> <base> <join code> <out>
const path = require("path")
const [, , S, BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const { privateRoom } = require("../support")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n").slice(0, 12).join(" / ")); for (const p of globalThis.pages ?? []) await p.screenshot({ path: `${OUT}/screen-fail-${Date.now()}.png` }).catch(() => {}) } }
async function join(b, name) {
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 }, permissions: ["microphone", "camera"] })).newPage()
  p.on("pageerror", e => e.name !== "AbortError" && console.log(`${name} pageerror: ${e.message}`))
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
  const b = await chromium.launch({ args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--auto-select-desktop-capture-source=Entire screen", "--auto-accept-this-tab-capture", "--autoplay-policy=no-user-gesture-required"] })
  const ana = await join(b, `Ana${t}`), ben = await join(b, `Ben${t}`)
  await privateRoom(BASE, ana, ben)
  globalThis.pages = [ ana, ben ]
  await step("share", async () => {
    await ana.locator(".huddle-btn").click()
    await ana.locator("#huddle .huddle__timer").filter({ hasText: /\d:\d\d/ }).waitFor({ timeout: 15000 })
    await ben.waitForFunction(() => document.querySelector(".huddle-btn")?.innerText.includes("Join"), null, { timeout: 10000 })
    await ben.locator(".huddle-btn").click()
    await ben.waitForFunction(() => document.querySelectorAll("#huddle .huddle__person").length === 2, null, { timeout: 15000 })
    await ana.locator('[data-huddle-control="screen"]').click()
    await ana.waitForFunction(() => document.querySelector('[data-huddle-control="screen"]')?.getAttribute("aria-pressed") === "true", null, { timeout: 10000 })
    check("sharing starts", true)
    await ben.locator("#huddle.huddle--expanded .huddle__tile--screen video").waitFor({ timeout: 10000 })
    check("the viewer's stage opens on the screen", true)
    await ben.waitForFunction(() => { const v = document.querySelector(".huddle__tile--screen video"); return v && v.videoWidth > 0 && !v.paused }, null, { timeout: 10000 })
    check("…and it plays", true)
    const box = await ben.locator(".huddle__tile--screen").boundingBox()
    const stage = await ben.locator(".huddle__grid").boundingBox()
    check("…filling the stage, not overflowing it", box.height <= stage.height + 1 && box.width > stage.width * 0.6, `${JSON.stringify(box)} in ${JSON.stringify(stage)}`)
    await ben.screenshot({ path: `${OUT}/screen-viewer.png` })
    await ana.screenshot({ path: `${OUT}/screen-sharer.png` })
    await ana.locator('[data-huddle-control="screen"]').click()
    await ben.locator(".huddle__tile--screen").waitFor({ state: "detached", timeout: 30000 })
    check("stopping removes it", true)
  })
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
