import { Controller } from "@hotwired/stimulus"
import { cable } from "@hotwired/turbo-rails"
import { throttle } from "helpers/timing_helpers"
import { pageIsTurboPreview } from "helpers/turbo_helpers"
import TypingTracker from "models/typing_tracker"

export default class extends Controller {
  static targets = [ "author", "indicator" ]
  static classes = [ "active" ]
  // threadId: typing in a thread is only shown in that thread (plans/threads.md)
  static values = { threadId: Number }

  async connect() {
    if (!pageIsTurboPreview()) {
      this.tracker = new TypingTracker(this.#update.bind(this))

      this.channel = await cable.subscribeTo(
        this.#channelParams,
        { received: this.#received.bind(this) }
      )
    }
  }

  disconnect() {
    this.tracker?.close()
    this.channel?.unsubscribe()
  }

  start({ target }) {
    if (target.value) {
      this.#throttledSend("start")
    } else {
      this.#send("stop")
    }
  }

  stop() {
    this.#send("stop");
  }

  #received({ action, user }) {
    if (user.id !== Current.user.id) {
      if (action === "start") {
        this.tracker.add(user.name)
      } else {
        this.tracker.remove(user.name)
      }
    }
  }

  get #channelParams() {
    const params = { channel: "TypingNotificationsChannel", room_id: Current.room.id }
    if (this.hasThreadIdValue && this.threadIdValue) params.thread_id = this.threadIdValue
    return params
  }

  #send(action) {
    this.channel.send({ action })
  }

  #update(message) {
    this.authorTarget.textContent = message
    this.indicatorTarget.classList.toggle(this.activeClass, !!message)
  }

  #throttledSend = throttle(action => this.#send(action))
}
