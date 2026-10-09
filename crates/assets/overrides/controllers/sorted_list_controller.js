import { Controller } from "@hotwired/stimulus"
import { throttle } from "helpers/timing_helpers"

export default class extends Controller {
  static targets = [ "item" ]

  itemTargetConnected(target) {
    this.#throttledSort()
  }

  updateItem({ detail: { targetId }}) {
    const itemTargetForUpdate = this.itemTargets.find(itemTarget => itemTarget.id == targetId)

    if (itemTargetForUpdate) {
      if (itemTargetForUpdate.dataset.sortedListNumber) {
        itemTargetForUpdate.dataset.sortedListNumber = new Date().getTime()
      }

      this.sort()
    }
  }

  sort() {
    const sortedItemTargets = this.itemTargets.sort((a, b) => {
      if (a.dataset.sortedListNumber) {
        return b.dataset.sortedListNumber - a.dataset.sortedListNumber
      } else {
        // Channels someone put in order (lib/workspace/reorder.js) come first, in that order.
        const [ positionA, positionB ] = [ a, b ].map(item => item.dataset.sortedListPosition ? Number(item.dataset.sortedListPosition) : Infinity)
        if (positionA !== positionB) return positionA - positionB
        return a.dataset.sortedListName.toLowerCase().localeCompare(b.dataset.sortedListName.toLowerCase())
      }
    })

    sortedItemTargets.forEach(item => this.element.appendChild(item))
  }

  #throttledSort = throttle(this.sort.bind(this))
}
