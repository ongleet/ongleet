"""Run inside the prototype's PTY; requires only Python's standard library."""

import argparse
import curses
import os
import select
import sys
import termios
import time
import tty


def screen(stdscr):
    curses.noecho()
    curses.cbreak()
    stdscr.keypad(True)
    curses.mousemask(curses.ALL_MOUSE_EVENTS | curses.REPORT_MOUSE_POSITION)
    last = "Resize, type keys, scroll, or click. Press q to return to the normal screen."
    while True:
        stdscr.erase()
        rows, cols = stdscr.getmaxyx()
        for row, text in enumerate([
            "Alternate screen / keyboard / mouse probe",
            f"Dimensions: {cols} columns x {rows} rows",
            "Unicode: café · λ · 日本語 · 🙂 · e\u0301",
            last,
        ]):
            if row < rows - 1:
                stdscr.addnstr(row, 0, text, max(0, cols - 1))
        stdscr.refresh()
        key = stdscr.get_wch()
        if key == "q":
            return
        if key == curses.KEY_MOUSE:
            last = f"Mouse event: {curses.getmouse()!r}"
        else:
            last = f"Key event: {key!r}"


def reload_probe():
    """Keep output moving and count cursor-query replies across frontend reloads."""
    previous = termios.tcgetattr(sys.stdin.fileno())
    responses = 0
    pressed = False
    tick = 0
    try:
        tty.setraw(sys.stdin.fileno())
        os.write(1, b"RELOAD PROBE: press x to test input, q to quit.\r\n\x1b[?2004h\x1b[6n")
        while True:
            if select.select([sys.stdin], [], [], 0.1)[0]:
                data = os.read(sys.stdin.fileno(), 8192)
                if not data or b"q" in data:
                    break
                # This fixture sends one DSR and tests only x/q as user input;
                # R terminates its cursor reply. It does not parse PTY output.
                responses += data.count(b"R")
                pressed = pressed or b"x" in data
            tick += 1
            size = os.get_terminal_size()
            status = f"\r\x1b[2KPID={os.getpid()} TICK={tick} RESPONSES={responses} KEY={'x' if pressed else '-'} SIZE={size.columns}x{size.lines}"
            os.write(1, status.encode())
    finally:
        os.write(1, b"\x1b[?2004l\r\n")
        termios.tcsetattr(sys.stdin.fileno(), termios.TCSANOW, previous)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["unicode", "flood", "screen", "reload"])
    parser.add_argument("--batches", type=int, default=100, help="Flood batches of 1,000 lines (1..1000)")
    args = parser.parse_args()
    if not 1 <= args.batches <= 1000:
        parser.error("--batches must be between 1 and 1000")
    if args.mode == "unicode":
        for byte in "Split UTF-8: café λ 日本語 🙂 e\u0301\r\n".encode():
            os.write(1, bytes([byte]))
            time.sleep(0.005)
    elif args.mode == "flood":
        for batch in range(args.batches):
            data = (f"Batch {batch}: bounded output café λ 🙂\r\n" * 1000).encode()
            while data:
                data = data[os.write(1, data):]
    elif args.mode == "reload":
        reload_probe()
    else:
        print("NORMAL SCREEN: this must return after leaving the probe.", flush=True)
        curses.wrapper(screen)
        print("Normal screen restored.")


if __name__ == "__main__":
    main()
