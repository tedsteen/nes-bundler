#!/bin/bash

# Inspired by https://doc.rust-lang.org/rustc/profile-guided-optimization.html

export ROM_FILE=${1:-demo.nes}
TARGET=${2:-x86_64-apple-darwin}

echo "Building optimized binary for target '$TARGET' using '$ROM_FILE' to profile"

rm -rf /tmp/pgo-data/

# STEP 1: Build the instrumented binaries
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" cargo rustc -- -C link-args=-Wl,-stack_size,0x1000000 --target=$TARGET

# STEP 2: Run the binary to generate profiler data
./target/$TARGET/release/rusticnes-test &
PID=$!
./target/$TARGET/release/rusticnes-test &
PID2=$!

# Collect profiler data for 3 mins...
#sleep 180
#kill -SIGINT $PID
#kill -SIGINT $PID2
wait $PID
wait $PID2

# STEP 3: Merge and post-process all the `.profraw` files in /tmp/pgo-data
xcrun llvm-profdata merge -o /tmp/pgo-data/merged.profdata /tmp/pgo-data

# STEP 4: use the `.profdata` to build the optimized binary
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data/merged.profdata" cargo build --release --target=$TARGET

echo "Optimized binary over here: ./target/$TARGET/release/rusticnes-test"
