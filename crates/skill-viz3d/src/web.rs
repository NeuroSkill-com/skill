//! The views as JavaScript classes, drawing into a `<canvas>`.
//!
//! WebGPU where the web view has it, WebGL2 otherwise (WebKitGTK on Linux,
//! older macOS). The canvas is configured with a non-sRGB format whenever one
//! is offered, so threers encodes colour in the shader and blends the encoded
//! values — what three.js does on its 8-bit canvas, and what the views were
//! tuned against.

use std::sync::Arc;

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use threers::math::{Color, Vector3};
use threers::{Renderer, ToneMapping};

use crate::head::{Electrode, HeadView};
use crate::view::{Drag, Look, MeshMaterial, OrbitOptions, Shape, View};

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

fn err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// A canvas and the GPU drawing into it.
struct Canvas {
    canvas: HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    device: Arc<wgpu::Device>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    backend: &'static str,
}

impl Canvas {
    async fn open(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let (width, height) = (canvas.width().max(1), canvas.height().max(1));
        let target = || wgpu::SurfaceTarget::Canvas(canvas.clone());
        let options = |surface| wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: surface,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        };

        // WebGPU first, asked without a surface: a canvas gives out one kind
        // of context for life, so making a WebGPU surface to test with would
        // rule out the WebGL2 fallback on the same canvas.
        let mut d = wgpu::InstanceDescriptor::new_without_display_handle();
        d.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(d);
        let (surface, adapter, backend) = match instance.request_adapter(&options(None)).await {
            Ok(adapter) => {
                let surface = instance.create_surface(target()).map_err(err)?;
                (surface, adapter, "webgpu")
            }
            Err(webgpu) => {
                let mut d = wgpu::InstanceDescriptor::new_without_display_handle();
                d.backends = wgpu::Backends::GL;
                let instance = wgpu::Instance::new(d);
                // A WebGL2 context is the canvas: the surface comes first.
                let surface = instance.create_surface(target()).map_err(err)?;
                let adapter = instance
                    .request_adapter(&options(Some(&surface)))
                    .await
                    .map_err(|gl| err(format!("no GPU: webgpu: {webgpu}; webgl2: {gl}")))?;
                (surface, adapter, "webgl2")
            }
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("skill-viz3d"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .map_err(err)?;
        let (device, queue) = (Arc::new(device), Arc::new(queue));

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let alpha_mode = [
            wgpu::CompositeAlphaMode::PreMultiplied,
            wgpu::CompositeAlphaMode::PostMultiplied,
        ]
        .into_iter()
        .find(|m| caps.alpha_modes.contains(m))
        .unwrap_or(caps.alpha_modes[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Srgb,
        };
        surface.configure(&device, &config);
        let mut renderer = Renderer::new(Arc::clone(&device), queue, format, width, height);
        renderer.set_msaa(4);
        Ok(Self {
            canvas,
            surface,
            device,
            config,
            renderer,
            backend,
        })
    }

    /// Size the drawing buffer to `css × dpr` pixels.
    fn resize(&mut self, css_width: f32, css_height: f32, dpr: f32) -> (u32, u32) {
        let w = (css_width * dpr).round().max(1.0) as u32;
        let h = (css_height * dpr).round().max(1.0) as u32;
        if (w, h) != (self.config.width, self.config.height) {
            self.canvas.set_width(w);
            self.canvas.set_height(h);
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
            self.renderer.resize(w, h);
        }
        // threers sizes attenuated points by the drawing buffer's height and
        // then the pixel ratio again; three.js by the CSS height times the
        // ratio. At ratio 1 the two agree — and nothing here draws fixed-size
        // points or wide lines, the other things the ratio scales.
        self.renderer.set_pixel_ratio(1.0);
        (w, h)
    }

    fn draw(&mut self, scene: &mut threers::scene::Scene, camera: &threers::PerspectiveCamera) {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                self.renderer.render(scene, camera, &view, false);
                self.renderer.queue_arc().present(frame);
            }
            _ => self.surface.configure(&self.device, &self.config),
        }
    }
}

// ── The electrode head ─────────────────────────────────────────────────────

/// `ElectrodeHead3D`: the scalp, the electrodes on it, orbiting by itself.
#[wasm_bindgen]
pub struct HeadViewer {
    canvas: Canvas,
    head: HeadView,
}

#[wasm_bindgen]
impl HeadViewer {
    /// Open the GPU on `canvas`. The canvas is left transparent.
    pub async fn create(canvas: HtmlCanvasElement, dark: bool) -> Result<HeadViewer, JsValue> {
        let mut canvas = Canvas::open(canvas).await?;
        // Threlte's `<Canvas>` default, which the view was tuned under.
        canvas.renderer.set_tone_mapping(ToneMapping::AgX, 1.0);
        Ok(HeadViewer {
            canvas,
            head: HeadView::new(dark),
        })
    }

    /// `"webgpu"` or `"webgl2"`.
    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.canvas.backend.into()
    }

    /// The scalp model, `head.glb`'s bytes.
    #[wasm_bindgen(js_name = loadModel)]
    pub fn load_model(&mut self, glb: &[u8]) -> Result<(), JsValue> {
        self.head.load_model(glb).map_err(err)
    }

    /// Electrodes: `names[i]`, MNE position `positions[3i..3i+3]` (mm),
    /// colour `colors[i]` (`0xRRGGBB`), and whether to draw it larger.
    #[wasm_bindgen(js_name = setElectrodes)]
    pub fn set_electrodes(
        &mut self,
        names: Vec<String>,
        positions: &[f32],
        colors: &[u32],
        emphasised: &[u8],
    ) -> Result<(), JsValue> {
        let n = names.len();
        if positions.len() != n * 3 || colors.len() != n || emphasised.len() != n {
            return Err(err("setElectrodes: lengths disagree"));
        }
        let list = names
            .into_iter()
            .enumerate()
            .map(|(i, name)| Electrode {
                name,
                pos: [positions[i * 3], positions[i * 3 + 1], positions[i * 3 + 2]],
                color: Color::from_hex(colors[i]),
                emphasised: emphasised[i] != 0,
            })
            .collect();
        self.head.set_electrodes(list);
        Ok(())
    }

    pub fn select(&mut self, name: Option<String>) {
        self.head.select(name.as_deref());
    }

    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(&mut self, dark: bool) {
        self.head.set_theme(dark);
    }

    #[wasm_bindgen(js_name = setElectrodeTransform)]
    pub fn set_electrode_transform(&mut self, px: f32, py: f32, pz: f32, sx: f32, sy: f32, sz: f32) {
        self.head
            .set_electrode_transform(Vector3::new(px, py, pz), Vector3::new(sx, sy, sz));
    }

    /// The canvas's CSS size and the device pixel ratio.
    pub fn resize(&mut self, css_width: f32, css_height: f32, dpr: f32) {
        let (_, h) = self.canvas.resize(css_width, css_height, dpr);
        self.head.view.set_viewport(css_width, css_height);
        self.head.set_buffer_height(h as f32);
    }

    #[wasm_bindgen(js_name = pointerDown)]
    pub fn pointer_down(&mut self, button: usize, x: f32, y: f32) {
        self.head.view.pointer_down(button, x, y);
    }

    #[wasm_bindgen(js_name = pointerMove)]
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.head.view.pointer_move(x, y);
    }

    #[wasm_bindgen(js_name = pointerUp)]
    pub fn pointer_up(&mut self, button: usize) {
        self.head.view.pointer_up(button);
    }

    pub fn wheel(&mut self, delta_y: f32, x: f32, y: f32) {
        self.head.view.wheel(delta_y, x, y);
    }

    /// Advance `dt` seconds and draw.
    pub fn frame(&mut self, dt: f32) {
        self.head.tick(dt);
        self.canvas.draw(&mut self.head.view.scene, &self.head.view.camera);
    }

    /// The index of the electrode under NDC `(x, y)`, or -1.
    #[wasm_bindgen(js_name = hitTest)]
    pub fn hit_test(&mut self, x: f32, y: f32) -> i32 {
        self.head.hit_test(x, y).map_or(-1, |i| i as i32)
    }
}

// ── A general scene, for the graph and UMAP views ─────────────────────────

/// A scene the page builds object by object, by handle: what
/// `InteractiveGraph3D` and `UmapViewer3D` used three.js for. Colours are
/// `0xRRGGBB` (sRGB, decoded as three.js does); per-vertex colours are linear.
/// Handle `0` is the scene root.
#[wasm_bindgen]
pub struct SceneViewer {
    canvas: Canvas,
    view: View,
}

fn look(color: u32, opacity: f32, transparent: bool, depth_test: bool, depth_write: i32, side: u32) -> Look {
    Look {
        color: Color::from_hex(color),
        opacity,
        transparent,
        depth_test,
        // -1: follow the material's transparency; 0 / 1: off / on.
        depth_write: match depth_write {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        },
        side,
    }
}

#[wasm_bindgen]
impl SceneViewer {
    /// `fov` in degrees. The scene starts with a transparent background.
    pub async fn create(canvas: HtmlCanvasElement, fov: f32, near: f32, far: f32) -> Result<SceneViewer, JsValue> {
        let canvas = Canvas::open(canvas).await?;
        Ok(SceneViewer {
            canvas,
            view: View::new(fov, near, far),
        })
    }

    #[wasm_bindgen(getter)]
    pub fn backend(&self) -> String {
        self.canvas.backend.into()
    }

    /// `undefined` for a transparent canvas.
    #[wasm_bindgen(js_name = setBackground)]
    pub fn set_background(&mut self, color: Option<u32>) {
        self.view.set_background(color);
    }

    #[wasm_bindgen(js_name = setCameraPosition)]
    pub fn set_camera_position(&mut self, x: f32, y: f32, z: f32) {
        self.view.set_camera_position(Vector3::new(x, y, z));
    }

    #[wasm_bindgen(js_name = cameraPosition)]
    pub fn camera_position(&self) -> Vec<f32> {
        let p = self.view.camera.position;
        vec![p.x, p.y, p.z]
    }

    /// The point the camera looks at and orbits.
    #[wasm_bindgen(js_name = setTarget)]
    pub fn set_target(&mut self, x: f32, y: f32, z: f32) {
        self.view.set_target(Vector3::new(x, y, z));
    }

    pub fn target(&self) -> Vec<f32> {
        let t = self.view.target();
        vec![t.x, t.y, t.z]
    }

    /// three.js `OrbitControls` settings; buttons are `MOUSE` numbers
    /// (0 rotate, 1 dolly, 2 pan) for left, middle, right.
    #[wasm_bindgen(js_name = setOrbit)]
    #[allow(clippy::too_many_arguments)]
    pub fn set_orbit(
        &mut self,
        enable_damping: bool,
        damping_factor: f32,
        auto_rotate: bool,
        auto_rotate_speed: f32,
        min_distance: f32,
        max_distance: f32,
        left: u32,
        middle: u32,
        right: u32,
    ) {
        self.view.set_orbit(OrbitOptions {
            enable_damping,
            damping_factor,
            auto_rotate,
            auto_rotate_speed,
            min_distance,
            max_distance,
            buttons: [
                Drag::from_three(left),
                Drag::from_three(middle),
                Drag::from_three(right),
            ],
            ..OrbitOptions::default()
        });
    }

    #[wasm_bindgen(js_name = setAutoRotate)]
    pub fn set_auto_rotate(&mut self, on: bool) {
        self.view.set_auto_rotate(on);
    }

    #[wasm_bindgen(js_name = addAmbientLight)]
    pub fn add_ambient_light(&mut self, color: u32, intensity: f32) -> u32 {
        self.view.add_ambient_light(color, intensity)
    }

    /// Shining from `(x, y, z)` towards the origin.
    #[wasm_bindgen(js_name = addDirectionalLight)]
    pub fn add_directional_light(&mut self, color: u32, intensity: f32, x: f32, y: f32, z: f32) -> u32 {
        self.view.add_directional_light(color, intensity, Vector3::new(x, y, z))
    }

    #[wasm_bindgen(js_name = addGroup)]
    pub fn add_group(&mut self, parent: u32) -> u32 {
        self.view.add_group(parent)
    }

    #[wasm_bindgen(js_name = addGrid)]
    pub fn add_grid(&mut self, parent: u32, size: f32, divisions: usize, center: u32, lines: u32, opacity: f32) -> u32 {
        let h = self.view.add_grid(parent, size, divisions, center, lines);
        if opacity < 1.0 {
            self.view.set_look(h, None, Some(opacity), Some(true));
        }
        h
    }

    /// A mesh. `shape`: 0 sphere (a radius, b width segments, c height
    /// segments), 1 octahedron (a radius), 2 tetrahedron (a radius), 3 ring
    /// (a inner, b outer, c segments), 4 cylinder (a radius, b height, c
    /// segments). `material`: 0 basic, 1 phong. `depth_write`: -1 auto.
    #[wasm_bindgen(js_name = addMesh)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_mesh(
        &mut self,
        parent: u32,
        shape: u32,
        a: f32,
        b: f32,
        c: f32,
        material: u32,
        color: u32,
        opacity: f32,
        transparent: bool,
        depth_test: bool,
        depth_write: i32,
        side: u32,
        shininess: f32,
        emissive: u32,
        emissive_intensity: f32,
    ) -> Result<u32, JsValue> {
        let shape = match shape {
            0 => Shape::Sphere {
                radius: a,
                width_segments: b as usize,
                height_segments: c as usize,
            },
            1 => Shape::Octahedron { radius: a },
            2 => Shape::Tetrahedron { radius: a },
            3 => Shape::Ring {
                inner: a,
                outer: b,
                segments: c as usize,
            },
            4 => Shape::Cylinder {
                radius: a,
                height: b,
                segments: c as usize,
            },
            n => return Err(err(format!("addMesh: unknown shape {n}"))),
        };
        let kind = match material {
            0 => MeshMaterial::Basic,
            1 => MeshMaterial::Phong {
                shininess,
                emissive,
                emissive_intensity,
            },
            n => return Err(err(format!("addMesh: unknown material {n}"))),
        };
        let l = look(color, opacity, transparent, depth_test, depth_write, side);
        Ok(self.view.add_mesh(parent, shape, kind, l))
    }

    /// Points at `positions` (xyz…), coloured per point by `colors` (linear
    /// rgb…) when given, else by `color`.
    #[wasm_bindgen(js_name = addPoints)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_points(
        &mut self,
        parent: u32,
        positions: &[f32],
        colors: Option<Vec<f32>>,
        size: f32,
        size_attenuation: bool,
        color: u32,
        opacity: f32,
        transparent: bool,
        depth_write: i32,
    ) -> u32 {
        let l = look(color, opacity, transparent, true, depth_write, 0);
        self.view
            .add_points(parent, positions, colors.as_deref(), size, size_attenuation, None, l)
    }

    /// Lines through `positions`: a strip when `strip`, else segment pairs.
    /// Dashed when `dash_size > 0`.
    #[wasm_bindgen(js_name = addLines)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_lines(
        &mut self,
        parent: u32,
        positions: &[f32],
        colors: Option<Vec<f32>>,
        strip: bool,
        dash_size: f32,
        gap_size: f32,
        color: u32,
        opacity: f32,
        transparent: bool,
        depth_test: bool,
    ) -> u32 {
        // three.js writes depth by default, transparent or not.
        let l = look(color, opacity, transparent, depth_test, 1, 0);
        let dash = (dash_size > 0.0).then_some((dash_size, gap_size));
        self.view
            .add_lines(parent, positions, colors.as_deref(), strip, dash, l)
    }

    /// A camera-facing picture, `width × height` straight-alpha RGBA bytes
    /// (a 2D canvas's `getImageData`), `sx × sy` world units.
    #[wasm_bindgen(js_name = addSprite)]
    #[allow(clippy::too_many_arguments)]
    pub fn add_sprite(
        &mut self,
        parent: u32,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        sx: f32,
        sy: f32,
        opacity: f32,
        depth_test: bool,
    ) -> u32 {
        let l = look(0xffffff, opacity, true, depth_test, 1, 0);
        self.view.add_sprite(parent, width, height, rgba, (sx, sy), l)
    }

    #[wasm_bindgen(js_name = setSpriteImage)]
    pub fn set_sprite_image(&mut self, h: u32, width: u32, height: u32, rgba: Vec<u8>) {
        self.view.set_sprite_image(h, width, height, rgba);
    }

    pub fn remove(&mut self, h: u32) {
        self.view.remove(h);
    }

    /// Remove everything under `h`.
    pub fn clear(&mut self, h: u32) {
        self.view.clear(h);
    }

    #[wasm_bindgen(js_name = setPosition)]
    pub fn set_position(&mut self, h: u32, x: f32, y: f32, z: f32) {
        self.view.set_position(h, Vector3::new(x, y, z));
    }

    #[wasm_bindgen(js_name = setScale)]
    pub fn set_scale(&mut self, h: u32, x: f32, y: f32, z: f32) {
        self.view.set_scale(h, Vector3::new(x, y, z));
    }

    #[wasm_bindgen(js_name = setVisible)]
    pub fn set_visible(&mut self, h: u32, visible: bool) {
        self.view.set_visible(h, visible);
    }

    #[wasm_bindgen(js_name = setRenderOrder)]
    pub fn set_render_order(&mut self, h: u32, order: i32) {
        self.view.set_render_order(h, order);
    }

    /// Turn `h`'s +Z towards the world point.
    #[wasm_bindgen(js_name = lookAt)]
    pub fn look_at(&mut self, h: u32, x: f32, y: f32, z: f32) {
        self.view.look_at(h, Vector3::new(x, y, z));
    }

    /// Turn `h`'s +Y along the direction (a cylinder between two points).
    #[wasm_bindgen(js_name = alignY)]
    pub fn align_y(&mut self, h: u32, x: f32, y: f32, z: f32) {
        self.view.align_y(h, Vector3::new(x, y, z));
    }

    #[wasm_bindgen(js_name = setColor)]
    pub fn set_color(&mut self, h: u32, color: u32) {
        self.view.set_look(h, Some(Color::from_hex(color)), None, None);
    }

    #[wasm_bindgen(js_name = setColorRgb)]
    pub fn set_color_rgb(&mut self, h: u32, r: f32, g: f32, b: f32) {
        self.view.set_look(h, Some(Color::new(r, g, b)), None, None);
    }

    #[wasm_bindgen(js_name = setOpacity)]
    pub fn set_opacity(&mut self, h: u32, opacity: f32, transparent: Option<bool>) {
        self.view.set_look(h, None, Some(opacity), transparent);
    }

    #[wasm_bindgen(js_name = setEmissive)]
    pub fn set_emissive(&mut self, h: u32, color: u32, intensity: f32) {
        self.view.set_emissive(h, Color::from_hex(color), intensity);
    }

    /// Keep `h` facing the camera (a ring around a point, say).
    #[wasm_bindgen(js_name = setBillboard)]
    pub fn set_billboard(&mut self, h: u32, on: bool) {
        self.view.set_billboard(h, on);
    }

    #[wasm_bindgen(js_name = setPointSize)]
    pub fn set_point_size(&mut self, h: u32, size: f32) {
        self.view.set_point_size(h, size);
    }

    /// Replace `position` or `color` (xyz… / rgb…).
    #[wasm_bindgen(js_name = setAttribute)]
    pub fn set_attribute(&mut self, h: u32, name: &str, data: &[f32]) {
        self.view.set_attribute(h, name, data);
    }

    /// The nearest object under NDC `(x, y)` among `among` and their
    /// children, as `[handle, index, distance]`, or empty. Points and lines
    /// count within `threshold` world units.
    pub fn pick(&mut self, x: f32, y: f32, among: &[u32], threshold: f32) -> Vec<f32> {
        match self.view.pick(x, y, among, threshold) {
            Some(p) => vec![p.handle as f32, p.index as f32, p.distance],
            None => vec![],
        }
    }

    /// World → NDC.
    pub fn project(&self, x: f32, y: f32, z: f32) -> Vec<f32> {
        let p = self.view.project(Vector3::new(x, y, z));
        vec![p.x, p.y, p.z]
    }

    pub fn resize(&mut self, css_width: f32, css_height: f32, dpr: f32) {
        self.canvas.resize(css_width, css_height, dpr);
        self.view.set_viewport(css_width, css_height);
    }

    #[wasm_bindgen(js_name = pointerDown)]
    pub fn pointer_down(&mut self, button: usize, x: f32, y: f32) {
        self.view.pointer_down(button, x, y);
    }

    #[wasm_bindgen(js_name = pointerMove)]
    pub fn pointer_move(&mut self, x: f32, y: f32) {
        self.view.pointer_move(x, y);
    }

    #[wasm_bindgen(js_name = pointerUp)]
    pub fn pointer_up(&mut self, button: usize) {
        self.view.pointer_up(button);
    }

    pub fn wheel(&mut self, delta_y: f32, x: f32, y: f32) {
        self.view.wheel(delta_y, x, y);
    }

    /// Advance the controls `dt` seconds and draw.
    pub fn frame(&mut self, dt: f32) {
        self.view.tick(dt);
        self.canvas.draw(&mut self.view.scene, &self.view.camera);
    }

    /// Draw without advancing (before reading the canvas back).
    pub fn render(&mut self) {
        self.canvas.draw(&mut self.view.scene, &self.view.camera);
    }
}
