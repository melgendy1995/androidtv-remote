import net from "net";
import { execSync, spawn } from "child_process";

const SERIAL = "192.168.1.162:5555";
const SCID = "12345678";

console.log("1. Setting up TCP listener on port 9876...");
const server = net.createServer((socket) => {
  console.log("🎉 SUCCESS! Connected to scrcpy server socket on office TV!");
  let bytesCount = 0;
  socket.on("data", (chunk) => {
    bytesCount += chunk.length;
    console.log(`Received ${chunk.length} video bytes! Total: ${bytesCount}`);
  });
});

server.listen(9876, "127.0.0.1", () => {
  execSync(`adb -s ${SERIAL} reverse localabstract:scrcpy_${SCID} tcp:9876`);
  console.log("2. Spawning scrcpy-server...");
  
  const cmd = `CLASSPATH=/data/local/tmp/scrcpy-server.jar app_process / com.genymobile.scrcpy.Server 3.3.1 scid=${SCID} log_level=info audio=false control=false`;
  console.log("Running:", cmd);

  const scrcpy = spawn("adb", ["-s", SERIAL, "shell", cmd]);

  scrcpy.stdout.on("data", (d) => console.log(`[scrcpy out] ${d.toString().trim()}`));
  scrcpy.stderr.on("data", (d) => console.log(`[scrcpy err] ${d.toString().trim()}`));

  setTimeout(() => {
    scrcpy.kill();
    server.close();
    process.exit(0);
  }, 6000);
});
