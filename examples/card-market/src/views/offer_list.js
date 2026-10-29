// @ts-check
// OfferList (offer_list.rs).

/**
 * "Show all": moves the server-rendered rows out of the template into the table.
 * @param {HTMLTableSectionElement} rows @param {HTMLTemplateElement} rest @param {HTMLButtonElement} button
 */
export function showAll(rows, rest, button) {
  rows.append(rest.content);
  rest.remove();
  button.remove();
}
