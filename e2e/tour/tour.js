// Screenshot tour of the main screens, for design review.  node tour/tour.js <base> <join code> <out>
const [, , BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const shots = [
  ["room", "/rooms/1"],
  ["search", "/searches?q=hello"], ["search-empty", "/searches"],
  ["settings", "/account/edit"],
  ["profile", "/users/me/profile"],
  ["new-room", "/rooms/opens/new"],
  ["new-dm", "/rooms/directs/new"],
]
;(async () => {
  const b = await chromium.launch()
  for (const [w, h, tag, scheme] of [[1440, 900, "desk", "light"], [1440, 900, "dark", "dark"], [390, 844, "phone", "light"]]) {
    // Signed in once, then the saved session is reused: signing in is rate-limited.
    const state = `${OUT}/.tour-${tag}.json`
    const ctx = await b.newContext({ viewport: { width: w, height: h }, colorScheme: scheme, storageState: require("fs").existsSync(state) ? state : undefined })
    const p = await ctx.newPage()
    p.on("pageerror", e => console.log(`${tag} pageerror: ${e.message}`))
    await p.goto(`${BASE}/rooms/1`, { waitUntil: "commit" })
    if (!/rooms\/\d+/.test(p.url())) {
      const email = `tour-${tag}@x.test`, password = "tour-password-1234"
      await p.goto(`${BASE}/session/new`)
      await p.evaluate(async ([ email, password ]) => {
        const form = document.querySelector('form[action$="/session"]'), data = new FormData(form)
        data.set("email_address", email)
        data.set("password", password)
        await fetch(form.action, { method: "POST", body: data })
      }, [ email, password ])
      await p.goto(`${BASE}/rooms/1`, { waitUntil: "commit" })
      if (!/rooms\/\d+/.test(p.url())) {
        await p.goto(`${BASE}/join/${CODE}`)
        await p.fill('input[name="user[name]"]', `Tour ${tag}`)
        await p.fill('input[name="user[email_address]"]', email)
        await p.fill('input[name="user[password]"]', password)
        await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
      }
      await ctx.storageState({ path: state })
    }
    for (const [name, path] of shots) {
      await p.goto(BASE + path).catch(e => console.log(name, e.message))
      await p.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {})
      await p.screenshot({ path: `${OUT}/tour-${tag}-${name}.png` })
    }
    await p.goto(BASE + "/rooms/1"); await p.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {})
    if (tag === "phone") {
      await p.locator("#sidebar .sidebar__toggle").click().catch(e => console.log("toggle", e.message))
      await p.waitForTimeout(800)
      await p.screenshot({ path: `${OUT}/tour-${tag}-sidebar.png` })
      await p.locator("#sidebar .sidebar__toggle").click().catch(() => {})
    }
    await p.locator(".message:not(.message--me) .message__avatar a").last().click().catch(e => console.log("avatar", e.message))
    await p.waitForTimeout(1500)
    await p.screenshot({ path: `${OUT}/tour-${tag}-person.png` })
    if (tag !== "phone") {
      await p.goto(BASE + "/rooms/1"); await p.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {})
      const row = p.locator(".message:not(.message--me)").last()
      await row.hover().catch(() => {})
      await p.screenshot({ path: `${OUT}/tour-${tag}-hover.png` })
      await row.locator(".message__options-btn, [aria-label*='More'], .message__toolbar summary, .message__toolbar button:last-child").last().click().catch(e => console.log("menu", e.message))
      await p.waitForTimeout(500)
      await p.screenshot({ path: `${OUT}/tour-${tag}-menu.png` })
      await p.keyboard.press("Escape")
      await p.locator(".message__thread a, .message__thread-summary a, a[href*='/threads/']").first().click().catch(e => console.log("thread", e.message))
      await p.waitForTimeout(1500)
      await p.screenshot({ path: `${OUT}/tour-${tag}-thread.png` })
    }
    for (const [name, path] of [["m-settings", "/account/edit"], ["m-profile", "/users/me/profile"], ["m-edit", "/rooms/opens/1/edit"], ["m-newroom", "/rooms/opens/new"], ["m-private", "/rooms/closeds/new"], ["m-bots", "/account/bots"], ["m-styles", "/account/custom_styles/edit"], ["m-newbot", "/account/bots/new"], ["m-browse", "/channels"], ["m-pins", "/rooms/1/pins"]]) {
      await p.goto(BASE + "/rooms/1"); await p.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {})
      await p.evaluate(path => { const a = document.createElement("a"); a.href = path; document.body.append(a); a.click() }, path)
      await p.waitForTimeout(1500)
      await p.screenshot({ path: `${OUT}/tour-${tag}-${name}.png` })
    }
    await ctx.close()
  }
  await b.close()
})()
