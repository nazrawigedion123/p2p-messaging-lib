import asyncio
import websockets
import json
import sys

async def test_client(name, room):
    uri = "ws://localhost:8000/ws"
    async with websockets.connect(uri) as websocket:
        print(f"Connected as {name}")
        
        # Login
        login_msg = {
            "type": "Login",
            "payload": { "username": name }
        }
        await websocket.send(json.dumps(login_msg))
        print(f"Sent Login: {login_msg}")
        
        # Join Room
        join_msg = {
            "type": "Join",
            "payload": { "room": room }
        }
        await websocket.send(json.dumps(join_msg))
        print(f"Sent Join: {join_msg}")
        
        # Listen for messages
        while True:
            try:
                message = await asyncio.wait_for(websocket.recv(), timeout=5.0)
                print(f"Received: {message}")
            except asyncio.TimeoutError:
                print("Timeout waiting for message")
                break

if __name__ == "__main__":
    name = sys.argv[1] if len(sys.argv) > 1 else "TestBot"
    room = sys.argv[2] if len(sys.argv) > 2 else "testroom"
    asyncio.run(test_client(name, room))
