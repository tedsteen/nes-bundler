#!/bin/bash
# Where to find the config
CONF_DIR=${1:-config}
# What to name the final binary
NAME=${2:-out}

# Pack the config
zip -jT $CONF_DIR/config.zip $CONF_DIR/config.yaml $CONF_DIR/rom.nes
# Merge with binary
cat target/release/nes-bundler $CONF_DIR/config.zip > $NAME
# Adjust self-extracting exe
zip -A $NAME
# Make it executable
chmod +x $NAME
