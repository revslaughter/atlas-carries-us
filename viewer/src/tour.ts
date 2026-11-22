// The guided tour: ordered steps from atlas.yaml. Each step opens a view and
// shows a short text. The tour explains; the map and the panel stay reference.

import { M, go } from "./app";
import type { State } from "./scene";
import { draftedNote, esc, inlineHTML } from "./text";
import type { View } from "./types";

const el = document.getElementById("tour")!;
let stepIndex = -1;

export function viewToState(v: View): State {
  switch (v.view) {
    case "overview": return { v: "overview" };
    case "region": return { v: "overview", region: v.id };
    case "unit": return { v: "unit", id: v.id };
    case "symbol": {
      const o = M.owner.get(v.id) ?? v.id;
      const m = M.memberName(v.id);
      return m ? { v: "symbol", id: o, member: m } : { v: "symbol", id: o };
    }
    case "trace":
      if ("entryPoint" in v) return { v: "trace", entry: v.entryPoint };
      return { v: "trace", sym: M.owner.get(v.symbol) ?? v.symbol, member: M.memberName(v.symbol) };
  }
}

function show(i: number) {
  const steps = M.doc.tour ?? [];
  if (i < 0 || i >= steps.length) return;
  stepIndex = i;
  const s = steps[i];
  go(viewToState(s.open));
  el.hidden = false;
  const last = i === steps.length - 1;
  el.innerHTML = `<div class="step">Tour · step ${i + 1} of ${steps.length}</div><h2>${esc(s.title)}</h2><p>${inlineHTML(s.text)}</p>${draftedNote(s.drafted)}
  <div class="row"><button class="btn" id="tClose">Close</button><button class="btn" id="tPrev" ${i === 0 ? "disabled" : ""}>Previous</button><button class="btn primary" id="tNext">${last ? "Finish" : "Next"}</button></div>`;
  el.querySelector<HTMLButtonElement>("#tClose")!.onclick = () => tour.end();
  el.querySelector<HTMLButtonElement>("#tPrev")!.onclick = () => show(i - 1);
  el.querySelector<HTMLButtonElement>("#tNext")!.onclick = () => (last ? tour.end() : show(i + 1));
}

export const tour = {
  start() { if (M.doc.tour?.length) show(0); },
  end() { stepIndex = -1; el.hidden = true; },
  next() { show(stepIndex + 1); },
  prev() { show(stepIndex - 1); },
  go(i: number) { show(i); },
  active() { return stepIndex >= 0; },
};
