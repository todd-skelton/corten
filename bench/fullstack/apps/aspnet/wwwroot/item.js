// Shared by the htmx apps (django, aspnet, spring): byte-identical copies.
// The server renders the listings cheapest first; "high to low" is the same rows reversed, which
// is the order sortListings gives. htmx does the form, "show all" and navigation (hx-boost).
document.addEventListener("click", (event) => {
  const button = event.target.closest("button.sort");
  if (!button) return;
  const rows = button.parentElement.querySelector("tbody");
  rows.append(...[...rows.rows].reverse());
  const descending = button.dataset.descending !== "true";
  button.dataset.descending = String(descending);
  button.textContent = descending ? "Price: high to low" : "Price: low to high";
});
// The browser benchmark reads this mark: the page's scripts have run.
window.__hydratedAt = performance.now();
