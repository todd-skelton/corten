// The module of the app's view classes. Empty here: the app's generated declarations
// (corten::types, types/app.d.ts) add one class per #[component(view)], with its holes:
//
//   import { Row } from "corten/views";
//   new Row(el).label = "Lamp";
//
// The browser gets the classes from Corten (assets/{version}/views.js, mapped by the import map).
declare module "corten/views" {}
