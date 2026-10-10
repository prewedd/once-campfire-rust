// The sidebar's Slack touches (plans/redesign.md):
//
// - Channels and Direct messages fold away under their headings, remembered on this device; a
//   folded section still shows the room you're in and anything unread.
// - The workspace's name opens a menu: invite people, create a channel, settings, your profile.
// - The channel's name in the header opens its settings, as Slack's channel details do.
// - The + by Direct messages opens "New message" in the modal rather than in the sidebar.
// It uses the notifications menu's look and closing (lib/workspace/involvement.js).

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
    menuItem("Invite people", "Share the join link", "/account/edit"),
    canCreate ? menuItem("Create a channel", "", "/rooms/opens/new") : "",
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
}
