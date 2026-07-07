import init, { Renderer } from "../pkg/realtime_wasm_raytracer.js";

const canvas = document.getElementById("view");
const ctx = canvas.getContext("2d", { alpha: false });
const fpsEl = document.getElementById("fps");
const resolution = document.getElementById("resolution");
const samples = document.getElementById("samples");
const depth = document.getElementById("depth");
const samplesValue = document.getElementById("samplesValue");
const depthValue = document.getElementById("depthValue");
const extraSpheres = document.getElementById("extraSpheres");

let renderer;
let imageData;
let yaw = 0.42;
let pitch = 0.22;
let distance = 4.35;
let dragging = false;
let lastX = 0;
let lastY = 0;
let lastTime = performance.now();
let fpsSmooth = 0;

function resizeRenderer() {
  const [w, h] = resolution.value.split("x").map(Number);
  canvas.width = w;
  canvas.height = h;
  canvas.style.aspectRatio = `${w} / ${h}`;
  imageData = ctx.createImageData(w, h);
  if (renderer) renderer.resize(w, h);
}

function updateLabels() {
  samplesValue.textContent = samples.value;
  depthValue.textContent = depth.value;
}

canvas.addEventListener("pointerdown", (event) => {
  dragging = true;
  lastX = event.clientX;
  lastY = event.clientY;
  canvas.setPointerCapture(event.pointerId);
});

canvas.addEventListener("pointermove", (event) => {
  if (!dragging) return;
  const dx = event.clientX - lastX;
  const dy = event.clientY - lastY;
  yaw += dx * 0.008;
  pitch = Math.max(-1.15, Math.min(1.15, pitch + dy * 0.006));
  lastX = event.clientX;
  lastY = event.clientY;
});

canvas.addEventListener("pointerup", () => {
  dragging = false;
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    distance = Math.max(2.3, Math.min(8.0, distance + event.deltaY * 0.004));
  },
  { passive: false }
);

resolution.addEventListener("change", resizeRenderer);
samples.addEventListener("input", updateLabels);
depth.addEventListener("input", updateLabels);

function frame(now) {
  const dt = Math.max(1, now - lastTime);
  lastTime = now;
  const instant = 1000 / dt;
  fpsSmooth = fpsSmooth ? fpsSmooth * 0.9 + instant * 0.1 : instant;
  fpsEl.textContent = fpsSmooth.toFixed(0);

  const pixels = renderer.render(
    now * 0.001,
    yaw,
    pitch,
    distance,
    Number(samples.value),
    Number(depth.value),
    extraSpheres.checked
  );
  imageData.data.set(pixels);
  ctx.putImageData(imageData, 0, 0);
  requestAnimationFrame(frame);
}

async function main() {
  await init();
  resizeRenderer();
  updateLabels();
  renderer = new Renderer(canvas.width, canvas.height);
  requestAnimationFrame(frame);
}

main().catch((error) => {
  document.body.innerHTML = `<pre>${error.stack || error}</pre>`;
});
