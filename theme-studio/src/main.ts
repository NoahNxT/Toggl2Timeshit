import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

const appTarget = document.getElementById("app");

const app = mount(App, {
  target: appTarget ?? document.body,
});

export default app;
