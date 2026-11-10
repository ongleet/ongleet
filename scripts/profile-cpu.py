#!/usr/bin/env python3
"""Sample CPU usage of an explicit Linux process tree, without ptrace."""
import argparse
import os
from pathlib import Path
import time


def stat(path):
    try:
        raw = path.read_text()
        end = raw.rindex(')')
        fields = raw[end + 2:].split()
        return (int(fields[1]), int(fields[11]) + int(fields[12]),
                int(fields[19]), raw[raw.index('(') + 1:end])
    except (OSError, ValueError, IndexError):
        return None


def snapshot(root, identity):
    processes = {}
    for path in Path('/proc').iterdir():
        if path.name.isdecimal():
            value = stat(path / 'stat')
            if value:
                processes[int(path.name)] = value
    if root not in processes or processes[root][2] != identity:
        return None
    selected = {root}
    while True:
        children = {pid for pid, value in processes.items() if value[0] in selected}
        expanded = selected | children
        if expanded == selected:
            break
        selected = expanded
    rows = {}
    for pid in selected:
        try:
            tasks = list(Path(f'/proc/{pid}/task').iterdir())
        except OSError:
            continue
        for task in tasks:
            value = stat(task / 'stat')
            if value:
                rows[(pid, int(task.name), value[2])] = (value[1], value[3])
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('pid', type=int, help='Ongleet GUI PID (explicit selection)')
    parser.add_argument('--duration', type=float, default=10, help='seconds (default: 10)')
    parser.add_argument('--interval', type=float, default=1, help='sample seconds (default: 1)')
    args = parser.parse_args()
    if not (0 < args.duration <= 3600 and 0.05 <= args.interval <= 60):
        parser.error('duration must be 0..3600 seconds; interval must be 0.05..60')
    root = stat(Path(f'/proc/{args.pid}/stat'))
    if root is None:
        parser.error('PID is unavailable; this tool requires Linux /proc access')
    previous = snapshot(args.pid, root[2])
    started = last = time.monotonic()
    totals = {}
    ticks = os.sysconf('SC_CLK_TCK')
    try:
        while time.monotonic() - started < args.duration:
            time.sleep(min(args.interval, args.duration - (time.monotonic() - started)))
            current = snapshot(args.pid, root[2])
            now = time.monotonic()
            if current is None:
                print('Root process exited; sampling stopped.')
                break
            rows = []
            for key, (cpu, name) in current.items():
                if key not in previous:
                    continue
                delta = max(0, cpu - previous[key][0]) / ticks
                total = totals.get(key, (0, name))[0] + delta
                totals[key] = (total, name)
                rows.append((delta / (now - last) * 100, key, name))
            print('\n CPU%     PID     TID  Thread (100% = one core)', flush=True)
            for percent, (pid, tid, _), name in sorted(rows, reverse=True)[:20]:
                print(f'{percent:6.1f} {pid:7} {tid:7}  {name}')
            previous, last = current, now
    except KeyboardInterrupt:
        pass
    print('\n CPU seconds     PID     TID  Thread (observed samples)')
    for (pid, tid, _), (cpu, name) in sorted(totals.items(), key=lambda item: item[1][0], reverse=True)[:30]:
        print(f'{cpu:12.3f} {pid:7} {tid:7}  {name}')


if __name__ == '__main__':
    main()
