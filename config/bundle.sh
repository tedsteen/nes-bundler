#!/bin/bash
if ! [ -x "$(command -v 7z)" ]; then
    echo 'Error: you need 7z' >&2
    exit 1
fi
7z a -r bundle.zip config.yaml rom.nes netplay-rom.nes linux/* macos/* windows/*
