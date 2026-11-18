//! A retained-mode 3D view: a scene, a camera, orbit controls and the objects
//! the app's views put in it, addressed by small integer handles.
//!
//! This is the part of the web views that does not care where it draws: the
//! wasm `Viewer` wraps it with a canvas, the tests with a headless renderer.
//! The operations are the ones the Svelte views used three.js for, no more.

use std::collections::HashMap;
use std::sync::Arc;

use threers::PerspectiveCamera;
use threers::controls::{OrbitControls, PointerEvent};
use threers::core::{
    BufferAttribute, BufferGeometry, LineSegments, Mesh, Object3D, ObjectId, ObjectKind, Points, Raycaster, Sprite,
};
use threers::geometries::{CylinderGeometry, OctahedronGeometry, RingGeometry, SphereGeometry, TetrahedronGeometry};
use threers::helpers::GridHelper;
use threers::lights::{AmbientLight, DirectionalLight};
use threers::materials::{
    BasicMaterial, LineBasicMaterial, Material, PhongMaterial, PhysicalMaterial, PointsMaterial, SpriteMaterial,
};
use threers::math::{Color, Vector2, Vector3};
use threers::scene::Scene;
use threers::textures::{Texture, TextureFormat};

/// A handle to an object in a [`View`]. `0` is the scene root.
pub type Handle = u32;

/// What a mouse button does to the camera, as three.js's `MOUSE` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    Rotate,
    Dolly,
    Pan,
    None,
}

impl Drag {
    /// three.js's numbering: `MOUSE.ROTATE = 0`, `DOLLY = 1`, `PAN = 2`.
    pub fn from_three(n: u32) -> Self {
        match n {
            0 => Self::Rotate,
            1 => Self::Dolly,
            2 => Self::Pan,
            _ => Self::None,
        }
    }
}

/// Orbit settings, with three.js's `OrbitControls` names.
#[derive(Debug, Clone, Copy)]
pub struct OrbitOptions {
    pub enable_damping: bool,
    pub damping_factor: f32,
    pub auto_rotate: bool,
    /// three.js units: 2.0 is one turn every 30 s at 60 fps.
    pub auto_rotate_speed: f32,
    pub enable_zoom: bool,
    pub enable_pan: bool,
    pub enable_rotate: bool,
    pub min_distance: f32,
    pub max_distance: f32,
    /// Left, middle, right.
    pub buttons: [Drag; 3],
}

impl Default for OrbitOptions {
    fn default() -> Self {
        Self {
            enable_damping: false,
            damping_factor: 0.05,
            auto_rotate: false,
            auto_rotate_speed: 2.0,
            enable_zoom: true,
            enable_pan: true,
            enable_rotate: true,
            min_distance: 0.0,
            max_distance: f32::INFINITY,
            buttons: [Drag::Rotate, Drag::Dolly, Drag::Pan],
        }
    }
}

#[derive(Default)]
struct Input {
    held: [bool; 3],
    last: Option<(f32, f32)>,
    dx: f32,
    dy: f32,
    wheel: f32,
    cursor: (f32, f32),
}

/// A hit from [`View::pick`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub handle: Handle,
    /// The point's index for points, the segment's for lines, the triangle's
    /// for meshes.
    pub index: u32,
    pub distance: f32,
    pub point: Vector3,
}

pub struct View {
    pub scene: Scene,
    pub camera: PerspectiveCamera,
    orbit: Option<(OrbitControls, OrbitOptions)>,
    input: Input,
    objects: HashMap<Handle, ObjectId>,
    next: Handle,
    /// CSS pixels, for turning pointer motion into orbit motion.
    viewport: (f32, f32),
    /// Objects turned to face the camera every frame (three.js code doing
    /// `obj.quaternion.copy(camera.quaternion)` in its render loop).
    billboards: Vec<Handle>,
}

/// A material to make a mesh with. Colours are `0xRRGGBB`, sRGB as in three.js.
#[derive(Debug, Clone, Copy)]
pub enum MeshMaterial {
    Basic,
    Phong {
        shininess: f32,
        emissive: u32,
        emissive_intensity: f32,
    },
    Physical {
        roughness: f32,
        metalness: f32,
        clearcoat: f32,
        clearcoat_roughness: f32,
        emissive: u32,
        emissive_intensity: f32,
    },
}

/// Common material switches.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub color: Color,
    pub opacity: f32,
    pub transparent: bool,
    pub depth_test: bool,
    pub depth_write: Option<bool>,
    /// three.js `side`: 0 front, 1 back, 2 double.
    pub side: u32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            color: Color::WHITE,
            opacity: 1.0,
            transparent: false,
            depth_test: true,
            depth_write: None,
            side: 0,
        }
    }
}

/// A geometry to make a mesh with, by three.js constructor.
#[derive(Debug, Clone, Copy)]
pub enum Shape {
    Sphere {
        radius: f32,
        width_segments: usize,
        height_segments: usize,
    },
    Octahedron {
        radius: f32,
    },
    Tetrahedron {
        radius: f32,
    },
    Ring {
        inner: f32,
        outer: f32,
        segments: usize,
    },
    Cylinder {
        radius: f32,
        height: f32,
        segments: usize,
    },
}

impl Shape {
    pub fn geometry(self) -> BufferGeometry {
        match self {
            Self::Sphere {
                radius,
                width_segments,
                height_segments,
            } => SphereGeometry::new(radius, width_segments, height_segments),
            Self::Octahedron { radius } => OctahedronGeometry::new(radius, 0),
            Self::Tetrahedron { radius } => TetrahedronGeometry::new(radius, 0),
            Self::Ring { inner, outer, segments } => {
                RingGeometry::new(inner, outer, segments, 1, 0.0, std::f32::consts::TAU)
            }
            Self::Cylinder {
                radius,
                height,
                segments,
            } => CylinderGeometry::new(radius, radius, height, segments, 1, false, 0.0, std::f32::consts::TAU),
        }
    }
}

fn apply_look_state(rs: &mut threers::materials::RenderState, look: &Look) {
    rs.depth_test = look.depth_test;
    rs.depth_write = look.depth_write;
}

fn mesh_material(kind: MeshMaterial, look: &Look) -> Material {
    match kind {
        MeshMaterial::Basic => {
            let mut m = BasicMaterial {
                color: look.color,
                ..Default::default()
            };
            m.opacity = look.opacity;
            m.transparent = look.transparent;
            m.side = look.side;
            apply_look_state(&mut m.render_state, look);
            Material::Basic(m)
        }
        MeshMaterial::Phong {
            shininess,
            emissive,
            emissive_intensity,
        } => {
            let mut m = PhongMaterial {
                color: look.color,
                ..Default::default()
            };
            m.shininess = shininess;
            // three.js multiplies `emissive` by `emissiveIntensity` in the shader.
            m.emissive = Color::from_hex(emissive).scaled(emissive_intensity);
            m.opacity = look.opacity;
            m.transparent = Some(look.transparent);
            m.side = look.side;
            apply_look_state(&mut m.render_state, look);
            Material::Phong(m)
        }
        MeshMaterial::Physical {
            roughness,
            metalness,
            clearcoat,
            clearcoat_roughness,
            emissive,
            emissive_intensity,
        } => {
            let mut m = PhysicalMaterial {
                color: look.color,
                ..Default::default()
            };
            m.roughness = roughness;
            m.metalness = metalness;
            m.clearcoat = clearcoat;
            m.clearcoat_roughness = clearcoat_roughness;
            m.emissive = Color::from_hex(emissive);
            m.emissive_intensity = emissive_intensity;
            m.opacity = look.opacity;
            m.transparent = Some(look.transparent);
            apply_look_state(&mut m.render_state, look);
            Material::Physical(m)
        }
    }
}

/// A geometry with `position` (and `color`, when given) attributes.
pub fn attributes(positions: &[f32], colors: Option<&[f32]>) -> BufferGeometry {
    let mut g = BufferGeometry::new();
    g.set_attribute("position", BufferAttribute::new(positions.to_vec(), 3));
    if let Some(c) = colors {
        g.set_attribute("color", BufferAttribute::new(c.to_vec(), 3));
    }
    g
}

impl View {
    pub fn new(fov: f32, near: f32, far: f32) -> Self {
        let mut scene = Scene::new();
        scene.background = Color::new(0.0, 0.0, 0.0);
        scene.background_alpha = 0.0;
        let mut objects = HashMap::new();
        objects.insert(0, scene.root);
        Self {
            scene,
            camera: PerspectiveCamera::new(fov, 1.0, near, far),
            orbit: None,
            input: Input::default(),
            objects,
            next: 1,
            viewport: (1.0, 1.0),
            billboards: Vec::new(),
        }
    }

    // ── Camera, background, size ────────────────────────────────────────

    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = (width.max(1.0), height.max(1.0));
        self.camera.aspect = self.viewport.0 / self.viewport.1;
    }

    pub fn viewport(&self) -> (f32, f32) {
        self.viewport
    }

    /// `None` leaves the canvas transparent.
    pub fn set_background(&mut self, color: Option<u32>) {
        match color {
            Some(hex) => {
                self.scene.background = Color::from_hex(hex);
                self.scene.background_alpha = 1.0;
            }
            // Black as well as clear: on a premultiplied canvas a colour
            // at alpha 0 still adds.
            None => {
                self.scene.background = Color::new(0.0, 0.0, 0.0);
                self.scene.background_alpha = 0.0;
            }
        }
    }

    pub fn set_camera_position(&mut self, p: Vector3) {
        self.camera.position = p;
        self.reseed_orbit();
    }

    /// Where the camera looks, and the point it orbits.
    pub fn set_target(&mut self, t: Vector3) {
        self.camera.look_at(t);
        if let Some((c, _)) = &mut self.orbit {
            c.target = t;
        }
        self.reseed_orbit();
    }

    pub fn target(&self) -> Vector3 {
        match &self.orbit {
            Some((c, _)) => c.target,
            None => self.camera.target,
        }
    }

    fn reseed_orbit(&mut self) {
        if let Some((c, _)) = &mut self.orbit {
            c.reseed_from_camera(&self.camera);
        }
    }

    /// World point → normalised device coordinates (`-1..1`, +y up).
    pub fn project(&self, p: Vector3) -> Vector3 {
        use threers::cameras::Camera;
        p.project(&self.camera.view_matrix(), &self.camera.projection_matrix())
    }

    // ── Orbit controls and input ────────────────────────────────────────

    pub fn set_orbit(&mut self, options: OrbitOptions) {
        let mut c = match self.orbit.take() {
            Some((c, _)) => c,
            None => OrbitControls::new(&self.camera),
        };
        c.enable_damping = options.enable_damping;
        c.damping = options.damping_factor;
        c.auto_rotate = options.auto_rotate;
        // three.js rotates by 2π/60/60·speed per frame at 60 fps.
        c.auto_rotate_speed = std::f32::consts::TAU / 60.0 * options.auto_rotate_speed;
        c.min_distance = options.min_distance;
        c.max_distance = options.max_distance;
        // three.js's wheel step is 0.95 per notch (deltaY 100); this one's
        // factor is 1 − 0.001·wheel·speed, so 0.5 lands on 0.95.
        c.zoom_speed = 0.5;
        self.orbit = Some((c, options));
    }

    pub fn orbit_options(&self) -> Option<OrbitOptions> {
        self.orbit.map(|(_, o)| o)
    }

    pub fn pointer_down(&mut self, button: usize, x: f32, y: f32) {
        if button < 3 {
            self.input.held[button] = true;
        }
        self.input.last = Some((x, y));
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) {
        if let Some((lx, ly)) = self.input.last
            && self.input.held.iter().any(|h| *h)
        {
            self.input.dx += x - lx;
            self.input.dy += y - ly;
        }
        self.input.last = Some((x, y));
    }

    pub fn pointer_up(&mut self, button: usize) {
        if button < 3 {
            self.input.held[button] = false;
        }
        if !self.input.held.iter().any(|h| *h) {
            self.input.last = None;
        }
    }

    /// `delta_y` as a DOM `WheelEvent` reports it (positive scrolls down,
    /// which zooms out); `x`, `y` in CSS pixels.
    pub fn wheel(&mut self, delta_y: f32, x: f32, y: f32) {
        self.input.wheel -= delta_y;
        self.input.cursor = (x / self.viewport.0 * 2.0 - 1.0, 1.0 - y / self.viewport.1 * 2.0);
    }

    /// Keep `h` turned to face the camera.
    pub fn set_billboard(&mut self, h: Handle, on: bool) {
        self.billboards.retain(|b| *b != h);
        if on {
            self.billboards.push(h);
        }
    }

    /// Turn every billboard to the camera's orientation.
    fn face_camera(&mut self) {
        use threers::cameras::Camera;
        if self.billboards.is_empty() {
            return;
        }
        let q = threers::math::Quaternion::from_rotation_matrix(&self.camera.view_matrix().invert());
        let objects = &self.objects;
        self.billboards.retain(|h| objects.contains_key(h));
        for h in self.billboards.clone() {
            if let Some(o) = self.object_mut(h) {
                o.set_rotation_from_quaternion(q);
            }
        }
    }

    /// Advance controls by `dt` seconds. `true` while the camera is moving.
    pub fn tick(&mut self, dt: f32) -> bool {
        let moved = self.step_controls(dt);
        self.face_camera();
        moved
    }

    fn step_controls(&mut self, dt: f32) -> bool {
        let Some((controls, options)) = &mut self.orbit else {
            self.input = Input {
                last: self.input.last,
                held: self.input.held,
                ..Input::default()
            };
            return false;
        };
        let mut rotating = false;
        let mut panning = false;
        let mut dolly = 0.0;
        for (i, held) in self.input.held.iter().enumerate() {
            if !*held {
                continue;
            }
            match options.buttons[i] {
                Drag::Rotate if options.enable_rotate => rotating = true,
                Drag::Pan if options.enable_pan => panning = true,
                Drag::Dolly if options.enable_zoom => dolly += self.input.dy * -4.0,
                _ => {}
            }
        }
        let wheel = if options.enable_zoom {
            self.input.wheel + dolly
        } else {
            0.0
        };
        let ev = PointerEvent {
            dx: self.input.dx,
            dy: self.input.dy,
            wheel,
            rotating,
            panning,
            cursor: self.input.cursor,
        };
        let before = (self.camera.position, controls.target);
        controls.update_dt(ev, &mut self.camera, self.viewport, dt);
        self.input.dx = 0.0;
        self.input.dy = 0.0;
        self.input.wheel = 0.0;
        before != (self.camera.position, controls.target)
    }

    // ── Objects ──────────────────────────────────────────────────────────

    fn insert(&mut self, parent: Handle, obj: Object3D) -> Handle {
        let parent_id = self.objects.get(&parent).copied().unwrap_or(self.scene.root);
        let id = self.scene.add_to(parent_id, obj);
        let h = self.next;
        self.next += 1;
        self.objects.insert(h, id);
        h
    }

    pub fn id(&self, h: Handle) -> Option<ObjectId> {
        self.objects.get(&h).copied()
    }

    pub fn object_mut(&mut self, h: Handle) -> Option<&mut Object3D> {
        let id = self.id(h)?;
        self.scene.get_mut(id)
    }

    pub fn object(&self, h: Handle) -> Option<&Object3D> {
        self.scene.get(self.id(h)?)
    }

    pub fn add_group(&mut self, parent: Handle) -> Handle {
        self.insert(parent, Object3D::group())
    }

    pub fn add_mesh(&mut self, parent: Handle, shape: Shape, kind: MeshMaterial, look: Look) -> Handle {
        self.add_mesh_geometry(parent, shape.geometry(), kind, look)
    }

    pub fn add_mesh_geometry(
        &mut self,
        parent: Handle,
        geometry: BufferGeometry,
        kind: MeshMaterial,
        look: Look,
    ) -> Handle {
        let mesh = Mesh::new(geometry, mesh_material(kind, &look));
        self.insert(parent, Object3D::mesh(mesh))
    }

    /// A point cloud; `colors` per point (linear RGB) or `None` for `look.color`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_points(
        &mut self,
        parent: Handle,
        positions: &[f32],
        colors: Option<&[f32]>,
        size: f32,
        size_attenuation: bool,
        map: Option<Arc<Texture>>,
        look: Look,
    ) -> Handle {
        let mut m = PointsMaterial::new(look.color, size);
        m.size_attenuation = size_attenuation;
        m.opacity = look.opacity;
        m.transparent = Some(look.transparent);
        m.map = map;
        apply_look_state(&mut m.render_state, &look);
        let points = Points::new(attributes(positions, colors), Material::Points(m));
        self.insert(parent, Object3D::points(points))
    }

    /// Lines: `strip` joins consecutive points (three.js `Line`), otherwise
    /// each pair of points is a segment (`LineSegments`).
    #[allow(clippy::too_many_arguments)]
    pub fn add_lines(
        &mut self,
        parent: Handle,
        positions: &[f32],
        colors: Option<&[f32]>,
        strip: bool,
        dash: Option<(f32, f32)>,
        look: Look,
    ) -> Handle {
        let mut m = LineBasicMaterial::new(look.color);
        m.opacity = look.opacity;
        m.transparent = Some(look.transparent);
        if let Some((dash_size, gap_size)) = dash {
            m.dashed = true;
            m.dash_size = dash_size;
            m.gap_size = gap_size;
        }
        apply_look_state(&mut m.render_state, &look);
        let geometry = attributes(positions, colors);
        let ls = if strip {
            LineSegments::strip(Arc::new(geometry), Arc::new(Material::Line(m)), false)
        } else {
            LineSegments::new(geometry, Material::Line(m))
        };
        self.insert(parent, Object3D::line_segments(ls))
    }

    /// A camera-facing sprite showing `rgba` (straight alpha, row 0 at the
    /// top) at `scale` world units. The bytes are taken as they are, not
    /// decoded from sRGB — three.js's `CanvasTexture` default (`NoColorSpace`),
    /// which is what the views' labels were drawn for.
    pub fn add_sprite(
        &mut self,
        parent: Handle,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        scale: (f32, f32),
        look: Look,
    ) -> Handle {
        let tex = Arc::new(Texture::new(width, height, TextureFormat::Rgba8Unorm, rgba));
        let mut m = SpriteMaterial::new(look.color).with_map(tex);
        m.opacity = look.opacity;
        m.transparent = Some(true);
        apply_look_state(&mut m.render_state, &look);
        let mut obj = Object3D::sprite(Sprite::new(Material::Sprite(m)));
        obj.scale = Vector3::new(scale.0, scale.1, 1.0);
        self.insert(parent, obj)
    }

    pub fn add_grid(&mut self, parent: Handle, size: f32, divisions: usize, center: u32, lines: u32) -> Handle {
        self.insert(parent, GridHelper::new_from_hex(size, divisions, center, lines))
    }

    pub fn add_ambient_light(&mut self, color: u32, intensity: f32) -> Handle {
        self.insert(0, Object3D::light(AmbientLight::new(Color::from_hex(color), intensity)))
    }

    /// A directional light shining from `from` towards the origin, as a
    /// three.js `DirectionalLight` at `position` with its default target.
    pub fn add_directional_light(&mut self, color: u32, intensity: f32, from: Vector3) -> Handle {
        let light = DirectionalLight::new(Color::from_hex(color), intensity).with_direction((-from).normalize());
        self.insert(0, Object3D::light(light))
    }

    /// Insert an already-built object (a loaded model, say).
    pub fn add_object(&mut self, parent: Handle, obj: Object3D) -> Handle {
        self.insert(parent, obj)
    }

    /// Track an object something else put in the scene (a model's children).
    pub fn adopt(&mut self, id: ObjectId) -> Handle {
        let h = self.next;
        self.next += 1;
        self.objects.insert(h, id);
        h
    }

    /// Remove an object and everything under it.
    pub fn remove(&mut self, h: Handle) {
        if h == 0 {
            return;
        }
        let Some(id) = self.objects.remove(&h) else { return };
        let mut gone = Vec::new();
        self.scene.arena.traverse(id, &mut |cid, _| gone.push(cid));
        self.scene.arena.remove_subtree(id);
        self.objects.retain(|_, v| !gone.contains(v));
    }

    /// Remove everything under `h`, keeping `h`.
    pub fn clear(&mut self, h: Handle) {
        let Some(id) = self.id(h) else { return };
        let children = self.scene.get(id).map(|o| o.children.clone()).unwrap_or_default();
        let handles: Vec<Handle> = self
            .objects
            .iter()
            .filter(|(_, v)| children.contains(v))
            .map(|(k, _)| *k)
            .collect();
        for c in handles {
            self.remove(c);
        }
    }

    // ── Per-object changes ───────────────────────────────────────────────

    pub fn set_position(&mut self, h: Handle, p: Vector3) {
        if let Some(o) = self.object_mut(h) {
            o.position = p;
        }
    }

    pub fn set_scale(&mut self, h: Handle, s: Vector3) {
        if let Some(o) = self.object_mut(h) {
            o.scale = s;
        }
    }

    pub fn set_visible(&mut self, h: Handle, visible: bool) {
        if let Some(o) = self.object_mut(h) {
            o.visible = visible;
        }
    }

    pub fn set_render_order(&mut self, h: Handle, order: i32) {
        if let Some(o) = self.object_mut(h) {
            o.render_order = order;
        }
    }

    /// Turn `h` so its +Z faces the world point `t` (three.js `lookAt` on a
    /// non-camera object).
    pub fn look_at(&mut self, h: Handle, t: Vector3) {
        if let Some(id) = self.id(h) {
            self.scene.update_world();
            self.scene.arena.look_at(id, t);
        }
    }

    /// Orient `h` so its local `+Y` points along `dir` (a cylinder between
    /// two points).
    pub fn align_y(&mut self, h: Handle, dir: Vector3) {
        let q = threers::math::Quaternion::from_unit_vectors(Vector3::UP, dir.normalize());
        if let Some(o) = self.object_mut(h) {
            o.set_rotation_from_quaternion(q);
        }
    }

    fn material_mut(obj: &mut Object3D) -> Option<&mut Material> {
        match &mut obj.kind {
            ObjectKind::Mesh(m) => Some(Arc::make_mut(&mut m.material)),
            ObjectKind::Points(p) => Some(Arc::make_mut(&mut p.material)),
            ObjectKind::LineSegments(l) => Some(Arc::make_mut(&mut l.material)),
            ObjectKind::Sprite(s) => Some(Arc::make_mut(&mut s.material)),
            _ => None,
        }
    }

    /// Change colour and opacity in place.
    pub fn set_look(&mut self, h: Handle, color: Option<Color>, opacity: Option<f32>, transparent: Option<bool>) {
        let Some(o) = self.object_mut(h) else { return };
        let Some(m) = Self::material_mut(o) else { return };
        macro_rules! set {
            ($m:expr, $tr:expr) => {{
                if let Some(c) = color {
                    $m.color = c;
                }
                if let Some(a) = opacity {
                    $m.opacity = a;
                }
                if let Some(t) = transparent {
                    $tr(&mut *$m, t);
                }
            }};
        }
        match m {
            Material::Basic(m) => set!(m, |m: &mut BasicMaterial, t| m.transparent = t),
            Material::Phong(m) => set!(m, |m: &mut PhongMaterial, t| m.transparent = Some(t)),
            Material::Physical(m) => set!(m, |m: &mut PhysicalMaterial, t| m.transparent = Some(t)),
            Material::Points(m) => set!(m, |m: &mut PointsMaterial, t| m.transparent = Some(t)),
            Material::Line(m) => set!(m, |m: &mut LineBasicMaterial, t| m.transparent = Some(t)),
            Material::Sprite(m) => set!(m, |m: &mut SpriteMaterial, t| m.transparent = Some(t)),
            _ => {}
        }
    }

    /// A physical/phong material's emissive colour and the physical
    /// parameters a theme change touches.
    pub fn set_surface(&mut self, h: Handle, emissive: Color, emissive_intensity: f32, roughness: f32, metalness: f32) {
        let Some(o) = self.object_mut(h) else { return };
        match Self::material_mut(o) {
            Some(Material::Physical(m)) => {
                m.emissive = emissive;
                m.emissive_intensity = emissive_intensity;
                m.roughness = roughness;
                m.metalness = metalness;
            }
            Some(Material::Phong(m)) => m.emissive = emissive.scaled(emissive_intensity),
            _ => {}
        }
    }

    /// Replace a sprite's picture (a thumbnail that finished loading).
    pub fn set_sprite_image(&mut self, h: Handle, width: u32, height: u32, rgba: Vec<u8>) {
        let tex = Arc::new(Texture::new(width, height, TextureFormat::Rgba8Unorm, rgba));
        if let Some(o) = self.object_mut(h)
            && let Some(Material::Sprite(m)) = Self::material_mut(o)
        {
            m.map = Some(tex);
        }
    }

    /// Switch automatic rotation on or off, leaving the other settings.
    pub fn set_auto_rotate(&mut self, on: bool) {
        if let Some((c, o)) = &mut self.orbit {
            c.auto_rotate = on;
            o.auto_rotate = on;
        }
    }

    /// A Phong or physical material's glow: `emissive × intensity`.
    pub fn set_emissive(&mut self, h: Handle, emissive: Color, intensity: f32) {
        let Some(o) = self.object_mut(h) else { return };
        match Self::material_mut(o) {
            Some(Material::Phong(m)) => m.emissive = emissive.scaled(intensity),
            Some(Material::Physical(m)) => {
                m.emissive = emissive;
                m.emissive_intensity = intensity;
            }
            _ => {}
        }
    }

    pub fn set_point_size(&mut self, h: Handle, size: f32) {
        if let Some(o) = self.object_mut(h)
            && let Some(Material::Points(m)) = Self::material_mut(o)
        {
            m.size = size;
        }
    }

    /// Replace a points/lines/mesh object's `position` or `color` attribute.
    pub fn set_attribute(&mut self, h: Handle, name: &str, data: &[f32]) {
        let Some(o) = self.object_mut(h) else { return };
        let geometry = match &mut o.kind {
            ObjectKind::Mesh(m) => &mut m.geometry,
            ObjectKind::Points(p) => &mut p.geometry,
            ObjectKind::LineSegments(l) => {
                // A strip is drawn from its line-list expansion: rebuild both.
                if let Some(strip) = &l.strip {
                    let mut s = (*strip.geometry).clone();
                    s.set_attribute(name, BufferAttribute::new(data.to_vec(), 3));
                    s.bump_version();
                    let material = Arc::clone(&l.material);
                    let closed = strip.closed;
                    *l = LineSegments::strip(Arc::new(s), material, closed);
                    return;
                }
                &mut l.geometry
            }
            _ => return,
        };
        let g = Arc::make_mut(geometry);
        g.set_attribute(name, BufferAttribute::new(data.to_vec(), 3));
        g.bump_version();
    }

    // ── Picking ──────────────────────────────────────────────────────────

    /// The nearest hit under NDC `(x, y)` among `among` (and their
    /// descendants). Points and lines count within `threshold` world units.
    /// The handle reported is the object hit, as three.js's `hit.object` —
    /// or, for a part of something loaded whole, its nearest ancestor with one.
    pub fn pick(&mut self, x: f32, y: f32, among: &[Handle], threshold: f32) -> Option<Pick> {
        self.scene.update_world();
        let mut ray = Raycaster::new(Vector3::ZERO, Vector3::new(0.0, 0.0, -1.0), 0.0, f32::INFINITY);
        ray.params.points.threshold = threshold;
        ray.params.line.threshold = threshold;
        ray.set_from_camera(Vector2::new(x, y), &self.camera);
        let ids: Vec<ObjectId> = among.iter().filter_map(|h| self.id(*h)).collect();
        let hit = ray
            .intersect_object_list(&self.scene.arena, &ids, true)
            .into_iter()
            .next()?;
        // Report the handle the caller knows: the object itself if it has
        // one, else the nearest listed ancestor.
        let mut id = Some(hit.object);
        while let Some(cur) = id {
            if let Some((h, _)) = self.objects.iter().find(|(_, v)| **v == cur) {
                return Some(Pick {
                    handle: *h,
                    index: hit.face_index as u32,
                    distance: hit.distance,
                    point: hit.point,
                });
            }
            id = self.scene.get(cur).and_then(|o| o.parent);
        }
        None
    }
}
