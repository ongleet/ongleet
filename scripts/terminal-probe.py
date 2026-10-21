"""Run inside the prototype's PTY; requires only Python's standard library."""

import argparse
import curses
import os
import time


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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["unicode", "flood", "screen"])
    args = parser.parse_args()
    if args.mode == "unicode":
        for byte in "Split UTF-8: café λ 日本語 🙂 e\u0301\r\n".encode():
            os.write(1, bytes([byte]))
            time.sleep(0.005)
    elif args.mode == "flood":
        for batch in range(100):
            data = (f"Batch {batch}: bounded output café λ 🙂\r\n" * 1000).encode()
            while data:
                data = data[os.write(1, data):]
    else:
        print("NORMAL SCREEN: this must return after leaving the probe.", flush=True)
        curses.wrapper(screen)
        print("Normal screen restored.")


if __name__ == "__main__":
    main()
