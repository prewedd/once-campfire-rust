import { Turbo } from "@hotwired/turbo-rails"

// Settings and the other "a page that's just a card" screens (plans/redesign.md) open in a modal
// over the conversation, as in Slack, rather than as white pages of their own. The modal shows the
// page's own content, fetched as usual, so every screen keeps working: its forms are submitted from
// here, a redirect to another such screen stays in the modal, and anything else navigates.
// A modified click (new tab) still opens the real page.
const MODAL_PATH = /^\/(channels|rooms\/\d+\/pins|account(\/.*)?|users\/me\/profile(\/.*)?|rooms\/\d+\/settings|rooms\/(opens|closeds|directs)\/(new|\d+\/edit)|qr_code\/[^/]+)$/
const TITLES = [
  [ /^\/channels/, "Browse channels" ],
  [ /^\/rooms\/\d+\/pins/, "Pinned messages" ],
  [ /^\/users\/me\/profile/, "Your profile" ],
  [ /^\/account\/edit/, "Workspace settings" ],
  [ /^\/account\/bots\/new/, "Add a bot" ],
  [ /^\/account\/bots\/\d+\/edit/, "Edit bot" ],
  [ /^\/account\/bots/, "Bots" ],
  [ /^\/account\/custom_styles/, "Custom styles" ],
  [ /^\/account\/users/, "People" ],
  [ /^\/rooms\/(\d+\/settings|(opens|closeds)\/\d+\/edit)/, "Channel settings" ],
  [ /^\/rooms\/directs\/\d+\/edit/, "Conversation settings" ],
  [ /^\/rooms\/directs\/new/, "New message" ],
  [ /^\/rooms\/(opens|closeds)\/new/, "New channel" ],
]
const ACCEPT = "text/vnd.turbo-stream.html, text/html, application/xhtml+xml"

let dialog
let stack = []
let changed = false

export function isModalPath(url) {
  const { origin, pathname } = new URL(url, location.href)
  return origin === location.origin && MODAL_PATH.test(pathname)
}

export async function openModal(url, { push = true } = {}) {
  const modal = ensureDialog()
  if (push) stack.push(url)
  modal.querySelector(".ws-modal__back").hidden = stack.length < 2
  modal.classList.add("ws-modal--loading")
  if (!modal.open) modal.showModal()
  try {
    const response = await fetch(url, { headers: { Accept: "text/html" } })
    await show(response)
  } catch (error) {
    console.error("modal:", error)
    closeModal()
    Turbo.visit(url)
  }
}

export function closeModal() {
  if (!dialog?.open) return
  dialog.close()
}

function ensureDialog() {
  if (dialog?.isConnected) return dialog
  dialog = document.createElement("dialog")
  dialog.className = "ws-modal"
  dialog.setAttribute("aria-labelledby", "ws-modal-title")
  dialog.innerHTML = `
    <header class="ws-modal__header">
      <button type="button" class="ws-modal__icon ws-modal__back" data-modal-back hidden aria-label="Back" title="Back">‹</button>
      <h2 class="ws-modal__title" id="ws-modal-title"></h2>
      <div class="ws-modal__actions"></div>
      <button type="button" class="ws-modal__icon" data-modal-close aria-label="Close" title="Close (Esc)">✕</button>
    </header>
    <div class="ws-modal__body"></div>`
  dialog.addEventListener("click", event => {
    if (event.target === dialog) return closeModal()
    if (event.target.closest("[data-modal-close]")) return closeModal()
    if (event.target.closest("[data-modal-back]") && stack.length > 1) {
      stack.pop()
      openModal(stack[stack.length - 1], { push: false })
    }
  })
  dialog.addEventListener("submit", submit)
  dialog.addEventListener("close", () => {
    stack = []
    dialog.querySelector(".ws-modal__body").replaceChildren()
    // What changed in the modal (a name, a photo, a room's members) shows on the page behind it.
    if (changed) {
      changed = false
      Turbo.visit(location.href, { action: "replace" })
    }
  })
  document.body.append(dialog)
  return dialog
}

async function show(response) {
  const type = response.headers.get("content-type") ?? ""
  if (type.includes("turbo-stream")) {
    Turbo.renderStreamMessage(await response.text())
    dialog.classList.remove("ws-modal--loading")
    return
  }
  if (!isModalPath(response.url)) {
    changed = false // the visit below shows the change; refreshing the page behind would cancel it
    closeModal()
    return Turbo.visit(response.url)
  }

  const page = new DOMParser().parseFromString(await response.text(), "text/html")
  const main = page.querySelector("#main-content")
  const path = new URL(response.url).pathname
  const title = TITLES.find(([ pattern ]) => pattern.test(path))?.[1] ?? (main?.querySelector("h1, h2")?.textContent.trim() || page.title)
  dialog.querySelector(".ws-modal__title").textContent = title
  dialog.querySelector(".ws-modal__actions").replaceChildren(...(page.querySelector("#nav .flex-item-justify-end")?.childNodes ?? []))
  // What's typed in a permanent field (a new channel's name) survives switching between screens, as with Turbo.
  const kept = [ ...dialog.querySelectorAll(".ws-modal__body input[data-turbo-permanent][id]") ].map(input => [ input.id, input.value ])
  dialog.querySelector(".ws-modal__body").replaceChildren(...(main?.childNodes ?? []))
  kept.forEach(([ id, value ]) => { const input = dialog.querySelector(`.ws-modal__body input[data-turbo-permanent]#${CSS.escape(id)}`); if (input && value) input.value = value })
  dialog.querySelectorAll("[style*='view-transition-name']").forEach(element => element.style.removeProperty("view-transition-name"))
  if (stack[stack.length - 1] !== response.url) stack[stack.length - 1] = response.url
  dialog.classList.remove("ws-modal--loading")
  dialog.querySelector(".ws-modal__body [autofocus]")?.focus()
}

// Forms in the modal are submitted from here (Turbo would replace the page with the response).
// Those inside a Turbo frame are the frame's to submit.
async function submit(event) {
  const form = event.target
  if (form.method === "dialog" || form.closest("turbo-frame") || form.dataset.turboFrame) return
  event.preventDefault()
  const confirmation = event.submitter?.dataset.turboConfirm ?? form.dataset.turboConfirm
  if (confirmation && !confirm(confirmation)) return

  const data = new FormData(form, event.submitter)
  const get = (form.getAttribute("method") ?? "get").toLowerCase() === "get"
  const url = new URL(event.submitter?.getAttribute("formaction") ?? form.action, location.href)
  if (get) data.forEach((value, key) => url.searchParams.append(key, value))
  const token = document.querySelector('meta[name="csrf-token"]')?.content
  dialog.classList.add("ws-modal--loading")
  try {
    const response = await fetch(url, {
      method: get ? "GET" : "POST",
      body: get ? undefined : data,
      headers: { Accept: ACCEPT, ...(token ? { "X-CSRF-Token": token } : {}) },
    })
    if (!get) changed = true
    await show(response)
  } catch (error) {
    console.error("modal submit:", error)
    dialog.classList.remove("ws-modal--loading")
  }
}

// Links to those screens, anywhere on the page, open the modal.
document.addEventListener("click", event => {
  const link = event.target.closest("a[href]")
  if (!link || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
  if (link.target && link.target !== "_self") return
  if (link.hasAttribute("download") || link.dataset.turboMethod || link.dataset.turboFrame || link.dataset.turbo === "false") return
  if (isModalPath(link.href)) {
    event.preventDefault()
    openModal(link.href)
  } else if (link.closest(".ws-modal")) {
    closeModal()
  }
}, true)

document.addEventListener("turbo:before-visit", closeModal)
