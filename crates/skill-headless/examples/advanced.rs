// SPDX-License-Identifier: GPL-3.0-only
//! Advanced tests — HTML rendering, screenshots, viewport sizes, WebGL, user-agent.
//!
//! On GTK/Linux, only one event loop can hold the main context at a time.
//! To work around this we run most tests on a single browser instance, resizing
//! the viewport as needed.  User-agent tests launch separate processes.

use skill_headless::{Browser, BrowserConfig, Command};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

/// Helper: inject an HTML page via document.write and wait for render.
fn load_html(browser: &Browser, html: &str) {
    let escaped = html
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n");
    browser
        .send(Command::EvalJsNoReturn {
            script: format!(
                "document.open(); document.write('{escaped}'); document.close();"
            ),
        })
        .expect("document.write failed");
    std::thread::sleep(Duration::from_millis(500));
}

/// Decode base64 (standard alphabet, with padding).
fn base64_decode(s: &str) -> Vec<u8> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    STANDARD.decode(s.trim()).unwrap_or_default()
}

fn main() {
    // If invoked with --ua-test, just print the navigator.userAgent and exit.
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "--ua-test" {
        let ua = args.get(2).map(String::as_str).unwrap_or("");
        let config = if ua.is_empty() {
            BrowserConfig {
                width: 200,
                height: 200,
                timeout: TIMEOUT,
                ..Default::default()
            }
        } else {
            BrowserConfig {
                width: 200,
                height: 200,
                user_agent: Some(ua.to_string()),
                timeout: TIMEOUT,
                ..Default::default()
            }
        };
        let b = Browser::launch(config).expect("launch");
        std::thread::sleep(Duration::from_millis(300));
        let resp = b
            .send(Command::EvalJs {
                script: "navigator.userAgent".into(),
            })
            .unwrap();
        println!("UA={}", resp.as_text().unwrap_or(""));
        let _ = b.send(Command::Close);
        return;
    }

    let mut passed = 0u32;
    let mut failed = 0u32;

    macro_rules! test {
        ($name:expr, $body:block) => {{
            print!("[TEST] {:<60}", $name);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)) {
                Ok(_) => {
                    println!(" PASS");
                    passed += 1;
                }
                Err(e) => {
                    let msg = e
                        .downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown panic");
                    println!(" FAIL: {msg}");
                    failed += 1;
                }
            }
        }};
    }

    println!("=== skill-headless advanced tests ===\n");

    let browser = Browser::launch(BrowserConfig {
        width: 1024,
        height: 768,
        timeout: TIMEOUT,
        ..Default::default()
    })
    .expect("launch failed");
    std::thread::sleep(Duration::from_millis(500));

    // ══════════════════════════════════════════════════════════════════════
    // 1. HTML RENDERING
    // ══════════════════════════════════════════════════════════════════════
    println!("── HTML Rendering ──────────────────────────────────────────\n");

    test!("Render styled HTML — background color", {
        load_html(
            &browser,
            r#"<!DOCTYPE html>
            <html><head><style>body { background-color: rgb(255, 0, 0); }</style></head>
            <body><div id="test">Red Background</div></body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "getComputedStyle(document.body).backgroundColor".into(),
            })
            .unwrap();
        let bg = resp.as_text().unwrap();
        assert!(
            bg.contains("255") && bg.contains("0"),
            "expected red bg, got: {bg}"
        );
    });

    test!("Render styled HTML — font size", {
        load_html(
            &browser,
            r#"<!DOCTYPE html>
            <html><head><style>#big { font-size: 48px; }</style></head>
            <body><div id="big">Big Text</div></body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "getComputedStyle(document.getElementById('big')).fontSize".into(),
            })
            .unwrap();
        assert_eq!(resp.as_text(), Some("48px"));
    });

    test!("Render styled HTML — flexbox layout", {
        load_html(
            &browser,
            r#"<!DOCTYPE html>
            <html><head><style>
                .flex { display: flex; width: 200px; }
                .child { flex: 1; }
            </style></head>
            <body><div class="flex"><div class="child" id="c1">A</div><div class="child" id="c2">B</div></div></body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "document.getElementById('c1').getBoundingClientRect().width".into(),
            })
            .unwrap();
        let w: f64 = resp.as_text().unwrap().parse().unwrap_or(0.0);
        assert!((w - 100.0).abs() < 2.0, "expected ~100px, got: {w}");
    });

    test!("Render styled HTML — CSS grid", {
        load_html(
            &browser,
            r#"<!DOCTYPE html>
            <html><head><style>
                .grid { display: grid; grid-template-columns: 1fr 2fr; width: 300px; }
            </style></head>
            <body><div class="grid"><div id="g1">1</div><div id="g2">2</div></div></body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "document.getElementById('g1').getBoundingClientRect().width".into(),
            })
            .unwrap();
        let w: f64 = resp.as_text().unwrap().parse().unwrap_or(0.0);
        assert!((w - 100.0).abs() < 2.0, "expected ~100px, got: {w}");
    });

    test!("Render styled HTML — visibility hidden", {
        load_html(
            &browser,
            r#"<!DOCTYPE html><html><body>
            <div id="vis" style="visibility: hidden;">Hidden</div>
            </body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "getComputedStyle(document.getElementById('vis')).visibility".into(),
            })
            .unwrap();
        assert_eq!(resp.as_text(), Some("hidden"));
    });

    test!("Render styled HTML — display none", {
        load_html(
            &browser,
            r#"<!DOCTYPE html><html><body>
            <div id="none" style="display: none;">Gone</div>
            </body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "getComputedStyle(document.getElementById('none')).display".into(),
            })
            .unwrap();
        assert_eq!(resp.as_text(), Some("none"));
    });

    test!("Render styled HTML — absolute positioning", {
        load_html(
            &browser,
            r#"<!DOCTYPE html><html><body style="margin:0;padding:0;">
            <div id="pos" style="position:absolute;top:50px;left:75px;width:10px;height:10px;">X</div>
            </body></html>"#,
        );
        let resp = browser
            .send(Command::EvalJs {
                script: "JSON.stringify(document.getElementById('pos').getBoundingClientRect())"
                    .into(),
            })
            .unwrap();
        let rect: serde_json::Value =
            serde_json::from_str(resp.as_text().unwrap()).unwrap();
        let top = rect["top"].as_f64().unwrap();
        let left = rect["left"].as_f64().unwrap();
        assert!((top - 50.0).abs() < 1.0, "top={top}");
        assert!((left - 75.0).abs() < 1.0, "left={left}");
    });

    // ══════════════════════════════════════════════════════════════════════
    // 2. SCREENSHOTS
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Screenshots ─────────────────────────────────────────────\n");

    browser
        .send(Command::SetViewport {
            width: 320,
            height: 240,
        })
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    test!("Screenshot — basic PNG capture", {
        load_html(
            &browser,
            r#"<!DOCTYPE html>
            <html><head><style>body{margin:0;background:blue;}</style></head>
            <body><div style="width:320px;height:240px;background:blue;"></div></body></html>"#,
        );
        let resp = browser.send(Command::Screenshot).unwrap();
        let text = resp.as_text().unwrap_or("");
        assert!(
            text.starts_with("data:image/png;base64,"),
            "expected PNG data URL, got: {}",
            &text[..text.len().min(60)]
        );
        let bytes =
            base64_decode(text.strip_prefix("data:image/png;base64,").unwrap());
        assert!(bytes.len() > 100, "PNG too small: {} bytes", bytes.len());
        assert_eq!(&bytes[0..4], &[0x89, 0x50, 0x4E, 0x47], "invalid PNG");
        println!("  ({} bytes)", bytes.len());
    });

    test!("Screenshot — different colors produce different images", {
        load_html(
            &browser,
            r#"<!DOCTYPE html><html><head><style>body{margin:0;background:rgb(0,0,255);}</style></head><body></body></html>"#,
        );
        let r1 = browser.send(Command::Screenshot).unwrap();
        let s1 = r1.as_text().unwrap().to_string();

        load_html(
            &browser,
            r#"<!DOCTYPE html><html><head><style>body{margin:0;background:rgb(255,0,0);}</style></head><body></body></html>"#,
        );
        let r2 = browser.send(Command::Screenshot).unwrap();
        let s2 = r2.as_text().unwrap().to_string();

        assert_ne!(s1, s2, "blue and red screenshots should differ");
    });

    test!("Screenshot — captures styled text", {
        load_html(
            &browser,
            r#"<!DOCTYPE html><html><head><style>body{margin:0;background:white;color:black;font-size:40px;}</style></head>
            <body><p>HELLO SCREENSHOT</p></body></html>"#,
        );
        let resp = browser.send(Command::Screenshot).unwrap();
        let text = resp.as_text().unwrap_or("");
        assert!(text.starts_with("data:image/png;base64,"));
        let bytes =
            base64_decode(text.strip_prefix("data:image/png;base64,").unwrap());
        assert!(bytes.len() > 200, "too small: {} bytes", bytes.len());
    });

    // ══════════════════════════════════════════════════════════════════════
    // 3. VIEWPORT SIZES (via SetViewport on same browser)
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Viewport Sizes ──────────────────────────────────────────\n");

    let sizes: &[(u32, u32)] = &[
        (320, 240),
        (375, 667),
        (768, 1024),
        (1024, 768),
        (1280, 720),
        (1920, 1080),
    ];

    for &(w, h) in sizes {
        test!(&format!("Viewport {w}x{h}"), {
            browser
                .send(Command::SetViewport {
                    width: w,
                    height: h,
                })
                .unwrap();
            std::thread::sleep(Duration::from_millis(300));

            let resp = browser
                .send(Command::EvalJs {
                    script: "window.innerWidth + 'x' + window.innerHeight".into(),
                })
                .unwrap();
            let size = resp.as_text().unwrap();
            let parts: Vec<&str> = size.split('x').collect();
            let iw: u32 = parts[0].parse().unwrap_or(0);
            let ih: u32 = parts[1].parse().unwrap_or(0);
            assert!(
                (iw as i64 - w as i64).unsigned_abs() <= 30
                    && (ih as i64 - h as i64).unsigned_abs() <= 30,
                "expected ~{w}x{h}, got {iw}x{ih}"
            );
        });
    }

    test!("SetViewport — resize changes 100%-width element", {
        load_html(
            &browser,
            r#"<!DOCTYPE html><html><head><style>body{margin:0;}#box{width:100%;height:50px;background:green;}</style></head>
            <body><div id="box"></div></body></html>"#,
        );

        browser
            .send(Command::SetViewport {
                width: 1000,
                height: 600,
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let resp = browser
            .send(Command::EvalJs {
                script: "document.getElementById('box').getBoundingClientRect().width".into(),
            })
            .unwrap();
        let w1: f64 = resp.as_text().unwrap().parse().unwrap_or(0.0);

        browser
            .send(Command::SetViewport {
                width: 500,
                height: 600,
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let resp = browser
            .send(Command::EvalJs {
                script: "document.getElementById('box').getBoundingClientRect().width".into(),
            })
            .unwrap();
        let w2: f64 = resp.as_text().unwrap().parse().unwrap_or(0.0);

        assert!((w1 - 1000.0).abs() < 30.0, "first: {w1}");
        assert!((w2 - 500.0).abs() < 30.0, "second: {w2}");
        assert!(w1 > w2, "box should shrink on resize");
    });

    // ══════════════════════════════════════════════════════════════════════
    // 4. WEBGL
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── WebGL ───────────────────────────────────────────────────\n");

    browser
        .send(Command::SetViewport {
            width: 640,
            height: 480,
        })
        .unwrap();

    test!("WebGL — context creation", {
        let resp = browser
            .send(Command::EvalJs {
                script: r#"(() => {
                    const c = document.createElement('canvas');
                    const gl = c.getContext('webgl');
                    return gl ? 'ok' : 'null';
                })()"#
                    .into(),
            })
            .unwrap();
        println!("  (webgl: {})", resp.as_text().unwrap_or("?"));
    });

    test!("WebGL2 — context creation", {
        let resp = browser
            .send(Command::EvalJs {
                script: r#"(() => {
                    const c = document.createElement('canvas');
                    const gl = c.getContext('webgl2');
                    return gl ? 'ok' : 'null';
                })()"#
                    .into(),
            })
            .unwrap();
        println!("  (webgl2: {})", resp.as_text().unwrap_or("?"));
    });

    test!("WebGL — renderer string", {
        let resp = browser
            .send(Command::EvalJs {
                script: r#"(() => {
                    const c = document.createElement('canvas');
                    const gl = c.getContext('webgl');
                    if (!gl) return 'no webgl';
                    const d = gl.getExtension('WEBGL_debug_renderer_info');
                    return d ? gl.getParameter(d.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
                })()"#
                    .into(),
            })
            .unwrap();
        println!("  (renderer: {})", resp.as_text().unwrap_or("?"));
    });

    test!("WebGL — draw + readPixels", {
        let resp = browser
            .send(Command::EvalJs {
                script: r#"(() => {
                    const c = document.createElement('canvas');
                    c.width=2; c.height=2;
                    const gl = c.getContext('webgl', {preserveDrawingBuffer:true});
                    if (!gl) return 'no_webgl';
                    gl.clearColor(0,1,0,1);
                    gl.clear(gl.COLOR_BUFFER_BIT);
                    const px = new Uint8Array(4);
                    gl.readPixels(0,0,1,1,gl.RGBA,gl.UNSIGNED_BYTE,px);
                    return px[0]+','+px[1]+','+px[2]+','+px[3];
                })()"#
                    .into(),
            })
            .unwrap();
        let t = resp.as_text().unwrap();
        if t == "no_webgl" {
            println!("  (skipped — sw renderer)");
        } else {
            assert_eq!(t, "0,255,0,255", "expected green, got {t}");
        }
    });

    test!("WebGL — shader compile + link", {
        let resp = browser.send(Command::EvalJs {
            script: r#"(() => {
                const c = document.createElement('canvas');
                const gl = c.getContext('webgl');
                if (!gl) return 'no_webgl';
                const vs = gl.createShader(gl.VERTEX_SHADER);
                gl.shaderSource(vs, 'attribute vec4 p; void main(){gl_Position=p;}');
                gl.compileShader(vs);
                const fs = gl.createShader(gl.FRAGMENT_SHADER);
                gl.shaderSource(fs, 'precision mediump float; void main(){gl_FragColor=vec4(1,0,0,1);}');
                gl.compileShader(fs);
                const prog = gl.createProgram();
                gl.attachShader(prog, vs); gl.attachShader(prog, fs);
                gl.linkProgram(prog);
                return gl.getShaderParameter(vs,gl.COMPILE_STATUS)+','+
                       gl.getShaderParameter(fs,gl.COMPILE_STATUS)+','+
                       gl.getProgramParameter(prog,gl.LINK_STATUS);
            })()"#.into(),
        }).unwrap();
        let t = resp.as_text().unwrap();
        if t == "no_webgl" {
            println!("  (skipped)");
        } else {
            assert_eq!(t, "true,true,true", "shader fail: {t}");
        }
    });

    test!("Canvas 2D — draw + getImageData", {
        let resp = browser
            .send(Command::EvalJs {
                script: r#"(() => {
                    const c = document.createElement('canvas');
                    c.width=10; c.height=10;
                    const ctx = c.getContext('2d');
                    ctx.fillStyle='rgb(0,128,255)';
                    ctx.fillRect(0,0,10,10);
                    const px = ctx.getImageData(5,5,1,1).data;
                    return px[0]+','+px[1]+','+px[2]+','+px[3];
                })()"#
                    .into(),
            })
            .unwrap();
        assert_eq!(resp.as_text(), Some("0,128,255,255"));
    });

    // ══════════════════════════════════════════════════════════════════════
    // 5. SCREENSHOTS AT DIFFERENT SIZES
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── Screenshots at Various Sizes ────────────────────────────\n");

    let ss_sizes: &[(u32, u32)] = &[(320, 240), (800, 600), (1920, 1080)];
    for &(w, h) in ss_sizes {
        test!(&format!("Screenshot at {w}x{h}"), {
            browser
                .send(Command::SetViewport {
                    width: w,
                    height: h,
                })
                .unwrap();
            std::thread::sleep(Duration::from_millis(300));

            load_html(
                &browser,
                &format!(
                    r#"<!DOCTYPE html><html><head><style>body{{margin:0;background:linear-gradient(to right,red,blue);}}.b{{width:{w}px;height:{h}px;}}</style></head><body><div class="b"></div></body></html>"#
                ),
            );

            let resp = browser.send(Command::Screenshot).unwrap();
            let text = resp.as_text().unwrap_or("");
            assert!(
                text.starts_with("data:image/png;base64,"),
                "no PNG at {w}x{h}"
            );
            let bytes =
                base64_decode(text.strip_prefix("data:image/png;base64,").unwrap());
            assert!(bytes.len() > 100, "too small: {} bytes", bytes.len());
            assert_eq!(&bytes[0..4], &[0x89, 0x50, 0x4E, 0x47]);
            println!("  ({} bytes)", bytes.len());
        });
    }

    // ══════════════════════════════════════════════════════════════════════
    // 6. USER AGENT (via subprocess to avoid GTK context issue)
    // ══════════════════════════════════════════════════════════════════════
    println!("\n── User Agent ──────────────────────────────────────────────\n");

    // Close the main browser first so the subprocess can use GTK.
    let _ = browser.send(Command::Close);
    drop(browser);
    std::thread::sleep(Duration::from_millis(800));

    let exe = std::env::current_exe().expect("current_exe");

    test!("Custom user-agent — SkillHeadlessBot/1.0", {
        let out = std::process::Command::new(&exe)
            .args(["--ua-test", "SkillHeadlessBot/1.0 (Test Suite)"])
            .output()
            .expect("subprocess failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let ua_line = stdout.lines().find(|l| l.starts_with("UA=")).unwrap_or("");
        let ua = ua_line.strip_prefix("UA=").unwrap_or("");
        println!("  (UA: {ua})");
        assert!(
            ua.contains("SkillHeadlessBot/1.0"),
            "expected custom UA in: {ua}"
        );
    });

    test!("Default user-agent — contains WebKit/Mozilla", {
        let out = std::process::Command::new(&exe)
            .args(["--ua-test", ""])
            .output()
            .expect("subprocess failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let ua_line = stdout.lines().find(|l| l.starts_with("UA=")).unwrap_or("");
        let ua = ua_line.strip_prefix("UA=").unwrap_or("");
        println!("  (UA: {ua})");
        assert!(
            ua.contains("WebKit") || ua.contains("Mozilla") || ua.contains("Chrome"),
            "UA doesn't look like a browser: {ua}"
        );
    });

    test!("Custom user-agent — exact match", {
        let out = std::process::Command::new(&exe)
            .args(["--ua-test", "MyAgent/2.5 (NeuroSkill; rv:42)"])
            .output()
            .expect("subprocess failed");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let ua_line = stdout.lines().find(|l| l.starts_with("UA=")).unwrap_or("");
        let ua = ua_line.strip_prefix("UA=").unwrap_or("");
        assert!(ua.contains("MyAgent/2.5"), "missing UA: {ua}");
        assert!(ua.contains("NeuroSkill"), "missing details: {ua}");
    });

    // ══════════════════════════════════════════════════════════════════════
    // SUMMARY
    // ══════════════════════════════════════════════════════════════════════
    println!("\n================================================================");
    println!("  PASSED: {passed}");
    println!("  FAILED: {failed}");
    println!("================================================================");

    if failed > 0 {
        std::process::exit(1);
    }
}
