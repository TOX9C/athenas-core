# Athena's Core — Beta Install Instructions

Thanks for testing Athena's Core! This build is an **unsigned beta**, so macOS shows a
warning the first time you open it. That's expected — follow the steps below.

**Requirements:** Apple Silicon Mac (M1/M2/M3/M4), macOS 13.0 or later.

## Install

1. Download `Athena's Core_0.3.0_aarch64.dmg`.
2. Double-click the `.dmg` to open it.
3. Drag **Athena's Core** into your **Applications** folder.
4. Eject the disk image (drag it to the Trash, or right-click → Eject).

## First launch (important)

The first time you open it, macOS will say the app **"can't be opened because Apple
cannot check it for malicious software."** Do **not** double-click it normally.

1. Open **Finder → Applications**.
2. **Right-click** (or Control-click) **Athena's Core**.
3. Choose **Open** from the menu.
4. In the dialog that appears, click **Open** again.

The app now opens normally, and **you only need to do this once** — future launches
work with a normal double-click.

> If the right-click → Open dialog still doesn't offer an Open button: go to
> **System Settings → Privacy & Security**, scroll to the bottom, and click
> **Open Anyway** next to "Athena's Core".

## Reporting issues

If something breaks, freezes, or behaves unexpectedly, tell the release owner:
**what you did**, **what you expected**, and **what happened instead**. Include
your macOS version ( Apple menu → About This Mac ).

---

**Why the warning?** This beta is unsigned/not notarized. Signing requires a paid
Apple Developer membership, which is planned for the public release. The warning is
Apple's standard gate for any app distributed outside the App Store without a
Developer ID — it does not mean the app is unsafe.
