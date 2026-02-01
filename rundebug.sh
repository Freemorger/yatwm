#!/bin/bash 

# builds yatwm and runs it in Xephyr if it was built successful

cleanup() {
    echo "Cleaning up..."
    killall Xephyr 2>/dev/null
    cat ~/.local/state/yatwm.log
    exit 0
}

# set up trap for SIGINT (Ctrl+C)
trap cleanup SIGINT 

# build and run
cargo build || exit 1
Xephyr -ac -screen 800x600 -br -reset :2 > /tmp/xephyr.log &
XEPHYR_PID=$!

sleep 1

DISPLAY=:2 ./target/debug/yatwm
