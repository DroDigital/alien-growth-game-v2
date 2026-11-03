# Release process

This covers signing, notarization and packaging, which need your accounts. CI produces
unsigned artifacts: `alien-growth-macos-arm64` and `alien-growth-windows-x64`.

## Build

Fetch and verify assets first: `build.sh` copies `assets/` and `CREDITS.md` into the staged
build, so anything fetched afterwards would not be in it.

```bash
python3 tools/fetch_assets.py --locked --strict
cargo agtool verify-assets --require-all
DIST=1 tools/build.sh mac release
cargo agtool --root dist/mac-release verify-assets --require-all
```

The last command checks the staged copy (`dist/mac-release/assets`) against the manifest, so a
build missing assets or carrying stale ones fails before signing.

Output: `dist/<platform>-release/` containing `launcher`, `game`, `assets/`, `data/`,
`shaders/` and `CREDITS.md`.

## macOS: app bundle, signing, notarization

Requirements: Apple Developer Program membership, a "Developer ID Application" certificate in
the login keychain, and Xcode command-line tools.

1. **Bundle layout**: `AlienGrowth.app/Contents/MacOS/{launcher,game}`,
   `Contents/Resources/{assets,data,shaders,CREDITS.md}`, and `Contents/Info.plist` with
   `CFBundleExecutable=launcher`, `CFBundleIdentifier=dev.drodigital.alien-growth`,
   `LSMinimumSystemVersion=11.0`. `game_core::paths::content_root` already resolves
   `Contents/Resources`.
2. **Entitlements** (`entitlements.plist`): hardened runtime. No JIT entitlements are needed.
   Add `com.apple.security.cs.disable-library-validation` only if a third-party dylib
   requires it (Steam overlay in M6).
3. **Sign** (inside-out, hardened runtime, secure timestamp):
   ```bash
   codesign --force --options runtime --timestamp \
     --entitlements entitlements.plist \
     --sign "Developer ID Application: <NAME> (<TEAMID>)" \
     AlienGrowth.app/Contents/MacOS/game AlienGrowth.app/Contents/MacOS/launcher
   codesign --force --options runtime --timestamp \
     --entitlements entitlements.plist \
     --sign "Developer ID Application: <NAME> (<TEAMID>)" AlienGrowth.app
   codesign --verify --deep --strict --verbose=2 AlienGrowth.app
   ```
4. **Notarize**, using an app-specific password stored once with `notarytool store-credentials`:
   ```bash
   ditto -c -k --keepParent AlienGrowth.app AlienGrowth.zip
   xcrun notarytool submit AlienGrowth.zip --keychain-profile "AGG_NOTARY" --wait
   xcrun stapler staple AlienGrowth.app
   spctl --assess --type execute --verbose AlienGrowth.app
   ```
5. **Distribute** as a DMG (`hdiutil create -volname "Alien Growth" -srcfolder AlienGrowth.app -ov -format UDZO AlienGrowth.dmg`),
   then sign and notarize the DMG the same way.

## Windows: Authenticode signing

Requirements: a code-signing certificate (OV, or EV for instant SmartScreen reputation) on a
hardware token or cloud HSM, and the Windows SDK (`signtool`).

```powershell
signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 `
  /n "<Certificate subject>" dist\windows-release\launcher.exe dist\windows-release\game.exe
signtool verify /pa /v dist\windows-release\launcher.exe
```

Package with an installer (for example Inno Setup or WiX) or as a zip. Sign the installer too.

## Integrity manifest (M6)

The launcher verifies files against a SHA-256 manifest signed with Ed25519. The private key
must stay offline (hardware token or an air-gapped machine), and only the public key ships in
the launcher. Key generation and rotation will be documented here in M6.

## Steam (M6)

Steam builds hide the updater and default to "launch directly". Depot configuration,
`steam_appid.txt` and the Steamworks SDK redistributables will be documented in M6.
