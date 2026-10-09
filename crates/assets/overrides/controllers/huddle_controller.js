import { Controller } from "@hotwired/stimulus"
import { huddle, huddles } from "lib/huddle/huddle"

// The room's huddle button: start one, join the one going on (showing who's in it), or leave it.
export default class extends Controller {
  static targets = [ "people", "label" ]
  static values = { room: String, name: String }

  connect() {
    huddle.addEventListener("change", this.render)
    this.render()
  }

  disconnect() {
    huddle.removeEventListener("change", this.render)
  }

  toggle() {
    if (huddle.roomId === this.roomValue) huddle.leave()
    else huddle.join(this.roomValue, this.nameValue)
  }

  render = () => {
    const here = huddle.roomId === this.roomValue
    const people = huddles.get(this.roomValue) ?? []
    this.element.classList.toggle("huddle-btn--live", people.length > 0)
    this.element.classList.toggle("huddle-btn--joined", here)
    this.element.setAttribute("aria-pressed", here)
    this.labelTarget.textContent = here ? "Leave" : people.length ? "Join" : "Huddle"
    this.element.title = here ? "Leave the huddle" : people.length ? `Join ${people.map(person => person.name).join(", ")}` : "Start a huddle"
    this.peopleTarget.replaceChildren(...people.slice(0, 3).map(person => {
      const image = document.createElement("img")
      image.src = person.avatar_url
      image.alt = ""
      return image
    }))
  }
}
