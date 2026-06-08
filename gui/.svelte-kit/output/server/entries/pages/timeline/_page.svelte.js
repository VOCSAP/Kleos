import { a as attr, d as escape_html, e as ensure_array_like, b as attr_class, s as stringify } from "../../../chunks/renderer.js";
import "../../../chunks/engram.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let category = "";
    let guardInput = "";
    const categories = ["all", "task", "discovery", "decision", "state", "issue"];
    $$renderer2.push(`<div class="p-6 max-w-5xl"><h2 class="text-2xl font-bold mb-6">Timeline</h2> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> <div class="mb-6 p-4 bg-gray-900/40 border border-gray-800 rounded-lg"><p class="text-xs text-gray-500 uppercase tracking-wide mb-2">Guard Check</p> <div class="flex gap-2"><input type="text"${attr("value", guardInput)} placeholder="Describe an action to check..." class="flex-1 px-3 py-2 bg-gray-900/60 border border-gray-800 rounded-lg text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-indigo-500 transition-colors"/> <button${attr("disabled", !guardInput.trim(), true)} class="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:bg-gray-800 disabled:text-gray-600 rounded-lg text-sm font-medium transition-colors">${escape_html("Check")}</button></div> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--></div> <div class="flex gap-2 flex-wrap mb-4"><!--[-->`);
    const each_array_1 = ensure_array_like(categories);
    for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
      let cat = each_array_1[$$index_1];
      $$renderer2.push(`<button${attr_class(`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${stringify(cat === "all" && !category || category === cat ? "bg-indigo-500/20 text-indigo-400 border border-indigo-500/40" : "bg-gray-900/40 text-gray-400 border border-gray-800 hover:border-gray-700")}`)}>${escape_html(cat)}</button>`);
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<div class="text-sm text-gray-500">Loading timeline...</div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
