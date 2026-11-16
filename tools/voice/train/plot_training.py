"""The training run's page (docs/COMMANDS.md section 11, step 13): train_whisper.py's
`metrics.jsonl` drawn as one self-contained HTML page, rebuilt each time the metrics are pulled.

    python3 tools/voice/train/plot_training.py --metrics <out>/metrics.jsonl --out <page.html> \\
        [--title "Whisper base.en"] [--cost 1.23] [--price 0.60]

Charts, one measure each (never two scales on one axis): dev exact match clean and noisy, the
training and dev losses, the learning rate, the gradient norm and the speed; tiles for where the
run stands; and every evaluation as a table. Hover a chart for its values at that step.
"""

import argparse
import html
import json
import time
from pathlib import Path


def load(path):
    rows = []
    for line in Path(path).read_text().splitlines():
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError:
            continue  # a line cut off mid-pull
        # A resume from an older checkpoint (a machine lost between saves) trains those steps
        # again: the rows past the checkpoint it went back to no longer happened.
        r = rows[-1]
        if r["type"] == "start" and r.get("resumed_at") is not None:
            rows = [x for x in rows if x["type"] not in ("train", "eval") or x["step"] <= r["resumed_at"]] + [r]
    return rows


def summary(rows, cost, price):
    starts = [r for r in rows if r["type"] == "start"]
    train = [r for r in rows if r["type"] == "train"]
    evals = [r for r in rows if r["type"] == "eval"]
    tests = [r for r in rows if r["type"] == "test"]
    s = starts[-1] if starts else {}
    total = s.get("total_steps", 0)
    step = train[-1]["step"] if train else 0
    rate = train[-1]["clips_per_s"] if train else 0
    left = (total - step) * s.get("batch", 32) / rate if rate else 0
    best = max(evals, key=lambda r: r["dev_clean"]) if evals else None
    tiles = [
        ("Step", f"{step:,} of {total:,}", f"pass {train[-1]['epoch']:.2f} of {s.get('epochs', 0)}" if train else "not started"),
        ("Best dev, clean", f"{best['dev_clean'] * 100:.1f}%" if best else "–", f"at step {best['step']:,}" if best else "no evaluation yet"),
        ("Latest dev, noisy", f"{evals[-1]['dev_noisy'] * 100:.1f}%" if evals else "–", f"step {evals[-1]['step']:,}" if evals else ""),
        ("Speed", f"{rate:.0f} clips/s" if rate else "–", f"about {left / 3600:.1f} h left at most" if rate else ""),
    ]
    if cost is not None:
        tiles.append(("Cost so far", f"${cost:.2f}", f"at ${price:.2f}/h" if price else ""))
    if tests:
        t = tests[-1]
        tiles.append(("Test, clean", f"{t['test'] * 100:.1f}%", f"noisy {t['noisy'] * 100:.1f}%" if "noisy" in t else ""))
        if "unknown" in t:
            tiles.append(("Test, unknown", f"{t['unknown'] * 100:.1f}%", f"flagged {t['unknown_flagged'] * 100:.1f}%"))
    return tiles, evals


def build(rows, title, cost, price):
    tiles, evals = summary(rows, cost, price)
    train = [r for r in rows if r["type"] == "train"]
    charts = [
        {"id": "acc", "title": "Dev exact match", "note": "1% of the training vocabularies, never trained on; noisy is one fixed take from recordings training never hears",
         "pct": True, "series": [{"label": "clean", "slot": 1, "pts": [[e["epoch"], e["dev_clean"]] for e in evals]},
                                 {"label": "noisy", "slot": 2, "pts": [[e["epoch"], e["dev_noisy"]] for e in evals]}]},
        {"id": "unknown", "title": "Unknown names", "note": "dev commands with one spoken name gone from the prompt: right in full, flagged at all; and clean dev commands wrongly flagged",
         "pct": True, "series": [{"label": "right", "slot": 1, "pts": [[e["epoch"], e["dev_unknown"]] for e in evals if "dev_unknown" in e]},
                                 {"label": "flagged", "slot": 2, "pts": [[e["epoch"], e["unknown_flagged"]] for e in evals if "unknown_flagged" in e]},
                                 {"label": "false flags", "slot": 3, "pts": [[e["epoch"], e["false_unknown"]] for e in evals if "false_unknown" in e]}]},
        {"id": "loss", "title": "Loss (cross-entropy per label token)", "note": "training: mean of the last 50 steps, augmented; dev: clean, teacher-forced",
         "series": [{"label": "training", "slot": 1, "pts": [[r["epoch"], r["loss"]] for r in train]},
                    {"label": "dev", "slot": 2, "pts": [[e["epoch"], e["dev_loss"]] for e in evals]}]},
        {"id": "lr", "title": "Learning rate", "note": "warm-up, then a straight line down to the planned last step",
         "sci": True, "series": [{"label": "learning rate", "slot": 1, "pts": [[r["epoch"], r["lr"]] for r in train]}]},
        {"id": "grad", "title": "Gradient norm", "note": "before clipping at 1.0, mean of 50 steps",
         "series": [{"label": "gradient norm", "slot": 1, "pts": [[r["epoch"], r["grad_norm"]] for r in train]}]},
        {"id": "speed", "title": "Speed (clips a second)", "note": "since the run (or its resume) started",
         "series": [{"label": "clips/s", "slot": 1, "pts": [[r["epoch"], r["clips_per_s"]] for r in train]}]},
    ]
    tile_html = "".join(f'<div class="tile"><div class="k">{html.escape(k)}</div><div class="v">{html.escape(v)}</div><div class="s">{html.escape(sub)}</div></div>'
                        for k, v, sub in tiles)
    table = "".join(f"<tr><td>{e['step']:,}</td><td>{e['epoch']:.2f}</td><td>{e['dev_clean'] * 100:.1f}%</td><td>{e['dev_noisy'] * 100:.1f}%</td>"
                    f"<td>{e.get('dev_unknown', 0) * 100:.1f}%</td><td>{e.get('false_unknown', 0) * 100:.1f}%</td><td>{e['dev_loss']:.4f}</td><td>{'best' if e.get('best') else ''}</td></tr>" for e in evals)
    updated = time.strftime("%Y-%m-%d %H:%M", time.localtime())
    return (PAGE.replace("{title}", html.escape(title)).replace("{updated}", updated).replace("{tiles}", tile_html)
            .replace("{table}", table or '<tr><td colspan="8">No evaluation yet.</td></tr>').replace("{charts}", json.dumps(charts)))


PAGE = r"""<title>Whisper Training Run</title>
<meta name="claude:viewport-fit-sides" content="cover">
<style>
:root {
  --bg: #f6f6f4; --surface: #fcfcfb; --ink: #0b0b0b; --ink-2: #52514e; --muted: #77756f; --grid: #e4e3df; --axis: #c9c7c1;
  --s1: #2a78d6; --s2: #eb6834; --s3: #1baf7a;
}
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) {
  --bg: #121211; --surface: #1a1a19; --ink: #f0efec; --ink-2: #c3c2b7; --muted: #94938a; --grid: #2c2c2a; --axis: #45443f;
  --s1: #3987e5; --s2: #d95926; --s3: #199e70; color-scheme: dark } }
:root[data-theme="dark"] {
  --bg: #121211; --surface: #1a1a19; --ink: #f0efec; --ink-2: #c3c2b7; --muted: #94938a; --grid: #2c2c2a; --axis: #45443f;
  --s1: #3987e5; --s2: #d95926; --s3: #199e70; color-scheme: dark }
body { background: var(--bg); color: var(--ink); font: 14px/1.5 system-ui, -apple-system, "Segoe UI", sans-serif; padding-block: 24px 48px;
  padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)); }
main { max-width: 1040px; margin: 0 auto; display: grid; gap: 20px; }
header h1 { font-size: 1.45rem; margin: 0; text-wrap: balance; }
header p { margin: 4px 0 0; color: var(--ink-2); }
.tiles { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 12px; }
.tile { background: var(--surface); border: 1px solid var(--grid); border-radius: 10px; padding: 12px 14px; min-width: 0; }
.tile .k { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); }
.tile .v { font-size: 1.5rem; font-weight: 650; font-variant-numeric: tabular-nums; }
.tile .s { font-size: 12px; color: var(--ink-2); }
.grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 460px), 1fr)); gap: 16px; }
figure { margin: 0; background: var(--surface); border: 1px solid var(--grid); border-radius: 10px; padding: 14px; min-width: 0; position: relative; }
figure h2 { font-size: 14px; margin: 0; }
figure .note { font-size: 12px; color: var(--muted); margin: 2px 0 6px; }
.legend { display: flex; gap: 14px; font-size: 12px; color: var(--ink-2); margin-bottom: 4px; }
.legend i { display: inline-block; width: 14px; height: 2px; vertical-align: middle; margin-right: 6px; border-radius: 1px; }
svg { display: block; width: 100%; height: 220px; overflow: visible; touch-action: none; }
svg text { fill: var(--muted); font-size: 11px; font-variant-numeric: tabular-nums; }
.tip { position: absolute; pointer-events: none; background: var(--surface); border: 1px solid var(--axis); border-radius: 8px; padding: 6px 9px;
  font-size: 12px; box-shadow: 0 4px 14px rgb(0 0 0 / .12); white-space: nowrap; }
.tip b { font-variant-numeric: tabular-nums; }
.tip .r { display: flex; gap: 8px; align-items: center; }
.tip i { width: 10px; height: 2px; display: inline-block; }
.table-wrap { overflow-x: auto; background: var(--surface); border: 1px solid var(--grid); border-radius: 10px; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; font-size: 13px; }
th, td { text-align: right; padding: 6px 12px; border-bottom: 1px solid var(--grid); }
th { color: var(--muted); font-weight: 600; font-size: 11px; text-transform: uppercase; letter-spacing: .05em; }
th:last-child, td:last-child { text-align: left; }
details summary { cursor: pointer; color: var(--ink-2); padding: 4px 0; }
</style>
<main>
<header><h1>{title}</h1><p>Whisper trained straight on voice commands. Updated {updated}.</p></header>
<section class="tiles">{tiles}</section>
<section class="grid" id="charts"></section>
<details open><summary>Every evaluation</summary>
<div class="table-wrap"><table><thead><tr><th>Step</th><th>Pass</th><th>Dev clean</th><th>Dev noisy</th><th>Unknown right</th><th>False flags</th><th>Dev loss</th><th></th></tr></thead><tbody>{table}</tbody></table></div>
</details>
</main>
<script>
const CHARTS = {charts};
const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();
const NS = 'http://www.w3.org/2000/svg';
function el(tag, attrs, parent) { const e = document.createElementNS(NS, tag); for (const k in attrs) e.setAttribute(k, attrs[k]); parent && parent.appendChild(e); return e; }
function fmt(c, v) { if (c.pct) return (v * 100).toFixed(1) + '%'; if (c.sci) return v.toExponential(1); return Math.abs(v) >= 100 ? v.toFixed(0) : v.toPrecision(3); }
function ticks(lo, hi, n) { const span = hi - lo || 1, step0 = span / n, mag = Math.pow(10, Math.floor(Math.log10(step0)));
  const step = [1, 2, 2.5, 5, 10].map(m => m * mag).find(s => s >= step0); const out = []; for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-12; v += step) out.push(+v.toPrecision(12)); return out; }
function draw(fig, c) {
  const svg = fig.querySelector('svg'); svg.innerHTML = '';
  const W = svg.clientWidth || 460, H = 220, m = { l: 52, r: 56, t: 8, b: 26 };
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  const all = c.series.flatMap(s => s.pts);
  if (!all.length) { el('text', { x: W / 2, y: H / 2, 'text-anchor': 'middle' }, svg).textContent = 'No data yet'; return; }
  const xs = all.map(p => p[0]), ys = all.map(p => p[1]);
  const x0 = 0, x1 = Math.max(...xs) || 1;
  let y0 = Math.min(...ys), y1 = Math.max(...ys); if (c.pct) { y0 = Math.min(0, y0); y1 = Math.min(1, Math.max(y1 * 1.05, 0.1)); } else { y0 = Math.min(0, y0); y1 = y1 * 1.08 || 1; }
  const X = v => m.l + (v - x0) / (x1 - x0) * (W - m.l - m.r), Y = v => H - m.b - (v - y0) / (y1 - y0) * (H - m.t - m.b);
  for (const t of ticks(y0, y1, 4)) { el('line', { x1: m.l, x2: W - m.r, y1: Y(t), y2: Y(t), stroke: css('--grid') }, svg);
    el('text', { x: m.l - 6, y: Y(t) + 4, 'text-anchor': 'end' }, svg).textContent = fmt(c, t); }
  for (const t of ticks(x0, x1, 5)) el('text', { x: X(t), y: H - 8, 'text-anchor': 'middle' }, svg).textContent = t + (t === 0 ? ' passes' : '');
  el('line', { x1: m.l, x2: W - m.r, y1: H - m.b, y2: H - m.b, stroke: css('--axis') }, svg);
  c.series.forEach(s => { if (!s.pts.length) return; const col = css('--s' + s.slot);
    el('path', { d: s.pts.map((p, i) => (i ? 'L' : 'M') + X(p[0]).toFixed(1) + ',' + Y(p[1]).toFixed(1)).join(''), fill: 'none', stroke: col, 'stroke-width': 2, 'stroke-linejoin': 'round', 'stroke-linecap': 'round' }, svg);
    if (s.pts.length < 40) s.pts.forEach(p => el('circle', { cx: X(p[0]), cy: Y(p[1]), r: 3.5, fill: col, stroke: css('--surface'), 'stroke-width': 2 }, svg));
    const last = s.pts[s.pts.length - 1];
    if (c.series.length > 1) { const t = el('text', { x: X(last[0]) + 6, y: Y(last[1]) + 4 }, svg); t.textContent = s.label; t.style.fill = css('--ink-2'); } });
  const hair = el('line', { y1: m.t, y2: H - m.b, stroke: css('--axis'), visibility: 'hidden' }, svg);
  const dots = c.series.map(s => el('circle', { r: 4.5, fill: css('--s' + s.slot), stroke: css('--surface'), 'stroke-width': 2, visibility: 'hidden' }, svg));
  const tip = fig.querySelector('.tip');
  function nearest(pts, x) { let b = null; for (const p of pts) if (!b || Math.abs(p[0] - x) < Math.abs(b[0] - x)) b = p; return b; }
  svg.onpointermove = ev => { const r = svg.getBoundingClientRect(), px = (ev.clientX - r.left) * W / r.width;
    const xv = x0 + (px - m.l) / (W - m.l - m.r) * (x1 - x0); const ref = nearest(all, xv); if (!ref) return;
    hair.setAttribute('x1', X(ref[0])); hair.setAttribute('x2', X(ref[0])); hair.setAttribute('visibility', 'visible');
    tip.replaceChildren(); const head = document.createElement('div'); head.textContent = 'pass ' + ref[0].toFixed(2); head.style.color = css('--muted'); tip.appendChild(head);
    c.series.forEach((s, i) => { const p = nearest(s.pts, ref[0]); if (!p) { dots[i].setAttribute('visibility', 'hidden'); return; }
      dots[i].setAttribute('cx', X(p[0])); dots[i].setAttribute('cy', Y(p[1])); dots[i].setAttribute('visibility', 'visible');
      const row = document.createElement('div'); row.className = 'r'; const sw = document.createElement('i'); sw.style.background = css('--s' + s.slot);
      const b = document.createElement('b'); b.textContent = fmt(c, p[1]); const l = document.createElement('span'); l.textContent = s.label;
      row.append(sw, b, l); tip.appendChild(row); });
    tip.hidden = false; const fr = fig.getBoundingClientRect(); let left = ev.clientX - fr.left + 14; if (left + tip.offsetWidth > fr.width) left = ev.clientX - fr.left - tip.offsetWidth - 14;
    tip.style.left = left + 'px'; tip.style.top = (ev.clientY - fr.top - 10) + 'px'; };
  svg.onpointerleave = () => { hair.setAttribute('visibility', 'hidden'); dots.forEach(d => d.setAttribute('visibility', 'hidden')); tip.hidden = true; };
}
const host = document.getElementById('charts');
const figs = CHARTS.map(c => { const f = document.createElement('figure');
  const h = document.createElement('h2'); h.textContent = c.title; const n = document.createElement('p'); n.className = 'note'; n.textContent = c.note; f.append(h, n);
  if (c.series.length > 1) { const lg = document.createElement('div'); lg.className = 'legend';
    c.series.forEach(s => { const sp = document.createElement('span'); const i = document.createElement('i'); i.style.background = `var(--s${s.slot})`; sp.append(i, document.createTextNode(s.label)); lg.appendChild(sp); }); f.appendChild(lg); }
  f.appendChild(document.createElementNS(NS, 'svg')); const tip = document.createElement('div'); tip.className = 'tip'; tip.hidden = true; f.appendChild(tip);
  host.appendChild(f); return [f, c]; });
const redraw = () => figs.forEach(([f, c]) => draw(f, c));
redraw(); addEventListener('resize', redraw);
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', redraw);
new MutationObserver(redraw).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
</script>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--metrics", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--title", default="Whisper base.en on commands")
    ap.add_argument("--cost", type=float)
    ap.add_argument("--price", type=float)
    args = ap.parse_args()
    Path(args.out).write_text(build(load(args.metrics), args.title, args.cost, args.price))
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
