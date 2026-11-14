"""The TTS bake-off's blind listening page (docs/COMMANDS.md section 11, "Choosing a TTS"):

    python3 tools/voice/tts/page.py --lines <dir>/lines.tsv --audio <dir>/audio [<dir>/audio_cloud ...] --out <page.html>

Picks lines spread over the speakers and over English and invented names, and for each, every
model's clip in a shuffled order under letters. The listener rates each clip (how natural, 1-5,
and whether a word was mispronounced); the models' names and their averages stay hidden until
the listener reveals them, with a summary line to copy back. Clips are embedded (FLAC data URIs),
so the page stands alone.
"""

import argparse
import base64
import html
import json
import random
from pathlib import Path

STYLE = r"""
<title>Voice Bake-off</title>
<meta name="claude:viewport-fit-sides" content="cover">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Bricolage+Grotesque:opsz,wght@12..96,600;12..96,700&family=Atkinson+Hyperlegible:wght@400;700&family=IBM+Plex+Mono:wght@500&display=swap">
<style>
/* Layout: a single listening sheet; each line a block of lettered takes in a row that wraps. */
:root {
  --bg: #f3f5f2; --surface: #ffffff; --ink: #1d2420; --muted: #5d6a63; --line: #d5dcd7;
  --accent: #b5650d; --accent-ink: #ffffff; --good: #2f7d4f; --bad: #b3362a;
  --display: "Bricolage Grotesque", "Avenir Next", system-ui, sans-serif;
  --body: "Atkinson Hyperlegible", "Segoe UI", system-ui, sans-serif;
  --mono: "IBM Plex Mono", ui-monospace, Menlo, monospace;
}
@media (prefers-color-scheme: dark) { :root:not([data-theme="light"]) {
  --bg: #141917; --surface: #1d2421; --ink: #e5ebe7; --muted: #9aa8a0; --line: #303a35;
  --accent: #e39a45; --accent-ink: #1b1206; --good: #6cc48f; --bad: #ef7a6c; color-scheme: dark } }
:root[data-theme="dark"] {
  --bg: #141917; --surface: #1d2421; --ink: #e5ebe7; --muted: #9aa8a0; --line: #303a35;
  --accent: #e39a45; --accent-ink: #1b1206; --good: #6cc48f; --bad: #ef7a6c; color-scheme: dark }
body { background: var(--bg); color: var(--ink); font: 16px/1.5 var(--body);
  padding-left: max(16px, env(safe-area-inset-left, 0px)); padding-right: max(16px, env(safe-area-inset-right, 0px)); padding-block: 28px 64px; }
main { max-width: 980px; margin: 0 auto; display: grid; gap: 28px; }
h1 { font: 700 2rem/1.15 var(--display); margin: 0; text-wrap: balance; }
h2 { font: 600 1.15rem/1.3 var(--display); margin: 0; }
.lede { color: var(--muted); max-width: 65ch; margin: 6px 0 0; }
.meta { font: 500 .75rem/1.4 var(--mono); letter-spacing: .06em; text-transform: uppercase; color: var(--muted); }
.line { display: grid; gap: 12px; border-top: 1px solid var(--line); padding-top: 18px; }
.said { font-size: 1.05rem; max-width: 70ch; }
.names { color: var(--muted); font-size: .9rem; }
.takes { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 12px; }
.take { background: var(--surface); border: 1px solid var(--line); border-radius: 10px; padding: 12px; display: grid; gap: 8px; min-width: 0; }
.take audio { width: 100%; height: 36px; }
.letter { font: 600 1.1rem var(--display); }
.model { font: 500 .78rem var(--mono); color: var(--accent); }
.stars { display: flex; gap: 4px; flex-wrap: wrap; }
.stars button { font: 500 .85rem var(--mono); width: 34px; height: 30px; border: 1px solid var(--line); border-radius: 6px; background: transparent; color: var(--ink); cursor: pointer; }
.stars button[aria-pressed="true"] { background: var(--accent); color: var(--accent-ink); border-color: var(--accent); }
.stars button:focus-visible, .act:focus-visible, label:focus-within { outline: 2px solid var(--accent); outline-offset: 2px; }
label.flag { font-size: .85rem; color: var(--muted); display: flex; gap: 6px; align-items: center; }
.bar { position: sticky; bottom: 0; background: var(--bg); border-top: 1px solid var(--line); padding-block: 12px calc(12px + env(safe-area-inset-bottom, 0px)); display: flex; gap: 12px; flex-wrap: wrap; align-items: center; justify-content: space-between; }
.act { font: 700 .95rem var(--body); padding: 10px 16px; border-radius: 8px; border: 1px solid var(--accent); background: var(--accent); color: var(--accent-ink); cursor: pointer; }
.act.quiet { background: transparent; color: var(--accent); }
.progress { font: 500 .85rem var(--mono); color: var(--muted); font-variant-numeric: tabular-nums; }
table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
th, td { text-align: left; padding: 8px 10px; border-bottom: 1px solid var(--line); }
th { font: 500 .75rem var(--mono); letter-spacing: .06em; text-transform: uppercase; color: var(--muted); }
.scroll { overflow-x: auto; }
textarea { width: 100%; font: .85rem var(--mono); background: var(--surface); color: var(--ink); border: 1px solid var(--line); border-radius: 8px; padding: 10px; box-sizing: border-box; }
.legend { color: var(--muted); font-size: .9rem; }
@media (prefers-reduced-motion: no-preference) { .take { transition: border-color .15s } }
</style>
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--lines", required=True)
    ap.add_argument("--audio", nargs="+", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--count", type=int, default=18)
    ap.add_argument("--seed", type=int, default=7)
    args = ap.parse_args()
    rng = random.Random(args.seed)
    rows = [r.split("\t") for r in Path(args.lines).read_text().splitlines()]
    models = {}
    for d in args.audio:
        for m in sorted(p for p in Path(d).iterdir() if p.is_dir()):
            models[m.name] = m
    # Lines every model spoke, spread over speakers, a third English-named.
    usable = [r for r in rows if all((models[m] / f"{r[0]}.flac").exists() for m in models)]
    by_speaker = {}
    for r in usable:
        by_speaker.setdefault(r[4], []).append(r)
    picked, i = [], 0
    while len(picked) < min(args.count, len(usable)):
        for s in sorted(by_speaker):
            pool = [r for r in by_speaker[s] if r not in picked]
            if pool and len(picked) < args.count:
                picked.append(pool[i % len(pool)])
        i += 1
    lines = []
    for r in picked:
        order = list(models)
        rng.shuffle(order)
        takes = []
        for letter, m in zip("ABCDEFGH", order):
            data = base64.b64encode((models[m] / f"{r[0]}.flac").read_bytes()).decode()
            takes.append({"letter": letter, "model": m, "src": f"data:audio/flac;base64,{data}"})
        lines.append({"id": r[0], "text": r[1], "names": r[3].replace("|", ", "), "speaker": r[4], "takes": takes})
    names = sorted(models)
    body = [STYLE, "<main>",
            "<header><div class=\"meta\">Conquer Ops voice commands · TTS bake-off</div>",
            "<h1>Which voices sound real?</h1>",
            f"<p class=\"lede\">{len(lines)} spoken commands, each voiced by {len(names)} speech models in a shuffled order. "
            "Rate how natural each take sounds, 1 (robotic) to 5 (a real person), and tick the box if any word, especially a unit name, came out wrong. "
            "The models' names stay hidden until you reveal them.</p></header>",
            "<div class=\"legend\">Clones speak in a reference speaker's voice; one model can only use preset voices, so its voice won't match the others in a row.</div>"]
    for n, ln in enumerate(lines):
        body.append(f"<section class=\"line\" data-line=\"{ln['id']}\"><div class=\"meta\">Line {n + 1} of {len(lines)} · speaker {html.escape(ln['speaker'])}</div>")
        body.append(f"<div class=\"said\">“{html.escape(ln['text'])}”</div><div class=\"names\">Names in it: {html.escape(ln['names'])}</div><div class=\"takes\">")
        for t in ln["takes"]:
            key = f"{ln['id']}-{t['letter']}"
            stars = "".join(f"<button type=\"button\" aria-pressed=\"false\" data-v=\"{v}\">{v}</button>" for v in range(1, 6))
            body.append(f"<div class=\"take\" data-key=\"{key}\" data-model=\"{html.escape(t['model'])}\"><div class=\"letter\">{t['letter']} <span class=\"model\" hidden>{html.escape(t['model'])}</span></div>"
                        f"<audio controls preload=\"none\" src=\"{t['src']}\"></audio><div class=\"stars\" role=\"group\" aria-label=\"How natural, take {t['letter']}\">{stars}</div>"
                        f"<label class=\"flag\"><input type=\"checkbox\" id=\"flag-{key}\"> a word came out wrong</label></div>")
        body.append("</div></section>")
    body.append("<section id=\"results\" hidden><h2>Results</h2><div class=\"scroll\"><table><thead><tr><th>Model</th><th>Natural (1-5)</th><th>Takes with a wrong word</th><th>Rated</th></tr></thead><tbody id=\"rows\"></tbody></table></div>"
                "<p class=\"legend\">Copy this line back into the chat:</p><textarea id=\"summary\" rows=\"3\" readonly></textarea><div><button class=\"act quiet\" type=\"button\" id=\"copy\">Copy summary</button></div></section>")
    body.append("<div class=\"bar\"><span class=\"progress\" id=\"progress\"></span><button class=\"act\" type=\"button\" id=\"reveal\">Reveal the models</button></div></main>")
    body.append("<script>\n(() => {\n"
                "const KEY='voice-bakeoff-ratings'; let saved={}; try { saved = JSON.parse(localStorage.getItem(KEY)||'{}'); } catch (e) {}\n"
                "const takes=[...document.querySelectorAll('.take')];\n"
                "function save(){ try { localStorage.setItem(KEY, JSON.stringify(saved)); } catch (e) {} }\n"
                "function progress(){ const n=takes.filter(t=>saved[t.dataset.key]&&saved[t.dataset.key].v).length; document.getElementById('progress').textContent=`${n} of ${takes.length} takes rated`; }\n"
                "takes.forEach(t=>{ const k=t.dataset.key, s=saved[k]||{}; const box=t.querySelector('input');\n"
                "  t.querySelectorAll('.stars button').forEach(b=>{ if(String(s.v)===b.dataset.v) b.setAttribute('aria-pressed','true');\n"
                "    b.addEventListener('click',()=>{ t.querySelectorAll('.stars button').forEach(x=>x.setAttribute('aria-pressed','false')); b.setAttribute('aria-pressed','true'); saved[k]={...(saved[k]||{}), v:+b.dataset.v}; save(); progress(); }); });\n"
                "  box.checked=!!s.wrong; box.addEventListener('change',()=>{ saved[k]={...(saved[k]||{}), wrong:box.checked}; save(); }); });\n"
                "document.getElementById('reveal').addEventListener('click',()=>{ document.querySelectorAll('.model').forEach(m=>m.hidden=false);\n"
                "  const by={}; takes.forEach(t=>{ const m=t.dataset.model, s=saved[t.dataset.key]||{}; by[m]=by[m]||{sum:0,n:0,wrong:0,all:0}; by[m].all++; if(s.v){by[m].sum+=s.v; by[m].n++;} if(s.wrong) by[m].wrong++; });\n"
                "  const rows=Object.entries(by).sort((a,b)=>(b[1].sum/(b[1].n||1))-(a[1].sum/(a[1].n||1)));\n"
                "  document.getElementById('rows').innerHTML=rows.map(([m,x])=>`<tr><td>${m}</td><td>${x.n?(x.sum/x.n).toFixed(2):'–'}</td><td>${x.wrong} of ${x.all}</td><td>${x.n} of ${x.all}</td></tr>`).join('');\n"
                "  document.getElementById('summary').value='Bake-off ratings: '+rows.map(([m,x])=>`${m} ${x.n?(x.sum/x.n).toFixed(2):'-'} natural, ${x.wrong}/${x.all} wrong word`).join('; ');\n"
                "  const r=document.getElementById('results'); r.hidden=false; r.scrollIntoView({behavior:'smooth'}); });\n"
                "document.getElementById('copy').addEventListener('click',()=>{ const ta=document.getElementById('summary'); navigator.clipboard.writeText(ta.value).catch(()=>{ ta.select(); }); });\n"
                "progress();\n})();\n</script>")
    Path(args.out).write_text("\n".join(body))
    size = Path(args.out).stat().st_size / 1e6
    print(f"{len(lines)} lines x {len(names)} models ({', '.join(names)}), {size:.1f} MB -> {args.out}")


if __name__ == "__main__":
    main()
