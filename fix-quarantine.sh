#!/bin/bash
# Run this ONCE on the target Mac after copying EconProject.app to /Applications.
#
# Why this exists: the .dmg is ad-hoc signed (no Apple Developer ID, not
# notarized). When you transfer it via AirDrop, USB, web download, or email,
# macOS attaches a com.apple.quarantine extended attribute. Combined with
# the ad-hoc signature, Gatekeeper rejects the .app with one of:
#   - "Apple cannot verify that this app is free of malware"
#   - "EconProject.app has been modified or damaged. Move it to the Trash."
#
# Stripping the quarantine attribute resolves both. After this, the app
# launches normally on every subsequent run.

set -e

APP_PATH="${1:-/Applications/EconProject.app}"

if [ ! -d "$APP_PATH" ]; then
  echo "Error: $APP_PATH does not exist."
  echo "Usage: $0 [path-to-EconProject.app]"
  echo "Default path: /Applications/EconProject.app"
  exit 1
fi

echo "Clearing quarantine and other extended attributes from $APP_PATH..."
xattr -cr "$APP_PATH"
echo "Done. You can now launch EconProject normally."
