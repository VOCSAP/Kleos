import { a as attr, d as escape_html, b as attr_class, e as ensure_array_like, s as stringify } from "../../../chunks/renderer.js";
import "../../../chunks/engram.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let query = "";
    let mode = "";
    let results = [];
    const modes = ["fact", "timeline", "preference", "decision", "recent"];
    const categoryColors = {
      task: "text-blue-400",
      discovery: "text-emerald-400",
      decision: "text-amber-400",
      state: "text-purple-400",
      issue: "text-red-400",
      reference: "text-cyan-400",
      general: "text-gray-400"
    };
    $$renderer2.push(`<div class="p-6 max-w-5xl"><h2 class="text-2xl font-bold mb-6">Search</h2> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> <form class="mb-6 space-y-3"><div class="flex gap-2"><input type="text"${attr("value", query)} placeholder="Search memories..." class="flex-1 px-4 py-2.5 bg-gray-900/60 border border-gray-800 rounded-lg text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-indigo-500 transition-colors"/> <button type="submit"${attr("disabled", !query.trim(), true)} class="px-5 py-2.5 bg-indigo-600 hover:bg-indigo-500 disabled:bg-gray-800 disabled:text-gray-600 rounded-lg text-sm font-medium transition-colors">${escape_html("Search")}</button></div> <div class="flex gap-2 flex-wrap"><button type="button"${attr_class(`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${stringify(
      "bg-indigo-500/20 text-indigo-400 border border-indigo-500/40"
    )}`)}>auto</button> <!--[-->`);
    const each_array = ensure_array_like(modes);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let m = each_array[$$index];
      $$renderer2.push(`<button type="button"${attr_class(`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${stringify(mode === m ? "bg-indigo-500/20 text-indigo-400 border border-indigo-500/40" : "bg-gray-900/40 text-gray-400 border border-gray-800 hover:border-gray-700")}`)}>${escape_html(m)}</button>`);
    }
    $$renderer2.push(`<!--]--></div></form> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> `);
    if (results.length > 0) {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<p class="text-xs text-gray-500 mb-3">${escape_html(results.length)} result${escape_html(results.length !== 1 ? "s" : "")}</p>`);
    } else {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> <div class="space-y-2"><!--[-->`);
    const each_array_1 = ensure_array_like(results);
    for (let $$index_2 = 0, $$length = each_array_1.length; $$index_2 < $$length; $$index_2++) {
      let mem = each_array_1[$$index_2];
      $$renderer2.push(`<div class="p-3 bg-gray-900/40 border border-gray-800 rounded-lg hover:border-gray-700 transition-colors"><div class="flex items-center gap-2 mb-1"><span class="text-[10px] font-mono text-gray-600">#${escape_html(mem.id)}</span> <span${attr_class(`text-[10px] font-medium ${stringify(categoryColors[mem.category] || "text-gray-400")}`)}>${escape_html(mem.category)}</span> `);
      if (mem.score != null) {
        $$renderer2.push("<!--[0-->");
        $$renderer2.push(`<span class="text-[10px] text-gray-500">score: ${escape_html(mem.score.toFixed(3))}</span>`);
      } else {
        $$renderer2.push("<!--[-1-->");
      }
      $$renderer2.push(`<!--]--> <span class="text-[10px] text-gray-600 ml-auto">${escape_html(mem.created_at?.substring(0, 16))}</span></div> <p class="text-sm text-gray-300 mb-2">${escape_html(mem.content)}</p> `);
      if (mem.explain?.reasons?.length) {
        $$renderer2.push("<!--[0-->");
        $$renderer2.push(`<div class="flex flex-wrap gap-1 mb-2"><!--[-->`);
        const each_array_2 = ensure_array_like(mem.explain.reasons);
        for (let $$index_1 = 0, $$length2 = each_array_2.length; $$index_1 < $$length2; $$index_1++) {
          let reason = each_array_2[$$index_1];
          $$renderer2.push(`<span class="text-[10px] px-1.5 py-0.5 bg-gray-800/60 border border-gray-700/40 rounded text-gray-400">${escape_html(reason)}</span>`);
        }
        $$renderer2.push(`<!--]--></div>`);
      } else {
        $$renderer2.push("<!--[-1-->");
      }
      $$renderer2.push(`<!--]--> <div class="flex gap-2"><button class="text-[10px] px-2 py-1 rounded bg-gray-800/50 hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors">Archive</button> <button class="text-[10px] px-2 py-1 rounded bg-gray-800/50 hover:bg-red-900/40 text-gray-400 hover:text-red-400 transition-colors">Delete</button></div></div>`);
    }
    $$renderer2.push(`<!--]--></div></div>`);
  });
}
export {
  _page as default
};
