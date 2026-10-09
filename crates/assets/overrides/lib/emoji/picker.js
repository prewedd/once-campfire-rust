// The emoji picker: one popover per page, opened by the composer's emoji button and the "Add
// reaction" button, anchored to whichever opened it. Search, categories, recently used, and the
// keyboard: arrows to move, Enter to pick, Escape to close. The emoji (emoji/data.json) are
// fetched the first time it opens.

const DATA_URL = RAILS_ASSET_URL("/emoji/data.json")
const RECENTS_KEY = "emoji-picker-recents"
const MAX_RECENTS = 24
const COLUMNS = 9

let groupsRequest

export function loadEmoji() {
  groupsRequest ??= fetch(DATA_URL).then(response => response.json())
  return groupsRequest
}

class Picker {
  #element
  #search
  #body
  #tabs
  #preview
  #anchor
  #onPick
  #buttons = []
  #active = -1

  async toggle(anchor, onPick) {
    if (this.isOpenFor(anchor)) {
      this.close()
    } else {
      await this.open(anchor, onPick)
    }
  }

  isOpenFor(anchor) {
    return this.#element && !this.#element.hidden && this.#anchor === anchor
  }

  async open(anchor, onPick) {
    this.#anchor = anchor
    this.#onPick = onPick
    this.#build()
    this.#element.hidden = false
    this.#search.value = ""
    this.#position()
    anchor.setAttribute("aria-expanded", "true")
    this.#render(await loadEmoji())
    if (!this.#isTouch) this.#search.focus()
  }

  close() {
    if (!this.#element || this.#element.hidden) return
    this.#element.hidden = true
    this.#anchor?.setAttribute("aria-expanded", "false")
    this.#anchor = null
  }

  // Building

  #build() {
    // Turbo swaps the <body> on every visit: the picker goes back into the new one.
    if (this.#element) {
      if (!this.#element.isConnected) document.body.append(this.#element)
      return
    }

    this.#element = document.createElement("div")
    this.#element.className = "emoji-picker"
    this.#element.setAttribute("role", "dialog")
    this.#element.setAttribute("aria-label", "Emoji")
    this.#element.hidden = true
    this.#element.innerHTML = `
      <div class="emoji-picker__header">
        <input type="search" class="emoji-picker__search" placeholder="Search emoji" aria-label="Search emoji" autocomplete="off" spellcheck="false">
      </div>
      <nav class="emoji-picker__tabs" aria-label="Emoji categories"></nav>
      <div class="emoji-picker__body" tabindex="-1"></div>
      <div class="emoji-picker__preview" aria-hidden="true"><span class="emoji-picker__preview-char"></span><span class="emoji-picker__preview-text"></span></div>`
    document.body.append(this.#element)

    this.#search = this.#element.querySelector(".emoji-picker__search")
    this.#body = this.#element.querySelector(".emoji-picker__body")
    this.#tabs = this.#element.querySelector(".emoji-picker__tabs")
    this.#preview = this.#element.querySelector(".emoji-picker__preview")

    this.#search.addEventListener("input", () => this.#render())
    this.#element.addEventListener("keydown", this.#keydown)
    this.#body.addEventListener("click", this.#click)
    this.#body.addEventListener("pointerover", this.#hover)
    this.#tabs.addEventListener("click", this.#jump)
    document.addEventListener("pointerdown", this.#outside, true)
    document.addEventListener("turbo:before-visit", () => this.close())
    document.addEventListener("turbo:before-cache", () => this.close())
    window.addEventListener("resize", () => this.close())
  }

  async #render(groups) {
    groups ??= await loadEmoji()
    const query = this.#search.value.trim().toLowerCase()
    const sections = query ? [ { name: "Search results", emoji: this.#matching(groups, query) } ] : this.#browse(groups)

    this.#tabs.hidden = !!query
    this.#tabs.innerHTML = query ? "" : sections.map((section, index) =>
      `<button type="button" class="emoji-picker__tab" data-section="${index}" title="${section.name}" aria-label="${section.name}">${section.icon}</button>`).join("")

    this.#body.innerHTML = sections.map((section, index) => `
      <section class="emoji-picker__section" data-section="${index}">
        <h4 class="emoji-picker__section-name">${section.name}</h4>
        <div class="emoji-picker__grid" role="grid">${section.emoji.map(emojiButton).join("")}</div>
      </section>`).join("") || ""

    if (query && !sections[0].emoji.length) {
      this.#body.innerHTML = `<p class="emoji-picker__empty">No emoji match “${escapeHTML(query)}”</p>`
    }

    this.#buttons = Array.from(this.#body.querySelectorAll(".emoji-picker__emoji"))
    this.#body.scrollTop = 0
    this.#activate(query && this.#buttons.length ? 0 : -1)
  }

  #browse(groups) {
    const recents = this.#recents.map(char => findEmoji(groups, char)).filter(Boolean)
    const sections = groups.map(group => ({ name: group.name, icon: group.icon, emoji: group.emoji }))
    return recents.length ? [ { name: "Recently used", icon: "🕘", emoji: recents }, ...sections ] : sections
  }

  #matching(groups, query) {
    const terms = query.split(/\s+/)
    const results = []
    for (const group of groups) {
      for (const emoji of group.emoji) {
        if (terms.every(term => emoji[3].includes(term) || emoji[2].includes(term))) results.push(emoji)
      }
    }
    return results.sort((a, b) => rank(a, query) - rank(b, query))
  }

  // Positioning: above the anchor when there's room, else below; inside the viewport. Phones get
  // a sheet along the bottom.

  #position() {
    const element = this.#element
    if (window.innerWidth < 600) {
      element.classList.add("emoji-picker--sheet")
      element.style.inset = ""
      return
    }
    element.classList.remove("emoji-picker--sheet")
    const anchor = this.#anchor.getBoundingClientRect()
    const width = element.offsetWidth
    const height = element.offsetHeight
    const gap = 8
    const above = anchor.top - height - gap
    const top = above >= gap ? above : Math.min(anchor.bottom + gap, window.innerHeight - height - gap)
    const left = Math.min(Math.max(gap, anchor.right - width), window.innerWidth - width - gap)
    element.style.inset = `${Math.max(gap, top)}px auto auto ${left}px`
  }

  // Picking

  #pick(button) {
    const char = button.dataset.emoji
    this.#remember(char)
    const onPick = this.#onPick
    this.close()
    onPick?.(char)
  }

  #click = (event) => {
    const button = event.target.closest(".emoji-picker__emoji")
    if (button) this.#pick(button)
  }

  #hover = (event) => {
    const button = event.target.closest(".emoji-picker__emoji")
    if (button) this.#activate(this.#buttons.indexOf(button), { scroll: false })
  }

  #jump = (event) => {
    const tab = event.target.closest(".emoji-picker__tab")
    if (!tab) return
    const section = this.#body.querySelector(`.emoji-picker__section[data-section="${tab.dataset.section}"]`)
    this.#body.scrollTo({ top: section.offsetTop - this.#body.offsetTop })
  }

  #outside = (event) => {
    if (!this.#element || this.#element.hidden) return
    if (this.#element.contains(event.target) || this.#anchor?.contains(event.target)) return
    this.close()
  }

  #keydown = (event) => {
    const moves = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: COLUMNS, ArrowUp: -COLUMNS }
    if (event.key === "Escape") {
      event.preventDefault()
      event.stopPropagation()
      const anchor = this.#anchor
      this.close()
      anchor?.focus()
    } else if (event.key === "Enter" && this.#active >= 0) {
      event.preventDefault()
      this.#pick(this.#buttons[this.#active])
    } else if (moves[event.key] && this.#buttons.length) {
      if (event.target === this.#search && (event.key === "ArrowLeft" || event.key === "ArrowRight")) return
      event.preventDefault()
      const next = this.#active < 0 ? 0 : this.#active + moves[event.key]
      this.#activate(Math.max(0, Math.min(this.#buttons.length - 1, next)))
    }
  }

  #activate(index, { scroll = true } = {}) {
    this.#buttons[this.#active]?.classList.remove("emoji-picker__emoji--active")
    this.#active = index
    const button = this.#buttons[index]
    const [ char, text ] = button ? [ button.dataset.emoji, `:${button.dataset.code || button.title}:` ] : [ "", "" ]
    this.#preview.querySelector(".emoji-picker__preview-char").textContent = char
    this.#preview.querySelector(".emoji-picker__preview-text").textContent = button ? text : "Pick an emoji"
    if (!button) return
    button.classList.add("emoji-picker__emoji--active")
    if (scroll) button.scrollIntoView({ block: "nearest" })
  }

  // Recently used

  get #recents() {
    try {
      return JSON.parse(localStorage.getItem(RECENTS_KEY)) || []
    } catch {
      return []
    }
  }

  #remember(char) {
    try {
      const recents = [ char, ...this.#recents.filter(recent => recent !== char) ].slice(0, MAX_RECENTS)
      localStorage.setItem(RECENTS_KEY, JSON.stringify(recents))
    } catch {
      // Private windows and blocked storage: the picker works without recents
    }
  }

  get #isTouch() {
    return window.matchMedia("(pointer: coarse)").matches
  }
}

function emojiButton([ char, label, codes ]) {
  const code = codes.split(" ")[0]
  return `<button type="button" class="emoji-picker__emoji" data-emoji="${char}" data-code="${escapeHTML(code)}" title="${escapeHTML(label)}" aria-label="${escapeHTML(label)}">${char}</button>`
}

function findEmoji(groups, char) {
  for (const group of groups) {
    const emoji = group.emoji.find(([ candidate ]) => candidate === char)
    if (emoji) return emoji
  }
}

// An exact shortcode first, then shortcodes that start with the query, then labels, then the rest.
function rank([ , label, codes ], query) {
  const shortcodes = codes.split(" ")
  if (shortcodes.includes(query)) return 0
  if (shortcodes.some(code => code.startsWith(query))) return 1
  if (label.toLowerCase().startsWith(query)) return 2
  return 3
}

function escapeHTML(text) {
  return text.replace(/[&<>"']/g, char => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[char])
}

export const picker = new Picker()
