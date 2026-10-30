// @ts-check
// DeckBuilder (deck_builder.rs). The page holds the deck it shows: a DeckLine per card, which the
// form posts. Every change (Add, Add selected, a drop, a keyboard move, a typed count, undo,
// redo) computes the next deck from the one drawn, draws it (the lines, the count, the curve, the
// kinds, the deck list, each collection row's count in the deck) and saves it. A save sends the
// form with the version the deck was built on; its answer is the save note, whose DeckAnswered
// says what became of it. The saved deck's feed says when another tab saved.
import { keep, subscribe } from "corten";
import { CollectionRow, DeckLine, rows } from "corten/views";
/** @import { FeedUrl, Message, NumberFormat } from "corten" */
/** @import { Holds } from "corten/views" */
/** @import { DeckAnswered } from "corten/events" */

/** @typedef {import("../../types/app").Slot} Slot Generated from `Slot` in src/app.rs. */

/** A deck: copies by card id, no zeros. @typedef {Map<string, number>} Deck */
/** One change, as what it did to each card: undo does the opposite. @typedef {[string, number][]} Change */

/**
 * What drawing and saving the deck touch. The markup builds it once, `js::Parts { .. }`.
 * @typedef {object} Parts
 * @property {Holds<CollectionRow, HTMLTableSectionElement>} cards
 * @property {Holds<DeckLine, HTMLUListElement>} lines
 * @property {Holds<DeckLine, HTMLTemplateElement>} template
 * @property {HTMLFormElement} form
 * @property {HTMLInputElement} version
 * @property {HTMLOutputElement} total
 * @property {Map<string, SVGElement>} bars
 * @property {Map<string, HTMLSpanElement>} tallies
 * @property {Map<string, HTMLOutputElement>} kinds
 * @property {HTMLButtonElement} undo
 * @property {HTMLButtonElement} redo
 * @property {HTMLDivElement} status
 * @property {HTMLDivElement} conflict
 * @property {HTMLTextAreaElement} list
 * @property {HTMLParagraphElement} empty
 * @property {NumberFormat} number
 */

/**
 * What the collection's filter and selection touch, and the filter's count in the catalog's
 * words. The markup builds it once, `js::Browse { .. }`.
 * @typedef {object} Browse
 * @property {Holds<CollectionRow, HTMLTableSectionElement>} cards
 * @property {HTMLInputElement} all
 * @property {HTMLOutputElement} picked
 * @property {HTMLOutputElement} shown
 * @property {HTMLParagraphElement} none
 * @property {Message<"shown" | "total">} found
 * @property {NumberFormat} number
 */

/**
 * What a move touches: the two places a card goes, the region that says what a keyboard move
 * did, and its words. The markup builds it once, `js::Carry { .. }`.
 * @typedef {object} Carry
 * @property {HTMLElement} cards_zone
 * @property {HTMLElement} deck_zone
 * @property {HTMLParagraphElement} announce
 * @property {Message<"count">} several
 * @property {Message<"what">} lifted
 * @property {Message<"what">} added
 * @property {Message<"what">} removed
 * @property {Message<"what">} put_back
 * @property {string} over_deck
 * @property {string} over_cards
 */

/**
 * Cards being moved: where from, which, where they are over now, and, for a keyboard move, the
 * grip to give focus back to.
 * @typedef {{ from: "cards" | "deck", ids: string[], over: "cards" | "deck", back: HTMLButtonElement | null }} Moving
 */

/**
 * What the builder keeps between events. One object, since a component has one state.
 * `drawn` is the deck the page shows; `base` the deck saved at `version`, which the last save the
 * server took (or the feed) said; `history` the undo and redo stacks, and `kept` them as they
 * were at that save, which a refused save goes back to.
 */
const kept = () =>
  keep(() => ({
    /** @type {Deck} */ drawn: new Map(),
    /** @type {Deck} */ base: new Map(),
    version: 0,
    /** A save is on its way. */
    sending: false,
    /** Decks sent whose answer never came (a lost connection): one may have been saved. @type {Deck[]} */
    unanswered: /** @type {Deck[]} */ ([]),
    /** Another tab saved first: nothing is sent until the player keeps both or takes theirs. */
    conflict: false,
    /** A save was stale and the feed has not yet brought the deck that made it so. */
    waiting: false,
    /** The saved deck as its feed says, and its version. @type {Deck} */
    theirs: new Map(),
    theirVersion: 0,
    /** @type {Change[]} */ undo: [],
    /** @type {Change[]} */ redo: [],
    /** @type {{ undo: Change[], redo: Change[] }} */ saved: { undo: [], redo: [] },
    /** @type {Moving | null} */ moving: null,
  }));

/** @typedef {ReturnType<typeof kept>} State */

/** Card names in the page's language's order. */
const names = new Intl.Collator(document.documentElement.lang || undefined);

/**
 * Reads the deck the server rendered, and follows the saved deck until the builder leaves the
 * page: another tab's save arrives as a patch.
 * @param {Parts} parts @param {FeedUrl<Slot>} feed @param {AbortSignal} signal
 */
export function start(parts, feed, signal) {
  const s = kept();
  s.version = Number(parts.version.value);
  s.drawn = typedDeck(parts);
  s.base = new Map(s.drawn);
  s.theirs = new Map(s.drawn);
  s.theirVersion = s.version;
  subscribe(
    feed,
    {
      snapshot: (slots, seq) => {
        s.theirs = new Map(slots.map((slot) => [slot.id, slot.count]));
        s.theirVersion = seq;
        heard(s, parts);
      },
      patch: (changes, seq) => {
        for (const change of changes) {
          if (change.op === "upsert") s.theirs.set(change.id, change.value.count);
          else s.theirs.delete(change.id);
        }
        s.theirVersion = seq;
        heard(s, parts);
      },
    },
    { signal },
  );
  // Undo and redo wherever focus is on the page, except in a field, which has its own undo.
  document.addEventListener("keydown", (event) => shortcut(s, parts, event), { signal });
  // Back online: a save that could not be sent goes now.
  addEventListener("online", () => parts.status.dataset.state === "offline" && save(s, parts), { signal });
  // The runtime could not send the form, or the server failed: nothing is lost, the deck stays
  // as shown and is sent again with the next change, the Try again button, or when back online.
  parts.form.addEventListener(
    "corten:error",
    () => {
      s.sending = false;
      say(parts, "offline");
    },
    { signal },
  );
}

/**
 * The deck as its count inputs hold it, a cleared one as no copies.
 * @param {Parts} parts
 */
function typedDeck(parts) {
  /** @type {Deck} */
  const deck = new Map();
  for (const line of rows(parts.lines, DeckLine)) {
    const n = Math.max(0, Math.trunc(line.count) || 0);
    if (n > 0) deck.set(line.id, (deck.get(line.id) ?? 0) + n);
  }
  return deck;
}

/** Whether two decks hold the same cards. @param {Deck} a @param {Deck} b */
function same(a, b) {
  if (a.size !== b.size) return false;
  for (const [id, n] of a) if (b.get(id) !== n) return false;
  return true;
}

/** What turns `from` into `to`. @param {Deck} from @param {Deck} to @returns {Change} */
function diff(from, to) {
  /** @type {Change} */
  const change = [];
  for (const [id, n] of to) if (from.get(id) !== n) change.push([id, n - (from.get(id) ?? 0)]);
  for (const [id, n] of from) if (!to.has(id)) change.push([id, -n]);
  return change;
}

/** `deck` after `change`, never below no copies. @param {Deck} deck @param {Change} change */
function applied(deck, change) {
  const next = new Map(deck);
  for (const [id, by] of change) {
    const n = Math.max(0, (next.get(id) ?? 0) + by);
    if (n > 0) next.set(id, n);
    else next.delete(id);
  }
  return next;
}

/**
 * A change the player made: drawn, kept for undo (as what it did, after the floor at no copies),
 * and saved.
 * @param {State} s @param {Parts} parts @param {Change} change
 */
function change(s, parts, change) {
  const next = applied(s.drawn, change);
  const did = diff(s.drawn, next);
  if (did.length === 0) return;
  s.undo.push(did);
  s.redo.length = 0;
  draw(s, parts, next);
  save(s, parts);
}

/**
 * Shows `deck`: the lines (made, filled or removed by key, ordered by cost then name), each
 * collection row's count in the deck, the count, the curve, the kinds, the deck list, and whether
 * undo and redo have anything.
 * @param {State} s @param {Parts} parts @param {Deck} deck
 */
function draw(s, parts, deck) {
  const cards = rows(parts.cards, CollectionRow);
  for (const id of new Set([...s.drawn.keys(), ...deck.keys()])) {
    const n = deck.get(id) ?? 0;
    const row = cards.get(id);
    if (row && row.in_deck !== n) row.in_deck = n;
  }
  s.drawn = deck;
  const items = [];
  for (const [id, count] of deck) {
    const row = cards.get(id);
    if (row) items.push({ id, name: row.name, kind: row.kind, cost: row.cost, count, owned: row.owned });
  }
  items.sort((a, b) => a.cost - b.cost || names.compare(a.name, b.name));
  rows(parts.lines, DeckLine, parts.template).replace(items);

  let total = 0;
  const curve = new Array(parts.bars.size).fill(0);
  /** @type {Map<string, number>} */
  const kinds = new Map(Array.from(parts.kinds.keys(), (kind) => [kind, 0]));
  for (const item of items) {
    total += item.count;
    curve[Math.min(item.cost, curve.length - 1)] += item.count;
    kinds.set(item.kind, (kinds.get(item.kind) ?? 0) + item.count);
  }
  parts.total.value = parts.number(total);
  // The bars' scale, as the server draws it: the tallest bar, or 4 at least.
  const tallest = Math.max(4, ...curve);
  curve.forEach((n, cost) => {
    const height = Math.floor((n * 48) / tallest);
    const bar = parts.bars.get(String(cost));
    bar?.setAttribute("height", String(height));
    bar?.setAttribute("y", String(50 - height));
    const tally = parts.tallies.get(String(cost));
    if (tally) tally.textContent = parts.number(n);
  });
  for (const [kind, n] of kinds) {
    const out = parts.kinds.get(kind);
    if (out) out.value = parts.number(n);
  }
  parts.list.value = items.map((item) => `${item.count} ${item.name}`).join("\n");
  parts.empty.hidden = items.length > 0;
  parts.undo.disabled = s.undo.length === 0;
  parts.redo.disabled = s.redo.length === 0;
}

/**
 * Shows one of the status line's states (`saving`, `updated`, `offline`), or none: then the save
 * note says what the last save did.
 * @param {Parts} parts @param {"" | "saving" | "updated" | "offline"} state
 */
function say(parts, state) {
  parts.status.dataset.state = state;
}

/**
 * Sends the deck when it differs from the saved one, one save at a time: a change made while one
 * is on its way goes when its answer comes. The form holds the drawn deck; the version says what
 * it was built on.
 * @param {State} s @param {Parts} parts
 */
function save(s, parts) {
  if (s.sending || s.conflict || s.waiting) return;
  if (same(s.drawn, s.base)) return;
  s.sending = true;
  // At most a few kept: each is a deck the server may have saved without saying so.
  s.unanswered = [...s.unanswered.slice(-3), new Map(s.drawn)];
  say(parts, "saving");
  parts.form.requestSubmit();
}

/**
 * The save's answer, from the note it put on the page. Saved: the deck sent is the saved one at
 * the new version, and a change made since goes next. Refused: the page goes back to the saved
 * deck, and its history to what it was then; the note says why. Stale: another tab saved first,
 * which the feed shows (now, or when it arrives).
 * @param {DeckAnswered} event @param {Parts} parts
 */
export function answered(event, parts) {
  const s = kept();
  s.sending = false;
  const sent = s.unanswered.at(-1);
  if (event.outcome === "saved") {
    if (sent && event.version >= s.version) known(s, parts, sent, event.version);
    s.unanswered = [];
    s.saved = { undo: s.undo.slice(), redo: s.redo.slice() };
    say(parts, "");
    save(s, parts);
  } else if (event.outcome === "refused") {
    s.unanswered = [];
    s.undo = s.saved.undo.slice();
    s.redo = s.saved.redo.slice();
    draw(s, parts, new Map(s.base));
    say(parts, "");
  } else if (event.outcome === "stale") {
    s.waiting = true;
    say(parts, "");
    heard(s, parts);
  }
}

/**
 * The feed brought the saved deck at a newer version. It is one this page sent (a save whose
 * answer was lost): the page knows it now and sends what is left. Else another tab saved it:
 * with nothing unsaved here the page takes it; with a change of its own, the player chooses.
 * While a save is on its way its answer decides.
 * @param {State} s @param {Parts} parts
 */
function heard(s, parts) {
  if (s.theirVersion <= s.version || s.sending) return;
  s.waiting = false;
  if (s.unanswered.some((deck) => same(deck, s.theirs))) {
    known(s, parts, new Map(s.theirs), s.theirVersion);
    s.unanswered = [];
    say(parts, "");
    save(s, parts);
  } else if (same(s.drawn, s.base) && s.unanswered.length === 0) {
    known(s, parts, new Map(s.theirs), s.theirVersion);
    draw(s, parts, new Map(s.theirs));
    say(parts, "updated");
  } else {
    s.conflict = true;
    parts.conflict.hidden = false;
  }
}

/**
 * Keeps both: the other tab's deck with this page's changes on top of it (what each did to each
 * card since the deck both started from), then saves that.
 * @param {Parts} parts
 */
export function merge(parts) {
  const s = kept();
  const mine = diff(s.base, s.drawn);
  settle(s, parts);
  draw(s, parts, applied(s.theirs, mine));
  save(s, parts);
}

/**
 * Takes the other tab's deck, and drops this page's changes and its history, which was of a
 * deck no longer there.
 * @param {Parts} parts
 */
export function reload(parts) {
  const s = kept();
  settle(s, parts);
  s.undo = [];
  s.redo = [];
  s.saved = { undo: [], redo: [] };
  draw(s, parts, new Map(s.theirs));
  say(parts, "updated");
}

/**
 * The saved deck is `deck`, at `version`: what the next save is built on, which the form sends.
 * @param {State} s @param {Parts} parts @param {Deck} deck @param {number} version
 */
function known(s, parts, deck, version) {
  s.base = deck;
  s.version = version;
  parts.version.value = String(version);
}

/** The conflict is over: the saved deck is the feed's. @param {State} s @param {Parts} parts */
function settle(s, parts) {
  known(s, parts, new Map(s.theirs), s.theirVersion);
  s.conflict = false;
  s.waiting = false;
  s.unanswered = [];
  parts.conflict.hidden = true;
}

/** Sends the deck again, after the server could not be reached. @param {Parts} parts */
export function retry(parts) {
  const s = kept();
  say(parts, "");
  save(s, parts);
}

/** @param {Parts} parts */
export function undo(parts) {
  step(kept(), parts, true);
}

/** @param {Parts} parts */
export function redo(parts) {
  step(kept(), parts, false);
}

/**
 * Undoes the last change (or does the last undone one again), and saves that like any change.
 * @param {State} s @param {Parts} parts @param {boolean} back
 */
function step(s, parts, back) {
  const entry = (back ? s.undo : s.redo).pop();
  if (!entry) return;
  (back ? s.redo : s.undo).push(entry);
  draw(s, parts, applied(s.drawn, back ? entry.map(([id, by]) => [id, -by]) : entry));
  save(s, parts);
}

/**
 * A count typed into a deck line (its `change`, once the player leaves it or presses Enter).
 * @param {HTMLInputElement} input @param {Parts} parts
 */
export function typed(input, parts) {
  const s = kept();
  const line = rows(parts.lines, DeckLine).of(input);
  if (!line) return;
  const n = Math.max(0, Math.trunc(Number(input.value)) || 0);
  change(s, parts, [[line.id, n - (s.drawn.get(line.id) ?? 0)]]);
}

/**
 * A collection row's Add button: one more copy, with no round trip (the button would post the
 * deck with it added).
 * @param {MouseEvent} event @param {HTMLButtonElement} button @param {Parts} parts
 */
export function add(event, button, parts) {
  const row = rows(parts.cards, CollectionRow).of(button);
  if (!row) return;
  event.preventDefault();
  change(kept(), parts, [[row.id, 1]]);
}

/**
 * Add selected: one more copy of each checked card, then the selection clears.
 * @param {MouseEvent} event @param {Parts} parts @param {Browse} browse
 */
export function addPicked(event, parts, browse) {
  event.preventDefault();
  const ids = picks(browse.cards);
  if (ids.length === 0) return;
  change(kept(), parts, ids.map((id) => [id, 1]));
  clearPicks(browse);
}

/** The checked cards' ids. @param {Holds<CollectionRow, HTMLTableSectionElement>} cards */
function picks(cards) {
  const ids = [];
  for (const row of rows(cards, CollectionRow)) if (row.selected) ids.push(row.id);
  return ids;
}

// Moves: a pointer drags a grip (HTML drag and drop), a keyboard picks one up with Space. Both
// end in the same change: cards from the collection add a copy each to the deck, a line from the
// deck dropped on the collection leaves the deck.

/**
 * A collection row's grip starts a drag: the row, or every checked row when it is one of them.
 * @param {DragEvent} event @param {HTMLButtonElement} grip @param {Parts} parts @param {Carry} carry
 */
export function lift(event, grip, parts, carry) {
  const row = rows(parts.cards, CollectionRow).of(grip);
  if (!row) return;
  const ids = row.selected ? picks(parts.cards) : [row.id];
  kept().moving = { from: "cards", ids, over: "cards", back: null };
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("text/plain", ids.map((id) => cardName(parts, id)).join("\n"));
  }
  carry.cards_zone.dataset.from = "";
}

/**
 * A deck line's grip starts a drag, out of the deck.
 * @param {DragEvent} event @param {HTMLButtonElement} grip @param {Parts} parts @param {Carry} carry
 */
export function liftLine(event, grip, parts, carry) {
  const line = rows(parts.lines, DeckLine).of(grip);
  if (!line) return;
  kept().moving = { from: "deck", ids: [line.id], over: "deck", back: null };
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", `${line.count} ${line.name}`);
  }
  carry.deck_zone.dataset.from = "";
}

/** A drag over the deck: it takes cards from the collection. @param {DragEvent} event @param {Carry} carry */
export function overDeck(event, carry) {
  over(event, carry.deck_zone, "cards");
}

/** A drag over the collection: it takes lines from the deck. @param {DragEvent} event @param {Carry} carry */
export function overCards(event, carry) {
  over(event, carry.cards_zone, "deck");
}

/**
 * Lets the drop happen (and marks the place) when the cards come from the other place.
 * @param {DragEvent} event @param {HTMLElement} zone @param {"cards" | "deck"} takes
 */
function over(event, zone, takes) {
  const moving = kept().moving;
  if (moving?.from !== takes) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = takes === "cards" ? "copy" : "move";
  zone.dataset.over = "";
}

/** The drag left a place (not for an element inside it). @param {DragEvent} event @param {HTMLElement} zone */
export function leave(event, zone) {
  if (!zone.contains(/** @type {Node | null} */ (event.relatedTarget))) delete zone.dataset.over;
}

/** @param {DragEvent} event @param {Parts} parts @param {Carry} carry */
export function dropOnDeck(event, parts, carry) {
  event.preventDefault();
  drop(kept(), parts, carry, "deck");
}

/** @param {DragEvent} event @param {Parts} parts @param {Carry} carry */
export function dropOnCards(event, parts, carry) {
  event.preventDefault();
  drop(kept(), parts, carry, "cards");
}

/** The drag ended, dropped or not. @param {Carry} carry */
export function dropped(carry) {
  end(kept(), carry);
}

/**
 * Drops the cards being moved on `on`: from the collection onto the deck adds a copy of each;
 * from the deck onto the collection takes them out. Says so when it was a keyboard move.
 * @param {State} s @param {Parts} parts @param {Carry} carry @param {"cards" | "deck"} on
 */
function drop(s, parts, carry, on) {
  const moving = s.moving;
  if (!moving) return;
  end(s, carry);
  const what = whatMoves(parts, carry, moving.ids);
  if (moving.from === "cards" && on === "deck") {
    change(s, parts, moving.ids.map((id) => [id, 1]));
    if (moving.back) carry.announce.textContent = carry.added({ what });
  } else if (moving.from === "deck" && on === "cards") {
    change(s, parts, moving.ids.map((id) => [id, -(s.drawn.get(id) ?? 0)]));
    if (moving.back) carry.announce.textContent = carry.removed({ what });
  } else if (moving.back) {
    carry.announce.textContent = carry.put_back({ what });
  }
  // Focus goes back where the move started, or to the place it ended when that grip has gone
  // (a deck line taken out).
  if (moving.back) (moving.back.isConnected ? moving.back : on === "deck" ? carry.deck_zone : carry.cards_zone).focus();
}

/** No move: the places lose their marks. @param {State} s @param {Carry} carry */
function end(s, carry) {
  s.moving = null;
  for (const zone of [carry.cards_zone, carry.deck_zone]) {
    delete zone.dataset.over;
    delete zone.dataset.from;
  }
}

/**
 * Space or Enter on a collection row's grip picks the card up (or every checked card, when it is
 * one of them) and takes it over the deck.
 * @param {KeyboardEvent} event @param {HTMLButtonElement} grip @param {Parts} parts @param {Carry} carry
 */
export function pickUp(event, grip, parts, carry) {
  const row = rows(parts.cards, CollectionRow).of(grip);
  if (!row || !pressed(event) || kept().moving) return;
  event.preventDefault();
  carryFrom(kept(), parts, carry, { from: "cards", ids: row.selected ? picks(parts.cards) : [row.id], over: "deck", back: grip });
}

/**
 * Space or Enter on a deck line's grip picks the card up and takes it over the collection.
 * @param {KeyboardEvent} event @param {HTMLButtonElement} grip @param {Parts} parts @param {Carry} carry
 */
export function pickUpLine(event, grip, parts, carry) {
  const line = rows(parts.lines, DeckLine).of(grip);
  if (!line || !pressed(event) || kept().moving) return;
  event.preventDefault();
  carryFrom(kept(), parts, carry, { from: "deck", ids: [line.id], over: "cards", back: grip });
}

/** Space or Enter, with no modifier. @param {KeyboardEvent} event */
function pressed(event) {
  return (event.key === " " || event.key === "Enter") && !event.ctrlKey && !event.metaKey && !event.altKey;
}

/**
 * A keyboard move begins: focus goes to the place the cards are over, which says so.
 * @param {State} s @param {Parts} parts @param {Carry} carry @param {Moving} moving
 */
function carryFrom(s, parts, carry, moving) {
  s.moving = moving;
  const zone = moving.over === "deck" ? carry.deck_zone : carry.cards_zone;
  zone.dataset.over = "";
  (moving.from === "deck" ? carry.deck_zone : carry.cards_zone).dataset.from = "";
  zone.focus();
  const what = whatMoves(parts, carry, moving.ids);
  carry.announce.textContent = `${carry.lifted({ what })} ${moving.over === "deck" ? carry.over_deck : carry.over_cards}`;
}

/**
 * The keys of a keyboard move: the arrow keys move the cards between the collection and the deck,
 * Space or Enter drops them, Escape puts them back.
 * @param {KeyboardEvent} event @param {Parts} parts @param {Carry} carry
 */
export function keys(event, parts, carry) {
  const s = kept();
  if (event.defaultPrevented) return;
  const moving = s.moving;
  if (moving?.back) {
    if (event.key.startsWith("Arrow")) {
      event.preventDefault();
      moving.over = moving.over === "deck" ? "cards" : "deck";
      const [now, was] = moving.over === "deck" ? [carry.deck_zone, carry.cards_zone] : [carry.cards_zone, carry.deck_zone];
      delete was.dataset.over;
      now.dataset.over = "";
      now.focus();
      carry.announce.textContent = moving.over === "deck" ? carry.over_deck : carry.over_cards;
    } else if (pressed(event)) {
      event.preventDefault();
      drop(s, parts, carry, moving.over);
    } else if (event.key === "Escape") {
      event.preventDefault();
      drop(s, parts, carry, moving.from);
    }
  }
}

/**
 * Ctrl+Z (Cmd+Z) undoes, Ctrl+Shift+Z or Ctrl+Y redoes, except in a field, which has its own undo,
 * or during a keyboard move.
 * @param {State} s @param {Parts} parts @param {KeyboardEvent} event
 */
function shortcut(s, parts, event) {
  const field = event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement;
  if (event.defaultPrevented || field || s.moving || !(event.ctrlKey || event.metaKey) || event.altKey) return;
  const key = event.key.toLowerCase();
  if (key === "z" || key === "y") {
    event.preventDefault();
    step(s, parts, key === "z" && !event.shiftKey);
  }
}

/** A card's name, from its collection row. @param {Parts} parts @param {string} id */
function cardName(parts, id) {
  return rows(parts.cards, CollectionRow).get(id)?.name ?? id;
}

/** What is moving, in words: one card's name, or how many. @param {Parts} parts @param {Carry} carry @param {string[]} ids */
function whatMoves(parts, carry, ids) {
  return ids.length === 1 ? cardName(parts, ids[0]) : carry.several({ count: parts.number(ids.length) });
}

// The collection: filter, sort and selection, all in the page.

/** Lowercased name, set and kind, read once per row. @type {WeakMap<Element, string>} */
const texts = new WeakMap();

/**
 * Shows the rows whose name, set or kind has the filter's text, and how many of them.
 * @param {HTMLInputElement} input @param {Browse} browse
 */
export function filter(input, browse) {
  const query = input.value.trim().toLocaleLowerCase();
  let shown = 0;
  let total = 0;
  for (const row of rows(browse.cards, CollectionRow)) {
    let text = texts.get(row.el);
    if (text === undefined) texts.set(row.el, (text = `${row.name} ${row.set} ${row.kind}`.toLocaleLowerCase()));
    row.hidden = query !== "" && !text.includes(query);
    total += 1;
    if (!row.hidden) shown += 1;
  }
  browse.none.hidden = shown > 0;
  browse.shown.value = query ? browse.found({ shown: browse.number(shown), total: browse.number(total) }) : "";
  browse.shown.hidden = !query;
  tally(browse);
}

/** With JavaScript the filter applies as it is typed: Enter sends nothing. @param {SubmitEvent} event */
export function stay(event) {
  event.preventDefault();
}

/** @param {HTMLInputElement} box @param {Browse} browse */
export function pick(box, browse) {
  const row = rows(browse.cards, CollectionRow).of(box);
  if (!row) return;
  row.selected = box.checked;
  tally(browse);
}

/** Checks (or clears) every row shown. @param {Browse} browse */
export function pickAll(browse) {
  for (const row of rows(browse.cards, CollectionRow)) if (!row.hidden) row.selected = browse.all.checked;
  tally(browse);
}

/** @param {Browse} browse */
export function clearPicks(browse) {
  for (const row of rows(browse.cards, CollectionRow)) if (row.selected) row.selected = false;
  tally(browse);
}

/**
 * The count of checked rows, and the select-all box: checked when every row shown is,
 * indeterminate when some are.
 * @param {Browse} browse
 */
function tally({ cards, all, picked, number }) {
  let n = 0;
  let visible = 0;
  let visibleChecked = 0;
  for (const row of rows(cards, CollectionRow)) {
    if (row.selected) n += 1;
    if (!row.hidden) {
      visible += 1;
      if (row.selected) visibleChecked += 1;
    }
  }
  picked.value = number(n);
  all.checked = visible > 0 && visibleChecked === visible;
  all.indeterminate = visibleChecked > 0 && visibleChecked < visible;
}

// One handler per column, so the markup names a column the compiler knows. The header is a link
// to the same order from the server, for a browser without JavaScript.

/** @param {MouseEvent} event @param {HTMLAnchorElement} link @param {Holds<CollectionRow, HTMLTableSectionElement>} cards */
export function sortByName(event, link, cards) {
  sort(event, link, cards, () => 0, "ascending");
}

/** @param {MouseEvent} event @param {HTMLAnchorElement} link @param {Holds<CollectionRow, HTMLTableSectionElement>} cards */
export function sortByCost(event, link, cards) {
  sort(event, link, cards, (row) => row.cost, "ascending");
}

/** @param {MouseEvent} event @param {HTMLAnchorElement} link @param {Holds<CollectionRow, HTMLTableSectionElement>} cards */
export function sortByOwned(event, link, cards) {
  sort(event, link, cards, (row) => row.owned, "descending");
}

/**
 * Sorts the rows by a number (the name alone when it is the same for all), then by name, in the
 * order the server's link gives first, and the other way on the next click. Each row's keys are
 * read once, not at every comparison. A click that opens the link elsewhere is left to the browser.
 * @param {MouseEvent} event @param {HTMLAnchorElement} link
 * @param {Holds<CollectionRow, HTMLTableSectionElement>} cards @param {(row: CollectionRow) => number} by
 * @param {"ascending" | "descending"} first
 */
function sort(event, link, cards, by, first) {
  if (event.ctrlKey || event.metaKey || event.shiftKey || event.button !== 0) return;
  event.preventDefault();
  const th = /** @type {HTMLTableCellElement} */ (link.closest("th"));
  const order = th.getAttribute("aria-sort") !== first ? first : first === "ascending" ? "descending" : "ascending";
  for (const other of Array.from(th.parentElement?.children ?? [])) other.removeAttribute("aria-sort");
  th.setAttribute("aria-sort", order);
  const dir = order === "ascending" ? 1 : -1;
  const keyed = Array.from(rows(cards, CollectionRow), (row) => ({ el: row.el, n: by(row), name: row.name }));
  // Names go the other way only when they are the order; under a number they stay A to Z.
  const named = keyed.every((row) => row.n === keyed[0].n) ? dir : 1;
  keyed.sort((a, b) => dir * (a.n - b.n) || named * names.compare(a.name, b.name));
  cards.append(...keyed.map((row) => row.el));
}
