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
    const ctx = await b.newContext({ viewport: { width: w, height: h }, colorScheme: scheme })
    const p = await ctx.newPage()
    p.on("pageerror", e => console.log(`${tag} pageerror: ${e.message}`))
    // Stable accounts: tour/run makes them administrators, so every admin screen shows.
    const email = `tour-${tag}@x.test`, password = "tour-password-1234"
    await p.goto(`${BASE}/session/new`)
    await p.fill('input[name="email_address"]', email)
    await p.fill('input[name="password"]', password)
    await p.evaluate(() => document.querySelector('input[name="password"]').form.requestSubmit())
    await p.waitForURL(url => /rooms\/\d+|email_address=/.test(url.href), { timeout: 10000 }).catch(() => {})
    if (!/rooms\/\d+/.test(p.url())) {
      await p.goto(`${BASE}/join/${CODE}`)
      await p.fill('input[name="user[name]"]', `Tour ${tag}`)
      await p.fill('input[name="user[email_address]"]', email)
      await p.fill('input[name="user[password]"]', password)
      await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
    }
    for (const [name, path] of shots) {
      await p.goto(BASE + path).catch(e => console.log(name, e.message))
      await p.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {})
      await p.screenshot({ path: `${OUT}/tour-${tag}-${name}.png` })
    }
    for (const [name, path] of [["m-settings", "/account/edit"], ["m-profile", "/users/me/profile"], ["m-edit", "/rooms/opens/1/edit"], ["m-newroom", "/rooms/opens/new"], ["m-private", "/rooms/closeds/new"]]) {
      await p.goto(BASE + "/rooms/1"); await p.waitForLoadState("networkidle", { timeout: 5000 }).catch(() => {})
      await p.evaluate(path => { const a = document.createElement("a"); a.href = path; document.body.append(a); a.click() }, path)
      await p.waitForTimeout(1500)
      await p.screenshot({ path: `${OUT}/tour-${tag}-${name}.png` })
    }
    await ctx.close()
  }
  await b.close()
})()
