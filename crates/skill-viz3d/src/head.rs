//! The electrode head: the scalp model, electrodes snapped onto it, the
//! Muse band, a pulsing ring on the selected electrode, and hit-testing.
//!
//! A port of `ElectrodeHead3D.svelte` (three.js through Threlte). The numbers
//! — scale, offset, the cranium centre, the 0.1 lift off the scalp, the camera
//! and lights — are that file's.

use std::sync::Arc;

use threers::core::{Object3D, ObjectKind};
use threers::loaders::{GltfLoader, gltf_add_to_scene as add_to_scene};
use threers::materials::{Material, PhysicalMaterial};
use threers::math::{Color, Vector3};
use threers::textures::{Texture, TextureFormat};

use crate::view::{Handle, Look, MeshMaterial, OrbitOptions, Shape, View};

/// MNE positions are millimetres; the model is about 8.5 units wide.
const SCALE: f32 = 0.0235;
const OFFSET: Vector3 = Vector3::new(-0.6 * SCALE, 85.0 * SCALE, 29.0 * SCALE);
/// How far electrodes sit off the scalp, along its normal.
const NORMAL_OFFSET: f32 = 0.1;
const BAND_OFFSET: f32 = 0.08;
/// The four Muse electrodes the band joins, in order around the head.
const MUSE_BAND: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];

/// One electrode as the view needs it.
#[derive(Debug, Clone)]
pub struct Electrode {
    pub name: String,
    /// MNE head coordinates, millimetres, three.js axes.
    pub pos: [f32; 3],
    /// Linear RGB.
    pub color: Color,
    /// Shown larger (a Muse electrode in the 10-20 view).
    pub emphasised: bool,
}

pub struct Theme {
    pub dark: bool,
}

/// The scalp, as world-space triangles, for snapping onto it.
struct Scalp {
    triangles: Vec<[Vector3; 3]>,
    cranium: Vector3,
}

impl Scalp {
    /// Where the ray from the cranium centre towards `target` leaves the head,
    /// lifted `lift` along the surface normal — or, when it misses, the
    /// closest point of the surface.
    fn snap(&self, target: Vector3, lift: f32) -> Vector3 {
        let dir = (target - self.cranium).normalize();
        let mut far: Option<(f32, Vector3)> = None;
        for t in &self.triangles {
            if let Some(d) = ray_triangle(self.cranium, dir, t)
                && far.is_none_or(|(fd, _)| d > fd)
            {
                far = Some((d, t_normal(t)));
            }
        }
        if let Some((d, n)) = far {
            let p = self.cranium + dir * d;
            return p + self.outward(n, p) * lift;
        }
        let mut best: Option<(f32, Vector3, Vector3)> = None;
        for t in &self.triangles {
            let q = closest_on_triangle(target, t);
            let d = (q - target).length_sq();
            if best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, q, t_normal(t)));
            }
        }
        match best {
            Some((_, q, n)) => q + self.outward(n, q) * lift,
            None => target,
        }
    }

    fn outward(&self, n: Vector3, at: Vector3) -> Vector3 {
        if n.dot(at - self.cranium) < 0.0 { -n } else { n }
    }
}

fn t_normal(t: &[Vector3; 3]) -> Vector3 {
    (t[1] - t[0]).cross(t[2] - t[0]).normalize()
}

/// Möller–Trumbore, both faces. Distance along `dir` if hit.
fn ray_triangle(origin: Vector3, dir: Vector3, t: &[Vector3; 3]) -> Option<f32> {
    let e1 = t[1] - t[0];
    let e2 = t[2] - t[0];
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = origin - t[0];
    let u = s.dot(p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) * inv;
    (d > 1e-6).then_some(d)
}

/// Closest point of triangle `t` to `p` (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_on_triangle(p: Vector3, t: &[Vector3; 3]) -> Vector3 {
    let [a, b, c] = *t;
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

fn mne_to_world(p: [f32; 3]) -> Vector3 {
    Vector3::new(p[0], p[1], p[2]) * SCALE + OFFSET
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The electrode dot: a bright core, a ring and a faint halo, in alpha — the
/// fragment shader of the three.js version baked into a texture.
pub fn electrode_sprite(size: u32) -> Texture {
    let mut px = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let u = (x as f32 + 0.5) / size as f32 - 0.5;
            let v = (y as f32 + 0.5) / size as f32 - 0.5;
            let d = (u * u + v * v).sqrt() * 2.0;
            let inner = 1.0 - smoothstep(0.0, 0.55, d);
            let ring = smoothstep(0.45, 0.6, d) * (1.0 - smoothstep(0.65, 0.95, d));
            let halo = (1.0 - smoothstep(0.0, 1.0, d)) * 0.25;
            let a = (inner * 0.9 + ring * 0.5 + halo).clamp(0.0, 1.0);
            px.extend_from_slice(&[255, 255, 255, (a * 255.0).round() as u8]);
        }
    }
    Texture::new(size, size, TextureFormat::Rgba8Unorm, px)
}

/// Electrode sizes of the three.js version (`aSize`): plain, emphasised, selected.
const SIZES: [f32; 3] = [1.0, 2.0, 2.8];

pub struct HeadView {
    pub view: View,
    root: Handle,
    electrodes_group: Handle,
    head_meshes: Vec<Handle>,
    lights: Vec<Handle>,
    scalp: Option<Scalp>,
    electrodes: Vec<Electrode>,
    /// Snapped, in the electrode group's frame.
    placed: Vec<Vector3>,
    selected: Option<usize>,
    dots: [Option<Handle>; 3],
    band: Option<Handle>,
    ring: Option<Handle>,
    sprite: Arc<Texture>,
    dark: bool,
    clock: f32,
    /// Drawing-buffer height in pixels, which point sizes follow.
    buffer_height: f32,
}

impl HeadView {
    pub fn new(dark: bool) -> Self {
        let mut view = View::new(36.0, 0.1, 100.0);
        view.set_camera_position(Vector3::new(0.0, 4.0, 14.12));
        view.set_target(Vector3::ZERO);
        view.set_orbit(OrbitOptions {
            enable_damping: true,
            damping_factor: 0.12,
            enable_zoom: true,
            min_distance: 6.0,
            max_distance: 20.0,
            enable_pan: false,
            auto_rotate: true,
            auto_rotate_speed: 0.6,
            ..OrbitOptions::default()
        });
        let root = view.add_group(0);
        let electrodes_group = view.add_group(root);
        let mut me = Self {
            view,
            root,
            electrodes_group,
            head_meshes: Vec::new(),
            lights: Vec::new(),
            scalp: None,
            electrodes: Vec::new(),
            placed: Vec::new(),
            selected: None,
            dots: [None; 3],
            band: None,
            ring: None,
            sprite: Arc::new(electrode_sprite(64)),
            dark,
            clock: 0.0,
            buffer_height: 600.0,
        };
        me.build_lights();
        me
    }

    fn build_lights(&mut self) {
        for h in self.lights.drain(..) {
            self.view.remove(h);
        }
        let d = self.dark;
        let v = &mut self.view;
        self.lights = vec![
            v.add_ambient_light(0xffffff, if d { 0.5 } else { 0.7 }),
            v.add_directional_light(
                if d { 0xc8c8ff } else { 0xffffff },
                if d { 0.8 } else { 0.6 },
                Vector3::new(5.0, 8.0, 8.0),
            ),
            v.add_directional_light(0x818cf8, 0.3, Vector3::new(-6.0, 4.0, -3.0)),
            v.add_directional_light(0xffffff, 0.15, Vector3::new(0.0, -3.0, 5.0)),
            v.add_directional_light(0xa5b4fc, if d { 0.4 } else { 0.2 }, Vector3::new(0.0, 10.0, 0.0)),
        ];
    }

    fn head_material(&self) -> PhysicalMaterial {
        let d = self.dark;
        PhysicalMaterial {
            color: Color::from_hex(if d { 0x2a2a40 } else { 0xc8c8d8 }),
            roughness: if d { 0.35 } else { 0.5 },
            metalness: if d { 0.3 } else { 0.1 },
            clearcoat: 0.6,
            clearcoat_roughness: 0.15,
            env_map_intensity: 1.2,
            emissive: Color::from_hex(if d { 0x818cf8 } else { 0x6366f1 }),
            emissive_intensity: if d { 0.08 } else { 0.03 },
            ..Default::default()
        }
    }

    /// Load the scalp model (`head.glb`) and give every mesh the head material.
    pub fn load_model(&mut self, glb: &[u8]) -> Result<(), String> {
        let loaded = GltfLoader::parse_glb(glb).map_err(|e| format!("head model: {e:?}"))?;
        let roots = add_to_scene(&mut self.view.scene, loaded);
        let root_id = self.view.id(self.root).ok_or("no root")?;
        for r in &roots {
            self.view.scene.arena.remove_child(*r);
            self.view.scene.arena.add_child(root_id, *r);
        }
        self.view.scene.update_world();

        let material = Arc::new(Material::Physical(self.head_material()));
        let mut meshes = Vec::new();
        for r in &roots {
            self.view.scene.arena.traverse(*r, &mut |id, o| {
                if matches!(o.kind, ObjectKind::Mesh(_)) {
                    meshes.push(id);
                }
            });
        }
        let mut triangles = Vec::new();
        let (mut lo, mut hi) = (
            Vector3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            Vector3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        );
        for id in &meshes {
            let Some(obj) = self.view.scene.get_mut(*id) else {
                continue;
            };
            let world = obj.matrix_world;
            if let ObjectKind::Mesh(m) = &mut obj.kind {
                m.material = Arc::clone(&material);
                let g = &m.geometry;
                let Some(pos) = g.get_attribute("position") else {
                    continue;
                };
                let p = |i: u32| {
                    let i = i as usize * 3;
                    Vector3::new(pos.array[i], pos.array[i + 1], pos.array[i + 2])
                };
                // The bounding box is the geometry's own (three.js
                // `geometry.boundingBox`), as the cranium centre was taken.
                for i in 0..pos.count() as u32 {
                    let v = p(i);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
                for t in 0..g.triangle_count() {
                    let [a, b, c] = g.triangle(t);
                    triangles.push([
                        p(a).apply_matrix4(&world),
                        p(b).apply_matrix4(&world),
                        p(c).apply_matrix4(&world),
                    ]);
                }
            }
        }
        self.head_meshes = meshes.into_iter().map(|id| self.view.adopt(id)).collect();
        // The middle of the upper part of the head, slightly back: the
        // model includes the neck, so its own centre is too low.
        let cranium = Vector3::new(0.0, lo.y + (hi.y - lo.y) * 0.6, lo.z + (hi.z - lo.z) * 0.4);
        self.scalp = Some(Scalp { triangles, cranium });
        self.rebuild();
        Ok(())
    }

    pub fn set_theme(&mut self, dark: bool) {
        if dark == self.dark {
            return;
        }
        self.dark = dark;
        let material = Arc::new(Material::Physical(self.head_material()));
        for h in self.head_meshes.clone() {
            if let Some(Object3D {
                kind: ObjectKind::Mesh(m),
                ..
            }) = self.view.object_mut(h)
            {
                m.material = Arc::clone(&material);
            }
        }
        self.build_lights();
        self.rebuild();
    }

    /// Where the electrode group sits and how it is scaled
    /// (`electrodePosition` / `electrodeScale`).
    pub fn set_electrode_transform(&mut self, position: Vector3, scale: Vector3) {
        self.view.set_position(self.electrodes_group, position);
        self.view.set_scale(self.electrodes_group, scale);
    }

    pub fn set_electrodes(&mut self, electrodes: Vec<Electrode>) {
        self.electrodes = electrodes;
        self.selected = None;
        self.rebuild();
    }

    pub fn select(&mut self, name: Option<&str>) {
        self.selected = name.and_then(|n| self.electrodes.iter().position(|e| e.name == n));
        self.rebuild();
    }

    fn snap(&self, mne: [f32; 3], lift: f32) -> Vector3 {
        let target = mne_to_world(mne);
        match &self.scalp {
            Some(s) => s.snap(target, lift),
            None => target,
        }
    }

    fn rebuild(&mut self) {
        for h in self.dots.iter_mut().filter_map(Option::take) {
            self.view.remove(h);
        }
        if let Some(h) = self.band.take() {
            self.view.remove(h);
        }
        if let Some(h) = self.ring.take() {
            self.view.remove(h);
        }
        self.placed = self
            .electrodes
            .iter()
            .map(|e| self.snap(e.pos, NORMAL_OFFSET))
            .collect();

        // Dots: one cloud per size, the selected one lightened towards white.
        let mut by_size: [(Vec<f32>, Vec<f32>); 3] = Default::default();
        for (i, e) in self.electrodes.iter().enumerate() {
            let tier = if Some(i) == self.selected {
                2
            } else if e.emphasised {
                1
            } else {
                0
            };
            let mut c = e.color;
            if tier == 2 {
                c = c.lerp(Color::WHITE, 0.4);
            }
            // The shader's `brightness`: 1.3 for anything larger than plain.
            if tier > 0 {
                c = c.scaled(1.3);
            }
            let p = self.placed[i];
            by_size[tier].0.extend_from_slice(&[p.x, p.y, p.z]);
            by_size[tier].1.extend_from_slice(&[c.r, c.g, c.b]);
        }
        let opacity = if self.dark { 0.95 } else { 0.85 };
        for (tier, (pos, col)) in by_size.iter().enumerate() {
            if pos.is_empty() {
                continue;
            }
            let look = Look {
                opacity,
                transparent: true,
                depth_write: Some(false),
                ..Look::default()
            };
            let h = self.view.add_points(
                self.electrodes_group,
                pos,
                Some(col),
                self.point_size(SIZES[tier]),
                true,
                Some(Arc::clone(&self.sprite)),
                look,
            );
            self.view.set_render_order(h, 2);
            self.dots[tier] = Some(h);
        }

        // The Muse band, once the scalp is there to follow.
        if self.scalp.is_some() {
            let at: Vec<Vector3> = MUSE_BAND
                .iter()
                .filter_map(|n| self.electrodes.iter().position(|e| e.name == *n))
                .map(|i| self.placed[i])
                .collect();
            if at.len() == 4 {
                let steps = 48;
                let mut pts = Vec::with_capacity(steps + 1);
                for i in 0..=steps {
                    let t = i as f32 / steps as f32;
                    let p = if t < 0.333 {
                        at[0].lerp(at[1], t / 0.333)
                    } else if t < 0.667 {
                        at[1].lerp(at[2], (t - 0.333) / 0.334)
                    } else {
                        at[2].lerp(at[3], (t - 0.667) / 0.333)
                    };
                    let p = match &self.scalp {
                        Some(s) => s.snap(p, BAND_OFFSET),
                        None => p,
                    };
                    pts.push(p);
                }
                let mut segs = Vec::with_capacity(steps * 6);
                for w in pts.windows(2) {
                    segs.extend_from_slice(&[w[0].x, w[0].y, w[0].z, w[1].x, w[1].y, w[1].z]);
                }
                let look = Look {
                    color: Color::from_hex(if self.dark { 0x818cf8 } else { 0x6366f1 }),
                    opacity: 0.7,
                    transparent: true,
                    ..Look::default()
                };
                self.band = Some(
                    self.view
                        .add_lines(self.electrodes_group, &segs, None, false, None, look),
                );
            }
        }

        // The selection ring, facing out of the scalp.
        if let Some(i) = self.selected {
            let p = self.placed[i];
            let cranium = self.scalp.as_ref().map_or(Vector3::ZERO, |s| s.cranium);
            let normal = (p - cranium).normalize();
            let look = Look {
                opacity: self.pulse(),
                transparent: true,
                depth_write: Some(false),
                side: 2,
                ..Look::default()
            };
            let h = self.view.add_mesh(
                self.electrodes_group,
                Shape::Ring {
                    inner: 0.18,
                    outer: 0.28,
                    segments: 24,
                },
                MeshMaterial::Basic,
                look,
            );
            self.view.set_position(h, p);
            self.view.look_at(h, p + normal);
            self.ring = Some(h);
        }
    }

    fn pulse(&self) -> f32 {
        0.6 + 0.4 * (self.clock * 4.0).sin()
    }

    /// three.js drew dots `aSize · 300 / −z` device pixels across; an
    /// attenuated `PointsMaterial` here is `size · (height / 2) / −z` with
    /// `height` the drawing buffer's (the renderer runs at pixel ratio 1), so
    /// match the two.
    fn point_size(&self, a_size: f32) -> f32 {
        a_size * 600.0 / self.buffer_height.max(1.0)
    }

    /// The drawing buffer's height in pixels changed.
    pub fn set_buffer_height(&mut self, height: f32) {
        self.buffer_height = height;
        for (tier, h) in self.dots.iter().enumerate() {
            if let Some(h) = *h {
                let s = self.point_size(SIZES[tier]);
                self.view.set_point_size(h, s);
            }
        }
    }

    /// Advance the orbit and the ring's pulse.
    pub fn tick(&mut self, dt: f32) {
        self.clock += dt;
        if let Some(h) = self.ring {
            let a = self.pulse();
            self.view.set_look(h, None, Some(a), None);
        }
        self.view.tick(dt);
    }

    /// Where electrode `i` was placed, in world space — for tests.
    #[doc(hidden)]
    pub fn placed_for_test(&mut self, i: usize) -> Vector3 {
        self.view.scene.update_world();
        let world = self
            .view
            .id(self.electrodes_group)
            .and_then(|g| self.view.scene.get(g))
            .map(|o| o.matrix_world)
            .unwrap_or_default();
        self.placed[i].apply_matrix4(&world)
    }

    /// The electrode nearest NDC `(x, y)` within 0.06, by index.
    pub fn hit_test(&mut self, x: f32, y: f32) -> Option<usize> {
        self.view.scene.update_world();
        let group = self.view.id(self.electrodes_group)?;
        let world = self.view.scene.get(group)?.matrix_world;
        let mut best = (0.06_f32, None);
        for (i, p) in self.placed.iter().enumerate() {
            let s = self.view.project(p.apply_matrix4(&world));
            let d = ((s.x - x).powi(2) + (s.y - y).powi(2)).sqrt();
            if d < best.0 {
                best = (d, Some(i));
            }
        }
        best.1
    }
}
