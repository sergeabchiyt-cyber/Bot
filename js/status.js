/* Original status/settings drawer, now sourced from read-only health contracts. */
(function (App) {
  "use strict";
  const { $, escapeHtml } = App.utils;
  const state = { node1: null, error: null, stamp: null, open: false };
  const safe = (v) => escapeHtml(v == null ? "—" : v);
  function render() {
    const body = $("settings-body"); if (!body) return;
    const n1 = state.node1 || {}, freshness = n1.freshness || {};
    const n3 = App.node3 && App.node3.getState ? App.node3.getState() : {};
    const h = n3.health || {};
    const rows = [["Node 1 freshness",freshness.state],["Node 1 contract",n1.contract_version],["Feed",freshness.feed],
      ["Last tick age",freshness.age_ms == null ? "—" : `${Math.round(freshness.age_ms/1000)} s`],
      ["Node 3 health",h.status],["Node 3 freshness",h.freshness_state],["Node 3 contract",h.contract_version],
      ["Contract violations",h.contract_violations]];
    body.innerHTML = `<section class="drawer-section"><h3>Service status</h3><div class="drawer-rows">${rows.map(([k,v])=>`<div class="drawer-row"><span class="drawer-key">${safe(k)}</span><b class="drawer-val">${safe(v)}</b></div>`).join("")}</div></section><p class="drawer-note">Read-only dashboard. Values are published by Node 1 and Node 3. No order entry or sizing is available.</p>${state.error?`<div class="empty inline">Node 1 status unavailable · last good ${state.stamp?safe(App.format.time(state.stamp)):"—"}. Check Node 1 CORS if this is a browser error.</div>`:""}`;
    const stamp=$("settings-stamp"); if(stamp) stamp.textContent=state.stamp?App.format.time(state.stamp):"—";
  }
  function ingest(data) {
    state.node1=data; state.error=null; state.stamp=Date.now();
    if(App.node3&&App.node3.clearNode1Error)App.node3.clearNode1Error("status");
    const f=data.freshness||{}, badge=$("node1-freshness"), label=$("node1-freshness-label"), market=$("market-state"), content=$("market-body");
    if(badge){badge.dataset.state=f.state==="live"?"live":f.state==="stale"?"stale":f.state==="closed_market"?"closed":"offline";badge.title=`Node 1 freshness ${f.state||"unknown"}${f.market_open===false?" · market closed":f.feed?` · ${f.feed}`:""}`;}
    if(label) label.textContent=`NODE 1 ${String(f.state||"unknown").toUpperCase()}`;
    const contract=$("node1-contract");if(contract)contract.textContent=`N1 contract ${data.contract_version||"—"}`;
    if(market) market.textContent=String(f.state||"unknown").toUpperCase();
    if(content) content.innerHTML=`<div class="drawer-rows"><div class="drawer-row"><span class="drawer-key">Feed</span><b class="drawer-val">${safe(f.feed)}</b></div><div class="drawer-row"><span class="drawer-key">Last tick</span><b class="drawer-val">${f.age_ms==null?"—":safe(`${Math.round(f.age_ms/1000)}s ago`)}</b></div><div class="drawer-row"><span class="drawer-key">Market</span><b class="drawer-val">${f.market_open===true?"Open":f.market_open===false?"Closed":"—"}</b></div><div class="drawer-row"><span class="drawer-key">Contract</span><b class="drawer-val">${safe(data.contract_version)}</b></div></div>`;
    render();
    if(App.node3) App.node3.updateWarnings && App.node3.updateWarnings();
  }
  async function load() { try { ingest(await App.api.fetchStatus()); } catch(error) { state.error=error.message; render(); if(App.node3) App.node3.updateWarnings && App.node3.updateWarnings(); } }
  function setOpen(open) { state.open=!!open; const drawer=$("settings-drawer"), backdrop=$("settings-backdrop"), trigger=$("settings-open"); if(drawer)drawer.hidden=!state.open;if(backdrop)backdrop.hidden=!state.open;if(trigger)trigger.setAttribute("aria-expanded",String(state.open));if(state.open)render(); }
  function start() { $("settings-open")?.addEventListener("click",()=>setOpen(!state.open));$("settings-close")?.addEventListener("click",()=>setOpen(false));$("settings-backdrop")?.addEventListener("click",()=>setOpen(false));document.addEventListener("keydown",e=>{if(e.key==="Escape"&&state.open)setOpen(false);});load();setInterval(load,App.config.statusRefreshMs); }
  App.status={start,load,render,ingest,getState:()=>({...state})};
})((window.App=window.App||{}));
