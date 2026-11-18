/* tslint:disable */
/* eslint-disable */

/**
 * `ElectrodeHead3D`: the scalp, the electrodes on it, orbiting by itself.
 */
export class HeadViewer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Open the GPU on `canvas`. The canvas is left transparent.
     */
    static create(canvas: HTMLCanvasElement, dark: boolean): Promise<HeadViewer>;
    /**
     * Advance `dt` seconds and draw.
     */
    frame(dt: number): void;
    /**
     * The index of the electrode under NDC `(x, y)`, or -1.
     */
    hitTest(x: number, y: number): number;
    /**
     * The scalp model, `head.glb`'s bytes.
     */
    loadModel(glb: Uint8Array | number[]): void;
    pointerDown(button: number, x: number, y: number): void;
    pointerMove(x: number, y: number): void;
    pointerUp(button: number): void;
    /**
     * The canvas's CSS size and the device pixel ratio.
     */
    resize(css_width: number, css_height: number, dpr: number): void;
    select(name?: string | null): void;
    setElectrodeTransform(px: number, py: number, pz: number, sx: number, sy: number, sz: number): void;
    /**
     * Electrodes: `names[i]`, MNE position `positions[3i..3i+3]` (mm),
     * colour `colors[i]` (`0xRRGGBB`), and whether to draw it larger.
     */
    setElectrodes(names: string[], positions: Float32Array | number[], colors: Uint32Array | number[], emphasised: Uint8Array | number[]): void;
    setTheme(dark: boolean): void;
    wheel(delta_y: number, x: number, y: number): void;
    /**
     * `"webgpu"` or `"webgl2"`.
     */
    readonly backend: string;
}

/**
 * A scene the page builds object by object, by handle: what
 * `InteractiveGraph3D` and `UmapViewer3D` used three.js for. Colours are
 * `0xRRGGBB` (sRGB, decoded as three.js does); per-vertex colours are linear.
 * Handle `0` is the scene root.
 */
export class SceneViewer {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    addAmbientLight(color: number, intensity: number): number;
    /**
     * Shining from `(x, y, z)` towards the origin.
     */
    addDirectionalLight(color: number, intensity: number, x: number, y: number, z: number): number;
    addGrid(parent: number, size: number, divisions: number, center: number, lines: number, opacity: number): number;
    addGroup(parent: number): number;
    /**
     * Lines through `positions`: a strip when `strip`, else segment pairs.
     * Dashed when `dash_size > 0`.
     */
    addLines(parent: number, positions: Float32Array | number[], colors: Float32Array | number[] | null | undefined, strip: boolean, dash_size: number, gap_size: number, color: number, opacity: number, transparent: boolean, depth_test: boolean): number;
    /**
     * A mesh. `shape`: 0 sphere (a radius, b width segments, c height
     * segments), 1 octahedron (a radius), 2 tetrahedron (a radius), 3 ring
     * (a inner, b outer, c segments), 4 cylinder (a radius, b height, c
     * segments). `material`: 0 basic, 1 phong. `depth_write`: -1 auto.
     */
    addMesh(parent: number, shape: number, a: number, b: number, c: number, material: number, color: number, opacity: number, transparent: boolean, depth_test: boolean, depth_write: number, side: number, shininess: number, emissive: number, emissive_intensity: number): number;
    /**
     * Points at `positions` (xyz…), coloured per point by `colors` (linear
     * rgb…) when given, else by `color`.
     */
    addPoints(parent: number, positions: Float32Array | number[], colors: Float32Array | number[] | null | undefined, size: number, size_attenuation: boolean, color: number, opacity: number, transparent: boolean, depth_write: number): number;
    /**
     * A camera-facing picture, `width × height` straight-alpha RGBA bytes
     * (a 2D canvas's `getImageData`), `sx × sy` world units.
     */
    addSprite(parent: number, width: number, height: number, rgba: Uint8Array | number[], sx: number, sy: number, opacity: number, depth_test: boolean): number;
    /**
     * Turn `h`'s +Y along the direction (a cylinder between two points).
     */
    alignY(h: number, x: number, y: number, z: number): void;
    cameraPosition(): Float32Array;
    /**
     * Remove everything under `h`.
     */
    clear(h: number): void;
    /**
     * `fov` in degrees. The scene starts with a transparent background.
     */
    static create(canvas: HTMLCanvasElement, fov: number, near: number, far: number): Promise<SceneViewer>;
    /**
     * Advance the controls `dt` seconds and draw.
     */
    frame(dt: number): void;
    /**
     * Turn `h`'s +Z towards the world point.
     */
    lookAt(h: number, x: number, y: number, z: number): void;
    /**
     * The nearest object under NDC `(x, y)` among `among` and their
     * children, as `[handle, index, distance]`, or empty. Points and lines
     * count within `threshold` world units.
     */
    pick(x: number, y: number, among: Uint32Array | number[], threshold: number): Float32Array;
    pointerDown(button: number, x: number, y: number): void;
    pointerMove(x: number, y: number): void;
    pointerUp(button: number): void;
    /**
     * World → NDC.
     */
    project(x: number, y: number, z: number): Float32Array;
    remove(h: number): void;
    /**
     * Draw without advancing (before reading the canvas back).
     */
    render(): void;
    resize(css_width: number, css_height: number, dpr: number): void;
    /**
     * Replace `position` or `color` (xyz… / rgb…).
     */
    setAttribute(h: number, name: string, data: Float32Array | number[]): void;
    setAutoRotate(on: boolean): void;
    /**
     * `undefined` for a transparent canvas.
     */
    setBackground(color?: number | null): void;
    /**
     * Keep `h` facing the camera (a ring around a point, say).
     */
    setBillboard(h: number, on: boolean): void;
    setCameraPosition(x: number, y: number, z: number): void;
    setColorRgb(h: number, r: number, g: number, b: number): void;
    setColor(h: number, color: number): void;
    setEmissive(h: number, color: number, intensity: number): void;
    setOpacity(h: number, opacity: number, transparent?: boolean | null): void;
    /**
     * three.js `OrbitControls` settings; buttons are `MOUSE` numbers
     * (0 rotate, 1 dolly, 2 pan) for left, middle, right.
     */
    setOrbit(enable_damping: boolean, damping_factor: number, auto_rotate: boolean, auto_rotate_speed: number, min_distance: number, max_distance: number, left: number, middle: number, right: number): void;
    setPointSize(h: number, size: number): void;
    setPosition(h: number, x: number, y: number, z: number): void;
    setRenderOrder(h: number, order: number): void;
    setScale(h: number, x: number, y: number, z: number): void;
    setSpriteImage(h: number, width: number, height: number, rgba: Uint8Array | number[]): void;
    /**
     * The point the camera looks at and orbits.
     */
    setTarget(x: number, y: number, z: number): void;
    setVisible(h: number, visible: boolean): void;
    target(): Float32Array;
    wheel(delta_y: number, x: number, y: number): void;
    readonly backend: string;
}

export function start(): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_headviewer_free: (a: number, b: number) => void;
    readonly __wbg_sceneviewer_free: (a: number, b: number) => void;
    readonly headviewer_backend: (a: number) => [number, number];
    readonly headviewer_create: (a: any, b: number) => any;
    readonly headviewer_frame: (a: number, b: number) => void;
    readonly headviewer_hitTest: (a: number, b: number, c: number) => number;
    readonly headviewer_loadModel: (a: number, b: number, c: number) => [number, number];
    readonly headviewer_pointerDown: (a: number, b: number, c: number, d: number) => void;
    readonly headviewer_pointerMove: (a: number, b: number, c: number) => void;
    readonly headviewer_pointerUp: (a: number, b: number) => void;
    readonly headviewer_resize: (a: number, b: number, c: number, d: number) => void;
    readonly headviewer_select: (a: number, b: number, c: number) => void;
    readonly headviewer_setElectrodeTransform: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly headviewer_setElectrodes: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => [number, number];
    readonly headviewer_setTheme: (a: number, b: number) => void;
    readonly headviewer_wheel: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_addAmbientLight: (a: number, b: number, c: number) => number;
    readonly sceneviewer_addDirectionalLight: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
    readonly sceneviewer_addGrid: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => number;
    readonly sceneviewer_addGroup: (a: number, b: number) => number;
    readonly sceneviewer_addLines: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number) => number;
    readonly sceneviewer_addMesh: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number, m: number, n: number, o: number, p: number) => [number, number, number];
    readonly sceneviewer_addPoints: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number, k: number, l: number) => number;
    readonly sceneviewer_addSprite: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => number;
    readonly sceneviewer_alignY: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sceneviewer_backend: (a: number) => [number, number];
    readonly sceneviewer_cameraPosition: (a: number) => [number, number];
    readonly sceneviewer_clear: (a: number, b: number) => void;
    readonly sceneviewer_create: (a: any, b: number, c: number, d: number) => any;
    readonly sceneviewer_frame: (a: number, b: number) => void;
    readonly sceneviewer_lookAt: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sceneviewer_pick: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly sceneviewer_pointerDown: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_pointerMove: (a: number, b: number, c: number) => void;
    readonly sceneviewer_pointerUp: (a: number, b: number) => void;
    readonly sceneviewer_project: (a: number, b: number, c: number, d: number) => [number, number];
    readonly sceneviewer_remove: (a: number, b: number) => void;
    readonly sceneviewer_render: (a: number) => void;
    readonly sceneviewer_resize: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_setAttribute: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly sceneviewer_setAutoRotate: (a: number, b: number) => void;
    readonly sceneviewer_setBackground: (a: number, b: number) => void;
    readonly sceneviewer_setBillboard: (a: number, b: number, c: number) => void;
    readonly sceneviewer_setCameraPosition: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_setColor: (a: number, b: number, c: number) => void;
    readonly sceneviewer_setColorRgb: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sceneviewer_setEmissive: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_setOpacity: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_setOrbit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => void;
    readonly sceneviewer_setPointSize: (a: number, b: number, c: number) => void;
    readonly sceneviewer_setPosition: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sceneviewer_setRenderOrder: (a: number, b: number, c: number) => void;
    readonly sceneviewer_setScale: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly sceneviewer_setSpriteImage: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly sceneviewer_setTarget: (a: number, b: number, c: number, d: number) => void;
    readonly sceneviewer_setVisible: (a: number, b: number, c: number) => void;
    readonly sceneviewer_target: (a: number) => [number, number];
    readonly sceneviewer_wheel: (a: number, b: number, c: number, d: number) => void;
    readonly start: () => void;
    readonly wasm_bindgen_ef0584a8d5b09822___convert__closures_____invoke___js_sys_ea1fb2d410123938___Function_fn_wasm_bindgen_ef0584a8d5b09822___JsValue_____wasm_bindgen_ef0584a8d5b09822___sys__Undefined___js_sys_ea1fb2d410123938___Function_fn_wasm_bindgen_ef0584a8d5b09822___JsValue_____wasm_bindgen_ef0584a8d5b09822___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_ef0584a8d5b09822___convert__closures_____invoke___wasm_bindgen_ef0584a8d5b09822___JsValue__core_d941ad8722425d2e___result__Result_____wasm_bindgen_ef0584a8d5b09822___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_ef0584a8d5b09822___convert__closures_____invoke___wasm_bindgen_ef0584a8d5b09822___sys__JsNullable_wgpu_5dd5fb5d0cfc9c24___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_d941ad8722425d2e___result__Result_____wasm_bindgen_ef0584a8d5b09822___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_ef0584a8d5b09822___convert__closures_____invoke___wasm_bindgen_ef0584a8d5b09822___sys__JsNullable_wgpu_5dd5fb5d0cfc9c24___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_d941ad8722425d2e___result__Result_____wasm_bindgen_ef0584a8d5b09822___JsError___true__60: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_ef0584a8d5b09822___convert__closures_____invoke___wasm_bindgen_ef0584a8d5b09822___sys__JsNullable_wgpu_5dd5fb5d0cfc9c24___backend__webgpu__webgpu_sys__gen_GpuError__GpuError___core_d941ad8722425d2e___result__Result_____wasm_bindgen_ef0584a8d5b09822___JsError___true__61: (a: number, b: number, c: any) => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __externref_drop_slice: (a: number, b: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
