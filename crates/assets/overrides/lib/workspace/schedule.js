// Scheduled messages (plans/scheduled-messages.md): the composer's clock button offers times
// (Slack's presets, or any date and time); the message is saved and the server posts it then.
// A bar over the composer counts yours in the room, and opens them in the right-hand panel, to send
// now, reschedule or delete.
const DAY = 24 * 60 * 60 * 1000

function csrf() {
  return document.querySelector('meta[name="csrf-token"]')?.content ?? ""
}

function escapeHTML(text) {
  return String(text ?? "").replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char])
}

async function request(method, url, params) {
  const response = await fetch(url, {
    method,
    headers: { Accept: "application/json", "X-CSRF-Token": csrf() },
    body: params ? new URLSearchParams(params) : undefined,
  })
  const text = await response.text()
  const data = text ? JSON.parse(text) : null
  if (!response.ok) throw new Error(data?.error ?? `Couldn’t schedule it (${response.status})`)
  return data
}

// "Today at 3:00 PM", "Tomorrow at 9:00 AM", "Mon, Oct 13 at 9:00 AM", in your own time zone.
export function describe(date) {
  const time = date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
  const today = new Date()
  const days = Math.round((new Date(date).setHours(0, 0, 0, 0) - new Date(today).setHours(0, 0, 0, 0)) / DAY)
  if (days === 0) return `Today at ${time}`
  if (days === 1) return `Tomorrow at ${time}`
  return `${date.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" })} at ${time}`
}

// Slack's choices: in 30 minutes, tomorrow at 9, next Monday at 9.
export function presets(now = new Date()) {
  const at9 = days => { const date = new Date(now); date.setDate(date.getDate() + days); date.setHours(9, 0, 0, 0); return date }
  const monday = (8 - now.getDay()) % 7 || 7
  const choices = [
    [ "In 30 minutes", new Date(now.getTime() + 30 * 60 * 1000) ],
    [ "In 1 hour", new Date(now.getTime() + 60 * 60 * 1000) ],
    [ "Tomorrow", at9(1) ],
  ]
  if (monday !== 1) choices.push([ "Next Monday", at9(monday) ])
  return choices
}

function localInputValue(date) {
  const pad = number => String(number).padStart(2, "0")
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

function toast(text) {
  document.querySelector(".schedule-toast")?.remove()
  const element = document.createElement("div")
  element.className = "schedule-toast"
  element.setAttribute("role", "status")
  element.textContent = text
  document.body.append(element)
  setTimeout(() => element.remove(), 4000)
}

// The menu

let menu = null

export function closeMenu() {
  menu?.remove()
  menu = null
  document.querySelectorAll(".composer__schedule-btn[aria-expanded=true]").forEach(button => button.setAttribute("aria-expanded", "false"))
}

export function openMenu(button, { roomId, threadId }) {
  if (menu) return closeMenu()
  const form = button.closest("form")
  const editor = form?.querySelector("lexxy-editor")
  menu = document.createElement("div")
  menu.className = "schedule-menu"
  menu.setAttribute("role", "menu")
  const custom = new Date(Date.now() + 2 * 60 * 60 * 1000)
  custom.setMinutes(0, 0, 0)
  menu.innerHTML = `
    <p class="schedule-menu__title">Schedule message</p>
    ${presets().map(([ label, date ]) => `
      <button type="button" role="menuitem" class="schedule-menu__item" data-send-at="${Math.floor(date.getTime() / 1000)}">
        <strong>${label}</strong><small>${escapeHTML(describe(date))}</small>
      </button>`).join("")}
    <div class="schedule-menu__custom">
      <label>Custom time <input type="datetime-local" min="${localInputValue(new Date())}" value="${localInputValue(custom)}"></label>
      <button type="button" class="schedule-menu__go" data-custom>Schedule</button>
    </div>
    <p class="schedule-menu__error" hidden></p>`

  const fail = text => {
    const error = menu?.querySelector(".schedule-menu__error")
    if (error) { error.textContent = text; error.hidden = false }
  }

  menu.addEventListener("click", async event => {
    const choice = event.target.closest("[data-send-at], [data-custom]")
    if (!choice) return
    const sendAt = choice.dataset.sendAt ?? Math.floor(new Date(menu.querySelector("input").value).getTime() / 1000)
    if (!editor || editor.isBlank) return fail("Write your message first, then schedule it")
    if (!sendAt || Number.isNaN(Number(sendAt))) return fail("Pick a date and time")
    if (form.querySelector('[data-composer-target="fileList"]')?.children.length) return fail("Files can’t be scheduled yet: send them now, or remove them")
    try {
      const scheduled = await request("POST", `/rooms/${roomId}/scheduled_messages`, {
        body: editor.value,
        send_at: sendAt,
        ...(threadId ? { thread_parent_id: threadId } : {}),
      })
      editor.value = ""
      editor.dispatchEvent(new CustomEvent("lexxy:change", { bubbles: true }))
      closeMenu()
      toast(`Scheduled for ${describe(new Date(scheduled.send_at * 1000))}`)
      showScheduledBar()
    } catch (error) {
      fail(error.message)
    }
  })

  const anchor = button.getBoundingClientRect()
  document.body.append(menu)
  menu.style.left = `${Math.max(8, Math.min(anchor.right - menu.offsetWidth, innerWidth - menu.offsetWidth - 8))}px`
  menu.style.top = `${Math.max(8, anchor.top - menu.offsetHeight - 8)}px`
  button.setAttribute("aria-expanded", "true")
  menu.querySelector("[data-send-at]")?.focus()
}

document.addEventListener("click", event => {
  if (menu && !event.target.closest(".schedule-menu, .composer__schedule-btn")) closeMenu()
}, true)

document.addEventListener("keydown", event => {
  if (event.key === "Escape" && menu) {
    closeMenu()
    event.preventDefault()
  }
})

// The bar over the room's composer, and the panel listing them

function roomId() {
  return window.Current?.room?.id
}

export async function showScheduledBar() {
  const composer = document.querySelector("#composer")?.closest(".composer")
  const room = roomId()
  if (!composer || !room) return
  let list = []
  try {
    list = await request("GET", `/rooms/${room}/scheduled_messages`)
  } catch {
    return
  }
  let bar = document.querySelector(".scheduled-bar")
  if (!list.length) return bar?.remove()
  if (!bar) {
    bar = document.createElement("button")
    bar.type = "button"
    bar.className = "scheduled-bar"
    bar.addEventListener("click", () => openPanel())
    composer.before(bar)
  }
  const next = new Date(list[0].send_at * 1000)
  bar.innerHTML = `<img aria-hidden="true" src="${RAILS_ASSET_URL("/clock.svg")}" width="14" height="14"> ${list.length === 1 ? "1 scheduled message" : `${list.length} scheduled messages`} · next ${escapeHTML(describe(next).replace(/^./, c => c.toLowerCase()))} <span>See all</span>`
}

export async function openPanel() {
  const frame = document.getElementById("flexpane")
  const room = roomId()
  if (!frame || !room) return
  frame.removeAttribute("src")
  frame.closest(".flexpane").hidden = false
  document.body.classList.add("flexpane-open")
  frame.innerHTML = `
    <header class="flexpane__header">
      <h2 class="flexpane__title">Scheduled</h2>
      <button type="button" class="btn flexpane__close" data-flexpane-close title="Close">✕<span class="for-screen-reader">Close</span></button>
    </header>
    <div class="flexpane__body scheduled-list" data-scheduled-list><p class="scheduled-list__empty">Loading…</p></div>`
  await renderPanel()
}

async function renderPanel() {
  const container = document.querySelector("[data-scheduled-list]")
  if (!container) return
  const list = await request("GET", `/rooms/${roomId()}/scheduled_messages`).catch(() => [])
  container.innerHTML = list.length ? list.map(item => `
    <article class="scheduled-item" data-id="${item.id}">
      <p class="scheduled-item__when"><img aria-hidden="true" src="${RAILS_ASSET_URL("/clock.svg")}" width="13" height="13"> ${escapeHTML(describe(new Date(item.send_at * 1000)))}${item.thread_parent_id ? " · in a thread" : ""}</p>
      <p class="scheduled-item__preview">${escapeHTML(item.preview)}</p>
      <div class="scheduled-item__actions">
        <button type="button" data-action-send>Send now</button>
        <label class="scheduled-item__reschedule">Reschedule <input type="datetime-local" min="${localInputValue(new Date())}" value="${localInputValue(new Date(item.send_at * 1000))}"></label>
        <button type="button" data-action-delete class="scheduled-item__delete">Delete</button>
      </div>
      <p class="schedule-menu__error" hidden></p>
    </article>`).join("") : `<p class="scheduled-list__empty">No scheduled messages here. Use the clock by Send to schedule one.</p>`
}

document.addEventListener("click", async event => {
  const item = event.target.closest(".scheduled-item")
  if (!item) return
  const url = `/rooms/${roomId()}/scheduled_messages/${item.dataset.id}`
  try {
    if (event.target.closest("[data-action-send]")) await request("POST", `${url}/deliver`)
    else if (event.target.closest("[data-action-delete]")) {
      if (!confirm("Delete this scheduled message?")) return
      await request("DELETE", url)
    } else return
    await renderPanel()
    showScheduledBar()
  } catch (error) {
    const message = item.querySelector(".schedule-menu__error")
    message.textContent = error.message
    message.hidden = false
  }
})

document.addEventListener("change", async event => {
  const input = event.target.closest(".scheduled-item__reschedule input")
  if (!input) return
  const item = input.closest(".scheduled-item")
  try {
    await request("PATCH", `/rooms/${roomId()}/scheduled_messages/${item.dataset.id}`, { send_at: Math.floor(new Date(input.value).getTime() / 1000) })
    await renderPanel()
    showScheduledBar()
  } catch (error) {
    const message = item.querySelector(".schedule-menu__error")
    message.textContent = error.message
    message.hidden = false
  }
})

// Each room page shows its own count.
document.addEventListener("turbo:load", showScheduledBar)
