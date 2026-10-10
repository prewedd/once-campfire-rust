// Pinned messages (plans/redesign.md): a "📌 N" in the channel's header (opening its pins in the
// modal), a "Pinned" label on pinned messages, and "Pin to channel" / "Unpin from channel" in the
// "⋯" menu. Messages are cached and shared, so the channel's pins come from /rooms/:id/pins/ids
// when it opens, and live after that: a change is broadcast into the header's hidden
// #ws_pins_room_<id> (controllers/pins.rs), read here.
import { toast } from "lib/workspace/saved"

let room = null
let pinned = new Set()
let loadedAt = 0

async function load(force = false) {
  const id = window.Current?.room?.id
  if (!id) return null
  if (!force && id === room && Date.now() - loadedAt < 30_000) return pinned
  try {
    const response = await fetch(`/rooms/${id}/pins/ids`, { headers: { Accept: "application/json" } })
    if (!response.ok) return null
    const ids = await response.json()
    room = id
    pinned = new Set(ids)
    loadedAt = Date.now()
    return pinned
  } catch {
    return null // interrupted (leaving the page): next time
  }
}

function paint() {
  if (window.Current?.room?.id !== room) return
  document.querySelectorAll(".messages .message[data-message-id]").forEach(message => {
    message.classList.toggle("message--pinned", pinned.has(Number(message.dataset.messageId)))
  })
  const nav = document.querySelector("#nav")
  if (!nav?.querySelector(":scope > .room--current:not(.thread--current)")) return
  let button = nav.querySelector(":scope > .ws-pins-btn")
  if (!pinned.size) return button?.remove()
  if (!button) {
    button = document.createElement("a")
    button.className = "ws-pins-btn"
    nav.append(button)
  }
  // Only what changed: writing the header re-runs this (initializers/workspace.js watches it).
  const text = `📌 ${pinned.size}`
  if (button.textContent !== text) {
    button.href = `/rooms/${room}/pins`
    button.textContent = text
    button.title = `${pinned.size} pinned message${pinned.size === 1 ? "" : "s"}`
  }
}

function label(button) {
  const on = pinned.has(Number(button.dataset.messageId))
  button.toggleAttribute("data-pinned", on)
  button.title = on ? "Unpin from channel" : "Pin to channel"
  button.setAttribute("aria-label", button.title)
}

// A broadcast change: the room's pinned ids, as JSON.
function fromSignal() {
  const id = window.Current?.room?.id
  const text = document.getElementById(`ws_pins_room_${id}`)?.textContent.trim()
  if (!text || text === fromSignal.last) return
  fromSignal.last = text
  try {
    pinned = new Set(JSON.parse(text))
    room = id
    loadedAt = Date.now()
  } catch {}
}

export function showPins() {
  if (!window.Current?.user || !window.Current?.room) return
  fromSignal()
  paint()
  load().then(set => set && paint())
}

document.addEventListener("toggle", async event => {
  const menu = event.target
  if (!menu.open || !menu.querySelector?.(".message__pin-btn")) return
  await load()
  menu.querySelectorAll(".message__pin-btn").forEach(label)
}, true)

document.addEventListener("click", async event => {
  const button = event.target.closest(".message__pin-btn")
  if (!button) return
  event.preventDefault()
  await load()
  const id = Number(button.dataset.messageId)
  const unpinning = pinned.has(id)
  const response = await fetch(`/messages/${id}/pin`, {
    method: unpinning ? "DELETE" : "POST",
  }).catch(() => null)
  if (!response?.ok) return toast("Couldn't pin that message")
  button.closest("details")?.removeAttribute("open")
  await load(true)
  paint()
  label(button)
  toast(unpinning ? "Unpinned from the channel" : "Pinned to the channel")
})

document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") { loadedAt = 0; showPins() } })
