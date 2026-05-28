#!/usr/bin/env python3
"""Random 100 entries, extract with and without title_anchored, output combined JSONL."""

import json, random, subprocess, sys, tempfile, os, shutil
from pathlib import Path

DATA = Path.home() / '下载' / 'webmainbench.jsonl'
EXTRACTOR = Path.cwd() / 'target' / 'release' / 'extract_stdin'
OUT = '/tmp/eval100_compare.jsonl'

entries = [json.loads(l) for l in DATA.open()]
random.seed(42)
selected = random.sample(entries, 100)

print(f"Processing {len(selected)} entries x 2 modes...", file=sys.stderr)

# Build a version without title_anchored
# We'll compile a separate binary with title_anchored disabled
import tempfile
src_dir = Path.cwd() / 'src'
extract_rs = src_dir / 'extract.rs'

# Read the original
with open(extract_rs) as f:
    original = f.read()

# Create disabled version by commenting out the title_anchored block
# Find the block
disabled = original.replace(
    '    // Try title-anchored content extraction',
    '    // Try title-anchored content extraction — DISABLED\n    /*'
)
# Close the comment before "Try sophisticated content selector rules"
disabled = disabled.replace(
    '    }\n\n    // Try sophisticated content selector rules',
    '    }\n    */\n\n    // Try sophisticated content selector rules'
)

if disabled == original:
    print("WARNING: Failed to disable title_anchored", file=sys.stderr)
else:
    # Write temp version
    tmp_src = src_dir / 'extract_compare.rs'
    with open(tmp_src, 'w') as f:
        f.write(disabled)
    # Replace original
    shutil.copy2(extract_rs, src_dir / 'extract_orig.rs')
    shutil.copy2(tmp_src, extract_rs)
    tmp_src.unlink()

    # Build without
    print("Building without title_anchored...", file=sys.stderr)
    proc = subprocess.run(
        ['cargo', 'build', '--release', '--bin', 'extract_stdin'],
        capture_output=True, timeout=300,
        cwd=Path.cwd(),
    )
    if proc.returncode != 0:
        print("Build failed!", proc.stderr.decode()[:500], file=sys.stderr)
        # Restore
        shutil.copy2(src_dir / 'extract_orig.rs', extract_rs)
        (src_dir / 'extract_orig.rs').unlink()
        sys.exit(1)
    
    EXTRACTOR_NO_TA = Path.cwd() / 'target' / 'release' / 'extract_stdin_no_ta'
    shutil.copy2(Path.cwd() / 'target' / 'release' / 'extract_stdin', EXTRACTOR_NO_TA)
    
    # Restore original and rebuild with title_anchored
    shutil.copy2(src_dir / 'extract_orig.rs', extract_rs)
    (src_dir / 'extract_orig.rs').unlink()
    print("Building with title_anchored...", file=sys.stderr)
    subprocess.run(
        ['cargo', 'build', '--release', '--bin', 'extract_stdin'],
        capture_output=True, timeout=300, cwd=Path.cwd(),
    )

# Now run extraction for both modes
with open(OUT, 'w') as out:
    for i, e in enumerate(selected):
        url = e.get('url', '')
        html = e.get('html', '')
        ground_truth = e.get('convert_main_content', e.get('main_html', ''))
        gt_len = len(ground_truth)

        # With title_anchored
        proc1 = subprocess.run(
            [str(EXTRACTOR), '--url', url, '--markdown'],
            input=html.encode('utf-8'), capture_output=True, timeout=120,
        )
        try:
            r1 = json.loads(proc1.stdout.decode('utf-8', errors='replace'))
        except:
            r1 = {'main_content': '', 'content_markdown': None, 'confidence': 0.0}

        # Without title_anchored
        if EXTRACTOR_NO_TA.exists():
            proc2 = subprocess.run(
                [str(EXTRACTOR_NO_TA), '--url', url, '--markdown'],
                input=html.encode('utf-8'), capture_output=True, timeout=120,
            )
            try:
                r2 = json.loads(proc2.stdout.decode('utf-8', errors='replace'))
            except:
                r2 = {'main_content': '', 'content_markdown': None, 'confidence': 0.0}
        else:
            r2 = {'main_content': '', 'content_markdown': None, 'confidence': 0.0}

        record = {
            'track_id': i,
            'url': url,
            'ground_truth_len': gt_len,
            'extracted_len_ta': len(r1.get('main_content', '')),
            'extracted_len_nota': len(r2.get('main_content', '')),
            'md_len_ta': len(r1.get('content_markdown') or ''),
            'md_len_nota': len(r2.get('content_markdown') or ''),
            'confidence_ta': round(r1.get('confidence', 0.0), 4),
            'confidence_nota': round(r2.get('confidence', 0.0), 4),
            'ground_truth': ground_truth,
            'extracted_ta': r1.get('main_content', ''),
            'extracted_nota': r2.get('main_content', ''),
            'content_markdown_ta': r1.get('content_markdown') or '',
            'content_markdown_nota': r2.get('content_markdown') or '',
        }
        out.write(json.dumps(record, ensure_ascii=False) + '\n')
        sys.stderr.write(f'  [{i+1}/100]\r')

print(f"\nDone -> {OUT}", file=sys.stderr)

# Quick stats
data = [json.loads(l) for l in open(OUT)]
same_text = sum(1 for r in data if r['extracted_len_ta'] == r['extracted_len_nota'])
diff_md = sum(1 for r in data if r['md_len_ta'] != r['md_len_nota'])
sys.stderr.write(f"Same text length: {same_text}/100, Different markdown: {diff_md}/100\n")
