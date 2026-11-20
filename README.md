# pairkit

Pairs an iPhone with itself (iOS 27, no computer) and installs SideStore or AltStore, all from the Open Minis shell.

## Before you start
- Open Minis and keep it open.
- Turn LocalDevVPN **on**.

## 1. Download

```
wget -O pairkit https://github.com/Hamouddabest/pairkit/releases/download/latest/pairkit-aarch64
chmod +x pairkit
```

If `pairkit` already exists, run `rm pairkit` first.

## 2. Pair (once)

```
./pairkit
```

In Settings > Privacy & Security > Developer Mode, tap "Pair with pairkit", then enter your passcode and the 6-digit code shown in the terminal. The pairing file is copied to your clipboard.

## 3. Install

Pick one:

**AltStore**
```
./pairkit install altstore
```

**SideStore**
```
./pairkit install sidestore
```

A web page opens (or a `127.0.0.1` link is printed - tap it). Enter your Apple ID, password and 2FA code there. The password is never saved.

If your Apple ID already has certificates, you are asked whether to reuse the saved one (type 1) or revoke and make a new one (type 2).
