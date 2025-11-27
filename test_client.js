const WebSocket = require('ws');

const name = process.argv[2] || "TestBot";
const room = process.argv[3] || "testroom";
const url = "ws://localhost:8000/ws";

const ws = new WebSocket(url);

ws.on('open', function open() {
    console.log(`Connected as ${name}`);

    const loginMsg = {
        type: "Login",
        payload: { username: name }
    };
    ws.send(JSON.stringify(loginMsg));
    console.log(`Sent Login: ${JSON.stringify(loginMsg)}`);

    const joinMsg = {
        type: "Join",
        payload: { room: room }
    };
    ws.send(JSON.stringify(joinMsg));
    console.log(`Sent Join: ${JSON.stringify(joinMsg)}`);
});

ws.on('message', function incoming(data) {
    console.log(`Received: ${data}`);
});

ws.on('error', function error(err) {
    console.error(`Error: ${err}`);
});

setTimeout(() => {
    console.log("Timeout reached, closing.");
    ws.close();
}, 5000);
