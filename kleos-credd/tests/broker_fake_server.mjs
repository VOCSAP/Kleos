import { appendFileSync, existsSync, unlinkSync } from "node:fs";
import net from "node:net";

const socketPath = process.argv[2];
const requestLog = process.argv[3];
if (!socketPath) {
  process.exit(78);
}
if (existsSync(socketPath)) {
  process.exit(78);
}

const connections = new Set();
const server = net.createServer((socket) => {
  connections.add(socket);
  socket.on("close", () => connections.delete(socket));
  let buffer = Buffer.alloc(0);
  let handled = false;

  socket.on("data", (chunk) => {
    if (handled) {
      return;
    }
    buffer = Buffer.concat([buffer, chunk]);
    const headerEnd = buffer.indexOf("\r\n\r\n");
    if (headerEnd < 0) {
      return;
    }
    const headers = buffer.subarray(0, headerEnd).toString("utf8");
    const lengthMatch = /\r\ncontent-length:\s*(\d+)/i.exec(`\r\n${headers}`);
    const contentLength = lengthMatch ? Number(lengthMatch[1]) : 0;
    const bodyStart = headerEnd + 4;
    if (buffer.length < bodyStart + contentLength) {
      return;
    }
    handled = true;
    respond(socket, headers, buffer.subarray(bodyStart, bodyStart + contentLength));
  });
});

function send(socket, status, body) {
  const payload = Buffer.from(body);
  socket.write(
    `HTTP/1.1 ${status}\r\nContent-Type: application/json\r\nContent-Length: ${payload.length}\r\nConnection: close\r\n\r\n`,
  );
  socket.end(payload);
}

function error(socket, status, code) {
  send(socket, status, JSON.stringify({ protocol: 1, error: { code } }));
}

function respond(socket, headers, body) {
  const requestLine = headers.split("\r\n", 1)[0];
  if (requestLog) {
    appendFileSync(requestLog, `${requestLine}\n`);
  }
  if (requestLine.includes("/v1/entries")) {
    send(socket, "200 OK", JSON.stringify({
      protocol: 1,
      entries: [{ service: "success", key: "api-key", type: "api-key" }],
    }));
    return;
  }

  let service;
  try {
    service = JSON.parse(body.toString("utf8")).service;
  } catch {
    error(socket, "400 Bad Request", "INVALID_REQUEST");
    return;
  }

  if (typeof service === "string" && service.startsWith("echo-")) {
    send(socket, "200 OK", JSON.stringify({
      protocol: 1,
      value: `ECHO_VALUE_${service.slice(5)}`,
    }));
    return;
  }

  switch (service) {
    case "invalid-json":
      send(socket, "200 OK", "not-json");
      return;
    case "wrong-protocol":
      send(socket, "200 OK", JSON.stringify({ protocol: 2, value: "ignored" }));
      return;
    case "oversize":
      socket.end("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 524289\r\nConnection: close\r\n\r\n");
      return;
    case "timeout":
      setTimeout(() => {
        if (!socket.destroyed) {
          send(socket, "200 OK", JSON.stringify({ protocol: 1, value: "late" }));
        }
      }, 6000);
      return;
    case "redirect":
      socket.end("HTTP/1.1 302 Found\r\nLocation: http://example.invalid/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
      return;
    case "not-found":
      error(socket, "404 Not Found", "NOT_FOUND");
      return;
    case "internal":
      error(socket, "500 Internal Server Error", "INTERNAL");
      return;
    default:
      send(socket, "200 OK", JSON.stringify({
        protocol: 1,
        value: "FAKE_API_VALUE_FOR_CONTRACT_TESTS",
      }));
  }
}

server.listen(socketPath);

function stop() {
  for (const connection of connections) {
    connection.destroy();
  }
  server.close(() => {
    if (existsSync(socketPath)) {
      unlinkSync(socketPath);
    }
    process.exit(0);
  });
}

process.on("SIGINT", stop);
process.on("SIGTERM", stop);
