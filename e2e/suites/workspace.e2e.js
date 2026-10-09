// End-to-end check of the workspace redesign: emoji, reactions, sidebar, grouping.
//   node workspace.e2e.js <scratchpad> <base url> <join code>
const path = require("path")
const [, , S, BASE, CODE] = process.argv
const { chromium } = require("playwright")
const { privateRoom } = require("../support")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = []
const errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]) } }

async function join(b, name) {
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
  p.on("console", m => m.type() === "error" && !/Failed to load resource/.test(m.text()) && errors.push(`${name}: ${m.text().split("\n")[0]}`))
  p.on("pageerror", e => errors.push(`${name}: ${e.message}`))
  await p.goto(`${BASE}/join/${CODE}`)
  await p.fill('input[name="user[name]"]', name)
  await p.fill('input[name="user[email_address]"]', `${name.toLowerCase()}${t}@x.test`)
  await p.fill('input[name="user[password]"]', "secret-password-" + t)
  await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
  // Let the first page settle before moving on, so its frames aren't cut off mid-load.
  await p.waitForLoadState("networkidle")
  if (!p.url().endsWith("/rooms/1")) {
    await p.goto(`${BASE}/rooms/1`)
    await p.waitForLoadState("networkidle")
  }
  await p.locator("#composer lexxy-editor").waitFor()
  await p.waitForTimeout(500)
  return p
}
const editorText = p => p.locator("#composer .lexxy-editor__content").innerText()
async function clearComposer(p) { await p.locator("#composer lexxy-editor").click(); await p.keyboard.press("Control+A"); await p.keyboard.press("Backspace") }

;(async () => {
  const b = await chromium.launch()
  const ana = await join(b, "Ana"), ben = await join(b, "Ben")
  // Its own room: grouping needs consecutive messages, which other suites' posts would split.
  const ROOM = await privateRoom(BASE, ana, ben)

  await step("the composer says where you're writing", async () => {
    const placeholder = await ana.locator("#composer .lexxy-editor__content").getAttribute("placeholder")
    check("the composer says where you're writing", placeholder === "Message Ben", placeholder)
  })

  await step("emoji picker", async () => {
    await ana.locator(".composer__emoji-btn").click()
    await ana.locator(".emoji-picker:not([hidden]) .emoji-picker__emoji").first().waitFor({ timeout: 8000 })
    check("the picker opens from the composer", true)
    check("…with the search focused", await ana.evaluate(() => document.activeElement?.classList.contains("emoji-picker__search")))
    await ana.keyboard.type("rocket")
    await ana.keyboard.press("Enter")
    check("search and Enter insert the emoji", (await editorText(ana)).includes("🚀"), await editorText(ana))
    check("…and close the picker", await ana.locator(".emoji-picker").isHidden())
    await ana.locator(".composer__emoji-btn").click()
    await ana.locator(".emoji-picker__section-name", { hasText: "Recently used" }).waitFor({ timeout: 4000 })
    check("what you picked comes first next time", (await ana.locator(".emoji-picker__section").first().locator(".emoji-picker__emoji").first().getAttribute("data-emoji")) === "🚀")
    await ana.keyboard.press("Escape")
    check("Escape closes it", await ana.locator(".emoji-picker").waitFor({ state: "hidden", timeout: 3000 }).then(() => true, () => false))
    await clearComposer(ana)
  })

  await step(":shortcode suggestions", async () => {
    await ana.locator("#composer lexxy-editor").click()
    await ana.keyboard.type("party :tad")
    await ana.locator(".lexxy-prompt-menu__item", { hasText: ":tada:" }).first().waitFor({ timeout: 6000 })
    await ana.keyboard.press("Enter")
    check(":tad + Enter becomes 🎉", (await editorText(ana)).includes("party 🎉"), await editorText(ana))
    await clearComposer(ana)
  })

  let message
  await step("reactions", async () => {
    await ana.locator("#composer lexxy-editor").click()
    await ana.keyboard.type(`React here ${t}`)
    await ana.keyboard.press("Enter")
    message = (p) => p.locator(".message[data-message-id]", { hasText: `React here ${t}` })
    await message(ana).waitFor()
    await message(ben).waitFor({ timeout: 10000 })

    await message(ana).hover()
    await message(ana).locator('.message__react-btn[data-emoji-content-param="👍"]').click()
    const pill = (p) => message(p).locator(".boost--lead", { hasText: "👍" })
    await pill(ana).waitFor({ timeout: 8000 })
    check("a quick reaction makes a pill", (await pill(ana).locator(".boost__count").textContent()) === "1")
    check("…marked as yours", await pill(ana).evaluate(el => el.classList.contains("boost--mine")))
    await pill(ben).waitFor({ timeout: 8000 })
    check("…which others see live, not as theirs", !(await pill(ben).evaluate(el => el.classList.contains("boost--mine"))))

    await pill(ben).click()
    await ana.waitForFunction(() => [...document.querySelectorAll(".boost--lead .boost__count")].some(c => c.textContent === "2"), null, { timeout: 8000 })
    check("clicking someone's pill adds your reaction: 2", true)
    await ana.waitForFunction(() => [...document.querySelectorAll(".boost--lead")].some(p => /Ana/.test(p.title) && /Ben/.test(p.title)), null, { timeout: 5000 })
    check("the pill names who reacted", true)

    await pill(ben).click()
    await ana.waitForFunction(() => [...document.querySelectorAll(".boost--lead .boost__count")].some(c => c.textContent === "1"), null, { timeout: 8000 })
    check("clicking your own pill takes it back: 1", true)

    await message(ben).hover()
    await message(ben).locator(".message__react-btn--add").click()
    await ben.locator(".emoji-picker__search").fill("fire")
    await ben.keyboard.press("Enter")
    await message(ana).locator(".boost--lead", { hasText: "🔥" }).waitFor({ timeout: 8000 })
    check("the reaction picker reacts with what you pick", true)
  })

  await step("sidebar marks the room you're in", async () => {
    check("sidebar marks the room you're in", await ana.locator(`#sidebar [data-room-id="${ROOM}"].room--active`).count() === 1)
  })

  await step("follow-ups are grouped", async () => {
    await ana.locator("#composer lexxy-editor").click()
    await ana.keyboard.type(`Follow-up ${t}`)
    await ana.keyboard.press("Enter")
    const followUp = ana.locator(".message[data-message-id]", { hasText: `Follow-up ${t}` })
    await followUp.waitFor()
    await ana.waitForTimeout(400)
    check("a follow-up is grouped under its sender", await followUp.evaluate(el => el.classList.contains("message--threaded")))
    check("…without its avatar", !(await followUp.locator(".message__avatar").isVisible()))
    await followUp.hover()
    await ana.waitForTimeout(200)
    const opacity = await followUp.locator(".message__permalink").evaluate(el => getComputedStyle(el).opacity)
    check("…with its time on hover", opacity === "1", opacity)
  })

  await step("one click to a thread", async () => {
    await message(ana).hover()
    await message(ana).locator(".message__thread-btn--quick").click()
    await Promise.all([ana.waitForURL(/threads/), ana.locator(".flexpane__expand").click()])
    const placeholder = await ana.locator("#composer .lexxy-editor__content").getAttribute("placeholder")
    check("one click to a thread, which asks for a reply", placeholder === "Reply…", placeholder)
    await ana.locator(".composer__emoji-btn").click()
    await ana.locator(".emoji-picker__search").fill("heart")
    await ana.keyboard.press("Enter")
    check("…with emoji in the reply too", (await editorText(ana)).includes("❤️"), await editorText(ana))
  })

  check("no JavaScript errors", errors.length === 0, errors.slice(0, 4).join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
