// @ts-check
// RequiredWhen (required_when.rs): the fields in the set are required while another field of the
// form has one of the given values, and disabled (neither checked nor sent) while it does not.

/**
 * Applies the rule now, whenever the control changes (one joined to the form from elsewhere in the
 * page with `form=` included), and after a form reset once the fields have their first values,
 * until the set leaves the page.
 * @param {HTMLFieldSetElement} set @param {string} control @param {string[]} values @param {AbortSignal} signal
 */
export function connect(set, control, values, signal) {
  const form = set.form;
  if (!form) return;
  const apply = () => {
    const on = chosen(form, control).some((value) => values.includes(value));
    set.disabled = !on;
    // A checkbox's `required` means that box must be checked, so boxes are left as they are.
    for (const field of Array.from(set.querySelectorAll("input:not([type=checkbox]), select, textarea"))) {
      /** @type {HTMLInputElement} */ (field).required = on;
    }
  };
  /** @param {Event} e */
  const changed = (e) => {
    const target = /** @type {HTMLInputElement} */ (e.target);
    if (target.form === form && target.name === control) apply();
  };
  apply();
  // On the document: a control outside the form's element (`form=`) does not bubble to it.
  document.addEventListener("change", changed, { signal });
  document.addEventListener("input", changed, { signal });
  form.addEventListener("reset", () => setTimeout(apply), { signal });
}

/**
 * The values the field `name` has chosen: a select's selected options, the checked radios or
 * boxes of that name, or a text field's value.
 * @param {HTMLFormElement} form @param {string} name @returns {string[]}
 */
function chosen(form, name) {
  const item = form.elements.namedItem(name);
  if (item instanceof HTMLSelectElement) return Array.from(item.selectedOptions, (option) => option.value);
  if (item instanceof RadioNodeList) {
    return Array.from(item)
      .filter((el) => el instanceof HTMLInputElement && el.checked)
      .map((el) => /** @type {HTMLInputElement} */ (el).value);
  }
  if (item instanceof HTMLInputElement) return /^(checkbox|radio)$/.test(item.type) ? (item.checked ? [item.value] : []) : [item.value];
  if (item instanceof HTMLTextAreaElement) return [item.value];
  return [];
}
