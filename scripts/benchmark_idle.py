#!/usr/bin/env python3
"""Sample the main Tomo process; excludes WebKit helper processes and live workloads."""
import argparse
import json
import statistics
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('pid', type=int)
parser.add_argument('--samples', type=int, default=10)
parser.add_argument('--interval', type=float, default=2)
parser.add_argument('--output', type=Path, default=Path('docs/verification/idle.json'))
args = parser.parse_args()
samples = []
for index in range(args.samples):
    value = subprocess.check_output(['ps', '-p', str(args.pid), '-o', 'rss=,%cpu='], text=True).split()
    if len(value) != 2:
        raise SystemExit('The selected Tomo process is no longer running.')
    samples.append({'rssKiB': int(value[0]), 'cpuPercent': float(value[1])})
    if index + 1 < args.samples:
        time.sleep(args.interval)
result = {'scope': 'Main process only; excludes WebKit helpers. No connected accounts.',
          'sampleIntervalSeconds': args.interval, 'samples': samples,
          'meanCpuPercent': statistics.mean(item['cpuPercent'] for item in samples),
          'maxRssMiB': max(item['rssKiB'] for item in samples) / 1024}
args.output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({key: value for key, value in result.items() if key != 'samples'}, indent=2))
