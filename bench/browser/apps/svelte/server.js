import { render as renderSvelte } from "svelte/server";
import App from "./App.svelte";

export const render = (/** @type {{ id: number, label: string }[]} */ initial) => {
  const page = renderSvelte(App, { props: { initial } });
  return { head: page.head, html: page.body };
};
