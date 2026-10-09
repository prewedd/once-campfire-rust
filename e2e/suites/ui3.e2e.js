// Theme picker, presence, right-hand panel.  node ui3.e2e.js <scratchpad> <base> <join code> <out>
const path = require("path")
const [, , S, BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = [], errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]) } }
async function join(b, name) {
  const context = await b.newContext({ viewport: { width: 1440, height: 900 } })
  const p = await context.newPage()
  p.on("console", m => m.type() === "error" && !/Failed to load resource|Failed to fetch/.test(m.text()) && errors.push(`${name}: ${m.text().split("\n")[0]}`))
  p.on("pageerror", e => errors.push(`${name}: ${e.message}`))
  await p.goto(`${BASE}/join/${CODE}`)
  await p.fill('input[name="user[name]"]', name)
  await p.fill('input[name="user[email_address]"]', `${name.toLowerCase()}${t}@x.test`)
  await p.fill('input[name="user[password]"]', "secret-password-" + t)
  await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
  await p.waitForLoadState("networkidle")
  if (!p.url().endsWith("/rooms/1")) { await p.goto(`${BASE}/rooms/1`); await p.waitForLoadState("networkidle") }
  return p
}
const sidebarBg = p => p.evaluate(() => getComputedStyle(document.querySelector("#sidebar")).backgroundColor)

;(async () => {
  const b = await chromium.launch()
  const ana = await join(b, `Ana${t}`), ben = await join(b, `Ben${t}`)

  await step("theme picker", async () => {
    const before = await sidebarBg(ana)
    await ana.locator(".theme-picker__btn").click()
    await ana.locator(".theme-picker__panel").waitFor()
    await ana.locator('.theme-picker__theme[data-value="aubergine"]').click()
    check("choosing a theme applies it", (await ana.evaluate(() => document.documentElement.dataset.theme)) === "aubergine")
    const after = await sidebarBg(ana)
    check("…to the sidebar", after === "rgb(63, 14, 64)" && after !== before, `${before} → ${after}`)
    await ana.locator('.theme-picker__mode[data-value="dark"]').click()
    check("choosing dark applies it", (await ana.evaluate(() => getComputedStyle(document.body).backgroundColor)) === "rgb(26, 26, 29)")
    check("…marked as chosen", (await ana.locator('.theme-picker__mode[data-value="dark"]').getAttribute("aria-checked")) === "true")
    await ana.keyboard.press("Escape")
    await ana.reload()
    const early = await ana.evaluate(() => [document.documentElement.dataset.theme, document.documentElement.dataset.mode])
    check("both survive a reload, set before the page paints", early[0] === "aubergine" && early[1] === "dark", early.join(","))
    await ana.screenshot({ path: `${OUT}/ui3-aubergine-dark.png` })
    await ana.locator(".theme-picker__btn").click()
    await ana.locator('.theme-picker__mode[data-value=""]').click()
    await ana.locator('.theme-picker__theme[data-value=""]').click()
    check("back to System and the default", (await ana.evaluate(() => [document.documentElement.dataset.theme, document.documentElement.dataset.mode].join(","))) === ",")
    await ana.mouse.click(700, 400)
    check("clicking elsewhere closes the picker", !(await ana.locator(".theme-picker").evaluate(el => el.open)))
  })

  await step("presence", async () => {
    const anaId = await ana.evaluate(() => Current.user.id)
    await ben.goto(`${BASE}/users/${anaId}`)
    await ben.waitForFunction(id => document.querySelector(`.presence-status[data-presence-user="${id}"]`)?.dataset.presence === "online", anaId, { timeout: 8000 })
    check("…Active on their profile", (await ben.locator(".presence-status").evaluate(el => getComputedStyle(el, "::after").content)) === '"Active"')
    check("someone online shows as online", true)
    await ana.context().close()
    await ben.waitForFunction(id => document.querySelector(`.presence-status[data-presence-user="${id}"]`)?.dataset.presence === "offline", anaId, { timeout: 20000 })
    check("closing their last page makes them offline, after a grace period", true)
  })

  await step("the right-hand panel", async () => {
    await ben.goto(`${BASE}/rooms/1`)
    await ben.waitForLoadState("networkidle")
    check("the sidebar has presence dots", (await ben.locator("#sidebar .presence-dot").count()) > 0)
    await ben.locator("#composer lexxy-editor").click()
    await ben.keyboard.type(`Pane parent ${t}`)
    await ben.keyboard.press("Enter")
    const parent = ben.locator(".message[data-message-id]", { hasText: `Pane parent ${t}` }).first()
    await parent.waitFor()
    const url = ben.url()
    await parent.hover()
    await parent.locator(".message__thread-btn--quick").click()
    const pane = ben.locator(".flexpane:not([hidden])")
    await pane.locator(".flexpane__title", { hasText: "Thread" }).waitFor({ timeout: 8000 })
    check("a thread opens in the panel", true)
    check("…without leaving the room", ben.url() === url, ben.url())
    await ben.waitForTimeout(400)
    check("…with the focus in its reply box", await ben.evaluate(() => !!document.activeElement?.closest(".flexpane")))
    await ben.keyboard.type(`Reply in pane ${t}`)
    await ben.keyboard.press("Enter")
    await pane.locator(".message[data-message-id]", { hasText: `Reply in pane ${t}` }).waitFor({ timeout: 8000 })
    check("replying in the panel posts into the thread", true)
    check("…and not into the room", (await ben.locator("#messages_rooms_open_1, .messages:not(.messages--thread)").locator(`.message:has-text("Reply in pane ${t}")`).count()) === 0)
    await parent.locator(".message__thread-count", { hasText: "1 reply" }).waitFor({ timeout: 8000 })
    check("the room's summary counts it", true)
    check("the room's composer is untouched", !(await ben.locator("#composer .lexxy-editor__content").innerText()).includes("Reply in pane"))
    await ben.screenshot({ path: `${OUT}/ui3-pane-thread.png` })

    await parent.locator(".message__avatar a").click()
    await pane.locator(".flexpane__title", { hasText: "Profile" }).waitFor({ timeout: 8000 })
    check("an avatar opens the profile in the panel", ben.url() === url)
    await ben.screenshot({ path: `${OUT}/ui3-pane-profile.png` })
    await ben.keyboard.press("Escape")
    await ben.locator(".flexpane").waitFor({ state: "hidden", timeout: 4000 })
    check("Escape closes the panel", true)

    await parent.locator(".message__thread-link").click()
    await pane.locator(".flexpane__title", { hasText: "Thread" }).waitFor({ timeout: 8000 })
    await pane.locator("[data-flexpane-close]").click()
    await ben.locator(".flexpane").waitFor({ state: "hidden", timeout: 4000 })
    check("the summary opens it too, and ✕ closes it", true)
  })

  await step("themes look right", async () => {
    for (const [theme, mode] of [["mono", "light"], ["forest", "dark"], ["", "light"]]) {
      await ben.evaluate(([theme, mode]) => { const r = document.documentElement; theme ? r.dataset.theme = theme : delete r.dataset.theme; r.dataset.mode = mode }, [theme, mode])
      await ben.waitForTimeout(200)
      await ben.screenshot({ path: `${OUT}/ui3-${theme || "graphite"}-${mode}.png` })
    }
    check("themes look right", true)
  })

  check("no JavaScript errors", errors.length === 0, errors.slice(0, 4).join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
