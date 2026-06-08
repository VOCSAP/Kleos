import { e as ensure_array_like, d as escape_html, b as attr_class, s as stringify } from "../../chunks/renderer.js";
import "../../chunks/engram.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let recent = [];
    const categoryColors = {
      task: "text-blue-400",
      discovery: "text-emerald-400",
      decision: "text-amber-400",
      state: "text-purple-400",
      issue: "text-red-400",
      reference: "text-cyan-400",
      general: "text-gray-400"
    };
    $$renderer2.push(`<div class="p-6 max-w-5xl"><h2 class="text-2xl font-bold mb-6">Dashboard</h2> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> <h3 class="text-sm font-semibold text-gray-400 uppercase tracking-wide mb-3">Recent Memories</h3> <div class="space-y-2"><!--[-->`);
    const each_array_1 = ensure_array_like(recent);
    for (let $$index_1 = 0, $$length = each_array_1.length; $$index_1 < $$length; $$index_1++) {
      let mem = each_array_1[$$index_1];
      $$renderer2.push(`<div class="p-3 bg-gray-900/40 border border-gray-800 rounded-lg hover:border-gray-700 transition-colors"><div class="flex items-center gap-2 mb-1"><span class="text-[10px] font-mono text-gray-600">#${escape_html(mem.id)}</span> <span${attr_class(`text-[10px] font-medium ${stringify(categoryColors[mem.category] || "text-gray-400")}`)}>${escape_html(mem.category)}</span> <span class="text-[10px] text-gray-600 ml-auto">${escape_html(mem.created_at?.substring(0, 16))}</span></div> <p class="text-sm text-gray-300 line-clamp-2">${escape_html(mem.content)}</p></div>`);
    }
    $$renderer2.push(`<!--]--></div></div>`);
  });
}
export {
  _page as default
};
