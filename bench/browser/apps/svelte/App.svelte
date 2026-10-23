<!-- Svelte 5, written the way the js-framework-benchmark's keyed Svelte app is: a $state label
     per row, the rows in $state.raw, the selection one $state. -->
<script>
  import { label as makeLabel, nextId, patched } from "../shared/labels.js";

  /** @type {{ initial?: { id: number, label: string }[] }} */
  let { initial = [] } = $props();

  class Row {
    label = $state("");
    /** @param {number} id @param {string} label */
    constructor(id, label) {
      this.id = id;
      this.label = label;
    }
  }
  const build = (/** @type {number} */ n) => {
    const out = new Array(n);
    for (let i = 0; i < n; i++) out[i] = new Row(nextId(), makeLabel());
    return out;
  };

  let data = $state.raw(initial.map((r) => new Row(r.id, r.label)));
  let selected = $state(0);

  const relabel = (/** @type {number[]} */ at, /** @type {string} */ suffix) => {
    for (const i of at) data[i].label += suffix;
  };
  const ops = {
    create: () => (data = build(1000)),
    create10k: () => (data = build(10000)),
    append: () => (data = data.concat(build(1000))),
    update: () => {
      for (let i = 0; i < data.length; i += 10) data[i].label += " !!!";
    },
    swap: () => {
      if (data.length <= 998) return;
      const d = data.slice();
      const t = d[1];
      d[1] = d[998];
      d[998] = t;
      data = d;
    },
    clear: () => (data = []),
    patch1: () => relabel([data.length >> 1], " *"),
    patch100: () => relabel(patched(data.length), " *"),
  };
  const remove = (/** @type {number} */ id) => {
    const d = data.slice();
    d.splice(d.findIndex((r) => r.id === id), 1);
    data = d;
  };
</script>

<div class="controls">
  <button id="create" onclick={ops.create}>Create 1,000</button>
  <button id="create10k" onclick={ops.create10k}>Create 10,000</button>
  <button id="append" onclick={ops.append}>Append 1,000</button>
  <button id="update" onclick={ops.update}>Update every 10th</button>
  <button id="swap" onclick={ops.swap}>Swap rows</button>
  <button id="clear" onclick={ops.clear}>Clear</button>
  <button id="patch1" onclick={ops.patch1}>Patch 1</button>
  <button id="patch100" onclick={ops.patch100}>Patch 100</button>
</div>
<table>
  <tbody id="tbody">
    {#each data as row (row.id)}
      <tr class={selected === row.id ? "danger" : ""}>
        <td class="col-md-1">{row.id}</td>
        <td class="col-md-4"><a class="select" onclick={() => (selected = row.id)}>{row.label}</a></td>
        <td class="col-md-1"><a class="remove" onclick={() => remove(row.id)}><span class="remove" aria-hidden="true">x</span></a></td>
        <td class="col-md-6"></td>
      </tr>
    {/each}
  </tbody>
</table>
