// The Threads view (plans/redesign.md), as Slack's: the sidebar's "Threads" lists the threads
// you're in, most recently active first, in the right-hand panel; one opens there. The badge
// counts those whose latest reply isn't yours.
const REFRESH_EVERY = 60 * 1000

function escapeHTML(text) {
  return String(text ?? "").replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char])
}

function ago(seconds) {
  const minutes = Math.round((Date.now() / 1000 - seconds) / 60)
  if (minutes < 1) return "just now"
  if (minutes < 60) return `${minutes} min ago`
  const hours = Math.round(minutes / 60)
  if (hours < 24) return `${hours} hr ago`
  return new Date(seconds * 1000).toLocaleDateString([], { month: "short", day: "numeric" })
}

async function load() {
  const response = await fetch("/threads", { headers: { Accept: "application/json" } })
  if (!response.ok) throw new Error(`threads: ${response.status}`)
  return response.json()
}

function badge(threads) {
  const count = threads.filter(thread => thread.new).length
  document.querySelectorAll("[data-threads-badge]").forEach(element => {
    element.hidden = !count
    element.textContent = count > 9 ? "9+" : count
  })
}

export async function refreshThreadsBadge() {
  if (!document.querySelector("[data-threads-badge]")) return
  try { badge(await load()) } catch {}
}

export async function openThreads() {
  const frame = document.getElementById("flexpane")
  if (!frame) {
    sessionStorage.setItem("open-threads", "1")
    return Turbo.visit("/")
  }
  frame.removeAttribute("src")
  frame.closest(".flexpane").hidden = false
  document.body.classList.add("flexpane-open")
  frame.innerHTML = `
    <header class="flexpane__header">
      <h2 class="flexpane__title">Threads</h2>
      <button type="button" class="btn flexpane__close" data-flexpane-close title="Close">✕<span class="for-screen-reader">Close</span></button>
    </header>
    <div class="flexpane__body thread-list"><p class="thread-list__empty">Loading…</p></div>`
  const list = frame.querySelector(".thread-list")
  try {
    const threads = await load()
    badge(threads)
    list.innerHTML = threads.length ? threads.map(thread => `
      <a class="thread-card${thread.new ? " thread-card--new" : ""}" href="${escapeHTML(thread.url)}">
        <span class="thread-card__room">${thread.direct ? "" : "#"}${escapeHTML(thread.room_name)}</span>
        <span class="thread-card__parent">
          <img src="${escapeHTML(thread.author_avatar_url)}" alt="" width="20" height="20">
          <strong>${escapeHTML(thread.author)}</strong>
          <span>${escapeHTML(thread.preview) || "<em>Attachment</em>"}</span>
        </span>
        <span class="thread-card__meta">
          <span class="thread-card__people">${thread.participants.map(person => `<img src="${escapeHTML(person.avatar_url)}" alt="" title="${escapeHTML(person.name)}" width="18" height="18">`).join("")}</span>
          <span class="thread-card__count">${thread.reply_count === 1 ? "1 reply" : `${thread.reply_count} replies`}</span>
          <span>· ${ago(thread.last_reply_at)}</span>
          ${thread.new ? `<span class="thread-card__dot" aria-label="New replies"></span>` : ""}
        </span>
      </a>`).join("") : `<p class="thread-list__empty">Threads you start or reply to show up here.</p>`
  } catch {
    list.innerHTML = `<p class="thread-list__empty">Couldn’t load your threads. Try again in a moment.</p>`
  }
}

document.addEventListener("click", event => {
  if (!event.target.closest("[data-open-threads]")) return
  event.preventDefault()
  openThreads()
})

document.addEventListener("turbo:load", () => {
  refreshThreadsBadge()
  if (sessionStorage.getItem("open-threads")) {
    sessionStorage.removeItem("open-threads")
    openThreads()
  }
})

setInterval(refreshThreadsBadge, REFRESH_EVERY)

// A reply arriving anywhere may change the badge.
let pending
document.addEventListener("turbo:before-stream-render", event => {
  if (!event.target.getAttribute?.("target")?.startsWith("thread")) return
  clearTimeout(pending)
  pending = setTimeout(refreshThreadsBadge, 1500)
})

// The sidebar arrives in a frame of its own, after the page.
document.addEventListener("turbo:frame-load", event => {
  if (event.target.querySelector?.("[data-threads-badge]")) refreshThreadsBadge()
})
