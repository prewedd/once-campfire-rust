// Slack-style channel forms and settings: create a private channel, pick its members, notification
// settings from the profile.  node channels.e2e.js <scratchpad> <base> <join code> <out>
const [, , S, BASE, CODE, OUT] = process.argv
const { chromium } = require("playwright")
const t = Date.now().toString(36) + Math.random().toString(36).slice(2, 5)
const results = [], errors = []
const check = (name, ok, detail = "") => { results.push(!!ok); console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? " — " + detail : ""}`) }
let shot = 0
async function step(name, fn) { try { await fn() } catch (e) { check(name, false, String(e.message).split("\n")[0]); for (const p of globalThis.pages ?? []) await p.screenshot({ path: `${OUT}/channels-fail-${++shot}.png` }).catch(() => {}) } }
async function join(b, name) {
  const p = await (await b.newContext({ viewport: { width: 1440, height: 900 } })).newPage()
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
const openModal = (p, path) => p.evaluate(path => { const a = document.createElement("a"); a.href = path; document.body.append(a); a.click(); a.remove() }, path)
;(async () => {
  const b = await chromium.launch()
  const ana = await join(b, `Ana${t}`), ben = await join(b, `Ben${t}`)
  globalThis.pages = [ana]
  const channel = `plans-${t}`

  await step("create a private channel", async () => {
    await openModal(ana, "/rooms/opens/new")
    const modal = ana.locator("dialog.ws-modal[open]")
    await modal.locator('input[name="room[name]"]').waitFor({ timeout: 8000 })
    check("the form says what it makes", await modal.getByRole("button", { name: "Create channel" }).isVisible())
    await modal.locator('input[name="room[name]"]').fill(channel)
    await modal.locator(".ws-choice", { hasText: "Private" }).click()
    await modal.locator('input[name="user_ids[]"]').first().waitFor({ timeout: 8000 })
    check("switching to private keeps the name", (await modal.locator('input[name="room[name]"]').inputValue()) === channel)
    await modal.locator("#search").fill(`ben${t}`.toLowerCase())
    const ben_row = modal.locator(".ws-row", { hasText: `Ben${t}` })
    await ben_row.locator('input[type="checkbox"]').check()
    check("people are picked with a checkbox", await ben_row.locator('input[type="checkbox"]').isChecked())
    await ana.screenshot({ path: `${OUT}/channels-private.png` })
    await Promise.all([ana.waitForURL(url => !url.href.endsWith("/rooms/1"), { timeout: 10000 }), modal.getByRole("button", { name: "Create channel" }).click()])
    await ana.waitForLoadState("networkidle")
    check("creating it opens the channel", /rooms\/\d+/.test(ana.url()) && (await ana.locator("#nav").innerText()).includes(channel), ana.url())
    await ben.reload()
    await ben.locator("#sidebar", { hasText: channel }).waitFor({ timeout: 8000 })
    check("the member sees it in the sidebar", true)
  })

  await step("channel settings", async () => {
    const id = ana.url().match(/rooms\/(\d+)/)[1]
    await openModal(ana, `/rooms/closeds/${id}/edit`)
    const modal = ana.locator("dialog.ws-modal[open]")
    await modal.getByRole("button", { name: "Save changes" }).waitFor({ timeout: 8000 })
    check("members are counted", (await modal.locator(".room-access .ws-field__label").innerText()).includes("Members · 2"), await modal.locator(".room-access .ws-field__label").innerText())
    check("the creator can delete it", await modal.getByRole("button", { name: /Delete/ }).isVisible())
    await ana.keyboard.press("Escape")
  })

  await step("notifications from the profile", async () => {
    await openModal(ana, "/users/me/profile")
    const modal = ana.locator("dialog.ws-modal[open]")
    const row = modal.locator(".ws-row", { hasText: channel })
    await row.waitFor({ timeout: 8000 })
    await row.locator(".ws-involvement button").click()
    const menu = ana.locator(".involvement-menu")
    await menu.waitFor({ timeout: 3000 })
    check("each channel's bell opens the named choices", (await menu.innerText()).includes("All messages"))
    await menu.locator('[data-involvement="nothing"]').click()
    await row.locator(".ws-involvement button.nothing").waitFor({ timeout: 8000 })
    check("…and choosing one saves it", true)
  })

  check("no page errors", errors.length === 0, errors.join(" | "))
  await b.close()
  console.log(`\n${results.filter(Boolean).length}/${results.length} checks passed`)
  process.exit(results.every(Boolean) ? 0 : 1)
})()
