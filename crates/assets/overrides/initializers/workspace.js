// Page-wide touches that need no markup of their own:
//
// - Reactions: a message's boosts of the same emoji become one pill with a count, marked when
//   one of them is yours, and named in its tooltip. Clicking a pill takes your reaction back, or
//   adds it when you haven't reacted with that emoji yet, as in Slack. Boosts still arrive and
//   leave one by one (Turbo streams), so the pills are worked out again whenever they change.
// - The room you're in is marked in the sidebar.
// - Presence: who's online or away (OnlineChannel), on everything marked `data-presence-user`.
// - The right-hand panel: threads and profiles open beside the conversation, as in Slack.
// - The sidebar's folding sections and workspace menu (lib/workspace/sidebar.js).
// - Custom statuses by people's names (lib/workspace/status.js), saved messages (lib/workspace/saved.js), pins (lib/workspace/pins.js).

import { cable } from "@hotwired/turbo-rails"
import { addBoost } from "lib/emoji/boosts"
import { offerFromLink, showHuddles, showInvite, trackHuddles } from "lib/huddle/huddle"
import { closeModal } from "lib/workspace/modal"
import { addTooltips } from "lib/workspace/involvement"
import { enableReordering } from "lib/workspace/reorder"
import { enhanceSidebar } from "lib/workspace/sidebar"
import { showLocalTimes, showStatuses } from "lib/workspace/status"
import "lib/workspace/saved"
import { showPins } from "lib/workspace/pins"
import { showNewLine } from "lib/workspace/unread"
import "lib/workspace/schedule"
import "lib/workspace/threads"

const BOOSTS = '[id^="boosts_message_"]'

function boostContent(boost) {
  return boost.querySelector('[data-boost-delete-target="content"]')?.textContent.trim() ?? ""
}

function boosterName(boost) {
  const label = boost.querySelector(".boost__avatar img")?.getAttribute("aria-label") ?? ""
  return label.replace(/ boosted .*$/s, "")
}

function isMine(boost) {
  return boost.dataset.boostDeleteBoosterIdValue === String(Current.user?.id)
}

function groupBoosts(container) {
  const groups = new Map()
  for (const boost of container.querySelectorAll(":scope > .boost")) {
    const content = boostContent(boost)
    if (!groups.has(content)) groups.set(content, [])
    groups.get(content).push(boost)
  }

  for (const [ content, boosts ] of groups) {
    const [ lead, ...rest ] = boosts
    lead.classList.add("boost--lead")
    lead.classList.remove("boost--folded")
    rest.forEach(boost => {
      boost.classList.add("boost--folded")
      boost.classList.remove("boost--lead", "boost--mine")
    })
    lead.classList.toggle("boost--mine", boosts.some(isMine))
    lead.dataset.count = boosts.length
    lead.title = `${boosts.map(boosterName).join(", ")} reacted with ${content}`

    let count = lead.querySelector(".boost__count")
    if (!count) {
      count = document.createElement("span")
      count.className = "boost__count"
      lead.querySelector('[data-boost-delete-target="content"]')?.after(count)
    }
    count.textContent = boosts.length
  }
}

function groupAllBoosts(root = document) {
  root.querySelectorAll?.(BOOSTS).forEach(groupBoosts)
}

// Clicking a pill toggles your reaction.
function toggleReaction(event) {
  const pill = event.target.closest(".boost--lead")
  if (!pill || event.target.closest(".boost__avatar, form, button")) return

  event.preventDefault()
  event.stopImmediatePropagation()

  const container = pill.closest(BOOSTS)
  const content = boostContent(pill)
  const mine = Array.from(container.querySelectorAll(":scope > .boost")).find(boost => isMine(boost) && boostContent(boost) === content)

  if (mine) {
    takeBack(mine, container)
  } else {
    submitBoost(container, content)
  }
}

// Removes your boost at once, then deletes it (the broadcast removes it for everyone). Fetched
// rather than submitted: the broadcast can remove the boost's form before its own request ends.
async function takeBack(boost, container) {
  const form = boost.querySelector("form.button_to")
  if (!form) return

  boost.hidden = true
  boost.classList.remove("boost")
  groupBoosts(container)
  try {
    const response = await fetch(form.action, { method: "POST", body: new FormData(form), headers: { Accept: "text/vnd.turbo-stream.html, text/html" } })
    if (!response.ok) throw new Error(response.statusText)
    boost.remove()
  } catch {
    boost.hidden = false
    boost.classList.add("boost")
    groupBoosts(container)
  }
}

function submitBoost(container, content) {
  const newBoost = container.closest('turbo-frame[id^="boosting_"]')?.querySelector('a[href$="/boosts/new"]')
  if (newBoost) addBoost(newBoost.getAttribute("href").replace(/\/new$/, ""), content)
}

function markCurrentRoom() {
  const roomId = String(Current.room?.id ?? "")
  document.querySelectorAll("#sidebar [data-room-id]").forEach(link => {
    const current = link.dataset.roomId === roomId
    link.classList.toggle("room--active", current)
    if (current) link.setAttribute("aria-current", "page")
    else link.removeAttribute("aria-current")
  })
}

// Presence. One subscription per page, kept across Turbo visits.
const AWAY_AFTER = 2 * 60 * 1000
const statuses = new Map()
let onlineChannel
let awayTimer

function showPresence() {
  document.querySelectorAll("[data-presence-user]").forEach(element => {
    element.dataset.presence = statuses.get(element.dataset.presenceUser) ?? "offline"
  })
}

async function trackPresence() {
  if (onlineChannel || !Current.user) return
  onlineChannel = await cable.subscribeTo({ channel: "OnlineChannel" }, {
    received(data) {
      if (data.snapshot) {
        statuses.clear()
        Object.entries(data.snapshot).forEach(([ id, status ]) => statuses.set(id, status))
      } else {
        statuses.set(String(data.user_id), data.status)
      }
      showPresence()
    },
    connected() {
      if (document.visibilityState === "hidden") this.perform("away")
    }
  })
}

document.addEventListener("visibilitychange", () => {
  clearTimeout(awayTimer)
  if (document.visibilityState === "hidden") {
    awayTimer = setTimeout(() => onlineChannel?.perform("away"), AWAY_AFTER)
  } else {
    onlineChannel?.perform("active")
  }
})

// The right-hand panel. Links to a thread or a profile open in it, from anywhere on a room's page
// (the conversation, the sidebar, a settings modal), as in Slack; anything else (a modified click,
// "Open in full", other pages) navigates as before.
const PANE_PATH = /^\/(rooms\/\d+\/threads\/\d+(\/@\d+)?|users\/\d+)$/

function paneFrameFor(link) {
  const frame = document.getElementById("flexpane")
  if (!frame || link.closest(".flexpane__header")) return null
  const url = new URL(link.href, location.href)
  if (url.origin !== location.origin || !PANE_PATH.test(url.pathname)) return null
  if (link.closest(".ws-modal")) closeModal()
  return frame
}

function openPane(frame, url) {
  frame.closest(".flexpane").hidden = false
  document.body.classList.add("flexpane-open")
  if (!frame.childElementCount) {
    frame.innerHTML = `<header class="flexpane__header"><h2 class="flexpane__title">Loading…</h2>
      <button type="button" class="btn flexpane__close" data-flexpane-close title="Close">✕<span class="for-screen-reader">Close</span></button></header>`
  }
  frame.src = url
}

function closePane() {
  const pane = document.querySelector(".flexpane:not([hidden])")
  if (!pane) return false
  pane.hidden = true
  document.body.classList.remove("flexpane-open")
  const frame = pane.querySelector("turbo-frame")
  frame.removeAttribute("src")
  frame.replaceChildren()
  return true
}

document.addEventListener("click", event => {
  if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  if (event.target.closest("[data-flexpane-close]")) {
    closePane()
    return
  }
  const link = event.target.closest("a[href]")
  const frame = link && paneFrameFor(link)
  if (!frame) return
  event.preventDefault()
  event.stopPropagation()
  openPane(frame, link.href)
}, true)

document.addEventListener("keydown", event => {
  if (event.key === "Escape" && !event.defaultPrevented && !document.querySelector(".lexxy-prompt-menu--visible") && closePane()) {
    event.preventDefault()
  }
})

// A frame whose response lacks it (a stale cache, a sign-in redirect, a page changed mid-request)
// never shows Turbo's "Content missing": it loads once more, then the page itself opens instead.
document.addEventListener("turbo:frame-missing", event => {
  const frame = event.target
  event.preventDefault()
  if (!frame.dataset.retried) {
    frame.dataset.retried = "true"
    frame.reload()
    return
  }
  delete frame.dataset.retried
  if (frame.id === "flexpane") closePane()
  event.detail.visit(event.detail.response)
})
document.addEventListener("turbo:frame-load", event => delete event.target.dataset.retried)

// The panel's copy of a thread's parent would share its ids with the room's; the room's keeps them.
// shortcut: so edits to the parent reach the room's copy only, until the panel reloads.
document.addEventListener("turbo:frame-load", event => {
  if (event.target.id !== "flexpane") return
  event.target.querySelectorAll("[id]").forEach(element => {
    if (document.querySelectorAll(`[id="${CSS.escape(element.id)}"]`).length > 1) element.removeAttribute("id")
  })
  // A thread opens ready for a reply, as the composer does on a page (not on touch screens).
  if (!window.matchMedia("(pointer: coarse)").matches) {
    setTimeout(() => event.target.querySelector("lexxy-editor")?.focus(), 50)
  }
})

let scheduled = false
function refresh() {
  if (scheduled) return
  scheduled = true
  requestAnimationFrame(() => {
    scheduled = false
    groupAllBoosts()
    markCurrentRoom()
    showPresence()
    trackPresence()
    addTooltips()
    enableReordering()
    enhanceSidebar()
    showStatuses()
    showLocalTimes()
    showPins()
    showNewLine()
    showHuddles()
    showInvite()
    trackHuddles()
    offerFromLink()
  })
}

// Only what can change pills or the sidebar: not every keystroke in an editor.
function relevant(mutation) {
  const target = mutation.target.nodeType === Node.ELEMENT_NODE ? mutation.target : mutation.target.parentElement
  return target && !target.closest(".lexxy-editor__content") && !!target.closest(".messages, #sidebar, #nav, .ws-modal")
}

new MutationObserver(mutations => {
  if (mutations.some(relevant)) refresh()
}).observe(document.documentElement, { childList: true, subtree: true })
document.addEventListener("turbo:load", refresh)
document.addEventListener("turbo:frame-load", refresh)
document.addEventListener("click", toggleReaction, true)
