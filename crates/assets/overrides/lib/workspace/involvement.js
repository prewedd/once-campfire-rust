// A room's notification setting, as a menu of named choices (plans/redesign.md). Campfire's bell
// cycles through them one click at a time with no labels; this shows them all, says what each
// does, and marks the current one. Choosing submits the bell's own form, with that choice, so its
// Turbo frame updates as before.
const CHOICES = {
  everything: [ "All messages", "Notify me about every new message" ],
  mentions: [ "Mentions", "Only when someone @mentions me" ],
  nothing: [ "Off", "No notifications; the room still shows as unread" ],
  invisible: [ "Off and hidden", "No notifications, and the room leaves the sidebar" ],
}
const ICONS = {
  everything: RAILS_ASSET_URL("/notification-bell-everything.svg"),
  mentions: RAILS_ASSET_URL("/notification-bell-mentions.svg"),
  nothing: RAILS_ASSET_URL("/notification-bell-nothing.svg"),
  invisible: RAILS_ASSET_URL("/notification-bell-invisible.svg"),
}
const DIRECT = [ "everything", "nothing" ]
const ROOM = [ "everything", "mentions", "nothing", "invisible" ]

function bellForm(target) {
  const form = target.closest?.('form[action*="/involvement"]')
  return form?.closest("#nav") ? form : null
}

function current(form) {
  return Object.keys(CHOICES).find(name => form.querySelector(`button.${name}`)) ?? "mentions"
}

function closeMenu() {
  document.querySelectorAll(".involvement-menu").forEach(menu => menu.remove())
  document.querySelectorAll("[data-involvement-open]").forEach(button => button.setAttribute("aria-expanded", "false"))
}

function openMenu(form) {
  closeMenu()
  const direct = form.closest("turbo-frame")?.id.includes("rooms_direct_") ?? false
  const now = current(form)
  const menu = document.createElement("div")
  menu.className = "involvement-menu"
  menu.setAttribute("role", "menu")
  menu.innerHTML = `<p class="involvement-menu__title">Notifications</p>` + (direct ? DIRECT : ROOM).map(name => `
    <button type="button" role="menuitemradio" aria-checked="${name === now}" class="involvement-menu__item" data-involvement="${name}">
      <img aria-hidden="true" src="${ICONS[name]}" width="18" height="18">
      <span><strong>${CHOICES[name][0]}</strong><small>${CHOICES[name][1]}</small></span>
    </button>`).join("")
  menu.addEventListener("click", event => {
    const choice = event.target.closest("[data-involvement]")?.dataset.involvement
    if (!choice) return
    const url = new URL(form.action, location.href)
    url.searchParams.set("involvement", choice)
    form.action = url.pathname + url.search
    closeMenu()
    form.requestSubmit()
  })
  const button = form.querySelector("button")
  const anchor = button.getBoundingClientRect()
  menu.style.top = `${anchor.bottom + 6}px`
  menu.style.left = `${Math.max(8, Math.min(anchor.left, innerWidth - 300))}px`
  document.body.append(menu)
  button.dataset.involvementOpen = ""
  button.setAttribute("aria-expanded", "true")
  menu.querySelector('[aria-checked="true"]')?.focus()
}

document.addEventListener("click", event => {
  const form = bellForm(event.target)
  if (form && event.target.closest("button")) {
    event.preventDefault()
    event.stopPropagation()
    if (document.querySelector(".involvement-menu")) closeMenu()
    else openMenu(form)
  } else if (!event.target.closest(".involvement-menu")) {
    closeMenu()
  }
}, true)

document.addEventListener("keydown", event => {
  if (event.key === "Escape" && document.querySelector(".involvement-menu")) {
    closeMenu()
    event.preventDefault()
  }
})

// Every icon-only button says what it does on hover, from the label it has for screen readers.
export function addTooltips(root = document) {
  root.querySelectorAll(":is(.btn, button):not([title])").forEach(button => {
    const label = button.querySelector(".for-screen-reader")?.textContent.trim()
    if (label && !button.textContent.replace(label, "").trim()) button.title = label
  })
  root.querySelectorAll('#nav form[action*="/involvement"] button').forEach(button => {
    const state = current(button.form)
    button.title = `Notifications: ${CHOICES[state][0]}`
    button.setAttribute("aria-haspopup", "menu")
  })
}
