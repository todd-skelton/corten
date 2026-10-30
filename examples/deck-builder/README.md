# deck-builder

A deck builder: a player's collection of 240 cards on one side, the deck they build from it on the
other. It is the example of a rich client screen: everything a change does on the page happens in
the browser with no round trip, and the deck rules stay on the server. Read it when a screen keeps
state in the browser (selection, drag and drop, undo, a chart drawn from what is on the page) and
saves it as the user works.

| In the browser | How |
|---|---|
| Filter, sort and select the collection | a handler per column; the rows are `CollectionRow` views, read and written by name |
| Add a card, add the selected ones, type a count | a change: the next deck is drawn (`rows(lines, DeckLine, template).replace`) and saved |
| Drag a card onto the deck, or a line back out | HTML drag and drop on a grip; the same change |
| The same moves by keyboard | Space picks up, the arrow keys move between the collection and the deck, Space drops, Escape puts back, each announced |
| Undo and redo | the buttons, Ctrl+Z and Ctrl+Shift+Z; each step is a change, saved like any other |
| The count, the cost curve, the count by kind, the deck list | drawn again from the deck after each change (the curve is SVG bars, no `style`) |
| Autosave | the deck's form, sent one save at a time with the version it was built on; the answer is the save note, whose event says what became of it |
| A refused change | the page goes back to the saved deck, and its history to what it was then, with the rule's words |
| Another tab's edit | the page follows the saved deck's feed: with nothing unsaved it takes the edit; with a change of its own it offers to keep both or take the other tab's deck |

Without JavaScript the same deck is built with forms: Add and Add selected post the deck with the
cards added, Save posts the counts as typed, and the filter and the column headers are a `GET`
form and links the server answers.

## Run it

From this directory:

```sh
corten dev     # http://localhost:3000
```

`corten` is the framework's command (`cargo install corten-cli`).

Or in Docker, from the repository root:

```sh
docker build -f examples/deck-builder/Dockerfile -t corten-deck-builder .
docker run --rm -p 3000:8080 corten-deck-builder
```

`DECK_CARDS` sets how many cards the collection has (240). The deck is kept in memory: a restart
starts again from the starter deck.

## Where things are

The routes and what each does are in the table at the top of [`src/app.rs`](src/app.rs).

| Piece | File |
|---|---|
| The routes, the save (version, submit key, rules), the deck's feed | [`src/app.rs`](src/app.rs) |
| The collection and the deck rules, which only the server applies | [`src/cards.rs`](src/cards.rs) |
| The layout, and the screen in the order the address asks for | [`src/views.rs`](src/views.rs) |
| The builder: its markup, its script (every change, autosave, moves, the collection) and its styles | [`src/views/deck_builder.rs`](src/views/deck_builder.rs), [`src/views/deck_builder.js`](src/views/deck_builder.js), [`src/views/deck_builder.css`](src/views/deck_builder.css) |
| A collection row and a deck line | [`src/views/collection_row.rs`](src/views/collection_row.rs), [`src/views/deck_line.rs`](src/views/deck_line.rs) |
| The save note, which tells the builder a save's answer | [`src/views/save_note.rs`](src/views/save_note.rs), [`src/views/save_note.js`](src/views/save_note.js) |

Its browser test, which also times the screen: [`smoke.mjs`](smoke.mjs).