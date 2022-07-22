#!/bin/bash

# Inspired by https://doc.rust-lang.org/rustc/profile-guided-optimization.html

export PROFILE_FILE_PATH=/tmp/pgo-data

rm -rf $PROFILE_FILE_PATH
mkdir $PROFILE_FILE_PATH

# Build
RUSTFLAGS="-Cprofile-generate=$PROFILE_FILE_PATH -C llvm-args=-vp-counters-per-site=7" cargo build --release

# Run
./target/release/nes-bundler &
export PID1=$!

#./target/release/nes-bundler &
#export PID2=$!

#echo "Collecting profiler data for 3 mins..."
#sleep 180
#echo "Killing $PID1"
#kill -SIGINT $PID1
#echo "Killing $PID2"
#kill -SIGINT $PID2
echo "Waiting for $PID1"
wait $PID1
#echo "Waiting for $PID2"
#wait $PID2

echo "Merging..."
llvm-profdata merge -o ${PROFILE_FILE_PATH}/merged.profdata $PROFILE_FILE_PATH

echo "Making an optimized build using profiler data..."
RUSTFLAGS="-Cprofile-use=${PROFILE_FILE_PATH}/merged.profdata" cargo build --release
