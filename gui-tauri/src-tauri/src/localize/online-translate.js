// 注入远端 claude.ai 页面的 DOM 层翻译脚本。由 online.rs 包成
// `(function(CFG){ …本文件… })(<配置 JSON>)`，通过主进程 webContents.executeJavaScript 送入。
// 只改用户看到的文本节点和少量属性，不碰请求、不碰对话正文、不碰输入框。
"use strict";
var W = window, D = document, L = W.location;
if (!L || !(L.hostname === CFG.host || L.hostname.endsWith("." + CFG.host))) {
  return { skipped: "host", host: L && L.hostname };
}
if (W[CFG.marker]) {
  return W[CFG.marker].stats();
}

var stats = {
  replaced: 0, attrs: 0, nodes: 0, passes: 0,
  dict: 0, url: L.href, samples: [], errors: []
};
var dict = CFG.dict || {};
for (var k in dict) { if (Object.prototype.hasOwnProperty.call(dict, k)) stats.dict++; }

var patterns = [];
(CFG.patterns || []).forEach(function (p) {
  try { patterns.push([new RegExp(p[0]), p[1]]); } catch (e) { stats.errors.push("pattern:" + p[0]); }
});
var skipSel = (CFG.skipSelectors || []).join(",");
var SKIP_TAGS = { SCRIPT: 1, STYLE: 1, TEXTAREA: 1, INPUT: 1, CODE: 1, PRE: 1, NOSCRIPT: 1, SVG: 1, KBD: 1 };
var ATTRS = CFG.attrs || ["placeholder", "aria-label", "title", "alt"];

function isSkippedElement(el) {
  if (SKIP_TAGS[String(el.tagName).toUpperCase()]) return true;
  if (el.isContentEditable) return true;
  if (skipSel && el.matches) {
    try { if (el.matches(skipSel)) return true; } catch (e) { skipSel = ""; stats.errors.push("skipSelectors"); }
  }
  return false;
}
function insideSkipped(el) {
  for (var e = el; e && e.nodeType === 1; e = e.parentElement) {
    if (isSkippedElement(e)) return true;
  }
  return false;
}

var SUFFIX = /^(.*?)(\.\.\.|…|:|：)$/;
function lookup(t) {
  var hit = dict[t];
  if (hit !== undefined) return hit;
  var m = SUFFIX.exec(t);
  if (m) {
    var core = m[1].replace(/\s+$/, "");
    if (dict[core] !== undefined) return dict[core] + m[2];
  }
  for (var i = 0; i < patterns.length; i++) {
    if (patterns[i][0].test(t)) return t.replace(patterns[i][0], patterns[i][1]);
  }
  return undefined;
}
function translate(s) {
  if (!s || !/[A-Za-z]/.test(s)) return null;
  var t = s.replace(/^\s+|\s+$/g, "");
  if (!t) return null;
  var hit = lookup(t);
  if (hit === undefined || hit === t) return null;
  var lead = /^\s*/.exec(s)[0], trail = /\s*$/.exec(s)[0];
  return lead + hit + trail;
}

function processText(node) {
  var v = node.nodeValue;
  var out = translate(v);
  if (out !== null) {
    node.nodeValue = out;
    stats.replaced++;
    if (stats.samples.length < 12) stats.samples.push([v.replace(/^\s+|\s+$/g, ""), out.replace(/^\s+|\s+$/g, "")]);
  }
}
function processAttrs(el) {
  if (!el.hasAttribute) return;
  for (var i = 0; i < ATTRS.length; i++) {
    var a = ATTRS[i];
    if (!el.hasAttribute(a)) continue;
    var out = translate(el.getAttribute(a));
    if (out !== null) { el.setAttribute(a, out); stats.attrs++; }
  }
}
function walk(root) {
  if (!root) return;
  if (root.nodeType === 3) {
    if (!insideSkipped(root.parentElement)) processText(root);
    return;
  }
  if (root.nodeType !== 1 && root.nodeType !== 11) return;
  if (root.nodeType === 1) {
    if (insideSkipped(root)) return;
    processAttrs(root);
  }
  var walker = D.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: function (n) {
      if (n.nodeType === 1) return isSkippedElement(n) ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
      return NodeFilter.FILTER_ACCEPT;
    }
  });
  var n;
  while ((n = walker.nextNode())) {
    stats.nodes++;
    if (n.nodeType === 1) processAttrs(n); else processText(n);
  }
}

// 变更合并：去抖 debounceMs，但最长 maxWaitMs 必刷一次，流式输出时不会饿死。
var pending = [], pendingSet = typeof Set === "function" ? new Set() : null;
var timer = null, firstDue = 0;
function flush() {
  timer = null; firstDue = 0;
  var batch = pending; pending = []; if (pendingSet) pendingSet.clear();
  stats.passes++;
  for (var i = 0; i < batch.length; i++) {
    var n = batch[i];
    if (n.isConnected !== false) { try { walk(n); } catch (e) { stats.errors.push("walk:" + (e && e.message)); } }
  }
}
function schedule(n) {
  if (!n) return;
  if (pendingSet) { if (pendingSet.has(n)) return; pendingSet.add(n); }
  pending.push(n);
  var now = Date.now();
  if (!firstDue) firstDue = now;
  if (timer) clearTimeout(timer);
  var wait = Math.max(0, Math.min(CFG.debounceMs, firstDue + CFG.maxWaitMs - now));
  timer = setTimeout(flush, wait);
}
var observer = new MutationObserver(function (muts) {
  for (var i = 0; i < muts.length; i++) {
    var m = muts[i];
    if (m.type === "childList") {
      for (var j = 0; j < m.addedNodes.length; j++) schedule(m.addedNodes[j]);
    } else {
      schedule(m.target);
    }
  }
});

function start() {
  try { walk(D.body); stats.passes++; } catch (e) { stats.errors.push("initial:" + (e && e.message)); }
  observer.observe(D.documentElement, {
    childList: true, subtree: true, characterData: true,
    attributes: true, attributeFilter: ATTRS
  });
}
if (D.body) start(); else D.addEventListener("DOMContentLoaded", start, { once: true });

if (CFG.localeLockKey) {
  try { W.localStorage.setItem(CFG.localeLockKey, CFG.localeLockValue); } catch (e) { stats.errors.push("localeLock"); }
}

W[CFG.marker] = {
  stats: function () { return stats; },
  rerun: function () { walk(D.body); return stats; },
  stop: function () { observer.disconnect(); }
};

if (CFG.reportDelayMs) {
  return new Promise(function (resolve) { setTimeout(function () { resolve(stats); }, CFG.reportDelayMs); });
}
return stats;
