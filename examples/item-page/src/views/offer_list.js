// @ts-check
import { island } from "corten";

// "Show all": moves the server-rendered rows out of the template into the table.
export default island("offer-list", {
  actions: {
    showAll({ refs, target }) {
      refs.rows.append(refs.rest.content);
      refs.rest.remove();
      target.remove();
    },
  },
});
