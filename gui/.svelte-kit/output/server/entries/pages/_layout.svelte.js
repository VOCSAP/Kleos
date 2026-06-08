import { e as ensure_array_like, a as attr, b as attr_class, s as stringify, c as store_get, d as escape_html, u as unsubscribe_stores } from "../../chunks/renderer.js";
import { g as getContext } from "../../chunks/utils2.js";
import "clsx";
import "@sveltejs/kit/internal";
import "../../chunks/exports.js";
import "../../chunks/utils.js";
import "@sveltejs/kit/internal/server";
import "../../chunks/root.js";
import "../../chunks/state.svelte.js";
import { i as isAuthed } from "../../chunks/engram.js";
const getStores = () => {
  const stores$1 = getContext("__svelte__");
  return {
    /** @type {typeof page} */
    page: {
      subscribe: stores$1.page.subscribe
    },
    /** @type {typeof navigating} */
    navigating: {
      subscribe: stores$1.navigating.subscribe
    },
    /** @type {typeof updated} */
    updated: stores$1.updated
  };
};
const page = {
  subscribe(fn) {
    const store = getStores().page;
    return store.subscribe(fn);
  }
};
function _layout($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    var $$store_subs;
    let { children } = $$props;
    const nav = [
      { path: "/", label: "Dashboard", icon: "⊞" },
      { path: "/graph", label: "Graph", icon: "◉" },
      { path: "/search", label: "Search", icon: "⌕" },
      { path: "/inbox", label: "Inbox", icon: "☐" },
      { path: "/timeline", label: "Timeline", icon: "☰" },
      { path: "/entities", label: "Entities", icon: "◎" },
      { path: "/projects", label: "Projects", icon: "▦" }
    ];
    $$renderer2.push(`<div class="flex h-screen bg-gray-950 text-gray-200"><nav class="w-52 bg-gray-900/80 border-r border-gray-800 flex flex-col shrink-0"><div class="p-4 border-b border-gray-800"><h1 class="text-lg font-bold tracking-wider"><span class="bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent">ENGRAM</span></h1> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]--></div> <div class="flex-1 py-2"><!--[-->`);
    const each_array = ensure_array_like(nav);
    for (let $$index = 0, $$length = each_array.length; $$index < $$length; $$index++) {
      let item = each_array[$$index];
      $$renderer2.push(`<a${attr("href", item.path)}${attr_class(`flex items-center gap-3 px-4 py-2.5 text-sm transition-colors ${stringify(store_get($$store_subs ??= {}, "$page", page).url.pathname === item.path ? "bg-indigo-500/10 text-indigo-400 border-r-2 border-indigo-400" : "text-gray-400 hover:bg-gray-800/50 hover:text-gray-200")}`)}><span class="text-base w-5 text-center">${escape_html(item.icon)}</span> ${escape_html(item.label)}</a>`);
    }
    $$renderer2.push(`<!--]--></div> <div class="p-3 border-t border-gray-800"><button class="w-full px-3 py-2 text-xs rounded-lg bg-gray-800/50 hover:bg-gray-800 text-gray-400 hover:text-gray-200 transition-colors">${escape_html(store_get($$store_subs ??= {}, "$isAuthed", isAuthed) ? "API Key Set" : "Set API Key")}</button></div></nav> <main class="flex-1 overflow-auto">`);
    children($$renderer2);
    $$renderer2.push(`<!----></main></div> `);
    {
      $$renderer2.push("<!--[-1-->");
    }
    $$renderer2.push(`<!--]-->`);
    if ($$store_subs) unsubscribe_stores($$store_subs);
  });
}
export {
  _layout as default
};
