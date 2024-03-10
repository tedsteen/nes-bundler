#!/bin/bash
if ! [ -x "$(command -v 7z)" ]; then
    echo 'Error: you need 7z' >&2
    exit 1
fi
7z a -r config.zip config.yaml rom.nes netplay-rom.nes linux/* macos/* windows/* 2>&1 >/dev/null

echo "Configuration is zipped!"
echo "You can upload it through bashupload.com and use that URL when starting the GitHub Bundle action."
read -r -p "Do you want to upload? [y/N]"
echo
if [[ "$REPLY" =~ ^[Yy]$ ]]; then
    curl bashupload.com -T config.zip
fi