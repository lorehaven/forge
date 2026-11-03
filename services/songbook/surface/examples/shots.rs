#![allow(
    clippy::cast_precision_loss,
    clippy::unwrap_used,
    clippy::too_many_lines
)]
//! Draws the player's screens offscreen, with sample songs, to PNG files: `cargo run -p songbook-surface --example shots -- <dir>`.
//! Nothing is opened and no service is contacted.

use std::rc::Rc;

use quench_raster::Canvas;
use quench_ui::prelude::*;
use quench_ui::router::MemoryHistory;
use quench_ui_native::NativeRenderer;
use songbook_core::{AlbumInfo, ArtistInfo, CategoryInfo, Home, Mix, Overview, SongInfo, SongList};
use songbook_player::{Event as EngineEvent, Msg, Player, Setup, player};

fn song(id: i64, title: &str, artists: &[&str], album: &str) -> SongInfo {
    SongInfo {
        id,
        title: title.into(),
        artists: artists.iter().map(ToString::to_string).collect(),
        album: album.into(),
        album_id: 7,
        year: "1999".into(),
        duration: 187,
    }
}

fn popular() -> Vec<SongInfo> {
    vec![
        song(
            1,
            "Nobody Does It Better",
            &["Nate Dogg"],
            "Ghetto Preacher",
        ),
        song(
            2,
            "Hardest Man in Town",
            &["Nate Dogg", "Snoop Dogg"],
            "Ghetto Preacher",
        ),
        song(3, "Puppy Love", &["Nate Dogg"], "Ghetto Preacher"),
        song(4, "Stan", &["Eminem", "Dido"], "The Marshall Mathers LP"),
    ]
}

fn save(r: &NativeRenderer, canvas: &mut Canvas, dir: &str, name: &str) {
    r.render(canvas);
    let mut rgba = canvas.bytes().to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    let png = quench_image::encode_png(
        u32::try_from(canvas.width()).unwrap(),
        u32::try_from(canvas.height()).unwrap(),
        &rgba,
    );
    std::fs::write(format!("{dir}/{name}.png"), png).unwrap();
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("a directory for the PNG files");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, width, height) in [("phone", 420usize, 860usize), ("wide", 1000, 700)] {
        let renderer = Rc::new(NativeRenderer::new());
        songbook_surface::add_styles(&renderer);
        if let Ok(extra) = std::env::var("SHOTS_CSS") {
            renderer.add_stylesheet(&extra);
        }
        renderer.set_viewport(width as f32, height as f32);
        let commands: Rc<std::cell::RefCell<Vec<String>>> = Rc::default();
        let log = Rc::clone(&commands);
        let Player {
            view,
            deliver,
            hear,
            ..
        } = player(
            Rc::new(|_| {}),
            Setup {
                base: "/songbook".into(),
                history: Rc::new(MemoryHistory::new("/songbook/ui")),
                login_url: String::new(),
                mobile: true,
                engine: Some(Rc::new(move |c| log.borrow_mut().push(c))),
                sign_out: None,
            },
        );
        let _mounted = mount(&renderer, &renderer.root(), view);
        deliver(Msg::Overview(Overview {
            admin: true,
            songs: 4,
            albums: 2,
            artists: vec![
                ArtistInfo {
                    name: "Nate Dogg".into(),
                    songs: 3,
                    albums: 1,
                },
                ArtistInfo {
                    name: "Eminem".into(),
                    songs: 1,
                    albums: 1,
                },
            ],
            categories: vec![CategoryInfo {
                name: "American Rap".into(),
                songs: 4,
            }],
        }));
        deliver(Msg::Home(Home {
            popular: popular(),
            mixes: vec![Mix {
                name: "American Rap Mix".into(),
                kind: "mix".into(),
                category: "American Rap".into(),
                songs: 50,
            }],
            artists: vec![ArtistInfo {
                name: "Nate Dogg".into(),
                songs: 3,
                albums: 1,
            }],
            discover: popular(),
            albums: vec![AlbumInfo {
                id: 7,
                title: "Ghetto Preacher".into(),
                artist: "Nate Dogg".into(),
                year: "1999-05-01".into(),
                kind: "album".into(),
                songs: 3,
            }],
        }));
        let mut canvas = Canvas::new(width, height);
        save(&renderer, &mut canvas, &dir, &format!("{name}-home"));
        deliver(Msg::Songs(
            "meta".into(),
            SongList {
                total: 4,
                songs: popular(),
            },
        ));
        hear(EngineEvent::parse(r#"{"ev":"state","playing":true,"id":2,"index":1,"len":4,"pos":61.0,"dur":187.0,"shuffle":false,"repeat":"off","fade":0.0,"vol":1.0,"loading":false}"#).unwrap());
        save(&renderer, &mut canvas, &dir, &format!("{name}-playing"));
    }
}
