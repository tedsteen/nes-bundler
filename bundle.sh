#!/bin/bash
# Where to find the config
CONF_DIR=${1:-config}
NES_BUNDLER_BINARY=${2:-target/release/nes-bundler}
# What to name the final binary
NAME=${3:-out}

function cleanup {
    rm config.zip
}

# Pack the config
zip -jT config.zip $CONF_DIR/config.yaml $CONF_DIR/rom.nes &> /dev/null
trap cleanup EXIT

# Merge with binary
cat $NES_BUNDLER_BINARY config.zip > $NAME

# Adjust self-extracting exe
zip -A $NAME &> /dev/null

# Make it executable
chmod +x $NAME

echo "Finished!"
echo "./$NAME"