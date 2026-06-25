#!/usr/bin/env python3
"""Run a command and report wall time + peak RSS (KB).

Usage: measure_rss.py [--quiet] <cmd> [args...]
With --quiet, child stdout/stderr are discarded and the only output
is the final PEAK_RSS_KB=... line — useful when the child produces
huge output (e.g. the solver in -f text mode for L8+).

Reads /proc/<pid>/status for VmHWM (high-water mark of resident set
size), which the kernel updates live as RSS grows.
"""
import sys, time, subprocess, threading

def read_vmhwm_kb(pid):
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmHWM:"):
                    return int(line.split()[1])
    except (FileNotFoundError, ProcessLookupError, ValueError, IndexError):
        pass
    return 0

def main():
    quiet = False
    args = sys.argv[1:]
    if args and args[0] == "--quiet":
        quiet = True
        args = args[1:]
    if not args:
        print("usage: measure_rss.py [--quiet] <cmd> [args...]", file=sys.stderr)
        sys.exit(2)
    proc = subprocess.Popen(args, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True)
    def pump(src, dst):
        try:
            for chunk in iter(lambda: src.read(4096), ""):
                if not quiet:
                    dst.write(chunk); dst.flush()
        except Exception:
            pass
    t_out = threading.Thread(target=pump, args=(proc.stdout, sys.stdout), daemon=True)
    t_err = threading.Thread(target=pump, args=(proc.stderr, sys.stderr), daemon=True)
    t_out.start(); t_err.start()
    peak = 0
    start = time.monotonic()
    while proc.poll() is None:
        v = read_vmhwm_kb(proc.pid)
        if v > peak:
            peak = v
        time.sleep(0.01)
    v = read_vmhwm_kb(proc.pid)
    if v > peak:
        peak = v
    wall_ms = int((time.monotonic() - start) * 1000)
    proc.wait()
    t_out.join(timeout=1); t_err.join(timeout=1)
    print(f"\nPEAK_RSS_KB={peak} WALL_MS={wall_ms} EXIT={proc.returncode}")

if __name__ == "__main__":
    main()
