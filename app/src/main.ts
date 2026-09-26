import { mount } from "svelte";
import App from "./App.svelte";
import "./app.css";

// When Windows flips between light and dark, every color transition would fire at once and
// the switch would smear; snap instead.
matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
  const style = document.createElement("style");
  style.textContent = "*,*::before,*::after{transition:none !important}";
  document.head.append(style);
  void document.body.offsetHeight;
  requestAnimationFrame(() => style.remove());
});

export default mount(App, { target: document.getElementById("app")! });
