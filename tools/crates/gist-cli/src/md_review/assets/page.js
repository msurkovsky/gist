// The review page's glue: it long-polls `serve` for the review, draws the
// document and the comment cards, turns selections into draft cards and
// sends what the reviewer writes. The logic worth unit tests lives in
// anchor.js and margin.js; this file is DOM wiring, covered by the host
// case. docs/design/md-review-dld-page.md.

import { anchorFor, locate, selectsWhole, visible } from "./anchor.js";
import { pack } from "./margin.js";

/** Seconds the server may hold a poll; it holds at most 60. */
const HOLD = 50;
const BACKOFF_FIRST = 1000;
const BACKOFF_LAST = 30000;
const NARROW = matchMedia("(max-width: 1000px)");

const token = new URLSearchParams(location.search).get("token") ?? "";

/** The review as the server last sent it. */
let view = null;
/** The version whose document is on screen, and with which changes shown. */
let drawn = null;
/** The thread or draft whose card is focused. */
let focused = null;
let offline = false;
/** The message of the last write the server refused as stale. */
let stale = null;
let hideResolved = false;
const expanded = new Set();
/** Unsent text by key, kept in browser storage until `serve` logs it. */
let drafts = {};
/** Where each thread and draft sits in the document. */
const places = new Map();
let mermaid = null;
let diagrams = 0;

const $ = (id) => document.getElementById(id);

/** An element with attributes and children; strings become text. */
function el(tag, attributes = {}, ...children) {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attributes)) {
    if (value === false || value === null || value === undefined) continue;
    if (name.startsWith("on")) node.addEventListener(name.slice(2), value);
    else node.setAttribute(name, value === true ? "" : value);
  }
  node.append(...children.flat().filter((child) => child !== null && child !== undefined && child !== false));
  return node;
}

// Talking to serve

class Refused extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

async function call(method, path, body) {
  const headers = { Authorization: `Bearer ${token}` };
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const response = await fetch(path, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    cache: "no-store",
  });
  let data = null;
  try {
    data = await response.json();
  } catch {
    // An empty answer.
  }
  if (!response.ok) throw new Refused(response.status, data?.message ?? `the server answered ${response.status}`);
  return data;
}

/** A write from the page: a stale one shows the banner, a lost one marks the page offline. */
async function write(method, path, body) {
  try {
    return await call(method, path, body);
  } catch (error) {
    if (error instanceof Refused && error.status === 409) {
      stale = error.message;
      drawBanner();
    } else if (!(error instanceof Refused)) {
      setOffline(true);
    }
    throw error;
  }
}

async function poll() {
  let backoff = BACKOFF_FIRST;
  for (;;) {
    const query = view ? `?after=${view.seq}&hold=${HOLD}` : "";
    try {
      const next = await call("GET", `/api/review${query}`);
      backoff = BACKOFF_FIRST;
      const wasOffline = offline;
      setOffline(false);
      show(next);
      if (wasOffline) resend();
    } catch (error) {
      if (view?.phase === "approved") {
        drawApproved(null, true);
        return;
      }
      if (error instanceof Refused && (error.status === 401 || error.status === 403)) {
        fatal("This page cannot reach its review: the server there is another one, or was restarted with a new token. Open the URL gk md-review serve printed.");
        return;
      }
      setOffline(true);
      await new Promise((resolve) => setTimeout(resolve, backoff));
      backoff = Math.min(backoff * 2, BACKOFF_LAST);
    }
  }
}

function setOffline(now) {
  if (offline === now) return;
  offline = now;
  drawConnection();
}

// Drafts

function storageKey() {
  return `gk-md-review:${view.file}`;
}

function loadDrafts() {
  try {
    drafts = JSON.parse(localStorage.getItem(storageKey()) ?? "{}") ?? {};
  } catch {
    drafts = {};
  }
}

function saveDrafts() {
  try {
    localStorage.setItem(storageKey(), JSON.stringify(drafts));
  } catch {
    // Storage is full or off; the drafts live as long as the tab.
  }
}

function newId() {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  return "m-" + Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Drop drafts the server already logged, as after an answer that was lost. */
function reconcileDrafts() {
  const logged = new Set(view.threads.flatMap((thread) => thread.messages.map((message) => message.id)));
  let changed = false;
  for (const [key, draft] of Object.entries(drafts)) {
    if (draft.type !== "edit" && logged.has(draft.message)) {
      delete drafts[key];
      changed = true;
    }
  }
  if (changed) saveDrafts();
}

function resend() {
  for (const draft of Object.values(drafts)) {
    if (draft.status === "unsaved") send(draft);
  }
}

async function send(draft) {
  if (!draft.body.trim() || view.phase !== "open") return;
  draft.status = "sending";
  draft.error = null;
  saveDrafts();
  drawMargin();
  const at = { round: view.round, version: view.version };
  try {
    if (draft.type === "edit") {
      await write("PATCH", `/api/messages/${encodeURIComponent(draft.message)}`, { ...at, body: draft.body });
    } else {
      const target = draft.type === "thread" ? { anchor: draft.anchor } : { thread: draft.thread };
      const posted = await write("POST", "/api/threads", {
        ...at,
        message: draft.message,
        kind: "comment",
        body: draft.body,
        ...target,
      });
      if (focused === draft.key) focused = posted.thread;
    }
    delete drafts[draft.key];
  } catch (error) {
    draft.status = error instanceof Refused ? "editing" : "unsaved";
    draft.error = error instanceof Refused ? error.message : null;
  }
  saveDrafts();
  drawMargin();
}

function discard(draft) {
  delete drafts[draft.key];
  if (focused === draft.key) focused = null;
  saveDrafts();
  drawMargin();
}

// Drawing

function show(next) {
  const previous = view;
  view = next;
  if (!previous) {
    loadDrafts();
    document.title = `${view.file} · review`;
  }
  reconcileDrafts();
  if (view.phase === "approved") {
    drawApproved(view.approval, false);
    return;
  }
  if (previous && previous.seq === view.seq) {
    drawConnection();
    return;
  }
  // A newer review answers what a refused write was behind on.
  stale = null;
  const showChanges = $("show-changes").checked;
  if (!drawn || drawn.version !== view.version || drawn.showChanges !== showChanges) drawDocument();
  drawToolbar();
  drawBanner();
  drawMargin();
}

function fatal(message) {
  $("banner").hidden = false;
  $("banner").className = "banner stale";
  $("banner").replaceChildren(el("span", {}, message));
  setOffline(true);
}

function isPending(message) {
  return message.author === "human" && message.round === view.round && view.phase === "open";
}

function pendingCount() {
  return view.threads.flatMap((thread) => thread.messages).filter(isPending).length;
}

function drawToolbar() {
  $("file").textContent = view.file;
  const parts = [`round ${view.round}`, `v${view.version}`];
  const pending = pendingCount();
  if (pending) parts.push(`${pending} pending`);
  if (view.phase === "submitted") parts.push("agent is revising");
  $("where").textContent = "· " + parts.join(" · ");
  $("submit").disabled = view.phase !== "open" || view.blocks.length === 0;
  $("approve").disabled = view.phase === "approved";
  const changes = $("show-changes");
  changes.disabled = !view.changes;
  const open = view.threads.filter((thread) => thread.state === "open").length;
  $("drawer-toggle").textContent = open ? `Comments (${open})` : "Comments";
  drawConnection();
}

function clock(seconds) {
  return new Date(seconds * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function drawConnection() {
  const node = $("connection");
  const resume = $("resume");
  if (!view) return;
  const dot = el("span", { class: "dot" });
  resume.hidden = true;
  if (offline) {
    node.className = "connection offline";
    node.replaceChildren(dot, "server offline, reconnecting…");
    return;
  }
  if (view.agent.listening) {
    node.className = "connection listening";
    node.replaceChildren(dot, "agent listening");
    return;
  }
  node.className = "connection away";
  node.replaceChildren(dot, view.agent.last_wait ? `agent away since ${clock(view.agent.last_wait)}` : "agent not listening yet");
  // The page cannot wake the agent; it can only say how.
  const command = `gk md-review wait ${view.file} --timeout 110m`;
  resume.replaceChildren(
    view.phase === "submitted" ? "Your submit is kept. " : "",
    "To resume, ask the agent to run",
    el("code", {}, command),
    el("button", { type: "button", class: "link", onclick: () => navigator.clipboard?.writeText(command) }, "copy"),
  );
  resume.hidden = false;
}

function drawBanner() {
  const banner = $("banner");
  if (stale) {
    banner.className = "banner stale";
    banner.replaceChildren(
      el("span", {}, stale),
      el("button", { type: "button", onclick: () => location.reload() }, "Reload"),
    );
    banner.hidden = false;
  } else if (view.file_differs && view.phase === "open") {
    banner.className = "banner";
    banner.replaceChildren(
      el("span", {}, `${view.file} has changed since v${view.version}, the version on screen; comments refer to the text shown.`),
    );
    banner.hidden = false;
  } else {
    banner.hidden = true;
  }
}

function drawDocument() {
  const doc = $("document");
  const showChanges = $("show-changes").checked;
  drawn = { version: view.version, showChanges };
  doc.classList.toggle("changes", showChanges && Boolean(view.changes));
  if (view.blocks.length === 0) {
    doc.replaceChildren(el("p", { class: "placeholder" }, "Nothing to review: the file is empty."));
    return;
  }
  const deleted = new Map();
  for (const stub of view.changes?.deleted ?? []) {
    if (!deleted.has(stub.before)) deleted.set(stub.before, []);
    deleted.get(stub.before).push(stub);
  }
  const nodes = [];
  const pending = [];
  const template = document.createElement("template");
  view.blocks.forEach((block, index) => {
    nodes.push(...(deleted.get(index) ?? []).map(deletedStub));
    // The block's HTML is the server's rendering, with raw HTML dropped.
    template.innerHTML = block.html;
    const node = template.content.firstElementChild;
    const change = view.changes?.blocks[index];
    if (change && change.change !== "unchanged") {
      node.classList.add(change.change);
      node.dataset.change = change.change;
    }
    if (block.kind === "mermaid") pending.push({ node, source: block.text });
    nodes.push(node);
    if (change?.old_source !== undefined) {
      nodes.push(el("details", { class: "was" }, el("summary", {}, "was"), el("pre", {}, change.old_source)));
    }
  });
  nodes.push(...(deleted.get(view.blocks.length) ?? []).map(deletedStub));
  doc.replaceChildren(...nodes);
  drawDiagrams(pending);
}

function deletedStub(stub) {
  const first = stub.source.split("\n").find((line) => line.trim()) ?? "";
  const label = first.length > 60 ? first.slice(0, 60) + "…" : first;
  return el("details", { class: "deleted" }, el("summary", {}, `deleted: ${label}`), el("pre", { class: "old" }, stub.source));
}

function loadMermaid() {
  mermaid ??= new Promise((resolve, reject) => {
    const script = el("script", { src: "/assets/mermaid.min.js" });
    script.addEventListener("load", () => {
      window.mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "default",
      });
      resolve(window.mermaid);
    });
    script.addEventListener("error", () => reject(new Error("mermaid could not be loaded")));
    document.head.append(script);
  });
  return mermaid;
}

/** Replace each mermaid block's source with its diagram; a failure keeps the source and says why. */
async function drawDiagrams(pending) {
  if (pending.length === 0) return;
  let library;
  try {
    library = await loadMermaid();
  } catch (error) {
    for (const { node } of pending) node.append(el("p", { class: "mermaid-error" }, error.message));
    return;
  }
  for (const { node, source } of pending) {
    try {
      const { svg } = await library.render(`diagram-${++diagrams}`, source);
      const holder = el("div", { class: "mermaid-diagram" });
      holder.innerHTML = svg;
      node.replaceChildren(holder);
    } catch (error) {
      node.append(el("p", { class: "mermaid-error" }, `This diagram does not render: ${error.message ?? error}`));
    }
  }
  drawHighlights();
  layout();
}

function blockNode(index) {
  return $("document").querySelector(`:scope > .block[data-block="${index}"]`);
}

/** Visible characters before `container`/`offset` in a block, or null when the page's text of the block is not the server's. */
function offsetIn(node, container, offset) {
  const index = Number(node.dataset.block);
  if (visible(node.textContent) !== visible(view.blocks[index].text)) return null;
  const before = document.createRange();
  before.setStart(node, 0);
  before.setEnd(container, offset);
  return visible(before.toString());
}

/** The DOM point before the `at`th visible character of a block, or after it for an end. */
function pointAt(node, at, end) {
  const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
  let seen = 0;
  let last = null;
  for (let text = walker.nextNode(); text; text = walker.nextNode()) {
    let offset = 0;
    for (const c of text.data) {
      if (visible(c)) {
        if (!end && seen === at) return [text, offset];
        seen += 1;
        if (end && seen === at) return [text, offset + c.length];
      }
      offset += c.length;
    }
    last = text;
  }
  return last ? [last, last.data.length] : [node, node.childNodes.length];
}

function selectionAnchor() {
  const selection = getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return null;
  const range = selection.getRangeAt(0);
  const touched = [...$("document").querySelectorAll(":scope > .block")].filter((node) => range.intersectsNode(node));
  if (touched.length === 0) return null;
  const first = touched[0];
  const last = touched[touched.length - 1];
  const start = {
    block: Number(first.dataset.block),
    at: first.contains(range.startContainer) ? offsetIn(first, range.startContainer, range.startOffset) : null,
  };
  const end = {
    block: Number(last.dataset.block),
    at: last.contains(range.endContainer) ? offsetIn(last, range.endContainer, range.endOffset) : null,
  };
  return anchorFor(view.blocks, start, end, view.version);
}

function onSelect() {
  if (!view || view.phase !== "open") return;
  const anchor = selectionAnchor();
  if (!anchor) return;
  // A new selection replaces an empty draft rather than piling them up.
  for (const draft of Object.values(drafts)) {
    if (draft.type === "thread" && !draft.body.trim()) delete drafts[draft.key];
  }
  const message = newId();
  drafts[message] = { key: message, type: "thread", message, anchor, body: "", status: "editing" };
  saveDrafts();
  focused = message;
  openDrawer();
  drawMargin();
  document.querySelector(`[data-key="${message}"]`)?.focus();
}

/** Where every thread and draft sits: a range, or whole blocks. */
function measurePlaces() {
  places.clear();
  const anchored = [
    ...view.threads.filter((thread) => !thread.orphaned).map((thread) => [thread.id, thread.anchor]),
    ...Object.values(drafts)
      .filter((draft) => draft.type === "thread" && draft.anchor.version === view.version)
      .map((draft) => [draft.key, draft.anchor]),
  ];
  for (const [id, anchor] of anchored) {
    const found = locate(view.blocks, anchor);
    if (!found) continue;
    const blocks = [];
    for (let index = found.start.block; index <= found.end.block; index++) blocks.push(index);
    const whole = blocks.some((index) => {
      const node = blockNode(index);
      return !node || selectsWhole(view.blocks[index].kind) || visible(node.textContent) !== visible(view.blocks[index].text);
    });
    const place = { blocks, order: [found.start.block, found.start.at] };
    if (!whole) {
      const range = document.createRange();
      range.setStart(...pointAt(blockNode(found.start.block), found.start.at, false));
      range.setEnd(...pointAt(blockNode(found.end.block), found.end.at, true));
      place.range = range;
    }
    places.set(id, place);
  }
}

function drawHighlights() {
  if (!view) return;
  measurePlaces();
  for (const node of $("document").querySelectorAll(".whole-anchor")) node.classList.remove("whole-anchor", "focused");
  const plain = [];
  const strong = [];
  for (const [id, place] of places) {
    if (place.range) {
      (id === focused ? strong : plain).push(place.range);
    } else {
      for (const index of place.blocks) {
        const node = blockNode(index);
        node?.classList.add("whole-anchor");
        if (id === focused) node?.classList.add("focused");
      }
    }
  }
  if (globalThis.CSS?.highlights && globalThis.Highlight) {
    CSS.highlights.set("threads", new Highlight(...plain));
    CSS.highlights.set("focused", new Highlight(...strong));
  }
}

function label(thread) {
  if (thread.messages.some(isPending) && thread.messages.every(isPending)) return "pending";
  if (thread.state === "applied") return `applied in round ${thread.applied_in}`;
  if (thread.state === "resolved") return "resolved";
  const agent = [...thread.messages].reverse().find((message) => message.author === "agent");
  if (view.phase === "submitted" && thread.messages.some((message) => message.round === view.round)) {
    return "sent, agent is revising";
  }
  if (agent?.outcome === "declined" && agent.round === thread.messages.at(-1).round) return "declined";
  return "open";
}

function textArea(draft, placeholder) {
  const area = el("textarea", { rows: 3, placeholder, "data-key": draft.key });
  area.value = draft.body;
  area.addEventListener("input", () => {
    draft.body = area.value;
    saveDrafts();
  });
  area.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      send(draft);
    } else if (event.key === "Escape") {
      area.blur();
    }
  });
  area.addEventListener("focus", () => setFocus(draft.thread ?? draft.key));
  return area;
}

function draftControls(draft, verb) {
  const sending = draft.status === "sending";
  return [
    draft.status === "unsaved" ? el("p", { class: "unsaved" }, "not saved yet; sent when the server is back") : null,
    draft.error ? el("p", { class: "error" }, draft.error) : null,
    el(
      "div",
      { class: "row" },
      el("button", { type: "button", class: "primary", disabled: sending || view.phase !== "open", onclick: () => send(draft) }, verb),
      el("button", { type: "button", onclick: () => discard(draft) }, "Discard"),
    ),
  ];
}

function draftCard(draft) {
  const old = draft.anchor.version !== view.version;
  return el(
    "div",
    { class: `card${old ? " orphaned" : ""}`, "data-id": draft.key },
    el("div", { class: "head" }, el("span", {}, old ? `draft on v${draft.anchor.version}; select the text again` : "new comment")),
    old ? el("div", { class: "quote" }, draft.anchor.quote) : null,
    textArea(draft, "Comment"),
    draftControls(draft, "Comment"),
  );
}

function messageView(thread, message) {
  const edit = drafts[`edit:${message.id}`];
  const pending = isPending(message);
  const who =
    message.author === "human" ? "You" : message.outcome ? `Agent · ${message.outcome}` : "Agent";
  return el(
    "div",
    { class: `message ${message.author}` },
    el("div", { class: "who" }, `${who} · round ${message.round}${pending ? " · pending" : ""}`),
    edit ? [textArea(edit, "Comment"), draftControls(edit, "Save")] : el("div", { class: "body" }, message.body),
    pending && !edit
      ? el(
          "div",
          { class: "row" },
          el("button", { type: "button", class: "link", onclick: () => startEdit(thread, message) }, "Edit"),
          el("button", { type: "button", class: "link", onclick: () => remove(message) }, "Delete"),
        )
      : null,
  );
}

function threadCard(thread) {
  const reply = drafts[`reply:${thread.id}`];
  const closed = thread.state !== "open";
  const card = el("div", { class: "card", "data-id": thread.id });
  card.classList.toggle("orphaned", Boolean(thread.orphaned));
  const head = el(
    "div",
    { class: "head" },
    el("span", {}, `${thread.id} · ${thread.orphaned ? "orphaned · " : ""}${label(thread)}`),
    thread.orphaned ? el("span", {}, `lines ${thread.anchor.lines[0]}–${thread.anchor.lines[1]} of v${thread.anchor.version}`) : null,
  );
  if (closed && !expanded.has(thread.id) && focused !== thread.id && !reply) {
    card.classList.add("stub");
    const first = thread.messages[0]?.body.split("\n")[0] ?? "";
    card.append(
      head,
      el(
        "div",
        { class: "row" },
        el("span", { class: "muted" }, first.length > 50 ? first.slice(0, 50) + "…" : first),
        el("button", { type: "button", class: "link", onclick: () => { expanded.add(thread.id); drawMargin(); } }, "Expand"),
      ),
    );
    return card;
  }
  card.append(head);
  if (thread.orphaned) card.append(el("div", { class: "quote" }, thread.anchor.quote));
  for (const message of thread.messages) card.append(messageView(thread, message));
  if (reply) {
    card.append(textArea(reply, "Reply"), ...draftControls(reply, "Reply").filter(Boolean));
  } else if (view.phase === "open" && !thread.messages.some(isPending)) {
    card.append(
      el(
        "div",
        { class: "row" },
        el("button", { type: "button", onclick: () => startReply(thread) }, closed ? "Reopen" : "Reply"),
        closed ? null : el("button", { type: "button", onclick: () => resolve(thread) }, "Resolve"),
        closed ? el("button", { type: "button", class: "link", onclick: () => { expanded.delete(thread.id); drawMargin(); } }, "Collapse") : null,
      ),
    );
  }
  return card;
}

function drawMargin() {
  if (!view || view.phase === "approved") return;
  const active = document.activeElement;
  const key = active?.dataset?.key;
  const selection = key ? [active.selectionStart, active.selectionEnd] : null;

  drawHighlights();
  const margin = $("margin");
  const top = [];
  const anchored = [];
  for (const draft of Object.values(drafts)) {
    if (draft.type !== "thread") continue;
    (draft.anchor.version === view.version && places.has(draft.key) ? anchored : top).push([draft.key, draftCard(draft)]);
  }
  const resolved = view.threads.filter((thread) => thread.state === "resolved").length;
  for (const thread of view.threads) {
    if (hideResolved && thread.state === "resolved" && focused !== thread.id) continue;
    (places.has(thread.id) ? anchored : top).push([thread.id, threadCard(thread)]);
  }
  anchored.sort(([a], [b]) => {
    const [x, y] = [places.get(a).order, places.get(b).order];
    return x[0] - y[0] || x[1] - y[1];
  });
  const children = [];
  if (resolved) {
    const box = el("input", { type: "checkbox", onchange: (event) => { hideResolved = event.target.checked; drawMargin(); } });
    box.checked = hideResolved;
    children.push(el("label", { class: "filter" }, box, `Hide resolved (${resolved})`));
  }
  if (view.threads.length === 0 && anchored.length === 0 && top.length === 0 && view.phase === "open") {
    children.push(el("p", { class: "hint" }, "Select text to comment."));
  }
  if (top.length) children.push(el("div", { class: "orphans" }, top.map(([, card]) => card)));
  const cards = el("div", { class: "cards" }, anchored.map(([, card]) => card));
  children.push(cards);
  margin.replaceChildren(...children);
  for (const card of margin.querySelectorAll(".card")) {
    card.classList.toggle("focused", card.dataset.id === focused);
    card.addEventListener("mousedown", () => setFocus(card.dataset.id));
  }
  if (key) {
    const again = margin.querySelector(`[data-key="${CSS.escape(key)}"]`);
    if (again) {
      again.focus();
      again.setSelectionRange(...selection);
    }
  }
  drawToolbar();
  layout();
}

/** Put each anchored card level with its text. */
function layout() {
  const cards = $("margin")?.querySelector(".cards");
  if (!cards) return;
  for (const line of cards.querySelectorAll(".connector")) line.remove();
  const nodes = [...cards.querySelectorAll(":scope > .card")];
  if (NARROW.matches) {
    cards.classList.remove("packed");
    cards.style.minHeight = "";
    for (const node of nodes) node.style.top = "";
    return;
  }
  cards.classList.add("packed");
  const origin = cards.getBoundingClientRect().top;
  const entries = nodes.map((node) => {
    const place = places.get(node.dataset.id);
    const rect = place?.range ? place.range.getClientRects()[0] ?? place.range.getBoundingClientRect() : blockNode(place?.blocks[0])?.getBoundingClientRect();
    return { node, top: rect ? rect.top - origin : 0, height: node.offsetHeight };
  });
  const tops = pack(entries, entries.findIndex((entry) => entry.node.dataset.id === focused));
  let bottom = 0;
  entries.forEach((entry, index) => {
    entry.node.style.top = `${tops[index]}px`;
    bottom = Math.max(bottom, tops[index] + entry.height);
    if (tops[index] - entry.top > 6) {
      const line = el("div", { class: "connector" });
      line.style.top = `${entry.top + 8}px`;
      line.style.height = `${tops[index] - entry.top}px`;
      cards.append(line);
    }
  });
  cards.style.minHeight = `${bottom}px`;
}

function setFocus(id) {
  if (focused === id) return;
  focused = id;
  for (const card of $("margin").querySelectorAll(".card")) card.classList.toggle("focused", card.dataset.id === id);
  drawHighlights();
  layout();
}

function openDrawer() {
  if (NARROW.matches) document.body.classList.add("drawer");
}

// Actions

function startReply(thread) {
  const key = `reply:${thread.id}`;
  drafts[key] ??= { key, type: "reply", thread: thread.id, message: newId(), body: "", status: "editing" };
  saveDrafts();
  expanded.add(thread.id);
  focused = thread.id;
  drawMargin();
  document.querySelector(`[data-key="${CSS.escape(key)}"]`)?.focus();
}

function startEdit(thread, message) {
  const key = `edit:${message.id}`;
  drafts[key] ??= { key, type: "edit", thread: thread.id, message: message.id, body: message.body, status: "editing" };
  saveDrafts();
  focused = thread.id;
  drawMargin();
  document.querySelector(`[data-key="${CSS.escape(key)}"]`)?.focus();
}

async function remove(message) {
  try {
    await write("DELETE", `/api/messages/${encodeURIComponent(message.id)}`, { round: view.round, version: view.version });
  } catch {
    // The banner or the connection indicator says why.
  }
}

async function resolve(thread) {
  try {
    await write("POST", `/api/threads/${encodeURIComponent(thread.id)}/resolve`, { round: view.round, version: view.version });
  } catch {
    // The banner or the connection indicator says why.
  }
}

function pendingList() {
  const items = view.threads
    .filter((thread) => thread.messages.some(isPending))
    .map((thread) => {
      const message = thread.messages.find(isPending);
      return el("li", {}, el("span", { class: "muted" }, `${thread.id}: `), message.body.split("\n")[0]);
    });
  return items.length ? el("ul", {}, items) : null;
}

function unsentNote() {
  const unsent = Object.values(drafts).filter((draft) => draft.type !== "summary" && draft.body.trim()).length;
  return unsent ? el("p", { class: "muted" }, `${unsent} unsaved draft${unsent > 1 ? "s are" : " is"} not included.`) : null;
}

function openSubmit() {
  const summary = $("summary");
  summary.value = drafts.summary?.body ?? "";
  const pending = pendingCount();
  const update = () => {
    $("submit-confirm").disabled = pending === 0 && !summary.value.trim();
  };
  summary.oninput = () => {
    drafts.summary = { key: "summary", type: "summary", body: summary.value };
    saveDrafts();
    update();
  };
  $("submit-pending").replaceChildren(
    el(
      "div",
      {},
      pending ? el("p", {}, `${pending} comment${pending > 1 ? "s" : ""} go to the agent:`) : el("p", { class: "muted" }, "No comments; send an overall comment, or approve instead."),
      pendingList(),
      unsentNote(),
    ),
  );
  $("submit-error").hidden = true;
  update();
  $("submit-dialog").showModal();
}

async function confirmSubmit() {
  const summary = $("summary").value.trim();
  try {
    await write("POST", "/api/submit", { round: view.round, summary: summary || null });
    delete drafts.summary;
    saveDrafts();
    $("submit-dialog").close();
  } catch (error) {
    $("submit-error").textContent = error.message;
    $("submit-error").hidden = false;
  }
}

function openApprove() {
  const pending = pendingCount();
  $("approve-covers").textContent =
    view.phase === "submitted"
      ? `The agent is revising. The approval covers v${view.version}, the version on screen, not the edits in progress.`
      : `Approve round ${view.round}, v${view.version}, as shown.`;
  $("approve-pending").replaceChildren(
    el(
      "div",
      {},
      pending ? el("p", {}, `${pending} pending comment${pending > 1 ? "s are" : " is"} discarded:`) : null,
      pending ? pendingList() : null,
      unsentNote(),
    ),
  );
  $("approve-back").hidden = pending === 0;
  $("approve-confirm").textContent = pending ? "Discard and approve" : "Approve";
  $("approve-error").hidden = true;
  $("approve-dialog").showModal();
}

async function confirmApprove() {
  const note = $("note").value.trim();
  try {
    const approved = await write("POST", "/api/approve", { round: view.round, version: view.version, note: note || null });
    $("approve-dialog").close();
    drawApproved({ round: view.round, version: view.version, note, record: approved.record, store_kept: approved.store_kept }, false);
  } catch (error) {
    $("approve-error").textContent = error.message;
    $("approve-error").hidden = false;
  }
}

/** The final screen; `gone` once the server has ended after delivering the approval. */
function drawApproved(approval, gone) {
  const final = document.querySelector(".final");
  if (final) {
    if (gone) final.querySelector(".status").textContent = "The agent received the approval and the review server has ended. You can close this page.";
    return;
  }
  drafts = {};
  if (view) saveDrafts();
  document.querySelector(".review")?.remove();
  $("banner").hidden = true;
  $("submit").disabled = true;
  $("approve").disabled = true;
  $("where").textContent = "· approved";
  $("connection").replaceChildren();
  $("resume").hidden = true;
  const status = gone
    ? "The agent received the approval and the review server has ended. You can close this page."
    : "The agent receives the approval the next time it waits; then the review server ends.";
  const screen = el(
    "section",
    { class: "final" },
    el("h1", {}, "Approved"),
    approval ? el("p", {}, `Round ${approval.round}, v${approval.version} of ${view.file}.`) : null,
    approval?.note ? el("p", {}, "Note: ", approval.note) : null,
    approval ? el("p", {}, "Review record: ", el("code", {}, approval.record)) : null,
    approval?.store_kept
      ? el("p", { class: "error" }, `The review store was kept: ${approval.store_kept}. Stop the server with `, el("code", {}, `gk md-review stop ${view.file}`), ".")
      : null,
    el("p", { class: "status muted" }, status),
  );
  document.body.append(screen);
}

// Wiring

function wire() {
  const doc = $("document");
  doc.addEventListener("mouseup", () => setTimeout(onSelect, 0));
  doc.addEventListener("keyup", (event) => {
    if (event.shiftKey) onSelect();
  });
  doc.addEventListener("click", (event) => {
    if (!getSelection().isCollapsed) return;
    const point = document.caretPositionFromPoint?.(event.clientX, event.clientY);
    const range = point ? null : document.caretRangeFromPoint?.(event.clientX, event.clientY);
    const [node, offset] = point ? [point.offsetNode, point.offset] : range ? [range.startContainer, range.startOffset] : [null, 0];
    const block = event.target.closest?.(".block");
    for (const [id, place] of places) {
      const inRange = node && place.range?.isPointInRange(node, offset);
      const inBlock = !place.range && block && place.blocks.includes(Number(block.dataset.block));
      if (inRange || inBlock) {
        openDrawer();
        setFocus(id);
        $("margin").querySelector(`[data-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" });
        return;
      }
    }
  });
  $("show-changes").addEventListener("change", () => {
    drawDocument();
    drawMargin();
  });
  $("drawer-toggle").addEventListener("click", () => document.body.classList.toggle("drawer"));
  $("submit").addEventListener("click", openSubmit);
  $("submit-confirm").addEventListener("click", confirmSubmit);
  $("approve").addEventListener("click", openApprove);
  $("approve-confirm").addEventListener("click", confirmApprove);
  $("approve-back").addEventListener("click", () => {
    $("approve-dialog").close();
    openSubmit();
  });
  for (const button of document.querySelectorAll("dialog [data-close]")) {
    button.addEventListener("click", () => button.closest("dialog").close());
  }
  new ResizeObserver(() => layout()).observe(doc);
  // Sticky offsets and the drawer sit below the header, whose height varies.
  new ResizeObserver(() => document.documentElement.style.setProperty("--header-height", `${$("top").offsetHeight}px`)).observe($("top"));
  NARROW.addEventListener("change", () => layout());
}

wire();
poll();
