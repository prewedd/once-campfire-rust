// Every page in parity/screens.yml (with this database's ids), checked for Campfire's own UI that
// the redesign replaces: globe "translate" buttons, the lanyard, a card with Campfire's layered
// shadow, and visible icon-only black buttons.  node tour/audit.js <base> <out>
const [, , BASE, OUT] = process.argv
const { chromium } = require("playwright")
const BOT = ""
const PAGES = [
  "/", "/rooms/1", "/rooms/1/involvement", "/rooms/opens/new", "/rooms/opens/1/edit", "/rooms/closeds/new", "/rooms/directs/new",
  "/searches", "/searches?q=hello", "/searches?q=zebra", "/users/me/profile", "/users/me/push_subscriptions", "/users/999999999", "/users/3",
  "/account/edit", "/account/bots", "/account/bots/new", ...(BOT ? [`/account/bots/${BOT}/edit`] : []), "/account/custom_styles/edit",
  "/saved", "/mentions", "/channels", "/rooms/1/pins", "/404.html", "/500.html", "/422.html", "/502.html",
]
const ANON = [ "/session/new", "/join/not-a-code", "/session/transfers/expired-or-bogus" ]
async function audit(p, path) {
  await p.goto(BASE + path, { waitUntil: "commit" }).catch(() => {})
  await p.waitForTimeout(2500)
  return p.evaluate(() => {
    const visible = el => { const r = el.getBoundingClientRect(), s = getComputedStyle(el); return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none" && !el.closest("[hidden], dialog:not([open]), details:not([open]) > :not(summary)") }
    const found = []
    document.querySelectorAll('img[src*="/globe-"]').forEach(el => visible(el) && found.push("globe translate button"))
    document.querySelectorAll('img[src*="lanyard"]').forEach(el => visible(el) && found.push("lanyard"))
    document.querySelectorAll(".panel").forEach(el => visible(el) && getComputedStyle(el).boxShadow !== "none" && found.push("Campfire card shadow"))
    document.querySelectorAll(".btn--reversed").forEach(el => {
      if (!visible(el) || el.closest("#nav, .composer, .huddle, .ws-modal__header")) return
      const text = [ ...el.childNodes ].filter(n => n.nodeType === 3 || (n.nodeType === 1 && !n.classList.contains("for-screen-reader") && n.tagName !== "IMG")).map(n => n.textContent).join("").trim()
      const label = getComputedStyle(el, "::after").content
      if (!text && (!label || label === "none" || label === "normal")) found.push(`icon-only black button (${el.querySelector(".for-screen-reader")?.textContent.trim() || el.getAttribute("aria-label") || "?"})`)
    })
    return [ ...new Set(found) ]
  })
}
;(async () => {
  const b = await chromium.launch()
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 }, storageState: `${OUT}/.tour-desk.json` })).newPage()
  let problems = 0
  for (const path of PAGES) { const found = await audit(p, path); problems += found.length; console.log(`${found.length ? "FLAG" : "ok  "} ${path}${found.length ? " — " + found.join("; ") : ""}`) }
  const anon = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
  for (const path of ANON) { const found = await audit(anon, path); problems += found.length; console.log(`${found.length ? "FLAG" : "ok  "} ${path} (signed out)${found.length ? " — " + found.join("; ") : ""}`) }
  await b.close()
  process.exit(problems ? 1 : 0)
})()
