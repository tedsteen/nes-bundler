#!/bin/bash
if ! [ -x "$(command -v 7z)" ]; then
    echo 'Error: you need 7z' >&2
    exit 1
fi
7z a -r config.zip palette.pal config.yaml rom.nes netplay-rom.nes linux/* macos/* windows/* 2>&1 >/dev/null
