// Every app must render the same visible text for the same items (status included).
// Usage: node parity.mjs [apps=all] [reference=first]  (apps running on their catalog ports)
// Elements with the `hidden` attribute are not visible text (Corten renders both sort labels and
// hides one).
import { apps } from "./apps.mjs";

const ids = [1, 2, 3, 5, 17, 99, 250, 777, 1500, 1999, 2000, 2500, 3000, 12345, 49999].map((n) => `item_${n}`).concat(["does-not-exist"]);
const visible = (html) =>
  html
    .replace(/<script[\s\S]*?<\/script>/g, " ")
    .replace(/<style[\s\S]*?<\/style>/g, " ")
    .replace(/<template[\s\S]*?<\/template>/g, " ")
    .replace(/<head[\s\S]*?<\/head>/g, " ")
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<(span|p|div)\b[^>]*\shidden(?:="[^"]*")?[^>]*>[\s\S]*?<\/\1>/g, " ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&amp;|&#38;/g, "&").replace(/&#x27;|&#39;/g, "'").replace(/&quot;|&#34;/g, '"').replace(/&lt;|&#60;/g, "<").replace(/&gt;|&#62;/g, ">")
    .replace(/\s+/g, " ")
    .trim();
const names = Object.keys(apps).filter((n) => !process.argv[2] || process.argv[2].split(",").includes(n));
const reference = process.argv[3] ?? names[0];
const others = names.filter((n) => n !== reference);
let failures = 0;
for (const id of ids) {
  const pages = await Promise.all([reference, ...others].map(async (n) => {
    const res = await fetch(`http://localhost:${apps[n].port}/items/${id}`);
    return { n, status: res.status, text: visible(await res.text()) };
  }));
  const ref = pages.find((p) => p.n === reference);
  for (const page of pages.filter((p) => others.includes(p.n))) {
    // Not-found pages differ in wording per framework; only their status must match.
    const same = page.status === ref.status && (ref.status === 404 || page.text === ref.text);
    if (!same) {
      failures += 1;
      let i = 0;
      while (i < ref.text.length && ref.text[i] === page.text[i]) i += 1;
      console.log(JSON.stringify({ id, app: page.n, status: [ref.status, page.status], [reference]: ref.text.slice(Math.max(0, i - 50), i + 70), [page.n]: page.text.slice(Math.max(0, i - 50), i + 70) }));
    }
  }
}
console.log(JSON.stringify({ reference, compared: others, pages: ids.length, failures }));
process.exit(failures ? 1 : 0);
