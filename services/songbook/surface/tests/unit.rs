#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

use std::cell::RefCell;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use quench_android_surface::Bridge;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;
use songbook_native::Auth;
use songbook_native::store::Store;
use songbook_surface::{app, pictures, pool::Pool, signin};

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("songbook-surface-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[quench_test::test]
fn a_picture_is_kept_under_a_name_made_from_its_address_and_the_oldest_go_first() {
    let dir = scratch("pictures");
    let a = pictures::file_for(&dir, "/songbook/api/v1/art/song/1?w=320");
    assert_eq!(
        a,
        pictures::file_for(&dir, "/songbook/api/v1/art/song/1?w=320")
    );
    assert_ne!(
        a,
        pictures::file_for(&dir, "/songbook/api/v1/art/song/2?w=320")
    );
    for (n, src) in ["a", "b", "c"].iter().enumerate() {
        pictures::keep(&dir, src, &[0u8; 100]);
        // Modification times differ by the second on some systems: set them apart.
        let file = pictures::file_for(&dir, src);
        let when = std::time::SystemTime::now() - Duration::from_secs(100 - n as u64 * 10);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }
    pictures::trim(&dir, 250);
    assert!(!pictures::file_for(&dir, "a").exists(), "the oldest went");
    assert!(pictures::file_for(&dir, "b").exists());
    assert!(pictures::file_for(&dir, "c").exists());
    let mut wanted = pictures::Wanted::default();
    assert!(wanted.first("x"));
    assert!(!wanted.first("x"), "already asked for");
    wanted.done("x");
    assert!(wanted.first("x"));
}

#[quench_test::test]
fn the_pool_runs_every_job() {
    let pool = Pool::new(3);
    let count = Arc::new(AtomicUsize::new(0));
    for _ in 0..40 {
        let count = Arc::clone(&count);
        pool.run(move || {
            std::thread::sleep(Duration::from_millis(1));
            count.fetch_add(1, Ordering::SeqCst);
        });
    }
    for _ in 0..200 {
        if count.load(Ordering::SeqCst) == 40 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(count.load(Ordering::SeqCst), 40);
}

fn find(r: &TestRenderer, id: &str) -> NodeId {
    r.document_order()
        .into_iter()
        .find(|n| r.attr(*n, "id").as_deref() == Some(id))
        .unwrap_or_else(|| panic!("no #{id}: {}", r.html()))
}

use quench_ui::test_renderer::NodeId;

#[quench_test::test]
fn the_sign_in_hands_over_what_was_typed_and_not_twice_while_it_works() {
    let (view, _scope) = create_root(|| {
        let tried: Rc<RefCell<Vec<(String, String, String)>>> = Rc::default();
        let log = Rc::clone(&tried);
        let error = create_signal("wrong".to_string());
        let busy = create_signal(false);
        let busy_for_form = busy.clone();
        let form = signin::form(
            "https://h",
            error,
            &busy_for_form,
            Rc::new(move |s, u, p| {
                log.borrow_mut().push((s, u, p));
                busy.set(true);
            }),
        );
        (form, tried)
    });
    let (view, tried) = view;
    let r = Rc::new(TestRenderer::new());
    let _m = mount(&r, &r.root(), view);
    assert_eq!(r.text_of(find(&r, "signin-error")), "wrong");
    r.fire(find(&r, "signin-user"), &Event::Input(" ann ".into()));
    r.fire(find(&r, "signin-password"), &Event::Input("secret".into()));
    r.fire(find(&r, "signin-go"), &Event::Click);
    r.fire(find(&r, "signin-go"), &Event::Click);
    assert_eq!(
        *tried.borrow(),
        [(
            "https://h".to_string(),
            "ann".to_string(),
            "secret".to_string()
        )]
    );
}

// ── The app over a stand-in service ──────────────────────────────────────────

#[derive(Default)]
struct World {
    seen: Vec<(String, String)>,
}

fn serve(world: Arc<Mutex<World>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let w = Arc::clone(&world);
            std::thread::spawn(move || answer(stream, &w));
        }
    });
    base
}

fn answer(mut stream: TcpStream, world: &Mutex<World>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let target = line.split_whitespace().nth(1).unwrap().to_string();
    let (mut length, mut auth) = (0usize, String::new());
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).unwrap();
        if h.trim().is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap();
        match k.to_ascii_lowercase().as_str() {
            "content-length" => length = v.trim().parse().unwrap(),
            "authorization" => auth = v.trim().to_string(),
            _ => {}
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).unwrap();
    world
        .lock()
        .unwrap()
        .seen
        .push((target.clone(), auth.clone()));
    let (status, text) = if target.ends_with("/auth/login") {
        (200, r#"{"access_token":"access-1","refresh_token":"refresh-1","token_type":"Bearer","expires_in":900}"#.to_string())
    } else if auth != "Bearer access-1" {
        (401, String::new())
    } else if target.contains("/library/overview") {
        (
            200,
            r#"{"songs":0,"albums":0,"artists":[],"categories":[]}"#.to_string(),
        )
    } else if target.contains("/playlists") {
        (200, "[]".to_string())
    } else {
        (404, r#"{"error":"nothing here"}"#.to_string())
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
        text.len()
    );
}

/// Lets the program's loop run: timers, and what the pool brought back.
fn spin(r: &NativeRenderer, until: impl Fn() -> bool) {
    for _ in 0..300 {
        r.tick();
        if until() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out");
}

/// A touch on the element: found by asking what is at each point of the screen.
fn tap(r: &NativeRenderer, node: quench_ui_native::NodeId) {
    fn inside(
        r: &NativeRenderer,
        n: quench_ui_native::NodeId,
        out: &mut Vec<quench_ui_native::NodeId>,
    ) {
        out.push(n);
        for c in r.children(n) {
            inside(r, c, out);
        }
    }
    let mut all = Vec::new();
    inside(r, node, &mut all);
    let mut canvas = quench_raster::Canvas::new(420, 800);
    r.render(&mut canvas);
    for y in (0..800).step_by(4) {
        for x in (0..420).step_by(4) {
            if r.element_at(x as f32, y as f32)
                .is_some_and(|n| all.contains(&n))
            {
                r.pointer_down(x as f32 + 2.0, y as f32 + 2.0);
                r.pointer_up(x as f32 + 2.0, y as f32 + 2.0);
                return;
            }
        }
    }
    panic!("not on the screen");
}

fn has(r: &NativeRenderer, id: &str) -> bool {
    fn walk(r: &NativeRenderer, n: quench_ui_native::NodeId, id: &str) -> bool {
        r.attr(n, "id").as_deref() == Some(id) || r.children(n).into_iter().any(|c| walk(r, c, id))
    }
    walk(r, r.root(), id)
}

#[quench_test::test]
fn nobody_signed_in_sees_the_sign_in_and_signing_in_shows_the_player_which_asks_the_service() {
    let world = Arc::new(Mutex::new(World::default()));
    let base = serve(Arc::clone(&world));
    let dir = scratch("app");
    let store = Store::new(&dir);
    let renderer = Rc::new(NativeRenderer::new());
    songbook_surface::add_styles(&renderer);
    renderer.set_viewport(420.0, 800.0);
    let bridge = Bridge::new();
    let app = app::build(
        &renderer,
        &bridge,
        Arc::new(Auth::new(store.clone())),
        &store,
    );
    let back = app.back();
    let _m = mount(&renderer, &renderer.root(), app.view);
    assert!(has(&renderer, "signin"));
    assert!(!has(&renderer, "player"));
    // With nothing to go back to, Back leaves the program to the host.
    assert!(!back());

    // Typing into the fields and pressing the button.
    let find_node = |id: &str| {
        let mut stack = vec![renderer.root()];
        while let Some(n) = stack.pop() {
            if renderer.attr(n, "id").as_deref() == Some(id) {
                return n;
            }
            stack.extend(renderer.children(n));
        }
        panic!("no #{id}")
    };
    assert!(renderer.set_field_value(find_node("signin-server").index(), &base));
    assert!(renderer.set_field_value(find_node("signin-user").index(), "ann"));
    assert!(renderer.set_field_value(find_node("signin-password").index(), "secret"));
    tap(&renderer, find_node("signin-go"));
    spin(&renderer, || has(&renderer, "player"));
    assert!(!has(&renderer, "signin"));
    // The player asked for the library, with the token.
    spin(&renderer, || {
        world
            .lock()
            .unwrap()
            .seen
            .iter()
            .any(|(t, a)| t.contains("/songbook/api/v1/library/overview") && a == "Bearer access-1")
    });
    assert!(
        store.token().is_some(),
        "the session is kept for the next run"
    );
}

quench_test::main!();
