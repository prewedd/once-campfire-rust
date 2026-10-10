// Custom statuses (plans/redesign.md): an emoji by people's names, as in Slack, and the profile's
// status form.
//
// Everyone's current status comes from /users/statuses.json, at most once a minute (and right after
// you change yours). They're shown without touching the server's markup, so cached messages stay
// cached: an attribute on a message's author and a conversation's name (CSS draws the emoji), and
// a line under a profile's name.

const MAX_AGE = 60_000
let statuses = {}
let fetchedAt = 0
let fetching = null

async function load() {
  if (Date.now() - fetchedAt < MAX_AGE) return statuses
  fetching ??= fetch("/users/statuses.json", { headers: { Accept: "application/json" } })
    .then(response => response.ok ? response.json() : statuses)
    .then(json => { statuses = json; fetchedAt = Date.now(); return statuses })
    .catch(() => statuses)
    .finally(() => { fetching = null })
  return fetching
}

function mark(element, status) {
  if (status && element.dataset.statusEmoji === status.emoji && element.title.endsWith(status.text)) return
  if (status) {
    element.dataset.statusEmoji = status.emoji
    element.title = status.text ? `${status.emoji} ${status.text}` : status.emoji
  } else if (element.dataset.statusEmoji) {
    delete element.dataset.statusEmoji
    element.removeAttribute("title")
  }
}

function paint() {
  document.querySelectorAll(".message[data-user-id] .message__author strong").forEach(name => {
    mark(name, statuses[name.closest(".message").dataset.userId])
  })
  document.querySelectorAll("#sidebar a.direct:has(.presence-dot[data-presence-user]) .direct__author > span").forEach(name => {
    mark(name, statuses[name.closest("a.direct").querySelector(".presence-dot").dataset.presenceUser])
  })
  const me = document.querySelector('#sidebar .sidebar__tools a[href$="/users/me/profile"]')
  if (me && window.Current?.user) mark(me, statuses[Current.user.id])
  document.querySelectorAll(".presence-status[data-presence-user]").forEach(presence => {
    const status = statuses[presence.dataset.presenceUser]
    let line = presence.nextElementSibling?.classList.contains("ws-status-line") ? presence.nextElementSibling : null
    if (!status) return line?.remove()
    if (!line) {
      line = document.createElement("p")
      line.className = "ws-status-line"
      presence.after(line)
    }
    const text = `${status.emoji} ${status.text}`.trim()
    if (line.textContent !== text) line.textContent = text
  })
}

export function showStatuses() {
  if (!window.Current?.user) return // signed out: nobody's to show
  paint()
  load().then(paint)
}

// The profile's form: a suggestion fills it in; "Today" and "This week" clear at the end of them, here.
function minutesUntil(until) {
  const now = new Date(), end = new Date(now)
  end.setHours(24, 0, 0, 0)
  if (until === "week") end.setDate(end.getDate() + ((7 - end.getDay()) % 7))
  return Math.max(1, Math.round((end - now) / 60_000))
}

document.addEventListener("click", event => {
  const preset = event.target.closest("[data-status-preset]")
  if (!preset) return
  const form = preset.closest("form")
  const [ emoji, text, clear ] = preset.dataset.statusPreset.split("|")
  form.querySelector('[name="status[emoji]"]').value = emoji
  form.querySelector('[name="status[text]"]').value = text
  const select = form.querySelector('[name="status[clear_in]"]')
  const option = [ ...select.options ].find(option => option.dataset.until === clear || option.value === clear)
  select.value = option?.value ?? ""
  form.querySelector('[name="status[text]"]').focus()
})

document.addEventListener("submit", event => {
  const form = event.target
  if (form.matches?.(".ws-status__form")) {
    const option = form.querySelector('[name="status[clear_in]"]')?.selectedOptions[0]
    if (option?.dataset.until) option.value = minutesUntil(option.dataset.until)
  }
  if (form.matches?.(".ws-status__form, form[action$='/users/me/status']")) fetchedAt = 0
}, true)

// While the page is in view, and on coming back to it.
setInterval(() => { if (document.visibilityState === "visible") showStatuses() }, MAX_AGE)
document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") { fetchedAt = 0; showStatuses() } })
