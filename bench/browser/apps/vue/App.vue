<!-- Vue 3, written the way the js-framework-benchmark's keyed Vue app is: plain row objects in a
     shallowRef, triggerRef after an in-place change, v-memo on each row. -->
<script setup>
import { ref, shallowRef, triggerRef, onMounted } from "vue";
import { label, nextId, patched } from "../shared/labels.js";

const props = defineProps({ initial: { type: Array, default: () => [] } });
const build = (n) => {
  const out = new Array(n);
  for (let i = 0; i < n; i++) out[i] = { id: nextId(), label: label() };
  return out;
};
const rows = shallowRef(props.initial.map((r) => ({ id: r.id, label: r.label })));
const selected = ref(0);

const relabel = (at, suffix) => {
  const d = rows.value;
  for (const i of at) d[i].label += suffix;
  triggerRef(rows);
};
const create = () => (rows.value = build(1000));
const create10k = () => (rows.value = build(10000));
const append = () => (rows.value = rows.value.concat(build(1000)));
const update = () => {
  const d = rows.value;
  for (let i = 0; i < d.length; i += 10) d[i].label += " !!!";
  triggerRef(rows);
};
const swap = () => {
  const d = rows.value;
  if (d.length <= 998) return;
  const t = d[1];
  d[1] = d[998];
  d[998] = t;
  triggerRef(rows);
};
const clear = () => (rows.value = []);
const patch1 = () => relabel([rows.value.length >> 1], " *");
const patch100 = () => relabel(patched(rows.value.length), " *");
const select = (id) => (selected.value = id);
const remove = (id) => {
  const d = rows.value;
  d.splice(d.findIndex((r) => r.id === id), 1);
  triggerRef(rows);
};
onMounted(() => {
  document.documentElement.dataset.ready = String(performance.now());
});
</script>

<template>
  <div class="controls">
    <button id="create" @click="create">Create 1,000</button>
    <button id="create10k" @click="create10k">Create 10,000</button>
    <button id="append" @click="append">Append 1,000</button>
    <button id="update" @click="update">Update every 10th</button>
    <button id="swap" @click="swap">Swap rows</button>
    <button id="clear" @click="clear">Clear</button>
    <button id="patch1" @click="patch1">Patch 1</button>
    <button id="patch100" @click="patch100">Patch 100</button>
  </div>
  <table>
    <tbody id="tbody">
      <tr v-for="{ id, label } of rows" :key="id" :class="{ danger: id === selected }" v-memo="[label, id === selected]">
        <td class="col-md-1">{{ id }}</td>
        <td class="col-md-4"><a class="select" @click="select(id)">{{ label }}</a></td>
        <td class="col-md-1"><a class="remove" @click="remove(id)"><span class="remove" aria-hidden="true">x</span></a></td>
        <td class="col-md-6"></td>
      </tr>
    </tbody>
  </table>
</template>
