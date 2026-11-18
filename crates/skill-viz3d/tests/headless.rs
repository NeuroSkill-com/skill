//! Render each view headless and write it to `target/viz3d-shots/*.png`, so a
//! change can be looked at without the app — and check the numbers the views
//! depend on (electrodes on the scalp, hit-testing, picking).

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use skill_viz3d::head::{Electrode, HeadView};
use skill_viz3d::threers::math::Color;
use skill_viz3d::threers::{HeadlessRenderer, ToneMapping};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn shot(
    name: &str,
    width: u32,
    height: u32,
    tone: ToneMapping,
    f: impl FnOnce(&mut HeadlessRenderer) -> Vec<u8>,
) -> Option<Vec<u8>> {
    let mut r = match HeadlessRenderer::builder().size(width, height).build() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("skipping {name}: no GPU ({e})");
            return None;
        }
    };
    r.set_msaa(4);
    r.renderer().set_tone_mapping(tone, 1.0);
    let rgba = f(&mut r);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/viz3d-shots");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(format!("{name}.png")),
        skill_viz3d::threers::encode_png(width, height, &rgba),
    )
    .unwrap();
    Some(rgba)
}

fn hex(s: &str) -> Color {
    Color::from_hex(u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap())
}

/// The 10-20 set, as `getElectrodes("10-20")` gives it, coloured by region
/// the way `regionColors` would (near enough for a picture).
fn electrodes_1020() -> Vec<Electrode> {
    let json = std::fs::read_to_string(repo().join("src/lib/data/electrode-positions.json")).unwrap();
    let mut out = Vec::new();
    // A minimal reader: `"NAME": {"pos": [x, y, z], "systems": [...]}`.
    for entry in json.split("},") {
        let Some(name) = entry.split('"').nth(1) else { continue };
        if !entry.contains("\"10-20\"") {
            continue;
        }
        let Some(p) = entry.split('[').nth(1).and_then(|s| s.split(']').next()) else {
            continue;
        };
        let v: Vec<f32> = p.split(',').map(|x| x.trim().parse().unwrap()).collect();
        let color = match name.chars().next() {
            Some('F') => hex("#3b82f6"),
            Some('C') => hex("#a855f7"),
            Some('T') => hex("#f59e0b"),
            Some('P') => hex("#ec4899"),
            Some('O') => hex("#ef4444"),
            _ => hex("#6b7280"),
        };
        let emphasised = ["TP9", "AF7", "AF8", "TP10"].contains(&name);
        out.push(Electrode {
            name: name.to_string(),
            pos: [v[0], v[1], v[2]],
            color,
            emphasised,
        });
    }
    out
}

fn head(dark: bool) -> HeadView {
    let glb = std::fs::read(repo().join("static/models/head.glb")).unwrap();
    let mut v = HeadView::new(dark);
    v.view.set_viewport(480.0, 360.0);
    v.set_buffer_height(720.0);
    v.load_model(&glb).unwrap();
    v.set_electrodes(electrodes_1020());
    v
}

#[test]
fn head_renders_with_electrodes_on_the_scalp() {
    let mut v = head(true);
    v.select(Some("Cz"));
    v.tick(0.0);
    let Some(rgba) = shot("head-dark", 960, 720, ToneMapping::AgX, |r| {
        r.render_to_rgba(&mut v.view.scene, &v.view.camera)
    }) else {
        return;
    };
    // Something was drawn: a transparent background, an opaque head.
    let opaque = rgba.as_chunks::<4>().0.iter().filter(|p| p[3] > 200).count();
    let frac = opaque as f32 / (960.0 * 720.0);
    assert!(frac > 0.10 && frac < 0.80, "head covers {frac:.2} of the frame");

    let mut light = head(false);
    light.tick(0.0);
    shot("head-light", 960, 720, ToneMapping::AgX, |r| {
        r.render_to_rgba(&mut light.view.scene, &light.view.camera)
    });
}

#[test]
fn hit_test_finds_the_electrode_under_the_cursor() {
    let mut v = head(true);
    v.tick(0.0);
    let names = electrodes_1020();
    let cz = names.iter().position(|e| e.name == "Cz").unwrap();
    // Cz is on top of the head; project it and click there.
    let p = v.placed_for_test(cz);
    let s = v.view.project(p);
    assert_eq!(v.hit_test(s.x, s.y), Some(cz));
    assert_eq!(v.hit_test(0.99, -0.99), None);
}

#[test]
fn pick_finds_meshes_and_points_by_handle() {
    use skill_viz3d::threers::math::Vector3;
    use skill_viz3d::view::{Look, MeshMaterial, Shape, View};

    let mut v = View::new(55.0, 0.1, 500.0);
    v.set_viewport(800.0, 600.0);
    v.set_camera_position(Vector3::new(0.0, 0.0, 10.0));
    v.set_target(Vector3::ZERO);
    let group = v.add_group(0);
    let ball = v.add_mesh(
        group,
        Shape::Sphere {
            radius: 1.0,
            width_segments: 24,
            height_segments: 16,
        },
        MeshMaterial::Basic,
        Look::default(),
    );
    let cloud = v.add_points(
        0,
        &[3.0, 0.0, 0.0, -3.0, 0.0, 0.0],
        None,
        1.0,
        true,
        None,
        Look::default(),
    );

    // The ball is dead centre.
    let hit = v.pick(0.0, 0.0, &[ball], 0.5).expect("ball under the centre");
    assert_eq!(hit.handle, ball);
    assert!(
        (hit.distance - 9.0).abs() < 0.05,
        "front of the ball is 9 away: {}",
        hit.distance
    );
    // Asked by its group, the hit is still the ball, as three.js's
    // `intersectObject(group, true)[0].object` is.
    assert_eq!(v.pick(0.0, 0.0, &[group], 0.5).map(|p| p.handle), Some(ball));

    // The second point, by index, where it projects.
    let s = v.project(Vector3::new(-3.0, 0.0, 0.0));
    let hit = v.pick(s.x, s.y, &[cloud], 0.5).expect("point under its projection");
    assert_eq!((hit.handle, hit.index), (cloud, 1));

    assert_eq!(v.pick(0.95, 0.95, &[ball, cloud], 0.5), None);

    // Removing the group removes the ball with it.
    v.remove(group);
    assert_eq!(v.pick(0.0, 0.0, &[ball], 0.5), None);
}
