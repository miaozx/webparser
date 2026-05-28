#!/usr/bin/env python3
"""Run extraction on 100 random webmainbench entries, compare with ground truth."""

import json, random, subprocess, sys, re
from pathlib import Path
from rouge_score import rouge_scorer

DATA = Path.home() / '下载' / 'webmainbench.jsonl'
EXTRACTOR = Path.cwd() / 'target' / 'release' / 'extract_stdin'
OUT = '/tmp/bench_eval.jsonl'

random.seed(42)
selected = []
for i, line in enumerate(DATA.open()):
    if i < 100:
        selected.append(json.loads(line))
    else:
        j = random.randint(0, i)
        if j < 100:
            selected[j] = json.loads(line)

scorer = rouge_scorer.RougeScorer(['rougeL'], use_stemmer=False)
results = []

print(f"Processing {len(selected)} entries...", file=sys.stderr)

for i, e in enumerate(selected):
    url = e.get('url', '')
    html = e.get('html', '')
    gt = e.get('convert_main_content', e.get('main_html', ''))

    proc = subprocess.run(
        [str(EXTRACTOR), '--url', url, '--markdown'],
        input=html.encode('utf-8'),
        capture_output=True, timeout=120,
    )
    try:
        res = json.loads(proc.stdout.decode('utf-8', errors='replace'))
    except json.JSONDecodeError:
        res = {}

    extracted = res.get('content_markdown') or res.get('main_content', '')
    conf = res.get('confidence', 0.0)
    ta_used = res.get('title_anchored_used', False)
    xpath_warn = any('xpath' in w for w in res.get('warnings', []))

    # ROUGE-L F1
    if extracted and gt:
        score = scorer.score(gt, extracted)['rougeL'].fmeasure
    else:
        score = 0.0

    # Exact text overlap F1 (word-level)
    gt_words = set(re.findall(r'\w+', gt.lower())) if gt else set()
    ex_words = set(re.findall(r'\w+', extracted.lower())) if extracted else set()
    if gt_words and ex_words:
        overlap = len(gt_words & ex_words)
        prec = overlap / len(ex_words)
        rec = overlap / len(gt_words)
        f1 = 2 * prec * rec / (prec + rec) if (prec + rec) > 0 else 0.0
    else:
        prec = rec = f1 = 0.0

    record = {
        'track_id': i,
        'url': url,
        'gt_len': len(gt),
        'ext_len': len(extracted),
        'confidence': conf,
        'ta_used': ta_used,
        'rougeL_f1': round(score, 4),
        'word_f1': round(f1, 4),
        'word_prec': round(prec, 4),
        'word_rec': round(rec, 4),
    }
    results.append(record)

    flag = 'X' if xpath_warn else ('T' if ta_used else ' ')
    print(f"  [{i+1}/100] {flag} gt={record['gt_len']:6d} ex={record['ext_len']:6d} rougeL={score:.3f} f1={f1:.3f} conf={conf:.2f}", file=sys.stderr)

# Summary
rouge_scores = [r['rougeL_f1'] for r in results]
word_f1s = [r['word_f1'] for r in results]

print(f"\n=== Summary (N={len(results)}) ===", file=sys.stderr)
print(f"  ROUGE-L F1:      mean={sum(rouge_scores)/len(rouge_scores):.4f}  median={sorted(rouge_scores)[len(rouge_scores)//2]:.4f}", file=sys.stderr)
print(f"  Word F1:         mean={sum(word_f1s)/len(word_f1s):.4f}  median={sorted(word_f1s)[len(word_f1s)//2]:.4f}", file=sys.stderr)
print(f"  TA triggered:    {sum(1 for r in results if r['ta_used'])}/100", file=sys.stderr)

# Print worst cases
worst = sorted(results, key=lambda r: r['rougeL_f1'])[:5]
print(f"\n  Worst 5 by ROUGE-L:", file=sys.stderr)
for r in worst:
    print(f"    {r['url']:60s} rougeL={r['rougeL_f1']:.3f} f1={r['word_f1']:.3f} gt={r['gt_len']} ex={r['ext_len']}", file=sys.stderr)

# Write detailed results
with open(OUT, 'w') as f:
    for r in results:
        f.write(json.dumps(r, ensure_ascii=False) + '\n')

print(f"\n  Done -> {OUT}", file=sys.stderr)
