# Demo automation

A minimal, dependency-free Python script for testing GitGlass's **Automations**
feature and GitHub Actions scheduling.

## What it does

`hello_automation.py` prints a timestamped "it works" report using only the
Python standard library — no packages to install. It's a stand-in for whatever
real automation you want to schedule.

## Run it on GitHub Actions (via GitGlass)

1. Open this folder in **GitGlass**.
2. Toolbar → **Automations**.
3. Choose **hello_automation.py**, pick a frequency, click **Create automation**.
4. **Stage → commit → push.** GitHub runs it on the schedule you chose.
5. Watch it under the repo's **Actions** tab, or use the workflow's manual
   **Run** button to trigger it immediately.

Because the script has no dependencies, it runs on a fresh Actions runner with
just `setup-python` — nothing else needed.

## Run it locally (optional)

You'd need Python installed on your machine (it isn't required for the Actions
route above):

```bash
python examples/automation/hello_automation.py
```
