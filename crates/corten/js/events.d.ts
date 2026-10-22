// The module of the app's typed events. Empty here: the app's generated declarations
// (corten::types, types/app.d.ts) add one class per #[event] struct, with its fields:
//
//   import { RowToggled } from "corten/events";
//   row.dispatchEvent(new RowToggled({ id, selected: true }));
//
// The browser gets the classes from Corten (assets/{version}/events.js, mapped by the import map).
declare module "corten/events" {}
