// The main screens on emulated phones (WebKit iPhone, Chromium Pixel).  node tour/devices.js <base> <out>
const [, , BASE, OUT] = process.argv
const pw = require("playwright")
;(async () => {
  for (const [engine, device, tag] of [["webkit", "iPhone 13", "iphone"], ["chromium", "Pixel 5", "pixel"]]) {
    const b = await pw[engine].launch()
    const ctx = await b.newContext({ ...pw.devices[device] })
    const p = await ctx.newPage()
    p.on("pageerror", e => console.log(`${tag} pageerror: ${e.message}`))
    await p.goto(`${BASE}/session/new`)
    await p.evaluate(async () => {
      const form = document.querySelector('form[action$="/session"]'), data = new FormData(form)
      data.set("email_address", "tour-phone@x.test"); data.set("password", "tour-password-1234")
      await fetch(form.action, { method: "POST", body: data })
    })
    const shot = async name => { await p.waitForTimeout(6000); await p.screenshot({ path: `${OUT}/device-${tag}-${name}.png` }) }
    await p.goto(`${BASE}/rooms/1`, { waitUntil: "commit" }); await p.waitForFunction(() => document.querySelector(".message--formatted"), null, { timeout: 30000 }).catch(() => console.log(tag, "messages never formatted")); await shot("room")
    await p.locator("#sidebar .sidebar__toggle").tap().catch(e => console.log(tag, "toggle", e.message)); await shot("sidebar")
    await p.goto(`${BASE}/rooms/1`, { waitUntil: "commit" }); await p.waitForTimeout(800)
    await p.evaluate(() => { const a = document.createElement("a"); a.href = "/users/me/profile"; document.body.append(a); a.click() }); await shot("profile")
    await p.goto(`${BASE}/searches?q=hello`, { waitUntil: "commit" }); await shot("search")
    await b.close()
  }
})()
