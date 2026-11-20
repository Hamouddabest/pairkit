# pairkit

Pairs an iPhone with itself (iOS 27, no computer) and installs SideStore, AltStore, LiveContainer or any IPA, all from the Open Minis shell.

## Before you start
- Open Minis and keep it open.
- Turn LocalDevVPN **on**.

## 1. Download

```
wget -O pairkit https://github.com/Hamouddabest/pairkit/releases/download/latest/pairkit-aarch64
chmod +x pairkit
```

To update later, run `rm pairkit` first, then the same two commands.

## 2. Pair (once)

```
./pairkit
```

In Settings > Privacy & Security > Developer Mode, tap "Pair with pairkit", then enter your passcode and the 6-digit code shown in the terminal. The pairing file is copied to your clipboard and saved in `~/.pairkit`.

## 3. Install

```
./pairkit install
```

A web page opens (or a `127.0.0.1` link is printed - tap it). On the page:

1. Pick the app: **1** SideStore, **2** AltStore, **3** LiveContainer, **4** custom IPA (paste a link or file path). The latest version is downloaded.
2. Enter your Apple ID, password and 2FA code. The password is never saved.
3. If your Apple ID already has certificates, choose **1** to reuse the saved one or **2** to revoke and make a new one.
4. Use the **Export pairing file** button to save a copy of the pairing file (share sheet / Files), or copy its text.

Install the PairKit iPhone app itself (no other sideloader needed):

```
./pairkit install pairkit
```

You can skip the app question by naming it:

```
./pairkit install sidestore
./pairkit install altstore
./pairkit install livecontainer
```

## Notes
- A certificate can only be reused if pairkit made it earlier (it needs the saved key).
- Free Apple IDs have a small weekly app ID limit. Extensions reuse the main app's ID to save slots.
- If you see a DNS / "Try again" error, switch between Wi-Fi and mobile data and rerun; signing retries automatically.

## iPhone app (no terminal)
The same thing as a native app, with pairing, app picker, download progress and settings built in.

IPA: https://github.com/Hamouddabest/pairkit/releases/download/app/PairKit.ipa

Install it with AltStore/SideStore (or any sideloader). Same rules: LocalDevVPN on, keep the app open while installing.
