import { Controller } from "@hotwired/stimulus"
import { get } from "@rails/request.js"
import { cable } from "@hotwired/turbo-rails"
import { pageIsTurboPreview } from "helpers/turbo_helpers"

const OFFLINE_AFTER_DISCONNECTED_TIMEOUT = 5_000
const REFRESH_AFTER_HIDDEN_TIMEOUT = 60_000

// One HeartbeatChannel subscription for the page, shared by every message list on it (the room's,
// and a thread's in the right-hand panel). The server confirms a channel only once per connection,
// so a second subscription of its own would never hear "connected", and after five seconds would
// report the page offline, disabling every composer on it.
const heartbeat = { listeners: new Set(), connected: false, channel: null }

function subscribeToHeartbeat() {
  heartbeat.channel ??= cable.subscribeTo({ channel: "HeartbeatChannel" }, {
    connected() {
      heartbeat.connected = true
      heartbeat.listeners.forEach(listener => listener.heartbeatConnected())
    },
    disconnected() {
      heartbeat.connected = false
      heartbeat.listeners.forEach(listener => listener.heartbeatDisconnected())
    }
  })
  return heartbeat.channel
}

export default class extends Controller {
  static targets = [ "message" ]
  static values = { loadedAt: Number, url: String, }

  #lastLoadedAt
  #offlineTimer = null
  #hiddenAt = null

  async connect() {
    if (!pageIsTurboPreview()) {
      this.#lastLoadedAt = this.loadedAtValue
      heartbeat.listeners.add(this)
      if (heartbeat.connected) {
        this.#channelConnected()
      } else {
        this.#channelDisconnected()
      }
      this.channel = await subscribeToHeartbeat()
    }
  }

  disconnect() {
    heartbeat.listeners.delete(this)
    clearTimeout(this.#offlineTimer)
  }

  heartbeatConnected() {
    this.#channelConnected()
  }

  heartbeatDisconnected() {
    this.#channelDisconnected()
  }

  messageTargetConnected(target) {
    this.#lastLoadedAt = Math.max(this.#lastLoadedAt, target.dataset.messageUpdatedAt || 0)
  }

  visibilityChanged() {
    if (document.visibilityState === "visible") {
      if (this.#hiddenForTooLong()) {
        this.#refresh("visibility")
        this.dispatch("visible")
      }
      this.#hiddenAt = null
    } else {
      this.#hiddenAt = Date.now()
    }
  }

  online() {
    // Trigger reconnection attempt whenever the browser comes back
    // from being offline
    this.channel?.consumer.connection.monitor.visibilityDidChange()
  }

  #channelConnected() {
    this.#refresh("connection")

    clearTimeout(this.#offlineTimer)
    this.dispatch("online", { target: window })
  }

  #channelDisconnected() {
    clearTimeout(this.#offlineTimer)
    this.#offlineTimer = setTimeout(() => {
      this.dispatch("offline", { target: window })
    }, OFFLINE_AFTER_DISCONNECTED_TIMEOUT)
  }

  #refresh(reason) {
    get(this.urlValue, { query: { since: this.#lastLoadedAt, reason: reason }, responseKind: "turbo-stream" })
  }

  #hiddenForTooLong() {
    return this.#hiddenAt && Date.now() - this.#hiddenAt > REFRESH_AFTER_HIDDEN_TIMEOUT
  }
}
