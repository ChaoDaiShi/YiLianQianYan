import { useLayoutEffect, useRef } from "react";
import { memorySpaceReveal, type MemorySpaceScene, type SpaceCamera } from "./memorySpaceGeometry";
import type { SpaceEdge } from "./MemorySpaceView";
import { buildMemorySolidGeometry, MEMORY_MESH_STRIDE } from "./memorySolidGeometry";

type Props = {
  scene: MemorySpaceScene;
  camera: SpaceCamera;
  size: { width: number; height: number };
  edges: SpaceEdge[];
  progress: number;
  focusedId: string | null;
  onStatus: (ready: boolean) => void;
};

const vertexSource = `
  attribute vec3 a_position;
  attribute vec3 a_normal;
  attribute vec3 a_color;
  attribute float a_anchorZ;
  uniform vec3 u_center;
  uniform vec3 u_right;
  uniform vec3 u_down;
  uniform vec3 u_near;
  uniform vec2 u_zoom;
  uniform vec2 u_pan;
  uniform float u_focalLength;
  uniform float u_perspective;
  uniform float u_growth;
  varying mediump vec3 v_normal;
  varying mediump vec3 v_color;

  void main() {
    vec3 point = a_position - u_center;
    point.z += a_anchorZ * (u_growth - 1.0);
    float depth = dot(point, u_near);
    float w = 1.0 - depth * u_perspective / u_focalLength;
    float screenX = dot(point, u_right) * u_zoom.x + u_pan.x * w;
    float screenY = dot(point, u_down) * u_zoom.y + u_pan.y * w;
    gl_Position = vec4(screenX, -screenY, -depth / (u_focalLength * 2.0), w);
    v_normal = vec3(dot(a_normal, u_right), -dot(a_normal, u_down), dot(a_normal, u_near));
    v_color = a_color;
  }`;

const fragmentSource = `
  precision mediump float;
  uniform float u_alpha;
  varying mediump vec3 v_normal;
  varying mediump vec3 v_color;

  void main() {
    vec3 normal = normalize(v_normal);
    vec3 key = normalize(vec3(-0.48, 0.64, 0.69));
    vec3 rimLight = normalize(vec3(0.62, -0.65, 0.31));
    float shade = max(0.0, dot(normal, key));
    float reflected = max(0.0, dot(normal, rimLight));
    float highlight = pow(max(0.0, dot(normal, normalize(key + vec3(0.0, 0.0, 1.0)))), 28.0);
    vec3 lit = v_color * (0.48 + shade * 0.64 + reflected * 0.19);
    lit += vec3(0.25, 0.23, 0.28) * highlight;
    gl_FragColor = vec4(lit, u_alpha);
  }`;

function makeProgram(gl: WebGLRenderingContext) {
  const vertex = gl.createShader(gl.VERTEX_SHADER);
  const fragment = gl.createShader(gl.FRAGMENT_SHADER);
  const program = gl.createProgram();
  if (!vertex || !fragment || !program) {
    if (vertex) gl.deleteShader(vertex);
    if (fragment) gl.deleteShader(fragment);
    if (program) gl.deleteProgram(program);
    return null;
  }
  gl.shaderSource(vertex, vertexSource);
  gl.compileShader(vertex);
  gl.shaderSource(fragment, fragmentSource);
  gl.compileShader(fragment);
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (gl.getProgramParameter(program, gl.LINK_STATUS)) return program;
  gl.deleteProgram(program);
  return null;
}

function makeRenderer(gl: WebGLRenderingContext, canvas: HTMLCanvasElement) {
  const program = makeProgram(gl);
  const buffers = { base: gl.createBuffer(), upper: gl.createBuffer(), projection: gl.createBuffer() };
  const layers = ["base", "upper", "projection"] as const;
  if (!program || layers.some((layer) => !buffers[layer])) {
    if (program) gl.deleteProgram(program);
    for (const layer of layers) gl.deleteBuffer(buffers[layer]);
    return null;
  }
  const positionLocation = gl.getAttribLocation(program, "a_position");
  const normalLocation = gl.getAttribLocation(program, "a_normal");
  const colorLocation = gl.getAttribLocation(program, "a_color");
  const anchorLocation = gl.getAttribLocation(program, "a_anchorZ");
  const centerLocation = gl.getUniformLocation(program, "u_center");
  const rightLocation = gl.getUniformLocation(program, "u_right");
  const downLocation = gl.getUniformLocation(program, "u_down");
  const nearLocation = gl.getUniformLocation(program, "u_near");
  const zoomLocation = gl.getUniformLocation(program, "u_zoom");
  const panLocation = gl.getUniformLocation(program, "u_pan");
  const focalLengthLocation = gl.getUniformLocation(program, "u_focalLength");
  const perspectiveLocation = gl.getUniformLocation(program, "u_perspective");
  const alphaLocation = gl.getUniformLocation(program, "u_alpha");
  const growthLocation = gl.getUniformLocation(program, "u_growth");
  let previous: Pick<Props, "scene" | "edges" | "focusedId"> & { dark: boolean } | undefined;
  const sizes = { base: 0, upper: 0, projection: 0 };
  let frame = 0;

  function batch(layer: typeof layers[number], alpha: number, growth: number) {
    const count = sizes[layer];
    if (!count || alpha <= 0.005) return 0;
    gl.uniform1f(alphaLocation, alpha);
    gl.uniform1f(growthLocation, growth);
    gl.bindBuffer(gl.ARRAY_BUFFER, buffers[layer]);
    const stride = MEMORY_MESH_STRIDE * Float32Array.BYTES_PER_ELEMENT;
    gl.vertexAttribPointer(positionLocation, 3, gl.FLOAT, false, stride, 0);
    gl.vertexAttribPointer(normalLocation, 3, gl.FLOAT, false, stride, 3 * Float32Array.BYTES_PER_ELEMENT);
    gl.vertexAttribPointer(colorLocation, 3, gl.FLOAT, false, stride, 6 * Float32Array.BYTES_PER_ELEMENT);
    gl.vertexAttribPointer(anchorLocation, 1, gl.FLOAT, false, stride, 9 * Float32Array.BYTES_PER_ELEMENT);
    gl.drawArrays(gl.TRIANGLES, 0, count);
    return count;
  }

  gl.useProgram(program);
  gl.enableVertexAttribArray(positionLocation);
  gl.enableVertexAttribArray(normalLocation);
  gl.enableVertexAttribArray(colorLocation);
  gl.enableVertexAttribArray(anchorLocation);
  gl.clearColor(0, 0, 0, 0);
  gl.enable(gl.DEPTH_TEST);
  gl.depthFunc(gl.LEQUAL);
  gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA, gl.ONE, gl.ONE_MINUS_SRC_ALPHA);

  return {
    draw({ scene, camera, size, edges, progress, focusedId }: Props, dark: boolean) {
      const geometryChanged = !previous || previous.scene !== scene || previous.edges !== edges ||
        previous.focusedId !== focusedId || previous.dark !== dark;
      if (geometryChanged) {
        const geometry = buildMemorySolidGeometry(scene, edges, focusedId, dark);
        for (const layer of layers) {
          gl.bindBuffer(gl.ARRAY_BUFFER, buffers[layer]);
          gl.bufferData(gl.ARRAY_BUFFER, geometry[layer], gl.STATIC_DRAW);
          sizes[layer] = geometry[layer].length / MEMORY_MESH_STRIDE;
        }
        canvas.dataset.meshSpheres = String(geometry.spheres);
        canvas.dataset.meshTubes = String(geometry.tubes);
        previous = { scene, edges, focusedId, dark };
      }

      const pixelRatio = Math.min(2, window.devicePixelRatio || 1);
      const pixelWidth = Math.max(1, Math.round(size.width * pixelRatio));
      const pixelHeight = Math.max(1, Math.round(size.height * pixelRatio));
      if (canvas.width !== pixelWidth) canvas.width = pixelWidth;
      if (canvas.height !== pixelHeight) canvas.height = pixelHeight;
      gl.viewport(0, 0, pixelWidth, pixelHeight);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

      const sinAzimuth = Math.sin(camera.azimuth), cosAzimuth = Math.cos(camera.azimuth);
      const sinElevation = Math.sin(camera.elevation), cosElevation = Math.cos(camera.elevation);
      gl.uniform3f(centerLocation, scene.center.x, scene.center.y, scene.center.z);
      gl.uniform3f(rightLocation, cosAzimuth, -sinAzimuth, 0);
      gl.uniform3f(downLocation, sinAzimuth * cosElevation, cosAzimuth * cosElevation, -sinElevation);
      gl.uniform3f(nearLocation, sinAzimuth * sinElevation, cosAzimuth * sinElevation, cosElevation);
      gl.uniform2f(zoomLocation, camera.zoom * 2 / size.width, camera.zoom * 2 / size.height);
      gl.uniform2f(panLocation, camera.panX * 2 / size.width, camera.panY * 2 / size.height);
      gl.uniform1f(focalLengthLocation, scene.focalLength);
      gl.uniform1f(perspectiveLocation, camera.perspective);

      const reveal = memorySpaceReveal(progress);
      gl.enable(gl.BLEND);
      const vertices = batch("projection", reveal.projection, 0) +
        batch("base", reveal.base, 1) + batch("upper", reveal.upper, reveal.growth);
      gl.disable(gl.BLEND);
      canvas.dataset.renderer = "webgl";
      canvas.dataset.meshFrame = String(++frame);
      canvas.dataset.meshTriangles = String(vertices / 3);
    },
    dispose() {
      for (const layer of layers) gl.deleteBuffer(buffers[layer]);
      gl.deleteProgram(program);
    },
  };
}

export default function MemorySpaceMeshes(props: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const latestProps = useRef(props);
  const requestDraw = useRef<(() => void) | null>(null);
  latestProps.current = props;

  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const gl = canvas.getContext("webgl", { alpha: true, antialias: true, depth: true });
    if (!gl) {
      latestProps.current.onStatus(false);
      return;
    }
    let renderer = makeRenderer(gl, canvas);
    let dark = document.documentElement.dataset.colorScheme === "dark";
    const paint = () => renderer?.draw(latestProps.current, dark);
    requestDraw.current = paint;
    const report = () => {
      if (renderer) paint();
      latestProps.current.onStatus(Boolean(renderer));
    };
    report();

    const onContextLost = (event: Event) => {
      event.preventDefault();
      renderer = null;
      delete canvas.dataset.renderer;
      latestProps.current.onStatus(false);
    };
    const onContextRestore = () => {
      renderer = makeRenderer(gl, canvas);
      report();
    };
    canvas.addEventListener("webglcontextlost", onContextLost);
    canvas.addEventListener("webglcontextrestored", onContextRestore);
    const theme = new MutationObserver(() => {
      dark = document.documentElement.dataset.colorScheme === "dark";
      paint();
    });
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ["data-color-scheme"] });
    return () => {
      theme.disconnect();
      requestDraw.current = null;
      canvas.removeEventListener("webglcontextlost", onContextLost);
      canvas.removeEventListener("webglcontextrestored", onContextRestore);
      renderer?.dispose();
    };
  }, []);

  useLayoutEffect(() => { requestDraw.current?.(); });

  return <canvas ref={canvasRef} data-testid="memory-space-webgl" aria-hidden="true"
    style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }} />;
}
