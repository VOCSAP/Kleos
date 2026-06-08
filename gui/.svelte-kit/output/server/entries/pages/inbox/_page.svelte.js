import { d as escape_html, f as derived } from "../../../chunks/renderer.js";
import "../../../chunks/engram.js";
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let pending = [];
    let pendingCount = derived(() => pending.length);
    $$renderer2.push(`<div class="p-6 max-w-5xl"><div class="flex items-center gap-3 mb-6"><h2 class="text-2xl font-bold">Inbox</h2> `);
    if (pendingCount() > 0) {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<span class="px-2.5 py-0.5 bg-orange-500/20 text-orange-400 text-xs font-bold rounded-full border border-orange-500/30">${escape_html(pendingCount())}</span>`);
    } else {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--></div> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<div class="text-sm text-gray-500">Loading inbox...</div>`);
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
