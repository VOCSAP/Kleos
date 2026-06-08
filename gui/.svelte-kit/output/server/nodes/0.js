

export const index = 0;
let component_cache;
export const component = async () => component_cache ??= (await import('../entries/pages/_layout.svelte.js')).default;
export const universal = {
  "prerender": false,
  "ssr": false
};
export const universal_id = "src/routes/+layout.ts";
export const imports = ["_app/immutable/nodes/0.DdARQi6A.js","_app/immutable/chunks/DwQ-sTz3.js","_app/immutable/chunks/mBmaxWfN.js","_app/immutable/chunks/BUFBXpcZ.js","_app/immutable/chunks/c69CwTIL.js","_app/immutable/chunks/FLYLeVcw.js","_app/immutable/chunks/DOn2_yXa.js","_app/immutable/chunks/DYGgIWnh.js","_app/immutable/chunks/CDQ2KbXy.js"];
export const stylesheets = ["_app/immutable/assets/0.CjW86Buw.css"];
export const fonts = [];
