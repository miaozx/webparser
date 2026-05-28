#!/usr/bin/env python3

import json, random, subprocess, sys
from pathlib import Path

DATA = Path.home() / '下载' / 'webmainbench.jsonl'
EXTRACTOR = Path.cwd() / 'target' / 'release' / 'extract_stdin'
OUT = '/tmp/eval100_md.jsonl'

entries = [json.loads(l) for l in DATA.open()]
random.seed(42)
selected = random.sample(entries, 100)

print(f"Processing {len(selected)} entries...", file=sys.stderr)

with open(OUT, 'w') as out:
    for i, e in enumerate(selected):
        url = e.get('url', '')
        html = e.get('html', '')
        ground_truth = e.get('convert_main_content', e.get('main_html', ''))
        gt_len = len(ground_truth)

        proc = subprocess.run(
            [str(EXTRACTOR), '--url', url, '--markdown'],
            input=html.encode('utf-8'),
            capture_output=True, timeout=120,
        )
        raw = proc.stdout.decode('utf-8', errors='replace')

        try:
            result = json.loads(raw)
        except json.JSONDecodeError:
            result = {'main_content': '', 'content_markdown': None, 'confidence': 0.0, 'title_anchored_used': False}

        extracted_text = result.get('main_content', '')
        extracted_md = result.get('content_markdown') or ''
        ta_used = result.get('title_anchored_used', False)
        ex_len = len(extracted_text)
        md_len = len(extracted_md)
        confidence = result.get('confidence', 0.0)

        record = {
            'track_id': i,
            'url': url,
            'ground_truth_len': gt_len,
            'extracted_len': ex_len,
            'md_len': md_len,
            'confidence': round(confidence, 4),
            'title_anchored_used': ta_used,
            'ground_truth': ground_truth,
            'extracted': extracted_text,
            'content_markdown': extracted_md,
        }
        out.write(json.dumps(record, ensure_ascii=False) + '\n')
        print(f"  [{i+1}/100] gt={gt_len} ex={ex_len} md={md_len}", file=sys.stderr)

print(f"\nDone -> {OUT}", file=sys.stderr)

# quick summary
total = 0
md_total = 0
gt_total = 0
md_ok = 0
for rec in [json.loads(l) for l in open(OUT)]:
    gt_total += rec['ground_truth_len']
    total += rec['extracted_len']
    md_total += rec['md_len']
    if rec['md_len'] > 0:
        md_ok += 1
print(f"\nSummary: text_total={total} md_total={md_total} gt_total={gt_total} md_ok={md_ok}/100", file=sys.stderr)
