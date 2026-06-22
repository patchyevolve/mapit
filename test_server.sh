#!/usr/bin/env bash
# Test script to start the mapit server and check if it's alive

# Clean up any previous instances
pkill -f "mapit open" 2>/dev/null || true
sleep 1

# Start the server in the background
cargo run -- open > server_output.log 2>&1 &
SERVER_PID=$!
echo "Server started with PID $SERVER_PID, waiting 3 seconds..."

# Wait for the server to start
sleep 3

# Try to find which port the server is using from the log
PORT=$(grep -o "Starting mapit server on http://127.0.0.1:[0-9]*" server_output.log | grep -o "[0-9]*$" | head -n1)
echo "Detected port: $PORT"

# Test with curl
if [ -n "$PORT" ]; then
    echo "Testing server with curl http://127.0.0.1:$PORT..."
    curl -v http://127.0.0.1:$PORT
else
    echo "Could not detect port from log!"
    cat server_output.log
fi

# Clean up
echo "Killing server PID $SERVER_PID"
kill $SERVER_PID 2>/dev/null
