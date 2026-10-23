// dom-shim.js — minimal DOM implementation for headless panel tests.
// Implements exactly what src/render/dom.js (h/clear) touches:
// createElement, createTextNode, appendChild, removeChild, firstChild,
// className, attributes, dataset, style, addEventListener/removeEventListener.
//
// Import this module BEFORE any render module so `document` exists.

class TextNode {
  constructor(text) {
    this.nodeType = 3;
    this.text = String(text);
  }
}

class Element {
  constructor(tag) {
    this.nodeType = 1;
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.attributes = {};
    this.style = {};
    this.dataset = {};
    this.listeners = {};
    this.clientWidth = 800;
    this.clientHeight = 420;
  }

  // Canvas stub: a no-op 2D context so chart drawing runs headlessly.
  getContext() {
    if (!this._ctx) {
      const noop = () => {};
      this._ctx = {
        setTransform: noop, clearRect: noop, fillRect: noop, strokeRect: noop,
        beginPath: noop, moveTo: noop, lineTo: noop, arc: noop, stroke: noop,
        fill: noop, fillText: noop, strokeText: noop, setLineDash: noop,
        save: noop, restore: noop, closePath: noop, rect: noop,
        canvas: this,
      };
    }
    return this._ctx;
  }

  get className() {
    return this.attributes["class"] || "";
  }

  set className(value) {
    this.attributes["class"] = String(value);
  }

  setAttribute(key, value) {
    this.attributes[key] = String(value);
  }

  getAttribute(key) {
    return this.attributes[key] ?? null;
  }

  appendChild(child) {
    this.children.push(child);
    return child;
  }

  append(...children) {
    this.children.push(...children);
  }

  removeChild(child) {
    const idx = this.children.indexOf(child);
    if (idx >= 0) this.children.splice(idx, 1);
    return child;
  }

  get firstChild() {
    return this.children[0] || null;
  }

  addEventListener(type, fn) {
    (this.listeners[type] = this.listeners[type] || []).push(fn);
  }

  removeEventListener(type, fn) {
    const arr = this.listeners[type] || [];
    const idx = arr.indexOf(fn);
    if (idx >= 0) arr.splice(idx, 1);
  }

  dispatch(type, event) {
    for (const fn of this.listeners[type] || []) fn(event);
  }

  // Plain text of this element (not markup).
  get textContent() {
    let out = "";
    for (const child of this.children) {
      out += child.nodeType === 3 ? child.text : child.textContent;
    }
    return out;
  }

  // Nested markup string: <div class="x">text<span>..</span></div>
  toMarkup() {
    const attrs = Object.entries(this.attributes)
      .map(([k, v]) => (v === "" ? ` ${k}` : ` ${k}="${v}"`))
      .join("");
    if (this.children.length === 0) {
      return `<${this.tagName.toLowerCase()}${attrs}/>`;
    }
    const inner = this.children
      .map((c) => (c.nodeType === 3 ? c.text : c.toMarkup()))
      .join("");
    return `<${this.tagName.toLowerCase()}${attrs}>${inner}</${this.tagName.toLowerCase()}>`;
  }
}

export function installDomShim({ innerWidth = 1500 } = {}) {
  const document = {
    createElement(tag) {
      return new Element(tag);
    },
    createTextNode(text) {
      return new TextNode(text);
    },
    getElementById() {
      return null;
    },
    querySelector() {
      return null;
    },
  };
  const window = {
    devicePixelRatio: 1,
    innerWidth,
    addEventListener() {},
    removeEventListener() {},
  };
  globalThis.document = document;
  globalThis.window = window;
  return { document, window, Element, TextNode };
}

// Recursive text search helper: does the element tree contain `needle`
// (in text content or attribute values)?
export function contains(node, needle) {
  if (!node) return false;
  if (node.nodeType === 3) return String(node.text).includes(needle);
  if (node.tagName && node.textContent && node.textContent.includes(needle)) {
    return true;
  }
  return Object.values(node.attributes || {}).some((v) => String(v).includes(needle))
    || node.children.some((c) => contains(c, needle));
}

export function markupContains(node, needle) {
  if (!node) return false;
  if (node.nodeType === 3) return String(node.text).includes(needle);
  return node.toMarkup().includes(needle)
    || node.children.some((c) => markupContains(c, needle));
}
