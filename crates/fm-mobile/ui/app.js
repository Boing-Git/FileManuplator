// filemanuplator mobile. Renders the same TOML plugins as the desktop app,
// with the same widgets, colour roles and colour schemes. Rust (src-tauri)
// runs the pipelines; this page only draws and collects input.
"use strict";

const tauri = window.__TAURI__;
const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

// ---------------------------------------------------------------- icons --
// 24×24 stroke icons. Plugins name freedesktop icons; ALIASES maps those.
const ICONS = {
  convert:
    '<path d="M4.5 12a7.5 7.5 0 0 1 13-5.1L19.5 9"/><path d="M19.5 4.5V9H15"/><path d="M19.5 12a7.5 7.5 0 0 1-13 5.1L4.5 15"/><path d="M4.5 19.5V15H9"/>',
  image:
    '<rect x="3.5" y="4.5" width="17" height="15" rx="3"/><path d="M3.5 16l5-5 4.5 4.5 2.5-2.5 5 5"/><circle cx="15.5" cy="9" r="1.5"/>',
  video: '<rect x="3" y="6.5" width="12.5" height="11" rx="2.5"/><path d="M15.5 10.5 21 7.5v9l-5.5-3"/>',
  web: '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17"/><path d="M12 3.5c2.6 2.6 3.6 5.4 3.6 8.5S14.6 17.9 12 20.5C9.4 17.9 8.4 15.1 8.4 12S9.4 6.1 12 3.5z"/>',
  trash: '<path d="M4.5 7h15"/><path d="M9.5 7V4.5h5V7"/><path d="M6.5 7l1 12.5h9l1-12.5"/>',
  puzzle:
    '<path d="M9 4.5a2 2 0 0 1 4 0V6h4.5v4.5H19a2 2 0 0 1 0 4h-1.5V19H13v-1.5a2 2 0 0 0-4 0V19H4.5v-4.5H6a2 2 0 0 0 0-4H4.5V6H9z"/>',
  palette:
    '<path d="M12 3.5a8.5 8.5 0 0 0 0 17c1.3 0 1.9-1 1.4-2.1-.5-1.1.2-2.4 1.5-2.4H17a3.5 3.5 0 0 0 3.5-3.5c0-5-3.8-9-8.5-9z"/><circle cx="7.8" cy="11" r="1.2"/><circle cx="10.5" cy="7.3" r="1.2"/><circle cx="15" cy="8" r="1.2"/>',
  check: '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
  file: '<path d="M6.5 3.5h7l4.5 4.5v12.5h-11.5z"/><path d="M13.5 3.5V8H18"/>',
  save: '<path d="M12 4v11"/><path d="M7 10.5l5 5 5-5"/><path d="M5 20h14"/>',
  info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5"/><path d="M12 7.6v.1"/>',
  warning: '<path d="M12 4 21 19.5H3z"/><path d="M12 10v4.5"/><path d="M12 17.2v.1"/>',
  phone: '<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M11 18.5h2"/>',
  fullscreen: '<path d="M4 9V4h5"/><path d="M20 9V4h-5"/><path d="M4 15v5h5"/><path d="M20 15v5h-5"/>',
};
const ALIASES = {
  "view-refresh-symbolic": "convert",
  "image-x-generic-symbolic": "image",
  "video-x-generic-symbolic": "video",
  "web-browser-symbolic": "web",
  "edit-delete-symbolic": "trash",
  "phone-symbolic": "phone",
  "color-select-symbolic": "palette",
};
function icon(name) {
  const key = ICONS[name] ? name : ALIASES[name] || "puzzle";
  return `<svg class="icon" viewBox="0 0 24 24" aria-hidden="true">${ICONS[key]}</svg>`;
}

// ------------------------------------------------------------------- dom --
// Strings become text nodes, never HTML; only our own icon markup uses `html`.
function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (value == null || value === false) continue;
    if (key === "class") el.className = value;
    else if (key === "html") el.innerHTML = value;
    else if (key.startsWith("on")) el.addEventListener(key.slice(2), value);
    else el.setAttribute(key, value === true ? "" : value);
  }
  el.append(...children.flat().filter((c) => c != null));
  return el;
}
const roleClass = (color) => (color ? ` role-${color}` : "");
const $ = (id) => document.getElementById(id);

function formatSize(bytes) {
  const units = ["B", "KB", "MB", "GB"];
  let n = bytes;
  let i = 0;
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024;
    i++;
  }
  return `${n.toFixed(i ? 1 : 0)} ${units[i]}`;
}

// --------------------------------------------------------------- storage --
// Per-device conveniences only; everything still works without storage.
function load(key, fallback) {
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(key) || "{}") };
  } catch {
    return { ...fallback };
  }
}
function store(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* private mode or storage off: the choice lasts until the app closes */
  }
}

// --------------------------------------------------------------- snackbar --
let snackTimer;
function snack(text) {
  const bar = $("snackbar");
  bar.textContent = text;
  bar.hidden = false;
  requestAnimationFrame(() => bar.classList.add("show"));
  clearTimeout(snackTimer);
  snackTimer = setTimeout(() => {
    bar.classList.remove("show");
    setTimeout(() => (bar.hidden = true), 200);
  }, 4000);
}

// ------------------------------------------------------------------ theme --
const prefs = load("appearance", { scheme: "lavender", mode: "auto" });
const darkQuery = matchMedia("(prefers-color-scheme: dark)");
const isDark = () => prefs.mode === "dark" || (prefs.mode === "auto" && darkQuery.matches);

async function applyTheme() {
  const dark = isDark();
  const roles = await invoke("scheme", { id: prefs.scheme, dark });
  const root = document.documentElement;
  for (const [role, hex] of Object.entries(roles)) {
    root.style.setProperty(`--m3-${role.replaceAll("_", "-")}`, hex);
  }
  root.style.colorScheme = dark ? "dark" : "light";
}
darkQuery.addEventListener("change", () => prefs.mode === "auto" && applyTheme());

async function renderSchemeSheet() {
  const swatches = await invoke("swatches", { dark: isDark() });
  $("swatches").replaceChildren(
    ...swatches.map((s) =>
      h("button", {
        class: "swatch",
        type: "button",
        style: `--swatch: ${s.primary}`,
        "aria-label": s.label,
        "aria-pressed": String(s.id === prefs.scheme),
        html: icon("check"),
        onclick: () => choose({ scheme: s.id }),
      }),
    ),
  );
  $("modes").replaceChildren(
    ...[
      ["auto", "Auto"],
      ["light", "Light"],
      ["dark", "Dark"],
    ].map(([id, label]) =>
      h(
        "button",
        { type: "button", "aria-pressed": String(id === prefs.mode), onclick: () => choose({ mode: id }) },
        h("span", { html: icon("check") }),
        label,
      ),
    ),
  );
}
async function choose(change) {
  Object.assign(prefs, change);
  store("appearance", prefs);
  await applyTheme();
  await renderSchemeSheet();
}

// ------------------------------------------------------------ navigation --
const destinations = []; // { title, icon, view }
let current = -1;

function go(index) {
  if (index === current) return;
  current = index;
  const dest = destinations[index];
  $("title").textContent = dest.title;
  $("page").replaceChildren(dest.view);
  for (const [i, item] of [...$("nav").children].entries()) {
    if (i === index) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
  window.scrollTo(0, 0);
}

function buildNav() {
  $("nav").replaceChildren(
    ...destinations.map((dest, i) =>
      h(
        "button",
        { class: "nav-item", type: "button", onclick: () => go(i) },
        h("span", { class: "nav-indicator", html: icon(dest.icon) }),
        h("span", { class: "nav-label" }, dest.title),
      ),
    ),
  );
}

// ------------------------------------------------------------- running --
async function process(meta, file) {
  const head = new TextEncoder().encode(JSON.stringify(meta));
  const data = new Uint8Array(await file.arrayBuffer());
  const body = new Uint8Array(4 + head.length + data.length);
  new DataView(body.buffer).setUint32(0, head.length, true);
  body.set(head, 4);
  body.set(data, 4 + head.length);
  return invoke("process", body);
}

async function runInto(slot, button, meta, file) {
  button.disabled = true;
  slot.replaceChildren(h("div", { class: "progress", role: "progressbar", "aria-label": "Working" }));
  try {
    showResult(slot, await process(meta, file));
  } catch (error) {
    slot.replaceChildren();
    snack(String(error));
  } finally {
    button.disabled = false;
  }
}

async function showResult(slot, result) {
  const card = h("div", { class: "result" });
  if (/\.(png|jpe?g|webp|gif|bmp)$/i.test(result.name)) {
    const bytes = await invoke("last_output");
    const url = URL.createObjectURL(new Blob([bytes]));
    card.append(h("img", { src: url, alt: `Preview of ${result.name}` }));
  }
  card.append(
    h(
      "div",
      { class: "result-meta" },
      h("strong", {}, result.name),
      h("span", { class: "supporting" }, formatSize(result.size)),
    ),
    h(
      "button",
      { class: "btn filled big", type: "button", onclick: () => saveResult(result.name) },
      h("span", { html: icon("save") }),
      "Save",
    ),
  );
  slot.replaceChildren(card);
}

async function saveResult(name) {
  try {
    const dest = await tauri.dialog.save({ defaultPath: name });
    if (!dest) return;
    await invoke("save_output", { dest });
    snack(`Saved ${name}`);
  } catch (error) {
    snack(`Could not save: ${error}`);
  }
}

// ------------------------------------------------------------ converter --
function converterView(targets, hidden) {
  const state = { file: null, target: null };
  const input = h("input", { type: "file", hidden: true });
  const slot = h("div");
  const body = h("div", { class: "page" });
  const view = h("section", { class: "page" }, input, body, slot);

  const run = h("button", { class: "btn filled big", type: "button" }, "Convert");
  run.onclick = () => {
    if (!state.target) return snack("Pick a format first.");
    runInto(slot, run, { fileName: state.file.name, target: state.target }, state.file);
  };

  function render() {
    slot.replaceChildren();
    if (!state.file) {
      body.replaceChildren(
        h(
          "button",
          { class: "pick", type: "button", onclick: () => input.click() },
          h("span", { class: "pick-icon", html: icon("file") }),
          h("strong", {}, "Choose a file"),
          h("span", { class: "supporting" }, "Images convert right here on your phone."),
        ),
      );
    } else {
      const isImage = state.file.type.startsWith("image/");
      body.replaceChildren(
        h(
          "div",
          { class: "list" },
          h(
            "div",
            { class: "item" },
            h("span", { html: icon(isImage ? "image" : "file") }),
            h(
              "div",
              { class: "item-text" },
              h("span", { class: "item-title" }, state.file.name),
              h("span", { class: "supporting" }, `${formatSize(state.file.size)} · ${state.file.type || "unknown type"}`),
            ),
            h("button", { class: "btn tonal", type: "button", onclick: () => input.click() }, "Change"),
          ),
        ),
        ...(isImage
          ? [
              h(
                "div",
                { class: "chips", role: "radiogroup", "aria-label": "Convert to" },
                ...targets.map((t) =>
                  h(
                    "button",
                    {
                      class: "chip",
                      type: "button",
                      role: "radio",
                      "aria-checked": String(state.target === t),
                      onclick: () => {
                        state.target = t;
                        render();
                      },
                    },
                    h("span", { html: icon("check") }),
                    t.toUpperCase(),
                  ),
                ),
              ),
              h("div", { class: "actions" }, run),
            ]
          : [
              h(
                "div",
                { class: "notice" },
                h("span", { html: icon("info") }),
                h("p", {}, "Converting this kind of file needs ffmpeg, ImageMagick or pandoc, so it is desktop-only for now."),
              ),
            ]),
      );
    }
    if (hidden.length) {
      body.append(
        h(
          "div",
          { class: "notice" },
          h("span", { html: icon("info") }),
          h(
            "p",
            {},
            hidden.length === 1
              ? `${hidden[0]} is desktop-only: it needs programs a phone doesn't have.`
              : `${hidden.join(", ")} are desktop-only: they need programs a phone doesn't have.`,
          ),
        ),
      );
    }
  }
  input.onchange = () => {
    if (!input.files[0]) return;
    state.file = input.files[0];
    state.target = null;
    render();
  };
  render();
  return view;
}

// -------------------------------------------------------------- plugins --
// The same rules as the desktop canvas (fm-gtk/src/main.rs).
function parseDimensions(text) {
  const inner = /\{([^}]*)\}/.exec(text)?.[1] ?? text;
  for (const sep of ["x", "X", ":", "/"]) {
    const parts = inner.split(sep);
    if (parts.length === 2) {
      const [w, hgt] = parts.map((p) => parseFloat(p));
      if (w > 0 && hgt > 0) return [w, hgt];
    }
  }
  return null;
}
const varName = (template) => (template || "").replace(/[{}]/g, "");

function pluginView(info) {
  const plugin = info.plugin;
  const tab = { file: null, values: {}, inputs: {}, extra: {}, image: null, listeners: [] };
  const emit = () => tab.listeners.forEach((fn) => fn());
  const view = h("section", { class: "page" });
  const slot = h("div");
  if (plugin.plugin.description) view.append(h("p", { class: "lede" }, plugin.plugin.description));

  let group = null;
  const row = (el) => {
    if (!group) {
      group = h("div", { class: "list" });
      view.append(group);
    }
    group.append(el);
  };
  const block = (el) => {
    group = null;
    view.append(el);
  };

  for (const w of plugin.ui || []) {
    const label = w.label || w.id || "";
    const def = w.default == null ? "" : String(w.default);
    switch (w.type) {
      case "file_picker": {
        const accept = (w.filter?.length ? w.filter : plugin.mime_types || [])
          .map((f) => (f.startsWith("*.") ? f.slice(1) : f))
          .filter((f) => f !== "*/*")
          .join(",");
        const input = h("input", { type: "file", accept: accept || null, hidden: true });
        const name = h("span", { class: "supporting" }, "No file chosen");
        input.onchange = async () => {
          const file = input.files[0];
          if (!file) return;
          tab.file = file;
          tab.values[w.id] = file.name;
          name.textContent = `${file.name} · ${formatSize(file.size)}`;
          await readImage(tab);
          emit();
        };
        row(
          h(
            "div",
            { class: "item" + roleClass(w.color) },
            h("div", { class: "item-text" }, h("span", { class: "item-title" }, label), name),
            h("button", { class: "btn tonal" + roleClass(w.color), type: "button", onclick: () => input.click() }, "Choose"),
            input,
          ),
        );
        break;
      }
      case "picker": {
        const select = h("select", { class: "select", "aria-label": label });
        for (const option of w.options || []) select.append(h("option", { value: option }, option));
        if ((w.options || []).includes(def)) select.value = def;
        tab.values[w.id] = select.value;
        select.onchange = () => {
          tab.values[w.id] = select.value;
          const dims = parseDimensions(select.value);
          if (w.overwrites?.length === 2 && dims) {
            w.overwrites.forEach((id, i) => {
              tab.values[id] = String(dims[i]);
              if (tab.inputs[id]) tab.inputs[id].value = String(dims[i]);
            });
          }
          emit();
        };
        row(h("div", { class: "item" + roleClass(w.color) }, h("div", { class: "item-text" }, h("span", { class: "item-title" }, label)), select));
        break;
      }
      case "radio_group": {
        const options = w.options || [];
        const chosen = options.includes(def) ? def : options[0];
        tab.values[w.id] = chosen;
        const radios = h("div", { class: "radio-group", role: "radiogroup", "aria-label": label });
        for (const option of options) {
          const radio = h("input", { type: "radio", name: `${info.id}:${w.id}`, value: option, checked: option === chosen });
          radio.onchange = () => {
            tab.values[w.id] = option;
            emit();
          };
          radios.append(h("label", { class: "radio-row" }, radio, h("span", {}, option)));
        }
        row(h("div", { class: "item stacked" + roleClass(w.color) }, h("span", { class: "item-title" }, label), radios));
        break;
      }
      case "slider": {
        const min = w.min ?? 0;
        const max = w.max ?? 100;
        const start = w.default ?? min;
        const range = h("input", { class: "slider", type: "range", min, max, step: 1, value: start, "aria-label": label });
        const out = h("output", {}, String(Math.round(start)));
        const paint = () => range.style.setProperty("--fill", `${((range.value - min) / (max - min)) * 100}%`);
        tab.values[w.id] = String(start);
        range.oninput = () => {
          out.textContent = range.value;
          tab.values[w.id] = range.value;
          paint();
        };
        range.onchange = emit;
        paint();
        row(
          h(
            "div",
            { class: "item stacked" + roleClass(w.color) },
            h("span", { class: "item-title" }, label),
            h("div", { class: "slider-row" }, range, out),
          ),
        );
        break;
      }
      case "text_input": {
        const input = h("input", { value: def, inputmode: /^\d+(\.\d+)?$/.test(def) ? "decimal" : null, "aria-label": label });
        tab.values[w.id] = def;
        tab.inputs[w.id] = input;
        input.oninput = () => {
          tab.values[w.id] = input.value;
          emit();
        };
        row(h("label", { class: "item field" + roleClass(w.color) }, h("span", { class: "field-label" }, label), input));
        break;
      }
      case "notice":
        block(
          h(
            "div",
            { class: "notice" + roleClass(w.color), role: "note" },
            h("span", { html: icon(w.color === "error" ? "warning" : "info") }),
            h("p", {}, w.text || ""),
          ),
        );
        break;
      case "image_canvas":
        block(cropView(w, tab));
        break;
      case "html_view":
        block(htmlView(tab));
        break;
      case "button": {
        const button = h("button", { class: "btn filled big" + roleClass(w.color), type: "button" }, label || "Run");
        button.onclick = () => {
          if (!tab.file) return snack("Choose a file first.");
          runInto(slot, button, { fileName: tab.file.name, pluginId: info.id, state: { ...tab.values, ...tab.extra } }, tab.file);
        };
        block(h("div", { class: "actions" }, button));
        break;
      }
      default:
        block(h("div", { class: "notice" }, h("span", { html: icon("info") }), h("p", {}, `This version can't show a '${w.type}' widget.`)));
    }
  }
  view.append(slot);
  return view;
}

// Picture size, for the canvas and for {file.width}-style variables.
async function readImage(tab) {
  tab.image = null;
  if (tab.url) URL.revokeObjectURL(tab.url);
  tab.url = URL.createObjectURL(tab.file);
  if (!tab.file.type.startsWith("image/")) return;
  const img = new Image();
  img.src = tab.url;
  try {
    await img.decode();
  } catch {
    return;
  }
  const { naturalWidth: w, naturalHeight: hgt } = img;
  tab.image = { w, h: hgt };
  for (const prefix of ["file", "image"]) {
    tab.extra[`${prefix}.width`] = String(w);
    tab.extra[`${prefix}.height`] = String(hgt);
    tab.extra[`${prefix}.aspect_ratio`] = String(w / hgt);
  }
}

// --------------------------------------------------------------- crop --
function cropView(w, tab) {
  const key = `${w.id}.crop_data`;
  const img = h("img", { alt: "Image being cropped", draggable: "false" });
  const box = h("div", { class: "crop-box" + (w.show_grid ? " grid" : "") });
  box.style.setProperty("--shade", w.shade_outside ?? 0.5);
  if (w.show_handles !== false) {
    for (const corner of ["nw", "ne", "sw", "se"]) box.append(h("span", { class: `handle ${corner}`, "data-corner": corner }));
  }
  const stage = h("div", { class: "crop-stage", hidden: true }, img, box);
  const empty = h("div", { class: "crop-empty" }, h("span", { html: icon("image") }), "Choose an image to crop it here.");
  const wrap = h("div", { class: "crop" }, empty, stage);
  const insideOnly = w.lock_inside_image !== false && w.lock_inside_image !== "false";
  let crop = null;
  let shownUrl = null;
  let lastLock = null;

  const locked = () => {
    const v = (tab.values[varName(w.lock_aspect_ratio)] || "").trim().toLowerCase();
    return v.includes("locked") || v.includes("ratio") || v === "true" || v === "1";
  };
  const ratio = () => {
    const source = tab.values[varName(w.aspect_ratio_source)] || "";
    const dims = !source.toLowerCase().includes("original") && parseDimensions(source);
    if (dims) return dims[0] / dims[1];
    if (tab.image) return tab.image.w / tab.image.h;
    const [cw, ch] = [parseFloat(tab.values.width), parseFloat(tab.values.height)];
    return cw > 0 && ch > 0 ? cw / ch : 1;
  };
  const scale = () => img.clientWidth / tab.image.w;

  function full() {
    crop = { x: 0, y: 0, w: tab.image.w, h: tab.image.h };
    if (locked()) fitRatio();
  }
  // Largest box of the locked ratio, centred on the current one.
  function fitRatio() {
    const r = ratio();
    const cx = crop.x + crop.w / 2;
    const cy = crop.y + crop.h / 2;
    let cw = crop.w;
    let ch = cw / r;
    if (ch > crop.h) {
      ch = crop.h;
      cw = ch * r;
    }
    crop = { x: cx - cw / 2, y: cy - ch / 2, w: cw, h: ch };
    clamp();
  }
  function clamp() {
    if (!insideOnly) return;
    crop.w = Math.min(crop.w, tab.image.w);
    crop.h = Math.min(crop.h, tab.image.h);
    crop.x = Math.max(0, Math.min(crop.x, tab.image.w - crop.w));
    crop.y = Math.max(0, Math.min(crop.y, tab.image.h - crop.h));
  }
  function commit() {
    const r = Math.round;
    tab.extra[key] = `${r(crop.w)}x${r(crop.h)}+${r(crop.x)}+${r(crop.y)}`;
    const s = scale();
    Object.assign(box.style, {
      left: `${crop.x * s}px`,
      top: `${crop.y * s}px`,
      width: `${crop.w * s}px`,
      height: `${crop.h * s}px`,
    });
  }

  tab.listeners.push(async () => {
    if (!tab.image) {
      stage.hidden = true;
      empty.hidden = false;
      return;
    }
    if (shownUrl !== tab.url) {
      shownUrl = tab.url;
      img.src = tab.url;
      await img.decode().catch(() => {});
      stage.hidden = false;
      empty.hidden = true;
      full();
    }
    const lockValue = (tab.values[varName(w.lock_aspect_ratio)] || "").toLowerCase();
    if (lockValue.includes("reset") && lastLock !== lockValue) full();
    else if (locked()) fitRatio();
    lastLock = lockValue;
    commit();
  });
  new ResizeObserver(() => crop && commit()).observe(stage);

  let drag = null;
  box.addEventListener("pointerdown", (e) => {
    const corner = e.target.dataset.corner;
    if (!corner && w.draggable_selection === false) return;
    drag = { corner, x: e.clientX, y: e.clientY, start: { ...crop } };
    box.setPointerCapture(e.pointerId);
    e.preventDefault();
  });
  box.addEventListener("pointermove", (e) => {
    if (!drag) return;
    const s = scale();
    const dx = (e.clientX - drag.x) / s;
    const dy = (e.clientY - drag.y) / s;
    const a = drag.start;
    const min = 24 / s;
    if (!drag.corner) {
      crop = { ...a, x: a.x + dx, y: a.y + dy };
    } else {
      const west = drag.corner.includes("w");
      const north = drag.corner.includes("n");
      let cw = Math.max(min, a.w + (west ? -dx : dx));
      let ch = Math.max(min, a.h + (north ? -dy : dy));
      if (locked()) ch = cw / ratio();
      if (insideOnly) {
        const maxW = west ? a.x + a.w : tab.image.w - a.x;
        const maxH = north ? a.y + a.h : tab.image.h - a.y;
        if (cw > maxW) cw = maxW;
        if (ch > maxH) ch = maxH;
        if (locked()) {
          const r = ratio();
          if (cw / ch > r) cw = ch * r;
          else ch = cw / r;
        }
      }
      crop = { x: west ? a.x + a.w - cw : a.x, y: north ? a.y + a.h - ch : a.y, w: cw, h: ch };
    }
    clamp();
    commit();
  });
  const end = () => (drag = null);
  box.addEventListener("pointerup", end);
  box.addEventListener("pointercancel", end);
  return wrap;
}

// --------------------------------------------------------- html runner --
function htmlView(tab) {
  // No allow-same-origin: the page runs isolated from the app and its files.
  const frame = h("iframe", { sandbox: "allow-scripts allow-forms allow-modals allow-popups", title: "Page being run" });
  const empty = h(
    "div",
    { class: "html-empty" },
    h("span", { html: icon("phone") }),
    h("strong", {}, "Nothing running"),
    h("span", { class: "supporting" }, "Choose an HTML file to run it here."),
  );
  const toggle = h("button", {
    class: "icon-button tonal fullscreen-toggle",
    type: "button",
    "aria-label": "Full screen",
    html: icon("fullscreen"),
    onclick: () => setFullscreen(!document.body.classList.contains("fullscreen")),
  });
  const wrap = h("div", { class: "html-view" }, empty, frame, toggle);
  let shown = null;
  tab.listeners.push(async () => {
    if (!tab.file || tab.file === shown) return;
    shown = tab.file;
    // Visible and laid out first, so the page's scripts see a real width.
    wrap.classList.add("running");
    const [text] = await Promise.all([tab.file.text(), new Promise(requestAnimationFrame)]);
    frame.srcdoc = text;
  });
  return wrap;
}

// Android's back button or gesture closes the top overlay (the scheme sheet,
// full screen) before it leaves the app: each overlay pushes a history entry.
const backStack = [];
let popping = false;
function pushBack(close) {
  backStack.push(close);
  history.pushState({ overlay: true }, "");
}
// The user closed the overlay some other way: drop its history entry too.
function popBack() {
  if (!popping && backStack.length) history.back();
}
window.addEventListener("popstate", () => {
  popping = true;
  backStack.pop()?.();
  popping = false;
});

function setFullscreen(on) {
  if (on === document.body.classList.contains("fullscreen")) return;
  document.body.classList.toggle("fullscreen", on);
  if (on) pushBack(() => document.body.classList.remove("fullscreen"));
  else popBack();
}

// ------------------------------------------------------------------ start --
async function start() {
  $("scheme-button").innerHTML = icon("palette");
  const sheet = $("scheme-sheet");
  $("scheme-button").onclick = async () => {
    await renderSchemeSheet();
    sheet.showModal();
    pushBack(() => sheet.close());
  };
  // Tapping the scrim closes the sheet; so do Escape and Back.
  sheet.addEventListener("click", (e) => e.target === sheet && sheet.close());
  sheet.addEventListener("close", popBack);

  await applyTheme();
  const [infos, targets] = await Promise.all([invoke("plugins"), invoke("image_targets")]);
  const usable = infos.filter((i) => i.plugin?.tab && i.desktopOnly.length === 0);
  const hidden = infos.filter((i) => i.plugin?.tab && i.desktopOnly.length > 0).map((i) => i.plugin.plugin.name);
  for (const bad of infos.filter((i) => i.problem)) console.warn(`${bad.id}: ${bad.problem}`);

  destinations.push({ title: "Converter", icon: "convert", view: converterView(targets, hidden) });
  for (const info of usable.slice(0, 4)) {
    destinations.push({ title: info.plugin.tab.title, icon: info.plugin.tab.icon, view: pluginView(info) });
  }
  buildNav();
  go(0);
}

start().catch((error) => snack(`Could not start: ${error}`));
