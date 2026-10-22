import { Controller } from "@hotwired/stimulus"

// The server renders the listings cheapest first; "high to low" is the same rows reversed, which
// is the order sortListings gives.
export default class extends Controller {
  static targets = ["button", "rows"]

  toggle() {
    this.rowsTarget.append(...[...this.rowsTarget.rows].reverse())
    this.descending = !this.descending
    this.buttonTarget.textContent = this.descending ? "Price: high to low" : "Price: low to high"
  }
}
