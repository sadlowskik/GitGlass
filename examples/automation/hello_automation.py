#!/usr/bin/env python3
"""
GitGlass demo automation.

A tiny, dependency-free "heartbeat" script — the kind of thing you'd schedule to
run on GitHub Actions. It uses only the Python standard library, so it runs on a
fresh Actions runner with no `pip install` step, and it prints a clear report so
you can confirm it actually ran (look at the run logs in the Actions tab).

Try it with GitGlass:
  1. Open this folder in GitGlass.
  2. Click "Automations" in the toolbar.
  3. Pick this file, choose a frequency (e.g. "Every hour"), and Create.
  4. Stage → commit → push. GitHub runs it on schedule. 🎉
"""

import platform
import sys
from datetime import datetime, timezone


def main() -> int:
    now_utc = datetime.now(timezone.utc)

    print("=" * 44)
    print("  GitGlass automation — it works! ✅")
    print("=" * 44)
    print(f"  Ran at        : {now_utc.strftime('%Y-%m-%d %H:%M:%S')} UTC")
    print(f"  Python version: {platform.python_version()}")
    print(f"  Running on    : {platform.system()} ({platform.machine()})")

    # A tiny "real" task so the output isn't just static text: a quick sum.
    total = sum(range(1, 101))
    print(f"  Sample work   : sum(1..100) = {total}")

    print("-" * 44)
    print("  If you can read this in your Actions logs,")
    print("  your scheduled automation is up and running.")
    print("=" * 44)

    return 0


if __name__ == "__main__":
    sys.exit(main())
