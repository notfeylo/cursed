# The verification VM

Everything in [`update-path.md`](update-path.md) marked **BLOCKED — awaiting
VM** needs a Windows machine that can be put back to a known state between
runs. This is how to get one, and it costs nothing.

## Why not this machine

The bug being verified deletes `%APPDATA%\Cursed`. The development machine has
the live install in that directory, with the author's imported packs, custom
cursors and — the one that cannot be re-made — the original-scheme snapshot.
Running a delete-the-directory failure mode against the directory holding the
irreplaceable data is not a test.

Windows Sandbox would be the obvious answer and is not available: it needs
Windows Pro, and this is Windows 11 Home. That is the same wall
[`v1.7.0.md`](v1.7.0.md) hit.

## What to install

Both free, neither needs a licence key.

| | |
| --- | --- |
| Hypervisor | [VirtualBox](https://www.virtualbox.org/wiki/Downloads), Windows host build |
| Guest image | The **Windows 11 Enterprise evaluation** ISO from the [Microsoft Evaluation Center](https://www.microsoft.com/en-us/evalcenter/evaluate-windows-11-enterprise) — 90 days, renewable, no key |

Microsoft also publishes ready-made VirtualBox images for testing, which expire
faster and arrive pre-configured. Either works. The evaluation ISO is preferred
here because a clean install is closer to what a user's machine looks like than
a developer-tools image.

### Guest settings that matter

- **4 GB RAM, 2 CPUs, 64 GB disk.** WebView2 is the memory-hungry part.
- **Enable EFI**, and attach a **TPM 2.0** device. Windows 11 setup refuses
  without both. VirtualBox 7 supports TPM under Machine → Settings → System.
- **Bridged or NAT networking.** The update path fetches from GitHub, so the
  guest needs to reach the internet — that is the mechanism under test.
- **Do not install Guest Additions on the pristine snapshot.** They are
  convenient and they are also software that touches the pointer, which is the
  one subsystem being measured. Install them after the snapshot if the shared
  clipboard is worth it, and know that the snapshot you roll back to is the one
  without them.

### The snapshot

After Windows setup finishes and the machine reaches the desktop, before
installing anything at all:

1. Sign in, skip everything optional, reach the desktop.
2. Shut down cleanly.
3. Machine → Snapshots → **Take**, named `pristine`.

Every run starts by restoring `pristine`. A machine that has had Cursed on it
once is not a first-install machine again, and the first install is one of the
things being checked.

## Running the matrix

Copy the repository — or just `scripts/` and the installers — into the guest,
then one command:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\verify-release.ps1 `
  -From  C:\builds\Cursed_1.20.0_x64-setup.exe `
  -To    1.21.0
```

`-From` is the **older** installer, the one the update is *from*. `-To` is the
version the update should produce. The script walks the whole sequence, pausing
where a human has to click something, and prints a pass/fail table at the end.
Paste that table into the verification record.

It refuses to run on a machine that looks like a real install — more than a
token amount of data in `%APPDATA%\Cursed` — unless `-Force` is passed. That
guard exists because the most likely way to lose the author's own data is to run
this script on the wrong machine at two in the morning.

### What one full run needs

Two installers, because an update needs something to update *from*:

- the previously released `Cursed_<old>_x64-setup.exe`, from the GitHub release;
- a release of the new version for the updater to find. Until one is published,
  the update step has nothing to fetch and the script says so and skips to the
  uninstall rows.

That ordering is awkward and unavoidable: the update path can only be verified
against a published release, and the release should not be published until the
update path is verified. The way through it is a **draft** release with the
assets attached — the updater reads the GitHub releases API, and a draft is not
returned by it, so a draft cannot be tested against either.

So the honest sequence is: publish, verify immediately on the VM, and be
prepared to pull the release within the hour if a row fails. The alternative —
verifying a build that is not the one users get — is how the original bug
survived three releases.

## Between runs

Restore `pristine`. Not "uninstall and try again": an uninstall that leaves
something behind is one of the things being measured, so the next run must not
start on the residue of the last one.
