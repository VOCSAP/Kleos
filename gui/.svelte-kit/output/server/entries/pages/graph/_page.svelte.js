import "clsx";
import { _ as ssr_context } from "../../../chunks/utils2.js";
import "../../../chunks/engram.js";
function onDestroy(fn) {
  /** @type {SSRContext} */
  ssr_context.r.on_destroy(fn);
}
function _page($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    onDestroy(() => {
    });
    $$renderer2.push(`<div class="fixed inset-0 z-40 bg-[#0a0a0a] overflow-hidden"><div class="w-full h-full"></div> `);
    {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<div class="absolute inset-0 flex items-center justify-center z-50 bg-[#0a0a0a]"><div class="text-center"><div class="w-12 h-12 border-2 border-teal-500/30 border-t-teal-400 rounded-full animate-spin mx-auto mb-4"></div> <p class="text-gray-500 text-sm">Loading memory graph...</p></div></div>`);
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--></div>`);
  });
}
export {
  _page as default
};
