// Scheduled messages.  node schedule.e2e.js <scratchpad> <base> <join code> <out>
const path = require("path")
const [, , S, BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
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
  if (!p.url().endsWith("/rooms/1")) await p.goto(`${BASE}/rooms/1`)
  await p.waitForLoadState("networkidle")
  return p
}
const localValue = date => { const p = n => String(n).padStart(2, "0"); return `${date.getFullYear()}-${p(date.getMonth() + 1)}-${p(date.getDate())}T${p(date.getHours())}:${p(date.getMinutes())}` }
const posted = (p, text) => p.locator(".message[data-message-id]", { hasText: text })

;(async () => {
  const b = await chromium.launch()
  const ana = await join(b, `Ana${t}`), ben = await join(b, `Ben${t}`)

  async function schedule(text, pick) {
    await ana.locator("#composer lexxy-editor").click()
    await ana.keyboard.type(text)
    await ana.locator("#composer .composer__schedule-btn").click()
    await ana.locator(".schedule-menu").waitFor({ timeout: 3000 })
    await pick(ana.locator(".schedule-menu"))
    await ana.locator(".schedule-toast").waitFor({ timeout: 5000 })
  }

  await step("schedule", async () => {
    await ana.locator("#composer lexxy-editor").click()
    await ana.locator("#composer .composer__schedule-btn").click()
    await ana.locator(".schedule-menu [data-send-at]").first().click()
    check("it asks for a message first", (await ana.locator(".schedule-menu__error").innerText()).includes("Write your message"))
    await ana.keyboard.press("Escape")

    await schedule(`Later one ${t}`, menu => menu.locator(".schedule-menu__item", { hasText: "Tomorrow" }).click())
    check("scheduling says when", (await ana.locator(".schedule-toast").innerText()).startsWith("Scheduled for Tomorrow at 9:00"), await ana.locator(".schedule-toast").innerText())
    check("…clears the composer", (await ana.locator("#composer .lexxy-editor__content").innerText()).trim() === "")
    await ana.waitForTimeout(800)
    check("…and doesn't post it yet", (await posted(ben, `Later one ${t}`).count()) === 0)
    await ana.locator(".scheduled-bar").waitFor({ timeout: 5000 })
    check("a bar counts it", (await ana.locator(".scheduled-bar").innerText()).includes("1 scheduled message"))
    await ana.screenshot({ path: `${OUT}/schedule-bar.png` })
  })

  await step("the list", async () => {
    await ana.locator(".scheduled-bar").click()
    const item = ana.locator(".flexpane:not([hidden]) .scheduled-item", { hasText: `Later one ${t}` })
    await item.waitFor({ timeout: 5000 })
    check("…which lists it in the panel", (await item.locator(".scheduled-item__when").innerText()).includes("Tomorrow at 9:00"))
    await ana.screenshot({ path: `${OUT}/schedule-panel.png` })
    await item.locator("[data-action-send]").click()
    await posted(ben, `Later one ${t}`).waitFor({ timeout: 8000 })
    check("Send now posts it, for everyone", true)
    await ana.waitForFunction(() => !document.querySelector(".scheduled-bar"), null, { timeout: 5000 })
    check("…and it leaves the list", (await ana.locator(".scheduled-item").count()) === 0)
  })

  await step("delete", async () => {
    await schedule(`Never mind ${t}`, menu => menu.locator(".schedule-menu__item", { hasText: "Next Monday" }).or(menu.locator(".schedule-menu__item", { hasText: "Tomorrow" })).first().click())
    await ana.locator(".scheduled-bar").click()
    const item = ana.locator(".scheduled-item", { hasText: `Never mind ${t}` })
    await item.waitFor({ timeout: 5000 })
    ana.once("dialog", dialog => dialog.accept())
    await item.locator("[data-action-delete]").click()
    await item.waitFor({ state: "detached", timeout: 5000 })
    check("deleting one removes it", true)
    await ana.locator("[data-flexpane-close]").click()
  })

  await step("on time", async () => {
    const at = new Date(Date.now() + 100 * 1000)
    await schedule(`Right on time ${t}`, async menu => {
      await menu.locator("input[type=datetime-local]").fill(localValue(at))
      await menu.locator("[data-custom]").click()
    })
    const due = new Date(localValue(at)).getTime()
    check("a custom time can be picked", true)
    await ana.reload(); await ana.waitForLoadState("networkidle")
    check("…and survives a reload", (await ana.locator(".scheduled-bar").innerText()).includes("1 scheduled"))
    await posted(ben, `Right on time ${t}`).waitFor({ timeout: 120000 })
    const late = (Date.now() - due) / 1000
    check("it posts by itself when it's due, for everyone", late > -1 && late < 15, `${late.toFixed(1)} s after its minute`)
    check("…exactly once", (await posted(ben, `Right on time ${t}`).count()) === 1)
  })

  check("no JavaScript errors", errors.length === 0, errors.slice(0, 4).join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
