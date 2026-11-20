import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import fs from "node:fs";
import path from "node:path";

const require = createRequire(import.meta.url);
const [command, modelDir, wav, outPath] = process.argv.slice(2);

if (!command || !modelDir || !wav || !outPath) {
  console.error("usage: node run.mjs transcribe|diarize MODEL_DIR WAV OUT.json");
  process.exit(2);
}

try {
  if (command === "transcribe") {
    const { Voz } = await import("@desert-ant-labs/voz");
    const ort = require("onnxruntime-node");
    const web = path.join(modelDir, "web");
    let base = pathToFileURL(web.endsWith(path.sep) ? web : web + path.sep).href;
    if (!base.endsWith("/")) base += "/";
    const voz = await Voz.load({ ort, modelBaseUrl: base, cache: false });
    const result = await voz.transcribe(wav);
    fs.writeFileSync(outPath, JSON.stringify(result));
  } else if (command === "diarize") {
    const { Who } = await import("@desert-ant-labs/who/native");
    const candidates = [
      modelDir,
      path.join(modelDir, "litert-int8"),
      path.join(modelDir, "coreml-int8"),
    ];
    let last = "Who did not load";
    for (const directory of candidates) {
      if (!fs.existsSync(directory)) continue;
      try {
        const who = await Who.load({ directory });
        const bytes = await fs.promises.readFile(wav);
        const result = await who.diarize(bytes);
        const turns = result.turns.map((turn) => ({
          speaker: turn.speaker,
          start: turn.start,
          end: turn.end,
        }));
        fs.writeFileSync(outPath, JSON.stringify({ turns }));
        who.dispose?.();
        last = "";
        break;
      } catch (error) {
        last = error && error.stack ? error.stack : String(error);
      }
    }
    if (last) throw new Error(last);
  } else {
    throw new Error(`unknown command ${command}`);
  }
} catch (error) {
  console.error(error && error.stack ? error.stack : String(error));
  process.exit(1);
}
