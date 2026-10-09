// The Threads view.  node threadsview.e2e.js <scratchpad> <base> <join code> <out>
const path = require("path")
const [, , S, BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const { privateRoom } = require("../support")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = [], errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]) } }
async function join(b, name) {
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
  p.on("console", m => m.type() === "error" && !/Failed to load resource|Failed to fetch/.test(m.text()) && errors.push(`${name}: ${m.text().split("\n")[0]}`))
  p.on("pageerror", e => e.name !== "AbortError" && errors.push(`${name}: ${e.message}`))
  await p.goto(`${BASE}/join/${CODE}`)
  await p.fill('input[name="user[name]"]', name)
  await p.fill('input[name="user[email_address]"]', `${name.toLowerCase()}${t}@x.test`)
  await p.fill('input[name="user[password]"]', "secret-password-" + t)
  await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
  return p
}
const posted = (p, text) => p.locator(".message[data-message-id]", { hasText: text })
async function say(p, text) { await p.locator("#composer lexxy-editor").click(); await p.keyboard.type(text); await p.keyboard.press("Enter"); await posted(p, text).first().waitFor() }
async function replyInPane(p, parentText, text) {
  const parent = posted(p, parentText).first()
  await parent.hover(); await parent.locator(".message__thread-btn--quick").click()
  await p.locator(".flexpane:not([hidden]) #thread-composer").waitFor({ timeout: 8000 }); await p.waitForTimeout(400)
  await p.keyboard.type(text); await p.keyboard.press("Enter")
  await p.locator(".flexpane .message[data-message-id]", { hasText: text }).waitFor({ timeout: 8000 })
}

;(async () => {
  const b = await chromium.launch()
  const ana = await join(b, `Ana${t}`), ben = await join(b, `Ben${t}`)
  // The thread lives in a room of its own (a DM between them); Ana then watches from General.
  const other = await privateRoom(BASE, ana, ben)

  await step("threads view", async () => {
    await say(ana, `Elsewhere ${t}`)
    await replyInPane(ana, `Elsewhere ${t}`, `Ana starts ${t}`)
    await replyInPane(ben, `Elsewhere ${t}`, `Ben answers ${t}`)
    await ana.goto(`${BASE}/rooms/1`); await ana.waitForLoadState("networkidle")
    await ana.waitForFunction(() => document.querySelector("[data-threads-badge]:not([hidden])")?.textContent === "1", null, { timeout: 10000 })
    check("the sidebar's Threads shows one with a reply from someone else", true)
    await ana.locator("[data-open-threads]").click()
    const card = ana.locator(".flexpane:not([hidden]) .thread-card", { hasText: `Elsewhere ${t}` })
    await card.waitFor({ timeout: 8000 })
    check("…and lists it in the panel", (await card.innerText()).includes("2 replies"), await card.innerText())
    check("…marked new", await card.evaluate(el => el.classList.contains("thread-card--new")))
    await ana.screenshot({ path: `${OUT}/threads-view.png` })
    await card.click()
    await ana.locator(".flexpane:not([hidden]) .message[data-message-id]", { hasText: `Ben answers ${t}` }).waitFor({ timeout: 8000 })
    check("a thread from another room opens in the panel", ana.url().endsWith("/rooms/1"), ana.url())
    await ana.waitForTimeout(6000)
    await ben.keyboard.type(`Live from elsewhere ${t}`); await ben.keyboard.press("Enter")
    await ana.locator(".flexpane .message[data-message-id]", { hasText: `Live from elsewhere ${t}` }).waitFor({ timeout: 8000 })
    check("…and gets its replies live", true)
    await ana.locator(".flexpane lexxy-editor").click()
    await ana.keyboard.type(`Ana again ${t}`); await ana.keyboard.press("Enter")
    await ben.locator(".flexpane .message[data-message-id]", { hasText: `Ana again ${t}` }).waitFor({ timeout: 8000 })
    check("…and replying there works", true)
    await ana.screenshot({ path: `${OUT}/threads-pane.png` })
    await ana.waitForFunction(() => document.querySelector("[data-threads-badge]")?.hidden, null, { timeout: 10000 })
    check("once you've replied, the badge clears", true)
  })

  check("no JavaScript errors", errors.length === 0, errors.slice(0, 4).join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
