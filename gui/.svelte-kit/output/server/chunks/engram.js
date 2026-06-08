import { w as writable, d as derived } from "./index.js";
const apiKey = writable(
  typeof window !== "undefined" ? localStorage.getItem("engram_api_key") || "" : ""
);
apiKey.subscribe((v) => {
  if (typeof window !== "undefined" && v) localStorage.setItem("engram_api_key", v);
});
const isAuthed = derived(apiKey, ($key) => !!$key);
export {
  isAuthed as i
};
