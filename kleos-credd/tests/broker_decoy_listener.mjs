import { appendFileSync, writeFileSync } from "node:fs";
import net from "node:net";

const [portFile, hitsFile] = process.argv.slice(2);
if (!portFile || !hitsFile) {
  process.exit(78);
}

const server = net.createServer((socket) => {
  appendFileSync(hitsFile, "connection\n");
  socket.destroy();
});

server.listen(0, "127.0.0.1", () => {
  writeFileSync(portFile, String(server.address().port));
});

process.on("SIGTERM", () => server.close(() => process.exit(0)));
process.on("SIGINT", () => server.close(() => process.exit(0)));
