import { Controller } from "@hotwired/stimulus"
import { openMenu } from "lib/workspace/schedule"

// The composer's clock: schedule what you've written (lib/workspace/schedule.js).
export default class extends Controller {
  static values = { room: String, thread: String }

  open(event) {
    event.preventDefault()
    openMenu(this.element, { roomId: this.roomValue, threadId: this.threadValue || null })
  }
}
