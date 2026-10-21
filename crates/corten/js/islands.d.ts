// Types for island() in corten.js, kept here so the runtime the browser loads carries none.
// `tsc --checkJs` reads them; nothing serves them.

/**
 * The app's islands. Empty here: the app's generated declarations (corten::types, types/app.d.ts)
 * add one entry per #[island] component, from its Rust markup:
 * `{ props: {...}; refs: { rows: "tbody" }; actions: { showAll: { on: "click"; el: "button" } } }`.
 */
interface CortenIslands {}

/** The element type of a tag name: "template" is HTMLTemplateElement; a custom element is HTMLElement. */
type CortenElementOf<K> = K extends keyof HTMLElementTagNameMap
  ? HTMLElementTagNameMap[K]
  : K extends keyof SVGElementTagNameMap
    ? SVGElementTagNameMap[K]
    : HTMLElement;

/** The event type of an event name: "click" is a PointerEvent; a custom event is Event. */
type CortenEventOf<E> = E extends keyof HTMLElementEventMap ? HTMLElementEventMap[E] : Event;

type CortenPart<I, F extends string> = I extends { [key in F]: infer V } ? V : {};

/** What an island's `connect` and actions get. `I` is its entry in `CortenIslands`. */
interface CortenContext<I, S> {
  /** The island's element, `<offer-list>`. */
  readonly el: HTMLElement;
  /**
   * The first element with each ref (`ref={Rows}` is `refs.rows`), typed by the element the
   * markup puts it on. Reading one the page does not have throws.
   */
  readonly refs: { readonly [R in keyof CortenPart<I, "refs">]: CortenElementOf<CortenPart<I, "refs">[R]> };
  /** Every element with each ref, in document order. */
  readonly all: { readonly [R in keyof CortenPart<I, "refs">]: CortenElementOf<CortenPart<I, "refs">[R]>[] };
  /** The `#[prop]` parameters, as the server sent them. */
  readonly props: CortenPart<I, "props">;
  /** This element's own state, made by `state()` when it first connects. */
  readonly state: S;
  /** Aborted when the element leaves the page. */
  readonly signal: AbortSignal;
}

/** One function per action the markup names (`on:click={ShowAll}` is `showAll`). */
type CortenActions<I, S> = {
  [A in keyof CortenPart<I, "actions">]: (
    ctx: CortenContext<I, S> & {
      /** The element with the `on:` attribute. */
      readonly target: CortenElementOf<CortenPart<CortenPart<I, "actions">[A], "el">>;
      readonly event: CortenEventOf<CortenPart<CortenPart<I, "actions">[A], "on">>;
    },
  ) => void;
};

/**
 * `island(tag, spec)`:
 *
 *     export default island("offer-list", {
 *       state: () => ({ shown: false }),            // optional: each element's own state
 *       connect({ el, refs, props, signal }) {},    // optional: when the element joins the page;
 *                                                   // a returned function runs when it leaves
 *       actions: {                                  // one per on:event={Name} in the markup
 *         showAll({ refs, target, event, state }) {},
 *       },
 *     });
 *
 * An element moved within the page keeps its state and does not reconnect.
 */
type CortenSpec<I, S> = {
  state?: () => S;
  connect?: (ctx: CortenContext<I, S>) => void | (() => void);
} & ([keyof CortenPart<I, "actions">] extends [never] ? { actions?: {} } : { actions: CortenActions<I, S> });
