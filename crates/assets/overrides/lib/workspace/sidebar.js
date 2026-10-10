// The sidebar's Slack touches (plans/redesign.md):
//
// - Channels and Direct messages fold away under their headings, remembered on this device; a
//   folded section still shows the room you're in and anything unread.
// - The workspace's name opens a menu: invite people, create a channel, settings, your profile.
// - The channel's name in the header opens its settings, as Slack's channel details do.
// - The + by Direct messages opens "New message" in the modal rather than in the sidebar.
// - "Add channel" offers Create a channel and Browse channels; Browse channels filters as you type.
// - Alt+↑ / Alt+↓ go to the previous / next channel or conversation (as Slack's), unless a channel
//   in the sidebar has the focus, where they move it (lib/workspace/reorder.js).
// - Direct messages show how many unread messages they have, in a red badge, as Slack's.
// - Ctrl/⌘+K jumps to any channel or conversation in the sidebar by name, as Slack's switcher does.
// It uses the notifications menu's look and closing (lib/workspace/involvement.js).

import { Turbo } from "@hotwired/turbo-rails"
import { openModal } from "lib/workspace/modal"

const SECTIONS = [ [ ".rooms", "Channels" ], [ ".directs", "Direct messages" ] ]

function folded(name) {
  try { return localStorage.getItem(`sidebar:folded:${name}`) === "1" } catch { return false }
}

function remember(name, value) {
  try { value ? localStorage.setItem(`sidebar:folded:${name}`, "1") : localStorage.removeItem(`sidebar:folded:${name}`) } catch {}
}

function addSectionToggles() {
  for (const [ selector, name ] of SECTIONS) {
    const section = document.querySelector(`#sidebar ${selector}`)
    if (!section || section.querySelector(":scope > .sidebar__section-toggle")) continue
    const toggle = document.createElement("button")
    toggle.type = "button"
    toggle.className = "sidebar__section-toggle"
    toggle.textContent = name
    section.prepend(toggle)
    section.classList.add("sidebar__section")
    setFolded(section, toggle, name, folded(name))
    toggle.addEventListener("click", () => {
      const fold = toggle.getAttribute("aria-expanded") === "true"
      setFolded(section, toggle, name, fold)
      remember(name, fold)
    })
  }
}

function setFolded(section, toggle, name, fold) {
  section.classList.toggle("sidebar__section--folded", fold)
  toggle.setAttribute("aria-expanded", String(!fold))
  toggle.title = fold ? `Show ${name.toLowerCase()}` : `Hide ${name.toLowerCase()}`
}

function menuItem(label, detail, href) {
  return `<a class="involvement-menu__item" role="menuitem" href="${href}"><span><strong>${label}</strong>${detail ? `<small>${detail}</small>` : ""}</span></a>`
}

function openWorkspaceMenu(header) {
  document.querySelectorAll(".involvement-menu").forEach(menu => menu.remove())
  const name = header.querySelector(".sidebar__workspace-name")?.textContent.trim() ?? "Workspace"
  const canCreate = !!document.querySelector("#sidebar .rooms__new-btn")
  const menu = document.createElement("div")
  menu.className = "involvement-menu workspace-menu"
  menu.setAttribute("role", "menu")
  menu.innerHTML = `<p class="involvement-menu__title">${name.replace(/[&<>"]/g, c => `&#${c.charCodeAt(0)};`)}</p>` + [
    menuItem("Update your status", "An emoji and a few words by your name", "/users/me/profile"),
    menuItem("Invite people", "Share the join link", "/account/edit"),
    canCreate ? menuItem("Create a channel", "", "/rooms/opens/new") : "",
    menuItem("Browse channels", "Join or leave channels", "/channels"),
    menuItem("Workspace settings", "Name, icon, members", "/account/edit"),
    menuItem("Your profile", "Name, photo, notifications", "/users/me/profile"),
  ].join("")
  const anchor = header.getBoundingClientRect()
  menu.style.top = `${anchor.bottom + 4}px`
  menu.style.left = `${anchor.left + 8}px`
  menu.addEventListener("click", event => { if (event.target.closest("a")) menu.remove() })
  document.body.append(menu)
  header.setAttribute("aria-expanded", "true")
  menu.querySelector("a")?.focus()
}

function openAddChannelMenu(button) {
  document.querySelectorAll(".involvement-menu").forEach(menu => menu.remove())
  const menu = document.createElement("div")
  menu.className = "involvement-menu workspace-menu"
  menu.setAttribute("role", "menu")
  menu.innerHTML = menuItem("Create a new channel", "", "/rooms/opens/new") + menuItem("Browse channels", "", "/channels")
  const anchor = button.getBoundingClientRect()
  menu.style.top = `${Math.min(anchor.bottom + 4, innerHeight - 120)}px`
  menu.style.left = `${anchor.left}px`
  menu.addEventListener("click", event => { if (event.target.closest("a")) menu.remove() })
  document.body.append(menu)
  menu.querySelector("a")?.focus()
}

document.addEventListener("input", event => {
  if (!event.target.matches?.("[data-browse-filter]")) return
  const query = event.target.value.trim().toLowerCase()
  event.target.closest(".ws-settings")?.querySelectorAll(".ws-browse > [data-name]").forEach(row => {
    row.hidden = !!query && !row.dataset.name.includes(query)
  })
})

function enableWorkspaceMenu() {
  const header = document.querySelector("#sidebar .sidebar__workspace:not([role])")
  if (!header) return
  header.setAttribute("role", "button")
  header.setAttribute("tabindex", "0")
  header.setAttribute("aria-haspopup", "menu")
  header.setAttribute("aria-expanded", "false")
  header.title = "Workspace menu"
}

// On window, so it runs before the notifications menu's own closing (on document).
window.addEventListener("click", event => {
  const header = event.target.closest?.("#sidebar .sidebar__workspace[role=button]")
  if (header) {
    event.stopPropagation()
    const open = document.querySelector(".workspace-menu")
    if (open) open.remove()
    else openWorkspaceMenu(header)
    return
  }
  const addChannel = event.target.closest?.("#sidebar a.rooms__new-btn")
  if (addChannel && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
    event.preventDefault()
    event.stopPropagation()
    return openAddChannelMenu(addChannel)
  }
  const newMessage = event.target.closest?.("#sidebar a.direct__new")
  if (newMessage && !event.metaKey && !event.ctrlKey && !event.shiftKey) {
    event.preventDefault()
    event.stopPropagation()
    return openModal(newMessage.href)
  }
  // The channel's name in the header: its settings
  if (event.target.closest?.("#nav .room--current")) document.querySelector("#nav a[data-room-id]")?.click()
}, true)

document.addEventListener("keydown", event => {
  if ((event.key === "Enter" || event.key === " ") && event.target.matches?.("#sidebar .sidebar__workspace[role=button]")) {
    event.preventDefault()
    openWorkspaceMenu(event.target)
  }
})

export function enhanceSidebar() {
  addSectionToggles()
  enableWorkspaceMenu()
  showUnreadCounts()
}

// The quick switcher: the sidebar's own links, filtered as you type.
// What a sidebar link shows, without its screen-reader-only words ("Ping with") or presence.
function shownName(link) {
  const copy = link.cloneNode(true)
  copy.querySelectorAll(".for-screen-reader, .presence-dot, .presence-status").forEach(node => node.remove())
  return copy.textContent.replace(/\s+/g, " ").trim()
}

function destinations() {
  const rooms = [ ...document.querySelectorAll("#sidebar .rooms a.room[href]:not(.rooms__new-btn)") ]
    .map(link => ({ name: shownName(link), href: link.href, kind: link.id.startsWith("list_rooms_closed_") ? "🔒" : "#" }))
  const people = [ ...document.querySelectorAll("#sidebar a.direct[href]:not(.direct__new)") ]
    .map(link => ({ name: shownName(link), href: link.href, kind: "@" }))
  return [ ...rooms, ...people ].filter(item => item.name)
}

export function matches(items, query) {
  const q = query.trim().toLowerCase()
  if (!q) return items
  const starts = items.filter(item => item.name.toLowerCase().startsWith(q))
  return [ ...starts, ...items.filter(item => !starts.includes(item) && item.name.toLowerCase().includes(q)) ]
}

function openSwitcher() {
  if (document.querySelector(".ws-switcher")) return
  const items = destinations()
  const dialog = document.createElement("dialog")
  dialog.className = "ws-switcher"
  dialog.setAttribute("aria-label", "Jump to")
  dialog.innerHTML = `<input class="ws-switcher__input" type="text" placeholder="Jump to a channel or person…" aria-label="Jump to" role="combobox" aria-expanded="true" aria-controls="ws-switcher-list" autocomplete="off"><ul class="ws-switcher__list" id="ws-switcher-list" role="listbox"></ul><p class="ws-switcher__hint">↑↓ to move · Enter to go · Esc to close</p>`
  const input = dialog.querySelector("input"), list = dialog.querySelector("ul")
  let shown = [], selected = 0
  const render = () => {
    shown = matches(items, input.value).slice(0, 12)
    selected = Math.min(selected, Math.max(shown.length - 1, 0))
    list.replaceChildren(...shown.map((item, index) => {
      const li = document.createElement("li")
      li.className = "ws-switcher__item"
      li.setAttribute("role", "option")
      li.setAttribute("aria-selected", String(index === selected))
      li.innerHTML = `<span class="ws-switcher__kind" aria-hidden="true"></span><span class="ws-switcher__name"></span>`
      li.firstChild.textContent = item.kind
      li.lastChild.textContent = item.name
      li.addEventListener("mousedown", event => { event.preventDefault(); go(item) })
      return li
    }))
    if (!shown.length) list.innerHTML = `<li class="ws-switcher__empty">Nothing by that name</li>`
  }
  const go = item => { dialog.close(); if (item) Turbo.visit(item.href) }
  input.addEventListener("input", () => { selected = 0; render() })
  input.addEventListener("keydown", event => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault()
      selected = (selected + (event.key === "ArrowDown" ? 1 : -1) + shown.length) % Math.max(shown.length, 1)
      render()
    } else if (event.key === "Enter") {
      event.preventDefault()
      go(shown[selected])
    }
  })
  dialog.addEventListener("click", event => { if (event.target === dialog) dialog.close() })
  dialog.addEventListener("close", () => dialog.remove())
  document.body.append(dialog)
  render()
  dialog.showModal()
  input.focus()
}

window.addEventListener("keydown", event => {
  if ((event.ctrlKey || event.metaKey) && !event.shiftKey && !event.altKey && event.key.toLowerCase() === "k" && document.querySelector("#sidebar")) {
    event.preventDefault()
    event.stopPropagation()
    openSwitcher()
  }
}, true)

// Alt+↑ / Alt+↓: the previous / next room in the sidebar, as it's shown (channels, then people).
function sidebarLinks() {
  return [ ...document.querySelectorAll("#sidebar .rooms a.room[href]:not(.rooms__new-btn), #sidebar a.direct[href]:not(.direct__new)") ]
    .filter(link => link.offsetParent)
    .sort((a, b) => Number(!!a.closest(".directs")) - Number(!!b.closest(".directs")))
}

window.addEventListener("keydown", event => {
  if (!event.altKey || event.ctrlKey || event.metaKey || event.shiftKey || !/^Arrow(Up|Down)$/.test(event.key)) return
  if (document.activeElement?.closest?.("#shared_rooms") || document.querySelector("dialog[open]")) return
  const links = sidebarLinks()
  const here = links.findIndex(link => new URL(link.href).pathname === location.pathname)
  if (here < 0 && !links.length) return
  const next = links[(here + (event.key === "ArrowDown" ? 1 : -1) + links.length) % links.length]
  if (!next) return
  event.preventDefault()
  Turbo.visit(next.href)
})

// Unread counts on direct messages: on load, every 20 seconds in view, and on coming back.
let countsAt = 0
async function showUnreadCounts(force = false) {
  if (!window.Current?.user || !document.querySelector("#sidebar a.direct")) return
  if (!force && Date.now() - countsAt < 15_000) return
  countsAt = Date.now()
  let counts = {}
  try {
    const response = await fetch("/users/me/sidebar/unread_counts", { headers: { Accept: "application/json" } })
    if (response.ok) counts = await response.json()
  } catch { return }
  document.querySelectorAll("#sidebar a.direct[id^='list_rooms_direct_']").forEach(link => {
    const count = link.classList.contains("unread") ? counts[link.id.replace("list_rooms_direct_", "")] : 0
    let badge = link.querySelector(":scope > .ws-count")
    if (!count) return badge?.remove()
    badge ??= link.appendChild(Object.assign(document.createElement("span"), { className: "ws-count" }))
    const text = count > 99 ? "99+" : String(count)
    if (badge.textContent !== text) badge.textContent = text
  })
}

document.addEventListener("turbo:load", () => showUnreadCounts(true))
setInterval(() => { if (document.visibilityState === "visible") showUnreadCounts(true) }, 20_000)
document.addEventListener("visibilitychange", () => { if (document.visibilityState === "visible") showUnreadCounts(true) })
export { showUnreadCounts }
