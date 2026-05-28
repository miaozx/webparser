#!/usr/bin/env python3
"""Run eval100 with TA on vs off, produce side-by-side comparison JSONL."""

import json, random, subprocess, sys
from pathlib import Path

DATA = Path.home() / '下载' / 'webmainbench.jsonl'
EXTRACTOR = Path.cwd() / 'target' / 'release' / 'extract_stdin'
OUT = '/tmp/ta_comparison.jsonl'

entries = [json.loads(l) for l in DATA.open()]
random.seed(29)
selected = random.sample(entries, 100)

print(f"Processing {len(selected)} entries...", file=sys.stderr)

with open(OUT, 'w') as out:
    for i, e in enumerate(selected):
        url = e.get('url', '')
        html = e.get('html', '')
        ground_truth = e.get('convert_main_content', e.get('main_html', ''))

        # TA ON
        proc_on = subprocess.run(
            [str(EXTRACTOR), '--url', url, '--markdown'],
            input=html.encode('utf-8'),
            capture_output=True, timeout=120,
        )
        try:
            res_on = json.loads(proc_on.stdout.decode('utf-8', errors='replace'))
        except json.JSONDecodeError:
            res_on = {}

        # TA OFF
        proc_off = subprocess.run(
            [str(EXTRACTOR), '--url', url, '--markdown', '--no-title-anchored'],
            input=html.encode('utf-8'),
            capture_output=True, timeout=120,
        )
        try:
            res_off = json.loads(proc_off.stdout.decode('utf-8', errors='replace'))
        except json.JSONDecodeError:
            res_off = {}

        record = {
            'track_id': i,
            'url': url,
            'ground_truth_len': len(ground_truth),
            'ground_truth': ground_truth,
            # TA ON
            'ta_on_text': res_on.get('main_content', ''),
            'ta_on_md': res_on.get('content_markdown') or '',
            'ta_on_conf': res_on.get('confidence', 0.0),
            'ta_on_used': res_on.get('title_anchored_used', False),
            # TA OFF
            'ta_off_text': res_off.get('main_content', ''),
            'ta_off_md': res_off.get('content_markdown') or '',
            'ta_off_conf': res_off.get('confidence', 0.0),
        }
        out.write(json.dumps(record, ensure_ascii=False) + '\n')

        ta_flag = "TA" if record['ta_on_used'] else "  "
        print(f"  [{i+1}/100] {ta_flag} gt={record['ground_truth_len']:6d} on={len(record['ta_on_text']):6d} off={len(record['ta_off_text']):6d}", file=sys.stderr)

# Summary
records = [json.loads(l) for l in open(OUT)]
ta_count = sum(1 for r in records if r['ta_on_used'])
same = sum(1 for r in records if r['ta_on_text'] == r['ta_off_text'])
diff = 100 - same
ta_changed = sum(1 for r in records if r['ta_on_used'] and r['ta_on_text'] != r['ta_off_text'])

print(f"\n=== Summary ===", file=sys.stderr)
print(f"  TA triggered:   {ta_count}/100", file=sys.stderr)
print(f"  Same output:    {same}/100", file=sys.stderr)
print(f"  Different:      {diff}/100", file=sys.stderr)
print(f"  TA caused diff: {ta_changed}/100", file=sys.stderr)
print(f"  Done -> {OUT}", file=sys.stderr)
