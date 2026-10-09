// Huddles: two people, fake media.  node huddle.e2e.js <scratchpad> <base> <join code> <out>
const path = require("path")
const [, , S, BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const { privateRoom } = require("../support")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = [], errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]) } }
async function join(b, name) {
  const context = await b.newContext({ viewport: { width: 1440, height: 900 }, permissions: ["microphone", "camera", "clipboard-read", "clipboard-write"] })
  const p = await context.newPage()
  p.on("console", m => m.type() === "error" && !/Failed to load resource|Failed to fetch|livekit|could not/i.test(m.text()) && errors.push(`${name}: ${m.text().slice(0, 600)} @ ${JSON.stringify(m.location())} after ${results.length}`))
  // AbortError: Turbo cancelling a fetch on navigation; happens without huddles too (e2e/abort_debug.js)
  p.on("pageerror", e => e.name !== "AbortError" && errors.push(`${name}: ${e.message} ${String(e.stack).slice(0, 800)} after ${results.length} ${e.name}`))
  await p.goto(`${BASE}/join/${CODE}`)
  await p.fill('input[name="user[name]"]', name)
  await p.fill('input[name="user[email_address]"]', `${name.toLowerCase()}${t}@x.test`)
  await p.fill('input[name="user[password]"]', "secret-password-" + t)
  await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
  await p.waitForLoadState("networkidle")
  if (!p.url().endsWith("/rooms/1")) { await p.goto(`${BASE}/rooms/1`); await p.waitForLoadState("networkidle") }
  return p
}
const people = p => p.locator("#huddle .huddle__person").count()

;(async () => {
  const b = await chromium.launch({ args: ["--use-fake-device-for-media-stream", "--use-fake-ui-for-media-stream", "--autoplay-policy=no-user-gesture-required"] })
  const ana = await join(b, `Ana${t}`), ben = await join(b, `Ben${t}`)
  const ROOM = await privateRoom(BASE, ana, ben)
  const anaName = `Ana${t}`

  await step("start", async () => {
    const button = ana.locator(".huddle-btn")
    check("rooms have a huddle button", await button.isVisible())
    await button.click()
    await ana.locator("#huddle:not([hidden]) .huddle__timer").filter({ hasText: /\d:\d\d/ }).waitFor({ timeout: 15000 })
    check("starting one connects, with a timer", true)
    await ana.waitForFunction(() => document.querySelector('[data-huddle-control="mic"]')?.getAttribute("aria-pressed") === "true", null, { timeout: 8000 })
    check("…with the microphone on", true)
    check("…and the button says Leave", (await button.innerText()).includes("Leave"))
    await ana.locator(".message", { hasText: "started a huddle" }).last().waitFor({ timeout: 8000 })
    check("the room hears it started", true)
  })

  await step("join", async () => {
    const button = ben.locator(".huddle-btn")
    await ben.waitForFunction(() => document.querySelector(".huddle-btn")?.innerText.includes("Join"), null, { timeout: 10000 })
    check("others see Join", true)
    check("…with who's in it", (await button.locator(".huddle-btn__people img").count()) === 1)
    check("…and 🎧 by the room in the sidebar", await ben.locator(`#sidebar [data-room-id="${ROOM}"].room--huddle`).count() === 1)
    const invite = ben.locator(".huddle-invite")
    check("…and a ring: who started it, where", (await invite.innerText()).includes(`${anaName} started a huddle`), await invite.innerText())
    await invite.locator('[data-invite="join"]').click()
    check("…with Join right there", await invite.count() === 0)
    await ben.waitForFunction(() => document.querySelectorAll("#huddle .huddle__person").length === 2, null, { timeout: 15000 })
    await ana.waitForFunction(() => document.querySelectorAll("#huddle .huddle__person").length === 2, null, { timeout: 15000 })
    check("joining puts both in the huddle", true)
    await ben.waitForFunction(() => document.querySelectorAll("#huddle .huddle__audio audio").length >= 1, null, { timeout: 10000 })
    await ana.waitForFunction(() => document.querySelectorAll("#huddle .huddle__audio audio").length >= 1, null, { timeout: 10000 })
    check("…hearing each other", true)
    await ben.waitForFunction(() => document.querySelector("#huddle .huddle__person--speaking"), null, { timeout: 15000 }).then(() => check("who's talking lights up", true), () => check("who's talking lights up", false))
    await ben.screenshot({ path: `${OUT}/huddle-bar.png` })
  })

  await step("controls", async () => {
    await ana.locator('[data-huddle-control="mic"]').click()
    await ben.locator("#huddle .huddle__muted").first().waitFor({ timeout: 8000 })
    check("muting shows to everyone", true)
    await ana.keyboard.press("Control+Shift+Space")
    await ben.locator("#huddle .huddle__muted").first().waitFor({ state: "detached", timeout: 8000 })
    check("Ctrl+Shift+Space unmutes", true)

    await ana.locator('[data-huddle-open="reactions"]').click()
    await ana.locator('[data-huddle-react="🎉"]').click()
    await ben.locator("#huddle .huddle__reaction", { hasText: "🎉" }).waitFor({ timeout: 8000 })
    check("reactions reach everyone", true)
    await ana.locator('#huddle [data-huddle-control="hand"]').click()
    await ben.locator("#huddle .huddle__hand").first().waitFor({ timeout: 8000 })
    check("raising a hand shows", true)
    await ana.keyboard.press("Escape")

    await ana.locator('[data-huddle-control="camera"]').click()
    await ana.locator("#huddle.huddle--expanded").waitFor({ timeout: 8000 })
    check("turning on the camera opens the stage", true)
    await ben.locator('[data-huddle-control="expand"]').click()
    await ben.locator(`#huddle .huddle__tile video`).first().waitFor({ timeout: 10000 })
    check("…and others see the video", true)
    check("the stage has a tile each", (await ben.locator("#huddle .huddle__tile").count()) === 2)
  })

  await step("volume", async () => {
    const slider = ben.locator("#huddle .huddle__tile input[type=range]")
    check("each other person has a volume", (await slider.count()) === 1)
    await slider.evaluate(input => { input.value = "160"; input.dispatchEvent(new Event("input", { bubbles: true })) })
    check("…up to 200%, shown", (await ben.locator("#huddle .huddle__volume output").innerText()) === "160%")
    const stored = await ben.evaluate(() => JSON.parse(localStorage.getItem("huddle-volumes")))
    check("…and remembered", Object.values(stored).includes(160), JSON.stringify(stored))
    await ben.screenshot({ path: `${OUT}/huddle-stage.png` })
  })

  await step("settings", async () => {
    await ben.locator('[data-huddle-open="settings"]').click()
    await ben.locator('#huddle select[data-device="audioinput"]').waitFor({ timeout: 5000 })
    check("settings list the microphones", (await ben.locator('#huddle select[data-device="audioinput"] option').count()) >= 1)
    const box = ben.locator('#huddle input[data-processing="noiseSuppression"]')
    check("…with noise suppression on", await box.isChecked())
    await ben.waitForFunction(() => parseFloat(document.querySelector("#huddle .huddle__meter span")?.style.width) > 0, null, { timeout: 5000 })
    check("…and a live microphone level", true)
    await ben.screenshot({ path: `${OUT}/huddle-settings.png` })
    await box.uncheck({ timeout: 5000 }).catch(e => console.log(e.message.split("\n").slice(0, 12).join("\n")))
    await ben.waitForTimeout(500)
    check("…which can be turned off, and stays off", (await ben.evaluate(() => JSON.parse(localStorage.getItem("huddle-prefs")).noiseSuppression)) === false)
    await ben.keyboard.press("Escape")
  })

  await step("huddle link", async () => {
    await ana.locator("#huddle [data-huddle-copy]").click()
    const copied = await ana.evaluate(() => navigator.clipboard.readText())
    check("the huddle's link can be copied", copied.endsWith(`/rooms/${ROOM}#huddle`), copied)
  })

  await step("keeps going across pages", async () => {
    await ben.locator('[data-huddle-control="expand"]').click()
    const anaId = await ana.evaluate(() => Current.user.id)
    await ben.evaluate(id => Turbo.visit(`/users/${id}`), anaId)
    await ben.waitForURL(/users\/\d+/)
    await ben.waitForTimeout(1000)
    check("the huddle survives moving between pages", (await people(ben)) === 2)
    await ben.evaluate(room => Turbo.visit(`/rooms/${room}`), ROOM)
    await ben.waitForURL(new RegExp(`rooms/${ROOM}$`))
    await ben.waitForTimeout(500)
    check("…and the room's button knows you're in", (await ben.locator(".huddle-btn").innerText()).includes("Leave"))
  })

  await step("leave", async () => {
    await ben.locator("#huddle [data-huddle-leave]").click()
    check("leaving closes the huddle bar", await ben.locator("#huddle").isHidden())
    await ana.waitForFunction(() => document.querySelectorAll("#huddle .huddle__person").length === 1, null, { timeout: 10000 })
    check("…and the others see you go", true)
    await ben.waitForFunction(() => document.querySelector(".huddle-btn")?.innerText.includes("Join"), null, { timeout: 10000 })
    check("…and you can join again", true)
    await ana.locator("#huddle [data-huddle-leave]").click()
    await ben.waitForFunction(() => document.querySelector(".huddle-btn")?.innerText.includes("Huddle"), null, { timeout: 15000 })
    check("when everyone's gone, it's over", await ben.locator('#sidebar .room--huddle').count() === 0)
    await ben.goto(`${BASE}/rooms/${ROOM}#huddle`)
    await ben.locator(".huddle-invite", { hasText: "Start a huddle here?" }).waitFor({ timeout: 8000 })
    check("a huddle link offers to start or join it", true)
  })

  check("no JavaScript errors", errors.length === 0, errors.slice(0, 4).join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
