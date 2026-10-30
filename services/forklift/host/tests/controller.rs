//! The controller against a fake device: what it asks the platform for, and what it does with the answers.
#![allow(clippy::unwrap_used, clippy::type_complexity)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use forklift_app::live::Msg;
use forklift_core::Remote;
use forklift_core::{Found, LARGE_FILE_BYTES, Settings, Status};
use forklift_host::{
    Controller, Device, MEDIA_PAGE, Node, PUBLISH_EVERY_MS, Platform, Target, UploadEvent,
};
use forklift_net::{Change, NetError, Storage, Store};

#[derive(Default)]
struct Fake {
    images: RefCell<Vec<Found>>,
    video: RefCell<Vec<Found>>,
    folders: RefCell<BTreeMap<String, Vec<Node>>>,
    device: Cell<Option<Device>>,
    token: RefCell<Option<Result<String, NetError>>>,
    picked: RefCell<Option<(String, String)>>,
    uploads: RefCell<Vec<(String, String, String, String)>>,
    tokens_asked: RefCell<Vec<bool>>,
    cancelled: RefCell<Vec<String>>,
    feed: RefCell<Vec<Change>>,
    feed_asked: RefCell<Vec<i64>>,
    stored: RefCell<Vec<Remote>>,
    quota: Cell<Option<(u64, u64)>>,
    downloads: RefCell<Vec<(String, String, String, String, &'static str, String)>>,
    cancelled_downloads: RefCell<Vec<String>>,
    storages: RefCell<Vec<Storage>>,
    now: Cell<i64>,
    later: RefCell<Vec<(u64, Box<dyn FnOnce()>)>>,
    media_calls: RefCell<Vec<(&'static str, u32)>>,
    /// When set, a page of the media library is not answered until the test says so.
    hold_media: Cell<bool>,
    held: RefCell<Vec<Box<dyn FnOnce()>>>,
}

impl Platform for Fake {
    fn media(
        &self,
        kind: &'static str,
        offset: u32,
        then: Box<dyn FnOnce(Result<Vec<Found>, String>)>,
    ) {
        self.media_calls.borrow_mut().push((kind, offset));
        let all = if kind == "video" {
            self.video.borrow().clone()
        } else {
            self.images.borrow().clone()
        };
        let page = all
            .into_iter()
            .skip(offset as usize)
            .take(MEDIA_PAGE as usize)
            .collect();
        if self.hold_media.get() {
            self.held
                .borrow_mut()
                .push(Box::new(move || then(Ok(page))));
        } else {
            then(Ok(page));
        }
    }
    fn folder(&self, uri: &str, then: Box<dyn FnOnce(Result<Vec<Node>, String>)>) {
        then(Ok(self
            .folders
            .borrow()
            .get(uri)
            .cloned()
            .unwrap_or_default()));
    }
    fn upload(&self, id: &str, uri: &str, url: &str, token: &str) {
        self.uploads
            .borrow_mut()
            .push((id.into(), uri.into(), url.into(), token.into()));
    }
    fn changes(
        &self,
        _storage: &str,
        since: i64,
        then: Box<dyn FnOnce(Result<Vec<Change>, NetError>)>,
    ) {
        self.feed_asked.borrow_mut().push(since);
        then(Ok(self.feed.borrow().clone()));
    }
    fn listing(&self, _storage: &str, then: Box<dyn FnOnce(Result<Vec<Remote>, NetError>)>) {
        then(Ok(self.stored.borrow().clone()));
    }
    fn quota(&self, _storage: &str, then: Box<dyn FnOnce(Option<(u64, u64)>)>) {
        then(self.quota.get());
    }
    fn download(&self, id: &str, url: &str, token: &str, name: &str, target: &Target) {
        self.downloads.borrow_mut().push((
            id.into(),
            url.into(),
            token.into(),
            name.into(),
            target.kind,
            target.folder.clone(),
        ));
    }
    fn cancel_download(&self, id: &str) {
        self.cancelled_downloads.borrow_mut().push(id.to_string());
    }
    fn storages(&self, then: Box<dyn FnOnce(Result<Vec<Storage>, NetError>)>) {
        then(Ok(self.storages.borrow().clone()));
    }
    fn cancel(&self, id: &str) {
        self.cancelled.borrow_mut().push(id.to_string());
    }
    fn token(&self, force: bool, then: Box<dyn FnOnce(Result<String, NetError>)>) {
        self.tokens_asked.borrow_mut().push(force);
        then(
            self.token
                .borrow()
                .clone()
                .unwrap_or_else(|| Ok("tok".into())),
        );
    }
    fn device(&self, then: Box<dyn FnOnce(Device)>) {
        then(self.device.get().unwrap_or(Device {
            online: true,
            wifi: true,
            charging: false,
        }));
    }
    fn pick_folder(&self, then: Box<dyn FnOnce(Option<(String, String)>)>) {
        then(self.picked.borrow().clone());
    }
    fn now_ms(&self) -> i64 {
        self.now.get()
    }
    fn later(&self, ms: u64, f: Box<dyn FnOnce()>) {
        self.later.borrow_mut().push((ms, f));
    }
}

fn file(id: &str, size: u64, modified: i64) -> Found {
    Found {
        local_id: id.into(),
        name: format!("{id}.jpg"),
        uri: format!("content://media/{id}"),
        size,
        modified_ms: modified,
    }
}

struct World {
    fake: Rc<Fake>,
    store: Store,
    sent: Rc<RefCell<Vec<Msg>>>,
    controller: Rc<Controller>,
    /// The uploads reported finished so far.
    ended: RefCell<Vec<String>>,
}

fn dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("forklift-surface-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn world(name: &str, settings: Settings, fake: Fake) -> World {
    let store = Store::new(dir(name));
    store.set_settings(&Settings {
        server: "https://h.example".into(),
        storage: "moto".into(),
        storage_confirmed: true,
        ..settings
    });
    let fake = Rc::new(fake);
    let sent: Rc<RefCell<Vec<Msg>>> = Rc::default();
    let log = Rc::clone(&sent);
    let controller = Controller::new(
        fake.clone(),
        store.clone(),
        Rc::new(move |m| log.borrow_mut().push(m)),
    );
    World {
        fake,
        store,
        sent,
        controller,
        ended: RefCell::default(),
    }
}

impl World {
    /// Lets the app be told what the throttle held back (the pictures of a burst come at most every
    /// `PUBLISH_EVERY_MS`).
    fn settle(&self) {
        loop {
            let due = {
                let mut later = self.fake.later.borrow_mut();
                later
                    .iter()
                    .position(|(ms, _)| *ms <= PUBLISH_EVERY_MS as u64)
                    .map(|at| later.remove(at))
            };
            let Some((_, call)) = due else { break };
            call();
        }
    }
    fn snapshots(&self) -> usize {
        self.sent
            .borrow()
            .iter()
            .filter(|m| matches!(m, Msg::Snapshot(_)))
            .count()
    }
    fn last_headline(&self) -> String {
        self.settle();
        self.sent
            .borrow()
            .iter()
            .rev()
            .find_map(|m| {
                if let Msg::Snapshot(s) = m {
                    Some(s.headline())
                } else {
                    None
                }
            })
            .unwrap_or_default()
    }
    fn done_all(&self) {
        let ids: Vec<String> = self
            .fake
            .uploads
            .borrow()
            .iter()
            .map(|u| u.0.clone())
            .collect();
        for id in ids {
            if !self.ended.borrow().contains(&id) {
                self.finish(&id, 200);
            }
        }
    }
    fn finish(&self, id: &str, status: u16) {
        self.ended.borrow_mut().push(id.to_string());
        self.controller.upload_event(&UploadEvent::Done {
            id: id.into(),
            status,
        });
    }
    fn notices(&self) -> Vec<String> {
        self.sent
            .borrow()
            .iter()
            .filter_map(|m| {
                if let Msg::Notice(t) = m {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect()
    }
    fn cleanup(self) {
        let _ = std::fs::remove_dir_all(self.store.dir());
    }
}

#[quench_test::test]
fn a_scan_queues_the_files_and_starts_as_many_uploads_as_may_run() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = (1..=5)
        .map(|i| file(&i.to_string(), 10, i * 1000))
        .collect();
    let w = world(
        "start",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let uploads = w.fake.uploads.borrow().clone();
    assert_eq!(uploads.len(), 3, "concurrency is 3");
    // The newest first, to the storage's file URL, with the token.
    assert_eq!(uploads[0].1, "content://media/5");
    assert_eq!(
        uploads[0].2,
        "https://h.example/warehouse/api/v1/files/moto/file?path=photos%2F1970%2F01%2F5-5.jpg"
    );
    assert_eq!(uploads[0].3, "tok");
    assert!(
        w.last_headline().starts_with("Backing up: 0 of 5"),
        "{}",
        w.last_headline()
    );
    drop(uploads);
    w.cleanup();
}

#[quench_test::test]
fn finished_uploads_make_room_for_the_next_and_the_end_is_said() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = (1..=4)
        .map(|i| file(&i.to_string(), 10, i * 1000))
        .collect();
    let w = world(
        "finish",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    for _ in 0..3 {
        let pending: Vec<String> = w
            .fake
            .uploads
            .borrow()
            .iter()
            .map(|u| u.0.clone())
            .collect();
        for id in pending {
            if w.last_headline() != "All 4 files are backed up" {
                w.controller
                    .upload_event(&UploadEvent::Done { id, status: 200 });
            }
        }
    }
    assert_eq!(w.last_headline(), "All 4 files are backed up");
    assert_eq!(w.fake.uploads.borrow().len(), 4, "each file once");
    w.cleanup();
}

#[quench_test::test]
fn what_is_done_is_remembered_across_runs() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1000)];
    let w = world(
        "remember",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    w.done_all();
    let again = Fake::default();
    *again.images.borrow_mut() = vec![file("1", 10, 1000)];
    let again = Rc::new(again);
    let second = Controller::new(again.clone(), w.store.clone(), Rc::new(|_| {}));
    second.start();
    assert!(
        again.uploads.borrow().is_empty(),
        "the file was already stored"
    );
    assert_eq!(
        w.store.manifest().get("photos", "1").unwrap().status,
        Status::Done
    );
    w.cleanup();
}

#[quench_test::test]
fn nothing_starts_without_a_network_and_videos_wait_for_wifi_when_asked() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1000)];
    *fake.video.borrow_mut() = vec![file("2", 10, 2000)];
    fake.device.set(Some(Device {
        online: false,
        wifi: false,
        charging: false,
    }));
    let w = world(
        "conditions",
        Settings {
            videos_wifi_only: true,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    assert!(w.fake.uploads.borrow().is_empty());
    w.fake.device.set(Some(Device {
        online: true,
        wifi: false,
        charging: false,
    }));
    w.controller.pump();
    let uris: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.1.clone())
        .collect();
    assert_eq!(
        uris,
        ["content://media/1"],
        "the photo goes, the video waits for Wi-Fi"
    );
    w.fake.device.set(Some(Device {
        online: true,
        wifi: true,
        charging: false,
    }));
    w.controller.pump();
    assert_eq!(w.fake.uploads.borrow().len(), 2);
    w.cleanup();
}

#[quench_test::test]
fn a_large_file_waits_for_the_charger() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", LARGE_FILE_BYTES + 1, 1000)];
    let w = world(
        "large",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    assert!(w.fake.uploads.borrow().is_empty());
    w.fake.device.set(Some(Device {
        online: true,
        wifi: true,
        charging: true,
    }));
    w.controller.pump();
    assert_eq!(w.fake.uploads.borrow().len(), 1);
    w.cleanup();
}

#[quench_test::test]
fn paging_goes_on_while_a_page_is_full() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = (0..i64::from(MEDIA_PAGE) + 1)
        .map(|i| file(&i.to_string(), 10, i))
        .collect();
    let w = world(
        "paging",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    assert_eq!(
        *w.fake.media_calls.borrow(),
        [("images", 0), ("images", MEDIA_PAGE)]
    );
    assert_eq!(w.store.manifest().len(), MEDIA_PAGE as usize + 1);
    w.cleanup();
}

#[quench_test::test]
fn a_picked_folder_is_walked_and_its_files_get_short_ids() {
    let fake = Fake::default();
    let doc = |name: &str| Found {
        local_id: String::new(),
        name: name.into(),
        uri: format!("content://tree/x/{name}"),
        size: 5,
        modified_ms: 10,
    };
    fake.folders.borrow_mut().insert(
        "content://tree/x".into(),
        vec![
            Node::File(doc("a.pdf")),
            Node::Dir("content://tree/x/sub".into()),
        ],
    );
    fake.folders.borrow_mut().insert(
        "content://tree/x/sub".into(),
        vec![Node::File(doc("b.pdf"))],
    );
    let mut settings = Settings {
        photos: false,
        videos: false,
        ..Settings::default()
    };
    settings.folders.push(forklift_core::Folder {
        slug: "docs".into(),
        name: "Docs".into(),
        uri: "content://tree/x".into(),
        enabled: true,
        wifi_only: false,
    });
    let w = world("folder", settings, fake);
    w.controller.start();
    let manifest = w.store.manifest();
    let names: Vec<String> = manifest.items().map(|i| i.name.clone()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(
        manifest
            .items()
            .all(|i| i.category == "custom:docs" && i.local_id.len() == 16)
    );
    let urls: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.2.clone())
        .collect();
    assert!(
        urls.iter()
            .all(|u| u.contains("path=custom%3Adocs%2F1970%2F01%2F")),
        "{urls:?}"
    );
    w.cleanup();
}

#[quench_test::test]
fn a_full_storage_stops_sending_and_says_so() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = (1..=2).map(|i| file(&i.to_string(), 10, i)).collect();
    let w = world(
        "quota",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let id = w.fake.uploads.borrow()[0].0.clone();
    w.finish(&id, 507);
    assert!(
        w.notices()
            .iter()
            .any(|n| n.starts_with("the storage is full")),
        "{:?}",
        w.notices()
    );
    assert_eq!(
        w.fake.uploads.borrow().len(),
        2,
        "no new upload after the pause"
    );
    // Asking for a backup again lifts the pause.
    w.controller.scan(true);
    assert!(w.fake.uploads.borrow().len() > 2);
    w.cleanup();
}

#[quench_test::test]
fn a_refused_token_is_renewed_and_the_file_goes_again() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    let w = world(
        "refused",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let id = w.fake.uploads.borrow()[0].0.clone();
    w.finish(&id, 401);
    assert_eq!(
        *w.fake.tokens_asked.borrow(),
        [false, true, false],
        "the renewal is forced, then the next pump asks as usual"
    );
    assert_eq!(w.fake.uploads.borrow().len(), 2);
    w.cleanup();
}

#[quench_test::test]
fn a_session_that_cannot_be_renewed_signs_out() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    *fake.token.borrow_mut() = Some(Err(NetError::Unauthorized));
    let w = world(
        "signout",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.store.set_token("old");
    w.controller.start();
    assert!(w.sent.borrow().iter().any(|m| matches!(m, Msg::SignedOut)));
    assert_eq!(w.store.token(), None);
    assert!(w.fake.uploads.borrow().is_empty());
    assert_eq!(
        w.store.manifest().get("photos", "1").unwrap().status,
        Status::Pending,
        "nothing is lost"
    );
    w.cleanup();
}

#[quench_test::test]
fn a_failure_is_tried_again_when_its_wait_is_over() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    let w = world(
        "retry",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let id = w.fake.uploads.borrow()[0].0.clone();
    w.controller.upload_event(&UploadEvent::Failed {
        id,
        message: "reset".into(),
    });
    assert_eq!(w.fake.uploads.borrow().len(), 1);
    // A look at the clock is set for when the 30 s are over.
    let (ms, call) = {
        let mut later = w.fake.later.borrow_mut();
        let at = later
            .iter()
            .position(|(ms, _)| (1_000..60_000).contains(ms))
            .expect("a retry was scheduled");
        later.remove(at)
    };
    assert!((30_000..31_000).contains(&ms), "{ms}");
    w.fake.now.set(31_000);
    call();
    assert_eq!(w.fake.uploads.borrow().len(), 2);
    w.cleanup();
}

#[quench_test::test]
fn a_folder_the_person_picks_is_added_scanned_and_told_to_the_app() {
    let fake = Fake::default();
    *fake.picked.borrow_mut() = Some(("My Docs".into(), "content://tree/d".into()));
    fake.folders.borrow_mut().insert(
        "content://tree/d".into(),
        vec![Node::File(Found {
            local_id: String::new(),
            name: "x.txt".into(),
            uri: "content://tree/d/x".into(),
            size: 1,
            modified_ms: 1,
        })],
    );
    let w = world(
        "addfolder",
        Settings {
            photos: false,
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.add_folder();
    assert_eq!(w.store.settings().folders.len(), 1);
    assert_eq!(w.store.settings().folders[0].slug, "my_docs");
    assert!(
        w.sent
            .borrow()
            .iter()
            .any(|m| matches!(m, Msg::Settings(s) if s.folders.len() == 1))
    );
    assert_eq!(w.fake.uploads.borrow().len(), 1);
    // The same folder twice is said, not added.
    w.controller.add_folder();
    assert_eq!(w.store.settings().folders.len(), 1);
    assert!(w.notices().iter().any(|n| n.contains("already")));
    w.cleanup();
}

#[quench_test::test]
fn another_storage_means_everything_is_sent_again() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    let w = world(
        "storage",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    w.done_all();
    assert_eq!(w.fake.uploads.borrow().len(), 1);
    let mut s = w.controller.settings();
    s.storage = "other".into();
    w.controller.set_settings(s);
    let urls: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.2.clone())
        .collect();
    assert_eq!(urls.len(), 2);
    assert!(urls[1].contains("/files/other/file"), "{urls:?}");
    w.cleanup();
}

#[quench_test::test]
fn the_server_of_a_sign_in_is_adopted_and_the_settings_screen_cannot_change_it() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    let w = world(
        "server",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    // The app was started before the sign-in: the settings screen holds no server.
    let mut from_screen = w.controller.settings();
    from_screen.server = String::new();
    w.controller.set_settings(from_screen);
    assert_eq!(w.controller.settings().server, "https://h.example");
    // A sign-in to another server; the person then chooses the storage of an older backup from its list.
    w.store.set_settings(&Settings {
        server: "https://other.example".into(),
        storage: "moto".into(),
        storage_confirmed: true,
        videos: false,
        ..Settings::default()
    });
    w.controller.adopt_server();
    w.controller.choose_storage("moto_edge_60_pro");
    assert!(
        w.fake.uploads.borrow()[0]
            .2
            .starts_with("https://other.example/warehouse/api/v1/files/moto_edge_60_pro/file")
    );
    w.cleanup();
}

#[quench_test::test]
fn going_to_the_background_gives_up_what_is_sending_and_coming_back_starts_again() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = (1..=5).map(|i| file(&i.to_string(), 10, i)).collect();
    let w = world(
        "hidden",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let started: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.0.clone())
        .collect();
    assert_eq!(started.len(), 3);
    w.controller.pause_in_background();
    assert_eq!(*w.fake.cancelled.borrow(), started);
    // The manifest says they wait again, for the background run.
    assert!(
        w.store
            .manifest()
            .items()
            .all(|i| i.status == Status::Pending)
    );
    // A late event for a given-up upload changes nothing and starts nothing.
    w.finish(&started[0], 200);
    assert_eq!(w.fake.uploads.borrow().len(), 3);
    // Meanwhile the background run sent one of them and wrote the manifest.
    let mut written = w.store.manifest();
    written.done("photos", "5", "photos/x");
    w.store.set_manifest(&written);
    w.controller.resume_in_foreground();
    let urls: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .skip(3)
        .map(|u| u.1.clone())
        .collect();
    assert_eq!(urls.len(), 3, "three more went out");
    assert!(
        !urls.iter().any(|u| u.ends_with("/5")),
        "not the one the background run sent: {urls:?}"
    );
    w.cleanup();
}

fn change(id: i64, path: &str, op: &str, size: Option<u64>) -> Change {
    Change {
        id,
        path: path.into(),
        op: op.into(),
        size,
    }
}

#[quench_test::test]
fn what_the_server_already_holds_is_recognised_and_not_sent_again() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1000), file("2", 20, 2000)];
    // The feed: "1" was put, "2" was put and then deleted (it is not there any more).
    *fake.feed.borrow_mut() = vec![
        change(4, "photos/1970/01/1-1.jpg", "put", Some(10)),
        change(5, "photos/1970/01/2-2.jpg", "put", Some(20)),
        change(6, "photos/1970/01/2-2.jpg", "delete", None),
    ];
    let w = world(
        "reconcile",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let sent: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.1.clone())
        .collect();
    assert_eq!(
        sent,
        ["content://media/2"],
        "only the file the server lacks goes"
    );
    assert_eq!(
        w.store.manifest().get("photos", "1").unwrap().status,
        Status::Done
    );
    assert_eq!(
        w.store.sync_cursor("moto"),
        6,
        "the feed is read from there next time"
    );
    assert_eq!(*w.fake.feed_asked.borrow(), [0]);
    w.cleanup();
}

#[quench_test::test]
fn the_quota_is_on_the_dashboard() {
    let fake = Fake::default();
    fake.quota.set(Some((250, 1000)));
    let w = world(
        "quota-line",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    let quota = w.sent.borrow().iter().rev().find_map(|m| {
        if let Msg::Snapshot(s) = m {
            s.quota
        } else {
            None
        }
    });
    assert_eq!(quota, Some((250, 1000)));
    w.cleanup();
}

fn views(w: &World) -> Vec<forklift_core::RestoreView> {
    w.sent
        .borrow()
        .iter()
        .filter_map(|m| {
            if let Msg::Restore(v) = m {
                Some((**v).clone())
            } else {
                None
            }
        })
        .collect()
}

fn restoring_world(name: &str) -> World {
    let fake = Fake::default();
    // The device has 1; the server holds 1, a lost photo, a lost video and a file of a folder that is set up.
    *fake.images.borrow_mut() = vec![file("1", 10, 1000)];
    *fake.stored.borrow_mut() = vec![
        Remote {
            path: "photos/1970/01/1-1.jpg".into(),
            size: 10,
        },
        Remote {
            path: "photos/1970/02/9-lost.jpg".into(),
            size: 5,
        },
        Remote {
            path: "videos/1970/02/8-lost.mp4".into(),
            size: 50,
        },
        Remote {
            path: "custom:docs/1970/01/ab-a.pdf".into(),
            size: 7,
        },
    ];
    let mut settings = Settings {
        videos: false,
        ..Settings::default()
    };
    settings.folders.push(forklift_core::Folder {
        slug: "docs".into(),
        name: "Docs".into(),
        uri: "content://tree/d".into(),
        enabled: true,
        wifi_only: false,
    });
    let w = world(name, settings, fake);
    w.controller.start();
    w
}

#[quench_test::test]
fn a_restore_is_planned_then_run_a_few_files_at_a_time_to_the_right_places() {
    let w = restoring_world("restore");
    w.controller.plan_restore();
    let all = views(&w);
    assert_eq!(all[0], forklift_core::RestoreView::Planning);
    let forklift_core::RestoreView::Plan(plan) = &all[1] else {
        panic!("{all:?}")
    };
    assert_eq!((plan.files, plan.bytes, plan.present), (3, 62, 1));
    assert!(
        w.fake.downloads.borrow().is_empty(),
        "nothing is fetched before the yes"
    );
    w.controller.start_restore();
    let downloads = w.fake.downloads.borrow().clone();
    assert_eq!(downloads.len(), 2, "two at a time");
    assert_eq!(
        downloads[0].1,
        "https://h.example/warehouse/api/v1/files/moto/file?path=photos%2F1970%2F02%2F9-lost.jpg"
    );
    assert_eq!(
        (
            downloads[0].2.as_str(),
            downloads[0].3.as_str(),
            downloads[0].4
        ),
        ("tok", "lost.jpg", "images")
    );
    assert_eq!(downloads[1].4, "video", "a video goes to the video library");
    w.controller.download_event(&downloads[0].0, true);
    let third = w.fake.downloads.borrow()[2].clone();
    assert_eq!((third.4, third.5.as_str()), ("folder", "Docs"));
    w.controller.download_event(&downloads[1].0, true);
    w.controller.download_event(&third.0, true);
    let last = views(&w).pop().unwrap();
    assert_eq!(
        last,
        forklift_core::RestoreView::Done(forklift_core::Progress {
            total: 3,
            done: 3,
            failed: 0
        })
    );
    w.cleanup();
}

#[quench_test::test]
fn a_file_that_cannot_be_fetched_is_tried_again_and_then_counted() {
    let w = restoring_world("restore-fail");
    w.controller.plan_restore();
    w.controller.start_restore();
    // Fail whatever is asked for the first photo three times, finish everything else.
    let mut guard = 0;
    loop {
        guard += 1;
        assert!(guard < 50);
        let next = w
            .fake
            .downloads
            .borrow()
            .iter()
            .find(|d| !w.ended.borrow().contains(&d.0))
            .cloned();
        let Some(d) = next else { break };
        w.ended.borrow_mut().push(d.0.clone());
        w.controller.download_event(&d.0, d.3 != "lost.jpg");
    }
    let last = views(&w).pop().unwrap();
    assert_eq!(
        last,
        forklift_core::RestoreView::Done(forklift_core::Progress {
            total: 3,
            done: 2,
            failed: 1
        })
    );
    assert_eq!(
        w.fake
            .downloads
            .borrow()
            .iter()
            .filter(|d| d.3 == "lost.jpg")
            .count(),
        3,
        "three tries"
    );
    w.cleanup();
}

#[quench_test::test]
fn a_restore_can_be_stopped_and_gives_up_what_was_running() {
    let w = restoring_world("restore-stop");
    w.controller.plan_restore();
    w.controller.start_restore();
    let started: Vec<String> = w
        .fake
        .downloads
        .borrow()
        .iter()
        .map(|d| d.0.clone())
        .collect();
    w.controller.cancel_restore();
    assert_eq!(*w.fake.cancelled_downloads.borrow(), started);
    assert!(matches!(
        views(&w).pop(),
        Some(forklift_core::RestoreView::Done(_))
    ));
    // A late report of a given-up download starts nothing.
    w.controller.download_event(&started[0], true);
    assert_eq!(w.fake.downloads.borrow().len(), 2);
    w.cleanup();
}

#[quench_test::test]
fn files_that_came_back_are_not_sent_again_when_they_are_found_on_the_device() {
    let w = restoring_world("restore-then-scan");
    w.controller.plan_restore();
    w.controller.start_restore();
    // The device saved the lost photo under a new id (99) and a new modified time.
    w.fake.images.borrow_mut().push(Found {
        local_id: "99".into(),
        name: "lost.jpg".into(),
        uri: "content://media/99".into(),
        size: 5,
        modified_ms: 9_999_000_000,
    });
    // Every download is reported saved, the ones started later too.
    loop {
        let next = w
            .fake
            .downloads
            .borrow()
            .iter()
            .find(|d| !w.ended.borrow().contains(&d.0))
            .cloned();
        let Some(d) = next else { break };
        w.ended.borrow_mut().push(d.0.clone());
        w.controller.download_event(&d.0, true);
    }
    let sent: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.1.clone())
        .collect();
    assert!(!sent.iter().any(|u| u.ends_with("/99")), "{sent:?}");
    assert_eq!(
        w.store.manifest().get("photos", "99").unwrap().status,
        Status::Done
    );
    w.cleanup();
}

#[quench_test::test]
fn a_photo_deleted_from_the_device_is_offered_for_restore() {
    let w = restoring_world("restore-deleted");
    w.controller.start();
    w.done_all();
    // The device lost photo 1 (it was backed up at photos/1970/01/1-1.jpg).
    w.fake.images.borrow_mut().clear();
    w.controller.scan(false);
    w.controller.plan_restore();
    let plan = views(&w)
        .into_iter()
        .rev()
        .find_map(|v| {
            if let forklift_core::RestoreView::Plan(p) = v {
                Some(p)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(plan.files, 4, "the photo that was deleted is among them");
    w.cleanup();
}

#[quench_test::test]
fn nothing_is_looked_at_or_sent_before_a_storage_is_chosen() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    *fake.storages.borrow_mut() = vec![
        Storage {
            name: "moto".into(),
            owner: Some("me".into()),
            quota_bytes: Some(1000),
            used_bytes: Some(10),
        },
        Storage {
            name: "moto_edge_60_pro".into(),
            owner: Some("me".into()),
            quota_bytes: None,
            used_bytes: None,
        },
    ];
    let w = world(
        "unchosen",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    // The settings on disk say no storage was chosen yet.
    let mut stored = w.store.settings();
    stored.storage_confirmed = false;
    w.store.set_settings(&stored);
    let c = Controller::new(w.fake.clone(), w.store.clone(), Rc::new(|_| {}));
    c.start();
    assert!(w.fake.media_calls.borrow().is_empty(), "no scan");
    assert!(w.fake.uploads.borrow().is_empty());
    w.cleanup();
}

#[quench_test::test]
fn the_storages_are_listed_for_the_chooser_and_a_choice_starts_the_backup_there() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    *fake.storages.borrow_mut() = vec![
        // One the server is configured with: no owner, so not offered.
        Storage {
            name: "static-media".into(),
            owner: None,
            quota_bytes: None,
            used_bytes: None,
        },
        Storage {
            name: "moto".into(),
            owner: Some("me".into()),
            quota_bytes: Some(1000),
            used_bytes: Some(10),
        },
        Storage {
            name: "moto_edge_60_pro".into(),
            owner: Some("me".into()),
            quota_bytes: None,
            used_bytes: None,
        },
    ];
    let w = world(
        "choose",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    let mut stored = w.store.settings();
    stored.storage_confirmed = false;
    w.store.set_settings(&stored);
    let sent: Rc<RefCell<Vec<Msg>>> = Rc::default();
    let log = Rc::clone(&sent);
    let c = Controller::new(
        w.fake.clone(),
        w.store.clone(),
        Rc::new(move |m| log.borrow_mut().push(m)),
    );
    c.list_storages();
    let listed = sent
        .borrow()
        .iter()
        .find_map(|m| {
            if let Msg::Storages(r) = m {
                Some((**r).clone())
            } else {
                None
            }
        })
        .unwrap()
        .unwrap();
    assert_eq!(
        listed.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["moto", "moto_edge_60_pro"]
    );
    assert_eq!((listed[0].used, listed[0].quota), (Some(10), Some(1000)));
    c.choose_storage("moto_edge_60_pro");
    let urls: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.2.clone())
        .collect();
    assert!(urls[0].contains("/files/moto_edge_60_pro/file"), "{urls:?}");
    let saved = w.store.settings();
    assert_eq!(
        (saved.storage.as_str(), saved.storage_confirmed),
        ("moto_edge_60_pro", true)
    );
    assert!(sent.borrow().iter().any(
        |m| matches!(m, Msg::Settings(s) if s.storage == "moto_edge_60_pro" && s.storage_confirmed)
    ));
    w.cleanup();
}

#[quench_test::test]
fn the_settings_screen_cannot_change_the_storage_but_a_choice_can() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = vec![file("1", 10, 1)];
    let w = world(
        "update",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    w.done_all();
    let mut from_screen = w.controller.settings();
    from_screen.storage = "sneaky".into();
    from_screen.storage_confirmed = false;
    from_screen.auto = false;
    w.controller.update_settings(from_screen);
    let now = w.controller.settings();
    assert_eq!(
        (now.storage.as_str(), now.storage_confirmed, now.auto),
        ("moto", true, false)
    );
    // Choosing another one starts over against it: the file is sent again, to the new storage.
    w.controller.choose_storage("other");
    let urls: Vec<String> = w
        .fake
        .uploads
        .borrow()
        .iter()
        .map(|u| u.2.clone())
        .collect();
    assert_eq!(urls.len(), 2);
    assert!(urls[1].contains("/files/other/file"), "{urls:?}");
    w.cleanup();
}

quench_test::main!();

fn last_snapshot(w: &World) -> forklift_core::Snapshot {
    w.settle();
    w.sent
        .borrow()
        .iter()
        .rev()
        .find_map(|m| {
            if let Msg::Snapshot(s) = m {
                Some((**s).clone())
            } else {
                None
            }
        })
        .expect("the app was told")
}

#[quench_test::test]
fn a_burst_of_ended_uploads_is_one_picture_for_the_app_not_one_each() {
    // What made a big backup slow to look at: every upload that ended meant a count of the whole manifest and a
    // new dashboard. With a few hundred waiting the app is told a handful of times, not once per file.
    let fake = Fake::default();
    *fake.images.borrow_mut() = (1..=600).map(|i| file(&i.to_string(), 10, i)).collect();
    let w = world(
        "burst",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    for _ in 0..400 {
        w.done_all();
    }
    w.settle();
    assert_eq!(w.fake.uploads.borrow().len(), 600);
    let told = w.snapshots();
    assert!(told <= 8, "the app was told {told} times for 600 files");
    // ... and the last word is the true one.
    assert_eq!(last_snapshot(&w).totals.done, 600);
    w.cleanup();
}

#[quench_test::test]
fn the_numbers_grow_while_the_scan_is_still_reading_and_say_how_far_it_is() {
    // The scan used to hand its files over at its very end: a big library showed nothing for the whole read.
    let fake = Fake::default();
    fake.hold_media.set(true);
    *fake.images.borrow_mut() = (0..i64::from(MEDIA_PAGE) * 2 + 10)
        .map(|i| file(&i.to_string(), 10, i))
        .collect();
    let w = world(
        "growing",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    // Nothing answered yet.
    let first = w.fake.held.borrow_mut().remove(0);
    first();
    let shot = last_snapshot(&w);
    assert!(shot.working);
    assert_eq!(shot.totals.pending, MEDIA_PAGE as usize);
    assert_eq!(shot.looked_at, MEDIA_PAGE as usize);
    // The next page is asked for only once the first was taken.
    assert_eq!(w.fake.media_calls.borrow().len(), 2);
    let second = w.fake.held.borrow_mut().remove(0);
    second();
    let third = w.fake.held.borrow_mut().remove(0);
    third();
    let shot = last_snapshot(&w);
    assert_eq!(shot.looked_at, 0, "a scan that is over says no more");
    assert_eq!(w.store.manifest().len(), MEDIA_PAGE as usize * 2 + 10);
    w.cleanup();
}

#[quench_test::test]
fn a_file_gone_from_the_device_is_still_noticed_when_the_scan_came_in_pages() {
    let fake = Fake::default();
    let total = i64::from(MEDIA_PAGE) + 5;
    *fake.images.borrow_mut() = (0..total).map(|i| file(&i.to_string(), 10, i)).collect();
    let w = world(
        "pages-gone",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    // The newest go first: 2004, 2003 and 2002 were sent.
    w.done_all();
    // One of the sent ones (on the second page) and one never sent (on the first) are deleted from the device.
    w.fake
        .images
        .borrow_mut()
        .retain(|f| f.local_id != "10" && f.local_id != "2003");
    w.controller.scan(false);
    let kept = w.store.manifest();
    assert!(kept.get("photos", "10").is_none(), "never sent: forgotten");
    let sent = kept.get("photos", "2003").expect("sent: kept as a record");
    assert!(sent.gone);
    assert!(!kept.get("photos", "2004").unwrap().gone);
    w.cleanup();
}

#[quench_test::test]
fn the_dashboard_numbers_are_kept_for_the_next_start_and_leave_with_the_session() {
    let fake = Fake::default();
    *fake.images.borrow_mut() = (1..=4).map(|i| file(&i.to_string(), 10, i)).collect();
    let w = world(
        "kept-snapshot",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.start();
    w.done_all();
    w.done_all();
    let kept = w.store.snapshot();
    assert_eq!(kept.totals.done, 4, "{kept:?}");
    assert_eq!(kept.headline(), "All 4 files are backed up");
    w.controller.sign_out();
    assert_eq!(w.store.snapshot(), forklift_core::Snapshot::default());
    w.cleanup();
}

#[quench_test::test]
fn the_counts_are_shown_before_any_scan_and_nothing_is_claimed_until_one_has_finished() {
    // A manifest of an earlier run, an empty library for now: before the scan has finished the app is told what it
    // had, and not "nothing to back up".
    let fake = Fake::default();
    fake.hold_media.set(true);
    let w = world(
        "before-scan",
        Settings {
            videos: false,
            ..Settings::default()
        },
        fake,
    );
    w.controller.show();
    let shot = last_snapshot(&w);
    assert!(!shot.settled);
    assert_eq!(shot.headline(), "Looking for files");
    w.controller.start();
    let page = w.fake.held.borrow_mut().remove(0);
    page();
    let shot = last_snapshot(&w);
    assert!(shot.settled, "a scan that read everything has finished");
    assert_eq!(shot.headline(), "Nothing to back up yet");
    w.cleanup();
}
