# Manual tests

Checks that need a real machine, a real container engine or a human at the keyboard, so no automated test covers them. Each section says what is checked, how, and what must happen. Record the date, host and result in the matching decision section of `container-runtimes.md` or in `PLAN.md`.

## Sleep prevention while a run executes (desktop)

Implemented 2026-11-25, design in `container-runtimes.md` "System sleep during desktop runs". Automated: `cargo test -p macworp_worker sleep_prevention` (fake power source and inhibitor). Not automated: the real OS calls below.

**Rules under test:** while a run executes on mains power the computer is kept awake (idle sleep only); on battery nothing is held; unplugging releases the hold within about 5 s, plugging in takes it back; closing the lid always sleeps; the status bar warns whenever a run executes.

### Setup

1. A laptop (the battery checks need one) with a working container engine: `cargo run -p macworp_worker -- doctor` shows no failure.
2. Start the desktop app: `cd packages/desktop && dx run --desktop --args=-vvv` (the log shows `Keeping the computer awake while workflows run` and `No longer keeping the computer awake`).
3. Create a workflow that runs for several minutes, for example a **Container** workflow on a small image (`alpine`) with the command `sleep 600`. Create a project for it.
4. Charger plugged in, battery not necessarily full.

### 1. Hold on mains power

1. Start the run. In the status bar the badge **Keeping awake** appears within 5 s.
2. macOS: `pmset -g assertions` lists `PreventUserIdleSystemSleep` named `A MAcWorP workflow is running`, owned by the app. Linux: `systemd-inhibit --list` lists the app with reason `A MAcWorP workflow is running`. Windows: `powercfg /requests` lists the app under `SYSTEM`.
3. Let the run end (or cancel it). The badge disappears within 5 s and the assertion is gone from the same command.

### 2. Unplug during a run

1. Start the run on mains power, check step 1.1 and 1.2.
2. Unplug the charger. Within about 5 s the badge changes to **Do not let the computer sleep** (hover shows the full warning) and the assertion disappears. The log shows `No longer keeping the computer awake`.
3. Plug the charger in again. Within about 5 s the badge is **Keeping awake** again and the assertion is back.
4. Cancel the run: badge and assertion are gone.

### 3. Battery from the start

1. Unplug the charger, then start the run.
2. The badge says **Do not let the computer sleep** at once and no assertion is listed.
3. Optional, shows what the warning is about: `pmset sleepnow` (macOS) or suspend from the menu. After wake the run is stuck or failed, the engine VM may need `podman machine start` or a restart of Docker Desktop. Note what you see (run state, log gap, container clock: `podman exec <container> date`).

### 4. Lid close on mains power

1. Start the run on mains power, without an external display.
2. Close the lid. The computer sleeps (the app must not prevent it). Open it after a minute.
3. Note what happens to the run as in 3.3. Expected: the run is not protected, only idle sleep is inhibited.

### 5. Idle sleep is prevented

1. Set the idle sleep time short (macOS: System Settings > Lock Screen or `sudo pmset -a sleep 1` and restore it afterwards; the display may still turn off).
2. Start the run on mains power and leave the computer alone for longer than that time.
3. The computer stays awake and the run keeps going. Without a run it would have slept.

### 6. Several runs and end of run

1. Start two runs (a second project, or queue one) and check that one assertion is listed, not two.
2. When the first ends the assertion stays; when the last ends it is gone.
3. Quit the app during a run: the assertion is gone (the OS releases it with the process).

### Not covered yet

Windows and Linux hosts, a desktop without a battery (must count as mains), a sleep note in the run log after a detected suspend (not implemented).

## Windows desktop app: Explorer button and responsiveness (interactive session)

Not automatable over SSH; run in the Windows VM's console. Context: `container-runtimes.md` "Windows host", PLAN.md P7.

1. Allow the app through Windows Firewall when the prompt appears (otherwise Nextflow/Snakemake runs hang, `doctor` shows `progress-receiver`).
2. `cd packages\desktop && dx run --desktop --args=-vvv`. Create a project, open its page, click **Open in file explorer**: Explorer opens the project folder (no `\\?\` form).
3. Start a Container workflow with `sleep 600`. For the whole run keep clicking through the app: it must stay responsive (the earlier 100 % CPU freeze came from running the UI inside `block_on`). If it freezes, record the symptom, the log and `Get-Process` CPU, and add it to PLAN.md P7.

## What CI does not cover
CI (`.github/workflows/ci.yml`) has no container runtime on its macOS and Windows runners, so only unit tests run there. Everything that starts a real engine on those systems (podman machine, Docker Desktop, native Windows podman, `wslc`) is checked by hand with the sections above. On Linux, CI runs the `Container` end-to-end test on Docker and Podman; Snakemake and CWL runs are not in CI.
