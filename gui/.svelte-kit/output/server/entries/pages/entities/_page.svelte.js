import { e as ensure_array_like, b as attr_class, d as escape_html, s as stringify } from "../../../chunks/renderer.js";
import "../../../chunks/engram.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let typeFilter = "";
    const types = ["all", "person", "server", "tool", "service"];
    $$renderer2.push(`<div class="p-6 max-w-5xl"><h2 class="text-2xl font-bold mb-6">Entities</h2> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> <div class="flex gap-2 flex-wrap mb-4"><!--[-->`);
    const each_array = ensure_array_like(types);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let t = each_array[$$index];
      $$renderer2.push(`<button${attr_class(`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${stringify(t === "all" && !typeFilter || typeFilter === t ? "bg-indigo-500/20 text-indigo-400 border border-indigo-500/40" : "bg-gray-900/40 text-gray-400 border border-gray-800 hover:border-gray-700")}`)}>${escape_html(t)}</button>`);
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<div class="text-sm text-gray-500">Loading entities...</div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
