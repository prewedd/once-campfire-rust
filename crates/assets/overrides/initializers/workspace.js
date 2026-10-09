// Page-wide touches that need no markup of their own:
//
// - Reactions: a message's boosts of the same emoji become one pill with a count, marked when
//   one of them is yours, and named in its tooltip. Clicking a pill takes your reaction back, or
//   adds it when you haven't reacted with that emoji yet, as in Slack. Boosts still arrive and
//   leave one by one (Turbo streams), so the pills are worked out again whenever they change.
// - The room you're in is marked in the sidebar.

import { addBoost } from "lib/emoji/boosts"

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

let scheduled = false
function refresh() {
  if (scheduled) return
  scheduled = true
  requestAnimationFrame(() => {
    scheduled = false
    groupAllBoosts()
    markCurrentRoom()
  })
}

// Only what can change pills or the sidebar: not every keystroke in an editor.
function relevant(mutation) {
  const target = mutation.target.nodeType === Node.ELEMENT_NODE ? mutation.target : mutation.target.parentElement
  return target && !target.closest(".lexxy-editor__content") && !!target.closest(".messages, #sidebar")
}

new MutationObserver(mutations => {
  if (mutations.some(relevant)) refresh()
}).observe(document.documentElement, { childList: true, subtree: true })
document.addEventListener("turbo:load", refresh)
document.addEventListener("turbo:frame-load", refresh)
document.addEventListener("click", toggleReaction, true)
