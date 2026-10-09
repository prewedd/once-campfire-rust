import { Controller } from "@hotwired/stimulus"

// The theme and light/dark mode, per device. layouts/application applies them before the page
// paints; this changes them.
export default class extends Controller {
  static targets = [ "mode", "theme" ]

  connect() {
    this.#reflect()
    document.addEventListener("click", this.#outside)
  }

  disconnect() {
    document.removeEventListener("click", this.#outside)
  }

  mode({ params: { mode } }) {
    this.#store("ws-mode", "mode", mode)
  }

  theme({ params: { name } }) {
    this.#store("ws-theme", "theme", name)
  }

  close() {
    this.element.open = false
  }

  #outside = (event) => {
    if (this.element.open && !this.element.contains(event.target)) this.close()
  }

  #store(key, attribute, value) {
    const root = document.documentElement
    if (value) root.dataset[attribute] = value
    else delete root.dataset[attribute]
    try {
      if (value) localStorage.setItem(key, value)
      else localStorage.removeItem(key)
    } catch {
      // Private windows: the choice lasts for this page only
    }
    this.#reflect()
  }

  #reflect() {
    const { mode = "", theme = "" } = document.documentElement.dataset
    this.modeTargets.forEach(button => button.setAttribute("aria-checked", button.dataset.value === mode))
    this.themeTargets.forEach(button => button.setAttribute("aria-checked", button.dataset.value === theme))
  }
}
