const AUTO_SCROLL_THRESHOLD = 100

export default class ScrollManager {
  static #pendingOperations = Promise.resolve()

  #container

  constructor(container) {
    this.#container = container
  }

  async autoscroll(forceScroll, render = () => {}) {
    return this.#appendOperation(async () => {
      const wasNearEnd = this.#scrolledNearEnd

      await render()

      if (wasNearEnd || forceScroll) {
        this.#container.scrollTop = this.#container.scrollHeight
        return true
      } else {
        return false
      }
    })
  }

  async keepScroll(top, render) {
    return this.#appendOperation(async () => {
      const scrollTop = this.#container.scrollTop
      const scrollHeight = this.#container.scrollHeight

      await render()

      if (top) {
        this.#container.scrollTop = scrollTop + (this.#container.scrollHeight - scrollHeight)
      } else {
        this.#container.scrollTop = scrollTop
      }
    })
  }

  // Private

  // One queue for every message list on the page (the room's, and a thread's in the panel). An
  // operation that fails is reported and skipped: left rejected, the queue would skip every later
  // operation, and the composer couldn't post again until a reload.
  #appendOperation(operation) {
    const result = ScrollManager.#pendingOperations.then(operation)
    ScrollManager.#pendingOperations = result.catch(error => console.error("scroll operation failed:", error))
    return result
  }

  get #scrolledNearEnd() {
    return this.#distanceScrolledFromEnd <= AUTO_SCROLL_THRESHOLD
  }

  get #distanceScrolledFromEnd() {
    return this.#container.scrollHeight - this.#container.scrollTop - this.#container.clientHeight
  }
}
