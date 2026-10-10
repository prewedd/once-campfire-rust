// The "New" line (plans/redesign.md): opening an unread room, a red line above the first message
// from someone else since you were last in it, as Slack's. The server says when that was
// (#ws-last-seen); it stays until you leave the room.
export function showNewLine() {
  const marker = document.querySelector("#ws-last-seen")
  if (!marker || marker.dataset.drawn) return
  const since = Number(marker.dataset.at)
  const me = String(window.Current?.user?.id)
  const first = [ ...document.querySelectorAll(".messages > .message[data-message-timestamp]") ]
    .find(message => Number(message.dataset.messageTimestamp) > since && message.dataset.userId !== me)
  if (!first) return
  marker.dataset.drawn = "true"
  const line = document.createElement("div")
  line.className = "ws-new-line"
  line.setAttribute("role", "separator")
  line.innerHTML = "<span>New</span>"
  first.before(line)
}
