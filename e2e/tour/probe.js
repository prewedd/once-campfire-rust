// Log in, open a path (in the modal with MODAL=1), and print the box and the width-related styles
// of an element and its ancestors.  node tour/probe.js <base> <join code> <path> <selector>
const [, , BASE, CODE, PATH, SEL] = process.argv
const { chromium } = require("playwright")
;(async () => {
  const b = await chromium.launch()
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
  const t = Date.now().toString(36)
  await p.goto(`${BASE}/join/${CODE}`)
  await p.fill('input[name="user[name]"]', "Probe")
  await p.fill('input[name="user[email_address]"]', `probe${t}@x.test`)
  await p.fill('input[name="user[password]"]', "secret-password-" + t)
  await Promise.all([p.waitForURL(/rooms\/\d+/), p.locator('input[name="user[password]"]').press("Enter")])
  if (process.env.MODAL) {
    await p.evaluate(path => { const a = document.createElement("a"); a.href = path; document.body.append(a); a.click() }, PATH)
    await p.waitForTimeout(1500)
  } else await p.goto(BASE + PATH)
  console.log(await p.evaluate(sel => {
    const out = []
    for (let e = document.querySelector(sel); e && e !== document.body; e = e.parentElement) {
      const s = getComputedStyle(e)
      out.push(`${e.tagName.toLowerCase()}.${[...e.classList].join(".")} w=${e.getBoundingClientRect().width} inline-size=${s.width} max=${s.maxWidth} margin=${s.marginLeft}/${s.marginRight} display=${s.display} cols=${s.gridTemplateColumns} jself=${s.justifySelf} jitems=${s.justifyItems} col=${s.gridColumn}`)
    }
    const el = document.querySelector(sel), rules = []
    const walk = list => { for (const r of list) { if (r.cssRules) walk(r.cssRules); if (r.selectorText) { try { if (el.matches(r.selectorText) && /width|inline-size|justify|margin|grid|place/.test(r.style.cssText)) rules.push(r.selectorText + " { " + r.style.cssText + " }") } catch {} } } }
    for (const sheet of document.styleSheets) { try { walk(sheet.cssRules) } catch {} }
    return out.join("\n") + "\n--- rules\n" + rules.join("\n")
  }, SEL))
  await b.close()
})()
