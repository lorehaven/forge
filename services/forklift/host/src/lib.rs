//! The backup's brain on a device: scans the sources, feeds the engine, starts the uploads the host streams, and
//! tells the app. It touches the device only through [`Platform`], so it runs the same under a test.
//!
//! One thread owns it (the program's main loop); everything slow (a refresh of the session, an upload) happens
//! elsewhere and comes back as a callback or an event.

pub mod worker;

use std::cell::RefCell;
use std::rc::Rc;

use forklift_app::live::Msg;
use forklift_core::{
    Category, Engine, Folder, Found, Outcome, Plan, Remote, RestoreView, Restorer, Settings,
    Snapshot, StorageInfo, Verdict, base_name, category_of, folder_slug, short_id,
};
use forklift_net::{Change, NetError, Storage, Store, upload_url};

/// What the device says it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device {
    pub online: bool,
    pub wifi: bool,
    pub charging: bool,
}

/// One thing in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    File(Found),
    Dir(String),
}

type Then<T> = Box<dyn FnOnce(T)>;

/// The device's side of the backup.
pub trait Platform {
    /// A page (up to [`MEDIA_PAGE`]) of the media library from `offset`: `images` or `video`.
    fn media(&self, kind: &'static str, offset: u32, then: Then<Result<Vec<Found>, String>>);
    /// What is directly in a picked folder.
    fn folder(&self, uri: &str, then: Then<Result<Vec<Node>, String>>);
    /// Starts streaming `uri` to `url`; the host reports with [`Controller::upload_event`].
    fn upload(&self, id: &str, uri: &str, url: &str, token: &str);
    /// Gives up an upload that was started.
    fn cancel(&self, id: &str);
    /// What changed in the storage since change number `since`.
    fn changes(&self, storage: &str, since: i64, then: Then<Result<Vec<Change>, NetError>>);
    /// Every file stored.
    fn listing(&self, storage: &str, then: Then<Result<Vec<Remote>, NetError>>);
    /// `(used, quota)` of the storage, when the server says.
    fn quota(&self, storage: &str, then: Then<Option<(u64, u64)>>);
    /// Starts fetching `url` (with the token) and puts the file where `target` says; the host reports with
    /// [`Controller::download_event`].
    fn download(&self, id: &str, url: &str, token: &str, name: &str, target: &Target);
    /// Gives up a download that was started.
    fn cancel_download(&self, id: &str);
    /// The storages this account may use.
    fn storages(&self, then: Then<Result<Vec<Storage>, NetError>>);
    /// An access token with life left; `force` gets a new one whatever the old one says.
    fn token(&self, force: bool, then: Then<Result<String, NetError>>);
    fn device(&self, then: Then<Device>);
    /// Lets the person pick a folder: its name and its tree.
    fn pick_folder(&self, then: Then<Option<(String, String)>>);
    fn now_ms(&self) -> i64;
    /// Runs `f` after `ms` milliseconds.
    fn later(&self, ms: u64, f: Box<dyn FnOnce()>);
}

/// Where a restored file goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// `images` or `video` (the media library) or `folder`.
    pub kind: &'static str,
    /// The folder's name, for `folder`: its files come back under Downloads/Forklift/ (the app only has read access to
    /// a picked folder, so they cannot be written into it again).
    pub folder: String,
}

/// What the host reports of an upload it started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadEvent {
    Done { id: String, status: u16 },
    Failed { id: String, message: String },
}

struct State {
    engine: Engine,
    settings: Settings,
    scanning: bool,
    /// When the manifest was last written, so that a long run does not write it for every file.
    saved_ms: i64,
    /// Whether a look at the clock for the next retry is already set.
    waiting: bool,
    /// Whether a pump is between asking the device and handing its uploads over.
    pumping: bool,
    quota: Option<(u64, u64)>,
    plan: Option<Plan>,
    restorer: Option<Restorer>,
    /// What the server listed for a restore: after the restored files are scanned, they are told from the stored ones.
    listing: Option<Vec<Remote>>,
    /// Whether every source of this scan was read to the end.
    scan_ok: bool,
    /// Whether the timer that looks for new files is already running.
    rescan_scheduled: bool,
    /// The ids of the files the running scan has met in the category it is reading (what it did not meet is gone).
    seen: Vec<String>,
    /// How many files the running scan has looked at.
    looked_at: usize,
    /// When the app was last told the numbers, and whether a telling is already set for later.
    published_ms: i64,
    publish_set: bool,
}

/// The controller.
pub struct Controller {
    platform: Rc<dyn Platform>,
    store: Store,
    emit: Rc<dyn Fn(Msg)>,
    state: RefCell<State>,
}

/// How many files of the media library are asked for at a time.
pub const MEDIA_PAGE: u32 = 2000;
/// The most often the app is told the numbers while files come in or go out (a burst of events is one picture).
pub const PUBLISH_EVERY_MS: i64 = 250;
/// How often the manifest is written while uploads go on.
const SAVE_EVERY_MS: i64 = 3000;
/// How often a running app looks for new files by itself.
const RESCAN_MS: u64 = 10 * 60 * 1000;

impl Controller {
    #[must_use]
    pub fn new(platform: Rc<dyn Platform>, store: Store, emit: Rc<dyn Fn(Msg)>) -> Rc<Self> {
        let manifest = store.manifest();
        Self::with_manifest(platform, store, emit, manifest)
    }

    /// [`new`](Self::new) with a manifest that was read already (a host reads it off its main thread, which the screen
    /// must not wait for).
    #[must_use]
    pub fn with_manifest(
        platform: Rc<dyn Platform>,
        store: Store,
        emit: Rc<dyn Fn(Msg)>,
        manifest: forklift_core::Manifest,
    ) -> Rc<Self> {
        let settings = store.settings();
        let engine = Engine::new(manifest, settings.concurrency);
        Rc::new(Self {
            platform,
            store,
            emit,
            state: RefCell::new(State {
                engine,
                settings,
                scanning: false,
                saved_ms: 0,
                waiting: false,
                pumping: false,
                quota: None,
                plan: None,
                restorer: None,
                listing: None,
                scan_ok: true,
                rescan_scheduled: false,
                seen: Vec::new(),
                looked_at: 0,
                published_ms: i64::MIN / 2,
                publish_set: false,
            }),
        })
    }

    #[must_use]
    pub fn settings(&self) -> Settings {
        self.state.borrow().settings.clone()
    }

    fn snapshot(&self) -> Snapshot {
        let st = self.state.borrow();
        let working = st.scanning || st.engine.running() > 0;
        let mut shot = Snapshot::of(&st.engine, &st.settings, working, self.store.checked_ms());
        shot.quota = st.quota;
        shot.looked_at = if st.scanning { st.looked_at } else { 0 };
        shot
    }

    fn publish(&self) {
        self.state.borrow_mut().published_ms = self.platform.now_ms();
        (self.emit)(Msg::Snapshot(Box::new(self.snapshot())));
    }

    /// [`publish`](Self::publish) for what comes in bursts (a page of files, an upload that ended): at most once per
    /// [`PUBLISH_EVERY_MS`], and the last word always comes.
    fn publish_soon(self: &Rc<Self>) {
        let wait = {
            let mut st = self.state.borrow_mut();
            if st.publish_set {
                return;
            }
            let since = self.platform.now_ms() - st.published_ms;
            if since >= PUBLISH_EVERY_MS {
                None
            } else {
                st.publish_set = true;
                Some(PUBLISH_EVERY_MS - since)
            }
        };
        match wait {
            None => self.publish(),
            Some(ms) => {
                let this = Rc::clone(self);
                self.platform.later(
                    u64::try_from(ms).unwrap_or(0),
                    Box::new(move || {
                        this.state.borrow_mut().publish_set = false;
                        this.publish();
                    }),
                );
            }
        }
    }

    fn save(&self) {
        let mut st = self.state.borrow_mut();
        st.saved_ms = self.platform.now_ms();
        self.store.set_manifest(&st.engine.manifest);
        drop(st);
        self.store.set_snapshot(&self.snapshot());
    }

    /// A save the controller does not wait for (the file is written by another thread): for the periodic one while
    /// uploads go on.
    fn save_in_background(&self) {
        let mut st = self.state.borrow_mut();
        st.saved_ms = self.platform.now_ms();
        self.store.set_manifest_in_background(&st.engine.manifest);
        drop(st);
        self.store.set_snapshot(&self.snapshot());
    }

    fn notice(&self, text: &str) {
        (self.emit)(Msg::Notice(text.to_string()));
    }

    /// The app is up and signed in: look for files, and keep looking while it is open.
    pub fn start(self: &Rc<Self>) {
        // Nothing is looked at, let alone sent, before the person has said where it goes.
        if !self.state.borrow().settings.storage_confirmed {
            return;
        }
        self.scan(false);
        let first = !std::mem::replace(&mut self.state.borrow_mut().rescan_scheduled, true);
        if first {
            self.schedule_rescan();
        }
    }

    /// Asks the server which storages this account may use, and tells the app.
    pub fn list_storages(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.platform.storages(Box::new(move |result| match result {
            Err(NetError::Unauthorized) => this.token_failed(&NetError::Unauthorized),
            Err(e) => (this.emit)(Msg::Storages(Box::new(Err(e.to_string())))),
            Ok(list) => {
                // Only the storages people made (they have an owner): the ones the server is configured with are not
                // places for a device's backup.
                let list = list
                    .into_iter()
                    .filter(|s| s.owner.is_some())
                    .map(|s| StorageInfo {
                        name: s.name,
                        used: s.used_bytes,
                        quota: s.quota_bytes,
                    })
                    .collect();
                (this.emit)(Msg::Storages(Box::new(Ok(list))));
            }
        }));
    }

    /// The person chose where this device's backup goes. What is already known to be there is recognised, and the
    /// backup starts (or, for a change of storage, starts over against the new one).
    pub fn choose_storage(self: &Rc<Self>, name: &str) {
        let settings = {
            let mut settings = self.state.borrow().settings.clone();
            settings.storage = name.to_string();
            settings.storage_confirmed = true;
            settings
        };
        (self.emit)(Msg::Settings(Box::new(settings.clone())));
        self.set_settings(settings);
        self.start();
    }

    fn schedule_rescan(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.platform.later(
            RESCAN_MS,
            Box::new(move || {
                if this.state.borrow().settings.auto {
                    this.scan(false);
                }
                this.schedule_rescan();
            }),
        );
    }

    /// Looks at every source and queues what is new or changed, then sends. `manual`: the person asked, so what
    /// failed is tried again and a stopped run starts again.
    pub fn scan(self: &Rc<Self>, manual: bool) {
        {
            let mut st = self.state.borrow_mut();
            if st.scanning {
                return;
            }
            st.scanning = true;
            st.scan_ok = true;
            st.looked_at = 0;
            if manual {
                st.engine.resume();
            }
        }
        self.publish();
        let categories = self.state.borrow().settings.categories();
        self.scan_category(categories, 0, manual);
    }

    fn scan_category(self: &Rc<Self>, categories: Vec<Category>, at: usize, manual: bool) {
        let Some(category) = categories.get(at).cloned() else {
            self.finish_scan();
            return;
        };
        {
            let mut st = self.state.borrow_mut();
            st.seen.clear();
        }
        let this = Rc::clone(self);
        let key = category.key.clone();
        // What was read of the category is already in the manifest (each page was taken as it came); this is the end of
        // it: only a scan that read everything may say what has gone.
        let done: Then<()> = Box::new(move |()| {
            {
                let mut st = this.state.borrow_mut();
                if st.scan_ok {
                    let seen = std::mem::take(&mut st.seen);
                    let ids: std::collections::BTreeSet<&str> =
                        seen.iter().map(String::as_str).collect();
                    st.engine.manifest.mark_missing_ids(&key, &ids);
                }
                st.seen.clear();
            }
            this.publish_soon();
            this.scan_category(categories, at + 1, manual);
        });
        match category.folder {
            None => {
                let kind = if category.key == forklift_core::VIDEOS {
                    "video"
                } else {
                    "images"
                };
                self.collect_media(category.key, kind, 0, manual, done);
            }
            Some(uri) => self.collect_folder(category.key, vec![uri], manual, done),
        }
    }

    /// Takes files a source just listed: they are in the manifest at once, so the numbers on the screen grow while the
    /// scan goes on, and nothing is held until its end but the ids.
    fn take(&self, category: &str, found: &[Found], manual: bool) {
        let mut st = self.state.borrow_mut();
        st.engine.manifest.scan(category, found, manual);
        st.seen.extend(found.iter().map(|f| f.local_id.clone()));
        st.looked_at += found.len();
    }

    fn collect_media(
        self: &Rc<Self>,
        category: String,
        kind: &'static str,
        offset: u32,
        manual: bool,
        done: Then<()>,
    ) {
        let this = Rc::clone(self);
        self.platform.media(
            kind,
            offset,
            Box::new(move |page| match page {
                Ok(page) => {
                    let full = page.len() >= MEDIA_PAGE as usize;
                    this.take(&category, &page, manual);
                    this.publish_soon();
                    if full {
                        this.collect_media(category, kind, offset + MEDIA_PAGE, manual, done);
                    } else {
                        done(());
                    }
                }
                Err(why) => {
                    this.state.borrow_mut().scan_ok = false;
                    this.notice(&format!("Could not read the media library: {why}"));
                    done(());
                }
            }),
        );
    }

    fn collect_folder(
        self: &Rc<Self>,
        category: String,
        mut queue: Vec<String>,
        manual: bool,
        done: Then<()>,
    ) {
        let Some(uri) = queue.pop() else {
            done(());
            return;
        };
        let this = Rc::clone(self);
        self.platform.folder(
            &uri,
            Box::new(move |nodes| {
                match nodes {
                    Ok(nodes) => {
                        let mut found = Vec::new();
                        for node in nodes {
                            match node {
                                Node::Dir(child) => queue.push(child),
                                Node::File(mut file) => {
                                    // A document is known by its URI: that is long, so it is shortened.
                                    file.local_id = short_id(&file.uri);
                                    found.push(file);
                                }
                            }
                        }
                        this.take(&category, &found, manual);
                        this.publish_soon();
                    }
                    Err(why) => {
                        this.state.borrow_mut().scan_ok = false;
                        this.notice(&format!("Could not read a folder: {why}"));
                    }
                }
                this.collect_folder(category, queue, manual, done);
            }),
        );
    }

    fn finish_scan(self: &Rc<Self>) {
        // What the server already holds is recognised before anything is sent (a fresh install, a new storage).
        let (storage, since) = {
            let st = self.state.borrow();
            (
                st.settings.storage.clone(),
                self.store.sync_cursor(&st.settings.storage),
            )
        };
        let this = Rc::clone(self);
        let kept = storage.clone();
        self.platform.changes(
            &storage,
            since,
            Box::new(move |result| this.reconciled(&kept, result)),
        );
    }

    fn reconciled(self: &Rc<Self>, storage: &str, result: Result<Vec<Change>, NetError>) {
        if let Err(NetError::Unauthorized) = result {
            self.state.borrow_mut().scanning = false;
            self.token_failed(&NetError::Unauthorized);
            return;
        }
        // No feed (a storage that keeps none, or no network): nothing is recognised this time.
        if let Ok(changes) = result {
            let mut known: std::collections::BTreeMap<String, u64> =
                std::collections::BTreeMap::new();
            for change in &changes {
                if change.op == "delete" {
                    known.remove(&change.path);
                } else {
                    known.insert(change.path.clone(), change.size.unwrap_or(0));
                }
            }
            if let Some(last) = changes.last() {
                self.store.set_sync_cursor(storage, last.id);
            }
            let known: Vec<Remote> = known
                .into_iter()
                .map(|(path, size)| Remote { path, size })
                .collect();
            self.state.borrow_mut().engine.manifest.reconcile(&known);
        }
        {
            let mut st = self.state.borrow_mut();
            // A restore just brought files back: they are on the device and in the storage already.
            if let Some(listing) = st.listing.take() {
                st.engine.manifest.reconcile(&listing);
            }
            st.scanning = false;
        }
        self.store.set_checked_ms(self.platform.now_ms());
        self.save();
        self.publish();
        self.pump();
        self.refresh_quota();
    }

    fn refresh_quota(self: &Rc<Self>) {
        let storage = self.state.borrow().settings.storage.clone();
        let this = Rc::clone(self);
        self.platform.quota(
            &storage,
            Box::new(move |quota| {
                this.state.borrow_mut().quota = quota;
                this.publish();
                this.store.set_snapshot(&this.snapshot());
            }),
        );
    }

    /// Starts what may start now: as many uploads as there are free places, under the device's conditions.
    pub fn pump(self: &Rc<Self>) {
        self.state.borrow_mut().pumping = true;
        let this = Rc::clone(self);
        self.platform
            .device(Box::new(move |device| this.pump_with(device)));
    }

    fn pump_with(self: &Rc<Self>, device: Device) {
        if !device.online {
            self.state.borrow_mut().pumping = false;
            return;
        }
        let now = self.platform.now_ms();
        let starts = {
            let mut st = self.state.borrow_mut();
            let wifi_only: Vec<String> = st
                .settings
                .categories()
                .into_iter()
                .filter(|c| c.wifi_only)
                .map(|c| c.key)
                .collect();
            let charging_needed = st.settings.charging_for_large;
            st.engine
                .pump_where(now, device.charging || !charging_needed, |category| {
                    device.wifi || !wifi_only.iter().any(|k| k == category)
                })
        };
        if starts.is_empty() {
            self.state.borrow_mut().pumping = false;
            self.schedule_retry();
            return;
        }
        self.publish_soon();
        let (this, base, storage) = {
            let st = self.state.borrow();
            (
                Rc::clone(self),
                st.settings.server.clone(),
                st.settings.storage.clone(),
            )
        };
        self.platform.token(
            false,
            Box::new(move |token| match token {
                Ok(token) => {
                    this.state.borrow_mut().pumping = false;
                    for start in starts {
                        let url = upload_url(&base, &storage, &start.remote);
                        this.platform
                            .upload(&start.upload_id, &start.uri, &url, &token);
                    }
                }
                Err(e) => this.token_failed(&e),
            }),
        );
    }

    fn token_failed(self: &Rc<Self>, error: &NetError) {
        self.state.borrow_mut().pumping = false;
        self.state.borrow_mut().engine.stop();
        self.save();
        if *error == NetError::Unauthorized {
            self.state.borrow_mut().engine.resume();
            self.store.clear_token();
            (self.emit)(Msg::SignedOut);
        } else {
            self.notice(&error.to_string());
        }
        self.publish();
    }

    /// Looks again when the earliest file waiting out a failure is due.
    fn schedule_retry(self: &Rc<Self>) {
        let now = self.platform.now_ms();
        let due = {
            let mut st = self.state.borrow_mut();
            if st.waiting || st.engine.running() > 0 {
                return;
            }
            let due = st.engine.manifest.next_due_ms(now);
            st.waiting = due.is_some();
            due
        };
        if let Some(due) = due {
            let this = Rc::clone(self);
            self.platform.later(
                u64::try_from(due - now).unwrap_or(0) + 50,
                Box::new(move || {
                    this.state.borrow_mut().waiting = false;
                    this.pump();
                }),
            );
        }
    }

    /// The host's report of an upload.
    pub fn upload_event(self: &Rc<Self>, event: &UploadEvent) {
        let (id, outcome) = match event {
            UploadEvent::Done { id, status } => (id, Outcome::Http(*status)),
            UploadEvent::Failed { id, message } => (id, Outcome::Failed(message.clone())),
        };
        let now = self.platform.now_ms();
        let verdict = self.state.borrow_mut().engine.finish(id, &outcome, now);
        let idle = self.state.borrow().engine.running() == 0;
        if idle && matches!(verdict, Verdict::Done) {
            self.refresh_quota();
        }
        if idle {
            self.save();
        } else if now - self.state.borrow().saved_ms >= SAVE_EVERY_MS {
            self.save_in_background();
        }
        if idle {
            self.publish();
        } else {
            self.publish_soon();
        }
        match verdict {
            Verdict::Done | Verdict::Retry => self.pump(),
            Verdict::Reauthenticate => {
                let this = Rc::clone(self);
                self.platform.token(
                    true,
                    Box::new(move |token| match token {
                        Ok(_) => this.pump(),
                        Err(e) => this.token_failed(&e),
                    }),
                );
            }
            Verdict::Paused(why) => {
                self.state.borrow_mut().engine.stop();
                self.save();
                self.notice(why.words());
                self.publish();
            }
        }
    }

    /// Whether nothing is going on and nothing is about to start: not scanning, no upload running, no pump under way.
    /// A background run ends when this is so.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        let st = self.state.borrow();
        !st.scanning && !st.pumping && st.engine.running() == 0
    }

    /// A sign-in stored the server it signed in to, and the storage chosen with it: the controller takes them from
    /// there. (The app's settings screen does not know the server until the next start, and never changes it.)
    pub fn adopt_server(&self) {
        let stored = self.store.settings();
        let mut st = self.state.borrow_mut();
        st.settings.server = stored.server;
    }

    /// A change from the settings screen: kept and acted on, except the server and the storage, which the screen does
    /// not decide (the storage is chosen from the server's list, see [`choose_storage`](Self::choose_storage)).
    pub fn update_settings(self: &Rc<Self>, mut settings: Settings) {
        {
            let st = self.state.borrow();
            settings.storage.clone_from(&st.settings.storage);
            settings.storage_confirmed = st.settings.storage_confirmed;
        }
        self.set_settings(settings);
    }

    fn restore_view(&self, view: RestoreView) {
        (self.emit)(Msg::Restore(Box::new(view)));
    }

    /// Reads what the server stores and works out what this device lacks, then tells the app (which asks the person).
    pub fn plan_restore(self: &Rc<Self>) {
        self.restore_view(RestoreView::Planning);
        let storage = self.state.borrow().settings.storage.clone();
        let this = Rc::clone(self);
        self.platform.listing(
            &storage,
            Box::new(move |listing| match listing {
                Err(NetError::Unauthorized) => this.token_failed(&NetError::Unauthorized),
                Err(e) => this.restore_view(RestoreView::Failed(e.to_string())),
                Ok(listing) => {
                    let plan = {
                        let st = this.state.borrow();
                        forklift_core::plan(
                            &listing,
                            &st.engine.manifest.present(),
                            &st.settings.folders,
                        )
                    };
                    this.restore_view(RestoreView::Plan(plan.summary()));
                    let mut st = this.state.borrow_mut();
                    st.plan = Some(plan);
                    st.listing = Some(listing);
                }
            }),
        );
    }

    /// The person said yes: fetches what the plan lists, a few files at a time.
    pub fn start_restore(self: &Rc<Self>) {
        let (plan, concurrency) = {
            let mut st = self.state.borrow_mut();
            (st.plan.take(), st.settings.concurrency)
        };
        let Some(plan) = plan else { return };
        if plan.files.is_empty() {
            self.restore_view(RestoreView::Done(forklift_core::Progress::default()));
            return;
        }
        self.state.borrow_mut().restorer = Some(Restorer::new(plan.files, concurrency.min(2)));
        self.restore_view(RestoreView::Running(forklift_core::Progress::default()));
        self.pump_restore();
    }

    /// The person stopped it.
    pub fn cancel_restore(self: &Rc<Self>) {
        let ids = {
            let mut st = self.state.borrow_mut();
            st.plan = None;
            st.restorer.as_mut().map(Restorer::stop)
        };
        for id in ids.unwrap_or_default() {
            self.platform.cancel_download(&id);
        }
        self.end_restore();
    }

    fn pump_restore(self: &Rc<Self>) {
        let starts = match self.state.borrow_mut().restorer.as_mut() {
            Some(r) => r.start(),
            None => return,
        };
        if starts.is_empty() {
            return;
        }
        let (this, base, storage) = {
            let st = self.state.borrow();
            (
                Rc::clone(self),
                st.settings.server.clone(),
                st.settings.storage.clone(),
            )
        };
        self.platform.token(
            false,
            Box::new(move |token| match token {
                Ok(token) => {
                    for fetch in starts {
                        let url = forklift_net::download_url(&base, &storage, &fetch.file.path);
                        let target = this.target_of(category_of(&fetch.file.path));
                        this.platform.download(
                            &fetch.id,
                            &url,
                            &token,
                            base_name(&fetch.file.path),
                            &target,
                        );
                    }
                }
                Err(e) => {
                    this.cancel_restore();
                    if e == NetError::Unauthorized {
                        this.token_failed(&e);
                    } else {
                        this.restore_view(RestoreView::Failed(e.to_string()));
                    }
                }
            }),
        );
    }

    fn target_of(&self, category: &str) -> Target {
        let st = self.state.borrow();
        let folder = st
            .settings
            .folders
            .iter()
            .find(|f| f.category() == category)
            .map(|f| f.name.clone());
        match (category, folder) {
            (forklift_core::VIDEOS, _) => Target {
                kind: "video",
                folder: String::new(),
            },
            (_, Some(folder)) => Target {
                kind: "folder",
                folder,
            },
            _ => Target {
                kind: "images",
                folder: String::new(),
            },
        }
    }

    /// The host's report of a download: it was saved, or it was not.
    pub fn download_event(self: &Rc<Self>, id: &str, ok: bool) {
        let (progress, finished) = {
            let mut st = self.state.borrow_mut();
            let Some(r) = st.restorer.as_mut() else {
                return;
            };
            r.finish(id, ok);
            (r.progress(), r.finished())
        };
        if finished {
            self.end_restore();
        } else {
            self.restore_view(RestoreView::Running(progress));
            self.pump_restore();
        }
    }

    fn end_restore(self: &Rc<Self>) {
        let progress = self
            .state
            .borrow_mut()
            .restorer
            .take()
            .map(|r| r.progress());
        self.restore_view(RestoreView::Done(progress.unwrap_or_default()));
        // What came back is on the device now: looked at, and told from the stored files it came from.
        self.scan(false);
    }

    /// The person changed a setting: kept, and acted on (a source switched on is scanned).
    pub fn set_settings(self: &Rc<Self>, settings: Settings) {
        let rescan = {
            let mut st = self.state.borrow_mut();
            let rescan = st.settings.categories() != settings.categories()
                || st.settings.storage != settings.storage;
            let storage_changed = st.settings.storage != settings.storage;
            // The server is not the settings screen's to change.
            let server = std::mem::take(&mut st.settings.server);
            st.settings = Settings { server, ..settings };
            if storage_changed {
                // What was sent to another storage is not in this one: everything is looked at again.
                let finished: Vec<(String, String)> = st
                    .engine
                    .manifest
                    .items()
                    .filter(|i| i.status == forklift_core::Status::Done)
                    .map(|i| (i.category.clone(), i.local_id.clone()))
                    .collect();
                for (c, id) in finished {
                    st.engine.manifest.reset(&c, &id);
                }
            }
            self.store.set_settings(&st.settings);
            rescan
        };
        self.save();
        self.publish();
        if rescan {
            self.scan(false);
        } else {
            self.pump();
        }
    }

    /// Lets the person pick a folder and adds it to the backup.
    pub fn add_folder(self: &Rc<Self>) {
        let this = Rc::clone(self);
        self.platform.pick_folder(Box::new(move |picked| {
            let Some((name, uri)) = picked else { return };
            let settings = {
                let st = this.state.borrow();
                if st.settings.folders.iter().any(|f| f.uri == uri) {
                    drop(st);
                    this.notice("That folder is already backed up");
                    return;
                }
                let mut settings = st.settings.clone();
                let slug = folder_slug(&name, &settings.folders);
                settings.folders.push(Folder {
                    slug,
                    name,
                    uri,
                    enabled: true,
                    wifi_only: false,
                });
                settings
            };
            (this.emit)(Msg::Settings(Box::new(settings.clone())));
            this.set_settings(settings);
        }));
    }

    /// The run ends (the host stops it, or there is nothing left to do): what was in flight waits again, and the
    /// manifest is written. The session stays.
    pub fn sign_out_keep_session(&self) {
        self.state.borrow_mut().engine.stop();
        self.save();
    }

    /// The window went to the background: uploads in flight are given up (they wait again), nothing more is sent, and
    /// the manifest is written, so the background run can take over.
    pub fn pause_in_background(&self) {
        let ids = self.state.borrow().engine.active_ids();
        for id in ids {
            self.platform.cancel(&id);
        }
        self.state
            .borrow_mut()
            .engine
            .pause_for(forklift_core::Pause::Hidden);
        self.save();
    }

    /// The window is back: sending starts again, from a look at what is new.
    pub fn resume_in_foreground(self: &Rc<Self>) {
        {
            // What the background run did while the window was away is in the file, not in memory.
            let mut st = self.state.borrow_mut();
            st.engine.replace_manifest(self.store.manifest());
            st.engine.resume();
        }
        self.scan(false);
    }

    /// Signed out: nothing more is sent, and the session is gone from the disk.
    pub fn sign_out(&self) {
        self.state.borrow_mut().engine.stop();
        self.save();
        self.store.clear_token();
        self.store.clear_snapshot();
    }
}
