// @ts-check
import { island } from "corten";

// The behavior of <ClickCounter /> in views.rs. Each counter keeps its own state.
export default island("click-counter", {
  state: () => ({ count: 0 }),
  actions: {
    count({ state, refs }) {
      state.count += 1;
      refs.total.value = String(state.count);
    },
  },
});
