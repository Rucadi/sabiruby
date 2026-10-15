#!/usr/bin/env python3
"""Write the comparison page: SabiRuby and mruby/edge on the same 48 files of mruby's test/t.

  python3 tools/edge-compare/report.py          (tools/edge_compare.sh calls it)

Reads docs/verification/edge-compare/{sabiruby,mrubyedge}.json (the shape of mruby/edge's
`mruby_spec_report.py --format json`) and tests/edge-spec/notes.tsv (hand-written, one line per
file: why SabiRuby does not pass it), writes docs/verification/edge-compare/index.html.
One self-contained file: inline CSS and one small inline script, no external resources.
Python standard library only.
"""
import difflib
import html
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DIR = ROOT / "docs/verification/edge-compare"
STATUSES = ("pass", "fail", "error", "warn", "skip", "stress", "interrupted")
OTHER = ("warn", "skip", "stress", "interrupted")
REF = ("total", "ok", "ko", "crash", "warn", "skip")
esc = lambda v: html.escape(str(v), quote=True)


def load():
    sabi = json.loads((DIR / "sabiruby.json").read_text())
    edge = json.loads((DIR / "mrubyedge.json").read_text())
    notes = {}
    path = ROOT / "tests/edge-spec/notes.tsv"
    if path.exists():
        for line in path.read_text().splitlines():
            if "\t" in line:
                f, n = line.split("\t", 1)
                notes[f] = n
    return sabi, edge, notes


def depth_map(assertions):
    by = {a["id"]: a for a in assertions}
    out = {}
    for a in assertions:
        d, p = 0, a["parent_id"]
        while p is not None and p in by:
            d, p = d + 1, by[p]["parent_id"]
        out[a["id"]] = d
    return out


def align(left, right):
    """Pairs the asserts of two runs of one file by description, keeping each side's order.
    A description only one side reached comes out with None on the other side."""
    ln = [a["name"] for a in left]
    rn = [a["name"] for a in right]
    rows = []
    sm = difflib.SequenceMatcher(None, ln, rn, autojunk=False)
    for op, i1, i2, j1, j2 in sm.get_opcodes():
        if op == "equal":
            rows += [(left[i1 + k], right[j1 + k]) for k in range(i2 - i1)]
        else:
            rows += [(a, None) for a in left[i1:i2]]
            rows += [(None, b) for b in right[j1:j2]]
    return rows


def chip(status, cls="chip"):
    return f'<span class="{cls} s-{esc(status)}">{esc(status.replace("_", " "))}</span>'


def counts_cells(f):
    c = f["counts"]
    other = "·".join(str(c[s]) for s in OTHER)
    title = ", ".join(f"{s} {c[s]}" for s in OTHER)
    return (f'<td class="n">{c["pass"]}</td><td class="n">{c["fail"]}</td><td class="n">{c["error"]}</td>'
            f'<td class="n o" title="{esc(title)}">{other}</td>')


def bar(counts, scale):
    total = sum(counts.values())
    segs = "".join(
        f'<span class="seg s-{s}" style="width:{100 * counts[s] / scale:.3f}%" title="{s} {counts[s]}"></span>'
        for s in STATUSES if counts[s])
    return f'<div class="bar" role="img" aria-label="{total} asserts reached">{segs}</div>'


def result_cell(a, depth_of):
    if a is None:
        return '<td class="res none">not reached</td>'
    det = "".join(f'<div class="det">{esc(d)}</div>' for d in a["details"])
    return f'<td class="res">{chip(a["status"])}{det}</td>'


def page(sabi, edge, notes):
    sm, em = sabi["metadata"], edge["metadata"]
    sf = {f["file"]: f for f in sabi["files"]}
    ef = {f["file"]: f for f in edge["files"]}
    files = sorted(set(sf) | set(ef))
    sac, eac = sabi["assert_counts"], edge["assert_counts"]
    rc = sabi.get("reference_counts", {})
    spassed = sum(1 for f in sabi["files"] if f["status"] == "passed")
    epassed = sum(1 for f in edge["files"] if f["status"] == "passed")
    scale = max(sum(sac.values()), sum(eac.values()), 1)
    day = sm["generated_at"][:10]
    out = []
    w = out.append
    w('<!doctype html>\n<meta charset="utf-8">\n'
      '<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">\n'
      '<meta name="claude:viewport-fit-sides" content="cover">\n'
      '<title>SabiRuby and mruby/edge</title>\n<style>' + CSS + '</style>\n<main>')
    w('<header class="top"><h1>mruby compatibility report: SabiRuby and mruby/edge</h1></header>')
    w('<section><div class="scroll"><table class="meta"><thead><tr><th></th><th>SabiRuby</th><th>mruby/edge</th></tr></thead><tbody>')
    for k, a, b in (
        ("Commit", f'<code>{esc(sm["commit"])}</code>', f'<code>{esc(em["commit"])}</code>'),
        ("dirty", esc(sm["dirty"]), esc(em["dirty"])),
        ("Generated", esc(sm["generated_at"]), esc(em["generated_at"])),
        ("Features", esc(sm.get("cargo_features", "")), esc(em.get("features") or "(none requested)") + " · default features: " + esc(em.get("default_features"))),
        ("Upstream", f'<code>{esc(sm["upstream_revision"])}</code>', f'<code>{esc(em["upstream_revision"])}</code>'),
    ):
        w(f'<tr><th>{k}</th><td>{a}</td><td>{b}</td></tr>')
    w('</tbody></table></div>')
    w(f'<p class="small">{esc(sm.get("note") or em.get("note") or "")}</p>')
    w('<p class="small">Both columns: the 48 files of mruby\'s <code>test/t</code> at the upstream revision, '
      'mruby/edge\'s <code>helpers.rb</code> as the assertion driver, one process with a fresh VM per file. '
      'The mruby/edge column was generated on this machine with mruby/edge\'s '
      '<code>scripts/mruby_spec_report.py</code>.</p></section>')

    # ---- totals
    w('<section><h2>Summary</h2><div class="scroll"><table class="sum"><thead><tr><th></th>'
      '<th class="n">SabiRuby</th><th class="n">mruby/edge</th></tr></thead><tbody>')
    w(f'<tr><th>Files</th><td class="n">{len(sf)}</td><td class="n">{len(ef)}</td></tr>')
    w(f'<tr><th>passed</th><td class="n">{spassed}</td><td class="n">{epassed}</td></tr>')
    sfc, efc = Counter(f["status"] for f in sabi["files"]), Counter(f["status"] for f in edge["files"])
    order = ["passed", "failed", "runtime_error", "compile_error", "panic", "aborted", "timeout", "premature_return", "incomplete"]
    for st in order[1:]:
        if sfc.get(st) or efc.get(st):
            w(f'<tr><th>{chip(st)}</th><td class="n">{sfc.get(st, 0)}</td><td class="n">{efc.get(st, 0)}</td></tr>')
    w(f'<tr class="sep"><th>Asserts reached</th><td class="n">{sum(sac.values())}</td><td class="n">{sum(eac.values())}</td></tr>')
    for st in STATUSES:
        w(f'<tr><th><i class="dot s-{st}"></i>{st}</th><td class="n">{sac[st]}</td><td class="n">{eac[st]}</td></tr>')
    w('</tbody></table></div>')
    if rc:
        w('<p class="small">Reference column (SabiRuby only, another driver): the same 48 files under '
          'mruby\'s own <code>test/assert.rb</code> with SabiRuby\'s ports of the C fixtures of mruby\'s '
          f'test driver (<code>sabiruby mrbtest</code>): total {rc["total"]}, ok {rc["ok"]}, ko {rc["ko"]}, '
          f'crash {rc["crash"]}, skip {rc["skip"]}, warn {rc["warn"]}. Per file in the table below.</p>')
    w('</section>')

    # ---- conditions
    w('<section><h2>Conditions</h2><div class="scroll"><table class="cond"><thead><tr><th></th>'
      '<th>SabiRuby</th><th>mruby/edge</th></tr></thead><tbody>')
    rows = [
        ("Tests", f'mruby <code>{esc(sm["upstream_revision"])}</code> <code>test/t</code>, 48 files, checked against mruby/edge\'s <code>SHA256SUMS</code>',
         f'the same files (<code>mrubyedge/tests/mruby_spec/upstream</code>)'),
        ("VM", f'<code>{esc(sm["commit"][:10])}</code> ({esc(sm.get("vm_version", ""))})',
         f'<code>{esc(em["commit"][:10])}</code> (origin/master)'),
        ("Driver", 'mruby/edge\'s <code>helpers.rb</code>, unmodified; natives <code>t_print</code>, <code>_str_match?</code>, <code>_spec_event</code> in the command (<code>sabiruby edge-spec</code>)',
         '<code>helpers.rb</code> and the same three natives (<code>mod.rs</code>, <code>events.rs</code>)'),
        ("Compiler", f'{esc(sm.get("compiler", ""))}', 'its own (<code>mruby-compiler2-sys</code>), from source at run time'),
        ("C fixtures", 'none (mruby\'s test driver writes some helpers in C: <code>__env_*</code>, <code>TestNotImplement</code>, <code>TestVFormat</code>, …)', 'none'),
        ("Run", f'one process per file, fresh VM, {sm["timeout_seconds"]:g} s per file, end-of-file marker <code>$spec_completed = true</code>',
         f'the same, {em["timeout_seconds"]:g} s'),
        ("Machine", f'{esc(sm.get("machine", ""))}', 'the same PC, in a <code>rust:1.97.0</code> container (its compiler needs libclang)'),
    ]
    for k, a, b in rows:
        w(f'<tr><th>{k}</th><td>{a}</td><td>{b}</td></tr>')
    w('</tbody></table></div><ul class="notes">'
      '<li>A test that calls a C fixture is an error on the side that reaches it.</li>'
      '<li>A file that stops (panic, abort, timeout, an error outside an assert) is not counted past that point.</li>'
      '<li>mruby/edge\'s <code>compile_error</code> files (<code>string</code>, <code>unicode</code>, <code>vformat</code>) '
      'stop at its loader (<code>rite::load</code>: <code>InvalidFormat</code>), before any assert runs.</li>'
      '<li>Tests a build does not enter (for example <code>MRB_USE_REFINEMENTS</code>) are not counted; '
      '<code>stress</code> tests are disabled on both.</li>'
      '</ul></section>')

    # ---- per file
    w('<section><h2>Files</h2><p class="small">pass · fail · error, then warn·skip·stress·interrupted. '
      'Every assert of a file, side by side, is under the table.</p><div class="scroll"><table class="files"><thead>'
      '<tr><th rowspan="2">file</th><th colspan="5" class="grp">SabiRuby</th><th colspan="5" class="grp">mruby/edge</th>'
      '<th rowspan="2" class="n">assert.rb<br><span class="small">ok / total</span></th><th rowspan="2">notes (SabiRuby)</th></tr>'
      '<tr><th>status</th><th class="n">pass</th><th class="n">fail</th><th class="n">err</th><th class="n">other</th>'
      '<th>status</th><th class="n">pass</th><th class="n">fail</th><th class="n">err</th><th class="n">other</th></tr></thead><tbody>')
    for fn in files:
        s, e = sf.get(fn), ef.get(fn)
        stem = fn[:-3]
        ref = s.get("reference", {}) if s else {}
        refc = f'{ref["ok"]} / {ref["total"]}' if ref.get("status") == "ran" else esc(ref.get("status", ""))
        w(f'<tr><td><a href="#f-{esc(stem)}">{esc(stem)}</a></td>'
          + (f'<td>{chip(s["status"])}</td>{counts_cells(s)}' if s else '<td colspan="5"></td>')
          + (f'<td>{chip(e["status"])}</td>{counts_cells(e)}' if e else '<td colspan="5"></td>')
          + f'<td class="n">{refc}</td><td class="note">{inline_code(notes.get(fn, ""))}</td></tr>')
    w('</tbody></table></div></section>')

    # ---- per assert
    w('<section><h2>Asserts</h2><label class="only"><input type="checkbox" id="only-diff"> '
      'show only asserts whose results differ</label>')
    for fn in files:
        s, e = sf.get(fn), ef.get(fn)
        stem = fn[:-3]
        sa, ea = (s or {}).get("assertions", []), (e or {}).get("assertions", [])
        rows = align(sa, ea)
        ndiff = sum(1 for a, b in rows if (a and a["status"]) != (b and b["status"]))
        sd, ed = depth_map(sa), depth_map(ea)
        w(f'<details id="f-{esc(stem)}"><summary><span class="fn">{esc(fn)}</span>'
          f'<span class="pair">SabiRuby {chip(s["status"]) if s else ""} mruby/edge {chip(e["status"]) if e else ""}</span>'
          f'<span class="small">{len(rows)} asserts, {ndiff} with different results · elapsed {s["elapsed_seconds"] if s else 0:.2f}s / {e["elapsed_seconds"] if e else 0:.2f}s</span></summary>')
        stops = []
        for name, f in (("SabiRuby", s), ("mruby/edge", e)):
            if f and not f["completed"]:
                stops.append(f'{name}: execution stopped before completion ({esc(f["status"].replace("_", " "))}). '
                             'Remaining tests are unevaluated.')
        if stops:
            w('<p class="stop">' + "<br>".join(stops) + '</p>')
        if fn in notes:
            w(f'<p class="small">{inline_code(notes[fn])}</p>')
        if rows:
            w('<div class="scroll"><table class="asserts"><thead><tr><th>assert</th><th>SabiRuby</th><th>mruby/edge</th></tr></thead><tbody>')
            for a, b in rows:
                same = (a and a["status"]) == (b and b["status"])
                ref = a or b
                d = sd.get(a["id"], 0) if a else ed.get(b["id"], 0)
                w(f'<tr class="{"same" if same else "diff"}"><td class="an" style="padding-left:{0.5 + 1.1 * d:.1f}em">{esc(ref["name"])}</td>'
                  f'{result_cell(a, sd)}{result_cell(b, ed)}</tr>')
            w('</tbody></table></div>')
        else:
            w('<p class="small">No assert was reached on either side.</p>')
        w('</details>')
    w('</section><footer class="small">Generated by <code>tools/edge-compare/report.py</code> from '
      '<code>sabiruby.json</code> and <code>mrubyedge.json</code> next to this page. mruby/edge\'s driver and '
      'report format: BSD-3-Clause, Uchio Kondo. mruby\'s tests: MIT, mruby developers.</footer></main>')
    w('<script>' + JS + '</script>\n')
    return "\n".join(out)


def inline_code(text):
    parts = text.split("`")
    return "".join(f"<code>{esc(p)}</code>" if i % 2 else esc(p) for i, p in enumerate(parts))


from collections import Counter  # noqa: E402

CSS = """
/* Layout: one reading column; summary first, wide tables in their own scrolling frames. */
:root{--bg:#f7f8f9;--panel:#ffffff;--fg:#1d2329;--muted:#5b6670;--line:#d9dee3;--accent:#a4471f;
--pass:#2f7d4f;--fail:#c2362b;--error:#b8660b;--warn:#8a7a12;--skip:#6d7782;--stress:#9aa3ab;--interrupted:#7046b0;
--diff:#fff4e5;--code:#eef1f3;
--sans:system-ui,-apple-system,"Segoe UI","Hiragino Sans","Noto Sans JP",sans-serif;
--mono:ui-monospace,"SF Mono",Menlo,Consolas,"Liberation Mono",monospace;}
@media (prefers-color-scheme: dark){:root:not([data-theme="light"]){--bg:#14181c;--panel:#1b2025;--fg:#e3e7ea;--muted:#9aa5ae;--line:#2e353c;--accent:#e08a5e;
--pass:#5cbf83;--fail:#f07167;--error:#e9a23b;--warn:#cdbb4a;--skip:#8d97a1;--stress:#5f6870;--interrupted:#a98ae6;--diff:#2a2218;--code:#232a30;color-scheme:dark}}
:root[data-theme="dark"]{--bg:#14181c;--panel:#1b2025;--fg:#e3e7ea;--muted:#9aa5ae;--line:#2e353c;--accent:#e08a5e;
--pass:#5cbf83;--fail:#f07167;--error:#e9a23b;--warn:#cdbb4a;--skip:#8d97a1;--stress:#5f6870;--interrupted:#a98ae6;--diff:#2a2218;--code:#232a30;color-scheme:dark}
body{background:var(--bg);color:var(--fg);font:15px/1.55 var(--sans);margin:0}
main{max-width:1120px;margin:0 auto;padding-block:24px 48px;padding-left:max(16px,env(safe-area-inset-left,0px));padding-right:max(16px,env(safe-area-inset-right,0px));display:flex;flex-direction:column;gap:28px}
section{display:flex;flex-direction:column;gap:12px;min-width:0}
h1{font-size:1.75rem;line-height:1.2;margin:0;text-wrap:balance;letter-spacing:-.01em}
h2{font-size:1.15rem;margin:0;padding-bottom:6px;border-bottom:2px solid var(--accent);align-self:flex-start}
p{margin:0}.lede{max-width:68ch;color:var(--fg)}
.eyebrow{font:600 .78rem var(--mono);letter-spacing:.06em;text-transform:uppercase;color:var(--accent)}
.top{display:flex;flex-direction:column;gap:10px}
code{font-family:var(--mono);font-size:.88em;background:var(--code);padding:0 .25em;border-radius:3px;overflow-wrap:anywhere}
.small{font-size:.85rem;color:var(--muted)}
.totals{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:16px}
.tot{background:var(--panel);border:1px solid var(--line);border-radius:6px;padding:14px 16px;display:flex;flex-direction:column;gap:10px;min-width:0}
.tot-h{display:flex;justify-content:space-between;align-items:baseline;gap:8px;flex-wrap:wrap}
.who{font-weight:700;font-size:1.05rem}.big{font-size:1.6rem;font-weight:700;font-variant-numeric:tabular-nums}
.big small{font-size:.85rem;font-weight:400;color:var(--muted)}
.bar{display:flex;height:12px;background:var(--code);border-radius:2px;overflow:hidden}
.seg{display:block;height:100%}
.cnt{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:4px 10px;margin:0}
.cnt div{display:flex;justify-content:space-between;gap:6px;font-variant-numeric:tabular-nums}
.cnt dt{color:var(--muted);font-size:.85rem;display:flex;align-items:center;gap:5px}.cnt dd{margin:0;font-weight:600}
.dot{display:inline-block;width:8px;height:8px;border-radius:50%}
.reached{font-size:.85rem;color:var(--muted)}
.ref{font-size:.9rem;max-width:80ch;border-left:3px solid var(--line);padding-left:10px}
.s-pass,.s-passed{--c:var(--pass)}.s-fail,.s-failed{--c:var(--fail)}.s-error,.s-runtime_error,.s-panic,.s-aborted,.s-compile_error{--c:var(--error)}
.s-warn{--c:var(--warn)}.s-skip{--c:var(--skip)}.s-stress{--c:var(--stress)}.s-interrupted,.s-timeout,.s-premature_return,.s-incomplete{--c:var(--interrupted)}
.seg,.dot{background:var(--c)}
.chip{display:inline-block;font:600 .74rem var(--mono);color:var(--c);border:1px solid var(--c);border-radius:3px;padding:0 .35em;white-space:nowrap}
table{border-collapse:collapse;font-variant-numeric:tabular-nums}
th,td{padding:5px 8px;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}
th{font-size:.8rem;color:var(--muted);font-weight:600}
.n{text-align:right}.o{color:var(--muted)}
.scroll{overflow-x:auto;max-width:100%;background:var(--panel);border:1px solid var(--line);border-radius:6px}
.fc{align-self:flex-start;background:var(--panel);border:1px solid var(--line)}
.cond{width:100%}.cond th{white-space:nowrap}.cond td{min-width:16em}
.files{width:100%;font-size:.9rem}.files .grp{text-align:center;border-bottom:1px solid var(--line)}
.files td.note{min-width:22em;font-size:.84rem;color:var(--muted)}
.files a{color:var(--accent)}
.meta th,.sum th,.cond th{text-align:left;font-weight:600;white-space:nowrap}
.sum tr.sep th,.sum tr.sep td{border-top:2px solid var(--line)}
.notes{margin:0;padding-left:1.2em;display:flex;flex-direction:column;gap:8px;max-width:80ch}
details{background:var(--panel);border:1px solid var(--line);border-radius:6px}
details>*:not(summary){margin:0 12px 12px}
summary{cursor:pointer;padding:9px 12px;display:flex;flex-wrap:wrap;gap:6px 14px;align-items:center}
summary:focus-visible,a:focus-visible,input:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.fn{font:600 .95rem var(--mono)}.pair{display:flex;gap:6px;align-items:center;font-size:.85rem;flex-wrap:wrap}
.stop{font-size:.88rem;color:var(--interrupted)}
.asserts{width:100%;font-size:.86rem}.asserts td.an{min-width:16em}.asserts td.res{min-width:12em}
.asserts tr.diff{background:var(--diff)}
.res.none{color:var(--muted);font-style:italic}
.det{font:.78rem/1.4 var(--mono);color:var(--muted);white-space:pre-wrap;overflow-wrap:anywhere;margin-top:3px}
.only{display:flex;gap:8px;align-items:center;font-size:.9rem}
body.only-diff tr.same{display:none}
footer{border-top:1px solid var(--line);padding-top:12px}
@media (max-width:520px){h1{font-size:1.4rem}.cnt{grid-template-columns:repeat(2,minmax(0,1fr))}}
"""

JS = """
(function(){var c=document.getElementById('only-diff');if(!c)return;
function apply(){document.body.classList.toggle('only-diff',c.checked);}
c.addEventListener('change',apply);apply();
if(location.hash){var d=document.getElementById(location.hash.slice(1));if(d&&d.tagName==='DETAILS')d.open=true;}
document.querySelectorAll('a[href^="#f-"]').forEach(function(a){a.addEventListener('click',function(){
var d=document.getElementById(a.getAttribute('href').slice(1));if(d)d.open=true;});});})();
"""


def main():
    sabi, edge, notes = load()
    (DIR / "index.html").write_text(page(sabi, edge, notes), encoding="utf-8")
    print(DIR / "index.html")


if __name__ == "__main__":
    main()
