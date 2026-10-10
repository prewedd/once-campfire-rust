// Saved for later (plans/redesign.md): the "⋯" menu's "Save for later" / "Remove from saved".
// Messages are cached and shared, so which ones you've saved comes from /saved/ids, when a menu
// first opens (and again a minute later).
let saved = null
let loadedAt = 0

async function load() {
  if (saved && Date.now() - loadedAt < 60_000) return saved
  try {
    const response = await fetch("/saved/ids", { headers: { Accept: "application/json" } })
    if (response.ok) {
      saved = new Set(await response.json())
      loadedAt = Date.now()
    }
  } catch {} // interrupted (leaving the page): next time
  return saved ?? new Set()
}

function label(button) {
  const on = saved?.has(Number(button.dataset.messageId))
  button.toggleAttribute("data-saved", !!on)
  button.title = on ? "Remove from saved" : "Save for later"
  button.setAttribute("aria-label", button.title)
}

export function toast(text) {
  document.querySelector(".ws-toast")?.remove()
  const note = document.createElement("div")
  note.className = "ws-toast"
  note.setAttribute("role", "status")
  note.textContent = text
  document.body.append(note)
  setTimeout(() => note.remove(), 2200)
}

// toggle doesn't bubble: listen while it's captured on the way down
document.addEventListener("toggle", async event => {
  const menu = event.target
  if (!menu.open || !menu.querySelector?.(".message__save-btn")) return
  await load()
  menu.querySelectorAll(".message__save-btn").forEach(label)
}, true)

document.addEventListener("click", async event => {
  const button = event.target.closest(".message__save-btn")
  if (!button) return
  event.preventDefault()
  await load()
  const id = Number(button.dataset.messageId)
  const removing = saved.has(id)
  const response = await fetch(`/messages/${id}/save`, {
    method: removing ? "DELETE" : "POST",
    headers: { "X-CSRF-Token": document.querySelector('meta[name="csrf-token"]')?.content ?? "" },
  }).catch(() => null)
  if (!response?.ok) return toast("Couldn't save that message")
  removing ? saved.delete(id) : saved.add(id)
  label(button)
  button.closest("details")?.removeAttribute("open")
  toast(removing ? "Removed from saved" : "Saved for later")
  if (removing && document.body.classList.contains("saved")) button.closest(".message")?.remove()
})
