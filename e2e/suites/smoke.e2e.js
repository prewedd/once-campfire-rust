// Smoke: one person, every surface once, no JavaScript errors. Runs before every deploy (~20 s).
//   node smoke.e2e.js <dir> <base> <join code> <out>
const [, , , BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = [], errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]) } }

;(async () => {
  const b = await chromium.launch()
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
  p.on("console", m => m.type() === "error" && !/Failed to load resource|Failed to fetch/.test(m.text()) && errors.push(m.text().split("\n")[0]))
  p.on("pageerror", e => e.name !== "AbortError" && errors.push(e.message))

  await step("signs up and lands in a room", async () => {
    await p.goto(`${BASE}/join/${CODE}`)
    await p.fill('input[name="user[name]"]', `Smoke${t}`)
    await p.fill('input[name="user[email_address]"]', `smoke${t}@x.test`)
    await p.fill('input[name="user[password]"]', "secret-password-" + t)
    await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
    await p.locator("#sidebar #shared_rooms > [data-room-id]").first().waitFor({ timeout: 10000 })
    check("signs up and lands in a room, with the sidebar", true)
  })
  await step("posts", async () => {
    await p.locator("#composer lexxy-editor").click()
    await p.keyboard.type(`smoke ${t}`)
    await p.keyboard.press("Enter")
    await p.locator(".message[data-message-id]", { hasText: `smoke ${t}` }).waitFor({ timeout: 8000 })
    check("posts a message", true)
  })
  await step("thread panel", async () => {
    const message = p.locator(".message[data-message-id]", { hasText: `smoke ${t}` }).first()
    await message.hover()
    await message.locator(".message__thread-btn--quick").click()
    await p.locator(".flexpane:not([hidden]) #thread-composer").waitFor({ timeout: 8000 })
    check("opens a thread in the panel", true)
  })
  await step("threads view", async () => {
    await p.locator("[data-open-threads]").click()
    await p.locator(".flexpane:not([hidden]) .thread-list").waitFor({ timeout: 8000 })
    check("opens the Threads view", true)
    await p.locator("[data-flexpane-close]").click()
  })
  await step("settings modal", async () => {
    await p.locator('#sidebar a[href$="/users/me/profile"]').first().click()
    await p.locator("dialog.ws-modal[open] .ws-modal__body form").first().waitFor({ timeout: 8000 })
    check("opens settings in a modal", true)
    await p.keyboard.press("Escape")
  })
  await step("schedule menu", async () => {
    await p.locator("#composer .composer__schedule-btn").click()
    await p.locator(".schedule-menu").waitFor({ timeout: 4000 })
    check("opens the schedule menu", true)
    await p.keyboard.press("Escape")
  })
  await step("emoji picker", async () => {
    await p.locator("#composer .composer__emoji-btn").click()
    await p.locator(".emoji-picker").waitFor({ state: "visible", timeout: 8000 })
    check("opens the emoji picker", true)
    await p.keyboard.press("Escape")
  })
  await step("huddle button", async () => {
    check("rooms have a huddle button", await p.locator(".huddle-btn").first().isVisible())
  })
  await p.screenshot({ path: `${OUT}/smoke.png` })
  check("no JavaScript errors", errors.length === 0, errors.slice(0, 4).join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
