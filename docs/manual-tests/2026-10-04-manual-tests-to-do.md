# Manual tests to do (2026-10-04)

These tests have to be done by hand in the real app. Tick each box when it passes. If one fails, note what you saw next to it.

Always test the **installed app opened from Finder or the Dock**. Don't use `tauri dev` or a launch from Terminal, because they inherit your shell's settings and can hide bugs.

## Already passed
- [x] **v0.3.1: mlx-lm starts from the Finder-launched app** (#87, PR #89, commit `565607f`). Passed 2026-10-04.

## A. Can be tested now on v0.3.1

### A1. Restore servers on launch (#82, PR #84, commit `43a5825`)
Settings → General has two switches: **Keep servers running when LocalBar quits** and **Restore servers that were running when LocalBar quit**.
- [ ] Turn **Restore** off and **Keep running** off. Start an mlx-lm instance, quit LocalBar, then reopen it. The instance should stay **stopped**.
- [ ] Turn **Restore** on, keeping **Keep running** off. Start the same instance, quit, then reopen. It should **start again by itself**.

### A2. Ollama model lists and model folders (#83, PR #85, commit `31fb6c4`)
- [ ] **Quit the Ollama app.** Check that `mistral:7b` and `bge-m3` still appear in all three places:
  - Settings → Discovery
  - the Ollama instance's Models list
  - Add Instance → Ollama
- [ ] In **Discovery → "Ollama — Models folder"**, choose an empty folder. The lists should go empty. Clear the field and the models should come back.
- [ ] In an instance's detail panel, the **"Model folder (overrides Discovery)"** field picks a folder and changes that instance's model list.
- [ ] In **Add Instance**, the **Model folder** field works for both Ollama and mlx-lm.

## B. Needs v0.3.2 or later

### B1. Install button and mlx-lm detection (#88, PR #91, commit `a6ed156`)
- [ ] **Add Instance → mlx-lm**: the executable field fills in by itself, probably with `uv` or `uvx`.
- [ ] **Not-found error:** set an instance's executable to a made-up name like `uvx-missing`, then click Start. You should see "Couldn't find `uvx-missing`. Looked in: …" with **Choose…** and **Install…** buttons.
- [ ] **Choose… in Settings:** pick `~/.local/bin/uvx`. The error should clear, and Start should work.
- [ ] **Choose… in the menu bar popover:** the popover may close while the file picker is open. Check that the chosen file is still saved, then note whether the behaviour feels acceptable.
- [ ] **Install… from the popover:** it should open Settings on that instance, with the install dialog showing.
- [ ] **Install… dialog:** it lists the exact commands before running anything. On your Mac it should only need `uv tool install mlx-lm`, since uv is already installed. Confirm it, and afterwards the instance should switch to `mlx_lm.server` and offer Start.
- [ ] **Install… on an Ollama instance:** it should show a link to ollama.com/download and install nothing.

### B2. Detection progress (#92, PR #95, commit `7e15a0b`)
- [ ] While Add Instance → mlx-lm is searching, a line shows each place it's checking, with a seconds counter.
- [ ] **Skip** stops the search, and the form stays usable the whole time.
- [ ] When it finishes, one plain line says what was found, or explains what to do if nothing was found.

## C. Needs v0.3.3

### C1. Version and update check (#97, PR #98, commit `ae5ebab`)
- [ ] The version **"LocalBar 0.3.3"** appears in three places: the tray icon tooltip, the bottom of the menu bar popover, and the bottom of Settings → General.
- [ ] **Settings → General → Updates:** "Check GitHub for new versions once a day" is ticked by default. Click **Check now**; it should say **"You're up to date"**.
- [ ] **Seeing the notice:** this can only be tested properly once a newer release (v0.3.4 or later) exists. Open v0.3.3 then; within about 15 seconds the popover and General tab should show "v0.3.4 is available", and clicking it should open the release page in your browser.
- [ ] Untick the checkbox. The notice should disappear, and **Check now** should still work when clicked.

## Related, but not a test
- [ ] **Opening the unsigned app** (#93). You decided not to join the Apple Developer Program for now. When you install v0.3.3, check that the README's "Installation → Option 1" steps and the release-page instructions match what you actually see.
