#!/usr/bin/env bash
# Decrypts the bundle configuration uploaded by `prepare_config` (config.zip.enc), and puts
# the filebin bin it's delivered through in $FILE_BIN for the following steps.
# With --unpack, also unpacks it into config/.
#
# Needs CONFIG_ZIP_PASSWORD in the environment.
set -euo pipefail

openssl enc -d -aes-256-cbc -pbkdf2 \
  -in config.zip.enc -out config.zip \
  -pass env:CONFIG_ZIP_PASSWORD

FILE_BIN=$(sha224sum config.zip | cut -d' ' -f1)
echo "::add-mask::$FILE_BIN"
echo "FILE_BIN=$FILE_BIN" >> "$GITHUB_ENV"

if [[ "${1:-}" == "--unpack" ]]; then
  unzip -o -q config.zip -d config
fi
