import { Controller } from "@hotwired/stimulus"
import { picker, loadEmoji } from "lib/emoji/picker"
import { addBoost } from "lib/emoji/boosts"

// Emoji for the composer and for reactions: the composer's emoji button inserts into its editor;
// a message's reaction buttons boost it, with a quick emoji or one picked from the picker.
export default class extends Controller {
  static values = { boostsUrl: String }

  insert(event) {
    const editor = this.element.closest("form")?.querySelector("lexxy-editor")
    if (!editor) return

    picker.toggle(event.currentTarget, (emoji) => {
      editor.focus()
      editor.contents.insertText(emoji)
    })
  }

  pickReaction(event) {
    picker.toggle(event.currentTarget, (emoji) => this.#boost(emoji))
  }

  react({ params: { content } }) {
    this.#boost(content)
  }

  preload() {
    loadEmoji()
  }

  #boost(content) {
    addBoost(this.boostsUrlValue, content)
  }
}
