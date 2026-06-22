#!/bin/bash
cd /home/daksh/working/codeWorks/mapit

# Kill any existing server
lsof -ti :9090 | xargs -r kill -9 2>/dev/null
sleep 0.5

# Start server in background
cargo run -- open > server.log 2>&1 &
SERVER_PID=$!
echo "Server started with PID $SERVER_PID"

# Wait for server to start
sleep 2

# Call /api/annotate
echo "Calling /api/annotate..."
curl -v -X POST http://127.0.0.1:9090/api/annotate -H "Content-Type: application/json" -d '{"all":false,"force":false}'

# Sleep a bit to let server process
sleep 1

# Show server logs
echo -e "\nServer logs:"
cat server.log

# Kill server
kill -9 $SERVER_PID 2>/dev/null
echo "Test complete"
