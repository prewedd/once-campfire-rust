// Channels in the sidebar can be put in any order (plans/redesign.md): drag one, or move the focused
// one with Alt+↑ / Alt+↓. The order is saved for this person (memberships.position) and the
// sidebar's sorting keeps it (controllers/sorted_list_controller.js).
const LIST = "#shared_rooms"
let dragged = null

function items(list) {
  return [ ...list.querySelectorAll(":scope > [data-room-id]") ]
}

// Numbered as they stand, right as they move: the sidebar re-sorts whenever an item moves, by these.
function renumber(list) {
  const ordered = items(list)
  ordered.forEach((item, index) => { item.dataset.sortedListPosition = index })
  return ordered
}

function save(list) {
  const ordered = renumber(list)
  const body = new URLSearchParams({ room_ids: ordered.map(item => item.dataset.roomId).join(",") })
  fetch("/users/me/sidebar/positions", {
    method: "POST",
    body,
    headers: { "X-CSRF-Token": document.querySelector('meta[name="csrf-token"]')?.content ?? "" },
  }).catch(error => console.error("reorder:", error))
}

export function enableReordering() {
  document.querySelectorAll(`${LIST} > [data-room-id]:not([draggable])`).forEach(item => {
    item.draggable = true
    item.setAttribute("aria-description", "Alt+Up or Alt+Down to move")
  })
}

document.addEventListener("dragstart", event => {
  const item = event.target.closest?.(`${LIST} > [data-room-id]`)
  if (!item) return
  dragged = item
  event.dataTransfer.effectAllowed = "move"
  event.dataTransfer.setData("text/plain", item.dataset.roomId)
  requestAnimationFrame(() => item.classList.add("room--dragging"))
})

document.addEventListener("dragover", event => {
  const list = dragged?.parentElement
  const over = event.target.closest?.(`${LIST} > [data-room-id]`)
  if (!list || !over || over.parentElement !== list) return
  event.preventDefault()
  event.dataTransfer.dropEffect = "move"
  if (over === dragged) return
  const { top, height } = over.getBoundingClientRect()
  list.insertBefore(dragged, event.clientY < top + height / 2 ? over : over.nextElementSibling)
  renumber(list)
})

document.addEventListener("drop", event => {
  if (dragged && event.target.closest?.(LIST)) event.preventDefault()
})

document.addEventListener("dragend", () => {
  if (!dragged) return
  const list = dragged.parentElement
  dragged.classList.remove("room--dragging")
  dragged = null
  save(list)
})

document.addEventListener("keydown", event => {
  if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return
  const item = document.activeElement?.closest?.(`${LIST} > [data-room-id]`)
  if (!item) return
  event.preventDefault()
  const sibling = event.key === "ArrowUp" ? item.previousElementSibling : item.nextElementSibling
  if (!sibling?.dataset.roomId) return
  item.parentElement.insertBefore(item, event.key === "ArrowUp" ? sibling : sibling.nextElementSibling)
  renumber(item.parentElement)
  item.focus()
  save(item.parentElement)
})
