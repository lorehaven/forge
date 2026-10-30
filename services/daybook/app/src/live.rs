//! The app against a host: sign in, see the lists, change tasks. The app does no I/O itself: it asks for things through
//! a `requests` function and is told what happened through `deliver` (both carry plain data), so the host decides how
//! to do the work and where the answers come in.

use std::cell::Cell;
use std::rc::Rc;

use daybook_core::calendar::{format_due, task_urgency};
use daybook_core::model::LABEL_COLORS;
use daybook_core::order::{open_count, view};
use daybook_core::{Data, Label, ListRef, Msg, Request, Task};
use quench_core::time::Time;
use quench_tz::Zone;
use quench_ui::form::{Field, Form, rules};
use quench_ui::kit::{app_bar, dialog, pull_to_refresh, sheet, snackbar, swipe_action, text_field};
use quench_ui::prelude::*;

use crate::form::{Context, Target, task_dialog};

/// What the host knows that the app needs.
#[derive(Clone)]
pub struct Setup {
    /// The server last used, for the sign-in form.
    pub server: String,
    /// Whether the sign-in asks for the server: a program does, a page is always on its own server.
    pub ask_server: bool,
    /// The view that was open.
    pub list: ListRef,
    /// Whether a refresh token is stored: the app starts by loading instead of asking for a password.
    pub has_session: bool,
    pub zone: Zone,
    pub clock: Rc<dyn Fn() -> Time>,
    /// How long a message stays, in milliseconds.
    pub toast_ms: u32,
}

/// The app and the function that feeds it.
pub struct Daybook {
    pub view: View,
    /// Hand every message from the host to this, on the thread of the views.
    pub deliver: Rc<dyn Fn(Msg)>,
}

impl std::fmt::Debug for Daybook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Daybook").finish_non_exhaustive()
    }
}

/// Something the person has to agree to first.
#[derive(Debug, Clone, PartialEq)]
enum Confirm {
    DeleteTask { id: String, title: String },
    DeleteProject { id: String, name: String },
    DeleteLabel { id: String, name: String },
    SignOut,
}

/// A list or a label being named.
#[derive(Debug, Clone, PartialEq)]
enum Naming {
    NewProject,
    Project { id: String, name: String },
    Label(Label),
}

type Inbox = Rc<std::cell::RefCell<Option<Rc<dyn Fn(Msg)>>>>;

/// Builds the app. The views are built when they are mounted.
#[must_use]
pub fn daybook(requests: Rc<dyn Fn(Request)>, setup: Setup) -> Daybook {
    let inbox: Inbox = Rc::default();
    let forward = Rc::clone(&inbox);
    let deliver: Rc<dyn Fn(Msg)> = Rc::new(move |m| {
        if let Some(f) = forward.borrow().clone() {
            f(m);
        }
    });
    let view = View::lazy(move || shell(&requests, &setup, &inbox));
    Daybook { view, deliver }
}

#[allow(clippy::too_many_lines)]
fn shell(requests: &Rc<dyn Fn(Request)>, setup: &Setup, inbox: &Inbox) -> View {
    let signed_in = create_signal(setup.has_session);
    let data = create_signal(Data::default());
    let list = create_signal(setup.list.clone());
    let loading = create_signal(setup.has_session);
    let message = create_signal(String::new());
    let login_busy = create_signal(false);
    let login_error = create_signal(String::new());
    let drawer = create_signal(false);
    let form_open = create_signal(false);
    let form_target: Signal<Option<Target>> = create_signal(None);
    let saving = create_signal(false);
    let save_error = create_signal(String::new());
    let made_label: Signal<Option<String>> = create_signal(None);
    let confirm_delete: Signal<Option<String>> = create_signal(None);
    let confirm: Signal<Option<Confirm>> = create_signal(None);
    let confirm_open = create_signal(false);
    let naming: Signal<Option<Naming>> = create_signal(None);
    let naming_open = create_signal(false);
    let blocked = create_signal(false);
    // What something outside asked to see, until the data it needs is there.
    let show_request: Signal<Option<(ListRef, bool, Option<String>)>> = create_signal(None);

    // What the host says comes to the signals.
    {
        let show_request = show_request.clone();
        let (
            signed_in,
            data,
            loading,
            message,
            login_busy,
            login_error,
            saving,
            save_error,
            form_open,
            made_label,
            blocked,
        ) = (
            signed_in.clone(),
            data.clone(),
            loading.clone(),
            message.clone(),
            login_busy.clone(),
            login_error.clone(),
            saving.clone(),
            save_error.clone(),
            form_open.clone(),
            made_label.clone(),
            blocked.clone(),
        );
        *inbox.borrow_mut() = Some(Rc::new(move |m: Msg| match m {
            Msg::LoggedIn => {
                login_busy.set(false);
                signed_in.set(true);
            }
            Msg::LoginFailed(why) => {
                login_busy.set(false);
                login_error.set(why);
            }
            Msg::Data(d) => data.set(*d),
            Msg::Saved(Ok(())) => {
                saving.set(false);
                form_open.set(false);
            }
            Msg::Saved(Err(why)) => {
                saving.set(false);
                save_error.set(why);
            }
            Msg::LabelCreated(label) => made_label.set(Some(label.id)),
            Msg::Notice(text) => message.set(text),
            Msg::Loading(on) => loading.set(on),
            Msg::NotificationsBlocked(b) => blocked.set(b),
            Msg::Show { list, add, task } => show_request.set(Some((list, add, task))),
            Msg::SignedOut => {
                login_busy.set(false);
                signed_in.set(false);
                data.set(Data::default());
                login_error.set("Sign in again".into());
            }
        }));
    }
    if setup.toast_ms > 0 {
        crate::toast::dismiss_after(&message, setup.toast_ms);
    }

    // The settings go to the host when they change (not for the first time: that is what they were).
    {
        let (ask, list) = (Rc::clone(requests), list.clone());
        let first = Rc::new(Cell::new(true));
        create_effect(move || {
            let now = list.get();
            if !first.replace(false) {
                ask(Request::Open(now));
            }
        });
    }
    {
        let (c, o) = (confirm.clone(), confirm_open.clone());
        create_effect(move || o.set(c.get().is_some()));
        let (c, o) = (confirm.clone(), confirm_open.clone());
        create_effect(move || {
            if !o.get() && c.get_untracked().is_some() {
                c.set(None);
            }
        });
        let (n, o) = (naming.clone(), naming_open.clone());
        create_effect(move || o.set(n.get().is_some()));
        let (n, o) = (naming.clone(), naming_open.clone());
        create_effect(move || {
            if !o.get() && n.get_untracked().is_some() {
                n.set(None);
            }
        });
        let (ask, target) = (confirm.clone(), confirm_delete.clone());
        let data = data.clone();
        create_effect(move || {
            if let Some(id) = target.get() {
                let title = data
                    .get_untracked()
                    .task(&id)
                    .map(|t| t.content.clone())
                    .unwrap_or_default();
                ask.set(Some(Confirm::DeleteTask { id, title }));
                target.set(None);
            }
        });
    }

    let ctx = Context {
        requests: Rc::clone(requests),
        data: data.clone(),
        zone: setup.zone.clone(),
        clock: Rc::clone(&setup.clock),
        saving,
        error: save_error,
        made_label,
        confirm_delete: confirm_delete.clone(),
    };

    let open_form: Rc<dyn Fn(Target)> = {
        let (target, open) = (form_target.clone(), form_open.clone());
        Rc::new(move |t| {
            target.set(Some(t));
            open.set(true);
        })
    };

    {
        let (request, list, data, open_form, signed) = (
            show_request.clone(),
            list.clone(),
            data.clone(),
            Rc::clone(&open_form),
            signed_in.clone(),
        );
        create_effect(move || {
            let Some((to, add, task)) = request.get() else {
                return;
            };
            if !signed.get() {
                request.set(None);
                return;
            }
            if add {
                list.set(to.clone());
                open_form(Target::New(to));
                request.set(None);
            } else if let Some(id) = task {
                // The task may not be loaded yet: wait for the data.
                if let Some(t) = data.get().task(&id).cloned() {
                    list.set(to);
                    open_form(Target::Edit(Box::new(t)));
                    request.set(None);
                }
            } else {
                list.set(to);
                request.set(None);
            }
        });
    }

    let nav_ctx = Nav {
        data: data.clone(),
        list: list.clone(),
        drawer: drawer.clone(),
        naming: naming.clone(),
        confirm: confirm.clone(),
    };
    let side = {
        let nav = nav_ctx.clone();
        move || {
            div()
                .class("dbk-side")
                .attr("id", "side")
                .child(View::dynamic({
                    let nav = nav.clone();
                    move || nav_view(&nav)
                }))
                .into_view()
        }
    };
    let drawer_sheet = {
        let nav = nav_ctx;
        sheet(&drawer, "Lists", move || nav_view(&nav))
    };

    let main = {
        let (
            signed,
            data,
            list,
            loading,
            requests,
            zone,
            clock,
            open_form,
            drawer,
            naming,
            confirm,
        ) = (
            signed_in.clone(),
            data.clone(),
            list.clone(),
            loading.clone(),
            Rc::clone(requests),
            setup.zone.clone(),
            Rc::clone(&setup.clock),
            Rc::clone(&open_form),
            drawer.clone(),
            naming.clone(),
            confirm.clone(),
        );
        let (server, ask_server, busy, error) = (
            setup.server.clone(),
            setup.ask_server,
            login_busy.clone(),
            login_error.clone(),
        );
        View::dynamic(move || {
            if !signed.get() {
                return login_form(
                    &server,
                    ask_server,
                    Rc::clone(&requests),
                    busy.clone(),
                    error.clone(),
                );
            }
            tasks_screen(&Screen {
                data: data.clone(),
                list: list.clone(),
                loading: loading.clone(),
                requests: Rc::clone(&requests),
                zone: zone.clone(),
                clock: Rc::clone(&clock),
                open_form: Rc::clone(&open_form),
                drawer: drawer.clone(),
                naming: naming.clone(),
                confirm: confirm.clone(),
            })
        })
    };
    let shell_class = {
        let signed = signed_in.clone();
        move || {
            if signed.get() {
                "dbk-shell"
            } else {
                "dbk-shell dbk-signed-out"
            }
            .to_string()
        }
    };
    let side_when = signed_in.clone();
    let blocked_note = {
        let blocked = blocked.clone();
        View::show(
            move || blocked.get(),
            || {
                p().attr("id", "notifications-blocked").class("dbk-note").text("Reminders are off: allow notifications for Daybook in the system settings.").into_view()
            },
        )
    };
    div()
        .attr("id", "daybook")
        .attr_fn("class", shell_class)
        .child(View::show(move || side_when.get(), side))
        .child(div().class("dbk-main").child(blocked_note).child(main))
        .child(drawer_sheet)
        .child(task_dialog(&ctx, &form_target, &form_open))
        .child(confirm_dialog(
            &confirm,
            &confirm_open,
            Rc::clone(requests),
            list.clone(),
        ))
        .child(naming_dialog(&naming, &naming_open, Rc::clone(requests)))
        .child(crate::toast::toast(&message))
        .child(snackbar(&create_signal(None), None))
        .into_view()
}

#[derive(Clone)]
struct Nav {
    data: Signal<Data>,
    list: Signal<ListRef>,
    drawer: Signal<bool>,
    naming: Signal<Option<Naming>>,
    confirm: Signal<Option<Confirm>>,
}

fn nav_row(
    id: &str,
    icon: Element,
    title: &str,
    count: usize,
    on: impl Fn() -> bool + 'static,
    pick: impl Fn() + 'static,
) -> View {
    button()
        .attr("type", "button")
        .attr("id", id)
        .class("dbk-nav-row")
        .class_if("dbk-nav-on", on)
        .child(icon)
        .child(span().class("dbk-nav-title").text(title))
        .child(span().class("dbk-nav-count").text(if count > 0 {
            count.to_string()
        } else {
            String::new()
        }))
        .on_click(pick)
        .into_view()
}

fn nav_view(nav: &Nav) -> View {
    let data = nav.data.get();
    let go = |to: ListRef| {
        let (list, drawer) = (nav.list.clone(), nav.drawer.clone());
        move || {
            list.set(to.clone());
            drawer.set(false);
        }
    };
    let is = |what: ListRef| {
        let list = nav.list.clone();
        move || list.get() == what
    };
    let mut rows: Vec<View> = vec![nav_row(
        "nav-inbox",
        crate::icons::inbox("q-icon"),
        "Inbox",
        open_count(&data.tasks, &ListRef::Inbox),
        is(ListRef::Inbox),
        go(ListRef::Inbox),
    )];
    rows.push(h3().class("dbk-nav-head").text("Lists").into_view());
    for p in &data.projects {
        let target = ListRef::Project(p.id.clone());
        rows.push(nav_row(
            &format!("nav-list-{}", p.id),
            crate::icons::list("q-icon"),
            &p.name,
            open_count(&data.tasks, &target),
            is(target.clone()),
            go(target),
        ));
    }
    let naming = nav.naming.clone();
    rows.push(
        button()
            .attr("type", "button")
            .attr("id", "new-list")
            .class("dbk-nav-row dbk-nav-add")
            .child(crate::icons::plus("q-icon"))
            .child(span().class("dbk-nav-title").text("New list"))
            .on_click(move || naming.set(Some(Naming::NewProject)))
            .into_view(),
    );
    rows.push(h3().class("dbk-nav-head").text("Labels").into_view());
    for l in &data.labels {
        let target = ListRef::Label(l.id.clone());
        rows.push(nav_row(
            &format!("nav-label-{}", l.id),
            crate::icons::tag("q-icon"),
            &l.name,
            open_count(&data.tasks, &target),
            is(target.clone()),
            go(target),
        ));
    }
    let confirm = nav.confirm.clone();
    rows.push(
        button()
            .attr("type", "button")
            .attr("id", "sign-out")
            .class("dbk-nav-row dbk-nav-add")
            .child(crate::icons::logout("q-icon"))
            .child(span().class("dbk-nav-title").text("Sign out"))
            .on_click(move || confirm.set(Some(Confirm::SignOut)))
            .into_view(),
    );
    div().class("dbk-nav").children(rows).into_view()
}

/// A task with the labels it shows.
#[derive(Clone, PartialEq)]
struct Row {
    task: Task,
    labels: Vec<Label>,
}

impl Row {
    /// What identifies the row across updates: its task and everything the row shows of it. A row with the same key is
    /// kept as it is, so the key changes when anything shown does.
    fn key(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        format!("{:?}{:?}", self.task, self.labels).hash(&mut hasher);
        format!("{}:{:x}", self.task.id, hasher.finish())
    }
}

struct Screen {
    data: Signal<Data>,
    list: Signal<ListRef>,
    loading: Signal<bool>,
    requests: Rc<dyn Fn(Request)>,
    zone: Zone,
    clock: Rc<dyn Fn() -> Time>,
    open_form: Rc<dyn Fn(Target)>,
    drawer: Signal<bool>,
    naming: Signal<Option<Naming>>,
    confirm: Signal<Option<Confirm>>,
}

fn title_of(data: &Data, list: &ListRef) -> String {
    match list {
        ListRef::Inbox => "Inbox".into(),
        ListRef::Project(id) => data
            .projects
            .iter()
            .find(|p| &p.id == id)
            .map_or_else(|| "List".into(), |p| p.name.clone()),
        ListRef::Label(id) => data
            .labels
            .iter()
            .find(|l| &l.id == id)
            .map_or_else(|| "Label".into(), |l| l.name.clone()),
    }
}

fn tasks_screen(s: &Screen) -> View {
    let (title_data, title_list) = (s.data.clone(), s.list.clone());
    let bar = {
        let drawer = s.drawer.clone();
        let menu = move || {
            let drawer = drawer.clone();
            button()
                .attr("type", "button")
                .attr("id", "menu")
                .attr("aria-label", "Lists")
                .class("q-iconbtn dbk-menu")
                .child(crate::icons::menu("q-icon"))
                .on_click(move || drawer.set(true))
                .into_view()
        };
        let (data, list, naming, confirm, requests) = (
            s.data.clone(),
            s.list.clone(),
            s.naming.clone(),
            s.confirm.clone(),
            Rc::clone(&s.requests),
        );
        let shape = {
            let (data, list) = (data.clone(), list.clone());
            create_memo(move || {
                let current = list.get();
                data.with(|d| {
                    let own = match &current {
                        ListRef::Inbox => String::new(),
                        ListRef::Project(_) => title_of(d, &current),
                        ListRef::Label(id) => d
                            .labels
                            .iter()
                            .find(|l| &l.id == id)
                            .map(|l| format!("{l:?}"))
                            .unwrap_or_default(),
                    };
                    format!("{current:?}|{own}")
                })
            })
        };
        let actions = move || {
            View::dynamic({
                let shape = shape.clone();
                let (data, list, naming, confirm, requests) = (
                    data.clone(),
                    list.clone(),
                    naming.clone(),
                    confirm.clone(),
                    Rc::clone(&requests),
                );
                move || {
                    let mut buttons: Vec<View> = Vec::new();
                    let reload = Rc::clone(&requests);
                    buttons.push(
                        button()
                            .attr("type", "button")
                            .attr("id", "reload")
                            .attr("aria-label", "Refresh")
                            .class("q-iconbtn")
                            .child(crate::icons::refresh("q-icon"))
                            .on_click(move || reload(Request::Reload))
                            .into_view(),
                    );
                    // Only a change of the view or of its own name or colour rebuilds these buttons.
                    let _ = shape.get();
                    let current = list.get_untracked();
                    let d = data.get_untracked();
                    let (edit, delete): (Option<Naming>, Option<Confirm>) = match &current {
                        ListRef::Inbox => (None, None),
                        ListRef::Project(id) => {
                            let name = title_of(&d, &current);
                            (
                                Some(Naming::Project {
                                    id: id.clone(),
                                    name: name.clone(),
                                }),
                                Some(Confirm::DeleteProject {
                                    id: id.clone(),
                                    name,
                                }),
                            )
                        }
                        ListRef::Label(id) => {
                            d.labels
                                .iter()
                                .find(|l| &l.id == id)
                                .map_or((None, None), |l| {
                                    (
                                        Some(Naming::Label(l.clone())),
                                        Some(Confirm::DeleteLabel {
                                            id: l.id.clone(),
                                            name: l.name.clone(),
                                        }),
                                    )
                                })
                        }
                    };
                    if let Some(edit) = edit {
                        let naming = naming.clone();
                        buttons.push(
                            button()
                                .attr("type", "button")
                                .attr("id", "edit-view")
                                .attr("aria-label", "Rename")
                                .class("q-iconbtn")
                                .child(crate::icons::edit("q-icon"))
                                .on_click(move || naming.set(Some(edit.clone())))
                                .into_view(),
                        );
                    }
                    if let Some(delete) = delete {
                        let confirm = confirm.clone();
                        buttons.push(
                            button()
                                .attr("type", "button")
                                .attr("id", "delete-view")
                                .attr("aria-label", "Delete")
                                .class("q-iconbtn")
                                .child(crate::icons::trash("q-icon"))
                                .on_click(move || confirm.set(Some(delete.clone())))
                                .into_view(),
                        );
                    }
                    div().class("dbk-bar-actions").children(buttons).into_view()
                }
            })
        };
        View::dynamic(move || {
            let title = title_data.with(|d| title_of(d, &title_list.get()));
            app_bar(&title, Some(menu()), vec![actions()]).into_view()
        })
    };
    // What the list shows, worked out once per change of the data or the view; the rows of tasks that did not change are
    // kept (the list is keyed by what a row shows), so checking one off no longer rebuilds every row.
    let shown: Memo<Vec<Row>> = {
        let (data, list, zone) = (s.data.clone(), s.list.clone(), s.zone.clone());
        create_memo(move || {
            let current = list.get();
            data.with(|d| {
                view(&d.tasks, &current, &zone)
                    .into_iter()
                    .map(|task| {
                        let labels = task
                            .label_ids
                            .iter()
                            .filter_map(|id| d.labels.iter().find(|l| &l.id == id).cloned())
                            .collect();
                        Row { task, labels }
                    })
                    .collect()
            })
        })
    };
    let rows = {
        let (zone, clock, requests, open_form, confirm) = (
            s.zone.clone(),
            Rc::clone(&s.clock),
            Rc::clone(&s.requests),
            Rc::clone(&s.open_form),
            s.confirm.clone(),
        );
        let (empty, items) = (shown.clone(), shown);
        div()
            .class("dbk-rows")
            .child(View::show(
                move || empty.with(Vec::is_empty),
                || {
                    p().attr("id", "empty")
                        .class("dbk-empty")
                        .text("Nothing here. Add a task with the + button.")
                        .into_view()
                },
            ))
            .child(ul().class("q-list dbk-tasks").child(View::keyed(
                move || items.get(),
                Row::key,
                move |row: Row| {
                    let row_view = task_row(
                        &row.task,
                        clock(),
                        &zone,
                        &row.labels,
                        &requests,
                        &open_form,
                    );
                    let (confirm, id, title) = (
                        confirm.clone(),
                        row.task.id.clone(),
                        row.task.content.clone(),
                    );
                    li().child(swipe_action(row_view, "Delete", move || {
                        confirm.set(Some(Confirm::DeleteTask {
                            id: id.clone(),
                            title: title.clone(),
                        }))
                    }))
                    .into_view()
                },
            )))
            .into_view()
    };
    let refreshing = s.loading.clone();
    let reload = Rc::clone(&s.requests);
    let list_view = pull_to_refresh(&refreshing, move || reload(Request::Reload), rows);
    let add = {
        let (open_form, list) = (Rc::clone(&s.open_form), s.list.clone());
        quench_ui::kit::fab("Add task", "+", move || {
            open_form(Target::New(list.get_untracked()))
        })
        .attr("id", "add-task")
    };
    let working = s.loading.clone();
    div()
        .class("dbk-screen")
        .child(bar)
        .child(div().class("dbk-body").child(list_view))
        .child(add)
        .child(View::show(
            move || working.get(),
            || {
                div()
                    .attr("id", "loader")
                    .attr("role", "progressbar")
                    .attr("aria-label", "Working")
                    .class("dbk-loader")
                    .child(span().class("dbk-dot"))
                    .into_view()
            },
        ))
        .into_view()
}

fn task_row(
    t: &Task,
    now: Time,
    zone: &Zone,
    labels: &[Label],
    requests: &Rc<dyn Fn(Request)>,
    open_form: &Rc<dyn Fn(Target)>,
) -> View {
    let (id, done) = (t.id.clone(), t.is_completed);
    let toggle = {
        let requests = Rc::clone(requests);
        button()
            .attr("type", "button")
            .attr("id", format!("check-{id}"))
            .attr("role", "checkbox")
            .attr("aria-checked", done.to_string())
            .attr("aria-label", if done { "Reopen" } else { "Complete" })
            .class("dbk-check")
            .class(if done { "dbk-check-on" } else { "" })
            .child(if done {
                crate::icons::check("q-icon")
            } else {
                span()
            })
            .on_click(move || {
                requests(Request::SetCompleted {
                    id: id.clone(),
                    done: !done,
                })
            })
    };
    let mut meta: Vec<View> = Vec::new();
    if let Some(due) = t.due_at {
        let mut chip = span()
            .class("dbk-due")
            .text(format_due(due, t.due_all_day, now, zone));
        if let Some((r, g, b)) = task_urgency(t, now, zone).and_then(daybook_core::Urgency::rgb) {
            chip = chip.css("color", move || format!("rgb({r},{g},{b})"));
        }
        meta.push(chip.into_view());
    }
    if let Some(rule) = &t.recurrence {
        meta.push(
            span()
                .class("dbk-meta")
                .child(crate::icons::repeat("dbk-meta-icon"))
                .text(rule.describe())
                .into_view(),
        );
    }
    for l in labels {
        let (r, g, b) = l.rgb();
        meta.push(
            span()
                .class("dbk-label")
                .css("color", move || format!("rgb({r},{g},{b})"))
                .text(l.name.clone())
                .into_view(),
        );
    }
    let open = {
        let (open_form, task) = (Rc::clone(open_form), t.clone());
        move || open_form(Target::Edit(Box::new(task.clone())))
    };
    div()
        .class("dbk-task")
        .class(if done { "dbk-task-done" } else { "" })
        .child(toggle)
        .child(
            button()
                .attr("type", "button")
                .attr("id", format!("task-{}", t.id))
                .class("dbk-task-main")
                .child(span().class("dbk-task-title").text(t.content.clone()))
                .child(div().class("dbk-task-meta").children(meta))
                .on_click(open),
        )
        .into_view()
}

/// The sign-in form: the server, a user name and a password; the password goes to the host once and is not kept.
fn login_form(
    server: &str,
    ask_server: bool,
    requests: Rc<dyn Fn(Request)>,
    busy: Signal<bool>,
    error: Signal<String>,
) -> View {
    let server_field = Field::new("server", server);
    // A page is always on its own server, so there is nothing to ask and nothing to check.
    let server_field = if ask_server {
        server_field.rule(rules::check(
            |v| v.starts_with("https://") || v.starts_with("http://"),
            "The address starts with https://",
        ))
    } else {
        server_field
    };
    let user = Field::new("username", "").rule(rules::required("Enter your user name"));
    let password = Field::new("password", "").rule(rules::required("Enter your password"));
    let form = Form::new()
        .field(&server_field)
        .field(&user)
        .field(&password);
    let (srv, usr, pwd, working, shown_busy) = (
        server_field.clone(),
        user.clone(),
        password.clone(),
        busy.clone(),
        busy,
    );
    div()
        .attr("id", "login")
        .class("dbk-login")
        .child(h2().text("Sign in to Daybook"))
        .child(
            form.element(move |_| {
                working.set(true);
                requests(Request::Login {
                    server: if ask_server {
                        srv.value().trim().trim_end_matches('/').to_string()
                    } else {
                        String::new()
                    },
                    username: usr.value().trim().to_string(),
                    password: pwd.value(),
                });
            })
            .attr("id", "login-form")
            .child(ask_server.then(|| text_field("Server URL", &server_field, "url", None)))
            .child(text_field("Username", &user, "text", None))
            .child(text_field("Password", &password, "password", None))
            .child(
                p().attr("id", "login-error")
                    .class("dbk-error")
                    .text_fn(move || error.get()),
            )
            .child(form.error_view())
            .child(
                button()
                    .attr("type", "submit")
                    .attr("id", "sign-in")
                    .class("dbk-primary")
                    .attr_fn("aria-busy", move || shown_busy.get().to_string())
                    .text("Sign in"),
            ),
        )
        .into_view()
}

fn confirm_dialog(
    confirm: &Signal<Option<Confirm>>,
    open: &Signal<bool>,
    requests: Rc<dyn Fn(Request)>,
    list: Signal<ListRef>,
) -> View {
    let (words, actions, close) = (confirm.clone(), confirm.clone(), open.clone());
    dialog(
        open,
        "Are you sure?",
        move || {
            let words = words.clone();
            p().attr("id", "confirm-words")
                .class("dbk-note")
                .text_fn(move || match words.get() {
                    Some(Confirm::DeleteTask { title, .. }) => format!("Delete “{title}”?"),
                    Some(Confirm::DeleteProject { name, .. }) => {
                        format!("Delete the list “{name}” and every task in it?")
                    }
                    Some(Confirm::DeleteLabel { name, .. }) => {
                        format!("Delete the label “{name}”? Tasks keep everything else.")
                    }
                    Some(Confirm::SignOut) => {
                        "Sign out? Reminders stop until you sign in again.".to_string()
                    }
                    None => String::new(),
                })
                .into_view()
        },
        move || {
            let (confirm, close, requests, list) = (
                actions.clone(),
                close.clone(),
                Rc::clone(&requests),
                list.clone(),
            );
            let no = close.clone();
            vec![
                button()
                    .attr("type", "button")
                    .attr("id", "confirm-no")
                    .text("Cancel")
                    .on_click(move || no.set(false))
                    .into_view(),
                button()
                    .attr("type", "button")
                    .attr("id", "confirm-yes")
                    .class("dbk-danger")
                    .text("Yes")
                    .on_click(move || {
                        match confirm.get_untracked() {
                            Some(Confirm::DeleteTask { id, .. }) => {
                                requests(Request::Delete { id })
                            }
                            Some(Confirm::DeleteProject { id, .. }) => {
                                if list.get_untracked() == ListRef::Project(id.clone()) {
                                    list.set(ListRef::Inbox);
                                }
                                requests(Request::DeleteProject { id });
                            }
                            Some(Confirm::DeleteLabel { id, .. }) => {
                                if list.get_untracked() == ListRef::Label(id.clone()) {
                                    list.set(ListRef::Inbox);
                                }
                                requests(Request::DeleteLabel { id });
                            }
                            Some(Confirm::SignOut) => requests(Request::Logout),
                            None => {}
                        }
                        close.set(false);
                    })
                    .into_view(),
            ]
        },
    )
}

fn naming_dialog(
    naming: &Signal<Option<Naming>>,
    open: &Signal<bool>,
    requests: Rc<dyn Fn(Request)>,
) -> View {
    let (state, close) = (naming.clone(), open.clone());
    dialog(
        open,
        "Name",
        move || {
            let current = state.get_untracked();
            let (start, is_label) = match &current {
                Some(Naming::Project { name, .. }) => (name.clone(), false),
                Some(Naming::Label(l)) => (l.name.clone(), true),
                _ => (String::new(), false),
            };
            let name = Field::new("name", &start);
            let color = create_signal(match &current {
                Some(Naming::Label(l)) => l.color.clone(),
                _ => LABEL_COLORS[0].to_string(),
            });
            let swatches: Vec<View> = if is_label {
                LABEL_COLORS
                    .iter()
                    .map(|hex| {
                        let (on, set, value) = (color.clone(), color.clone(), (*hex).to_string());
                        let selected = value.clone();
                        button()
                            .attr("type", "button")
                            .attr("id", format!("name-color-{}", &value[1..]))
                            .attr("aria-label", value.clone())
                            .class("dbk-swatch")
                            .class_if("dbk-swatch-on", move || on.get() == selected)
                            .css("background", {
                                let v = value.clone();
                                move || v.clone()
                            })
                            .on_click(move || set.set(value.clone()))
                            .into_view()
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let (requests, close, state, field) = (
                Rc::clone(&requests),
                close.clone(),
                state.clone(),
                name.clone(),
            );
            let cancel = close.clone();
            div()
                .child(text_field("Name", &name, "text", None))
                .child(div().class("dbk-swatches").children(swatches))
                .child(
                    div()
                        .class("q-dialog-actions")
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", "name-cancel")
                                .text("Cancel")
                                .on_click(move || cancel.set(false)),
                        )
                        .child(
                            button()
                                .attr("type", "button")
                                .attr("id", "name-ok")
                                .class("dbk-primary-small")
                                .text("Save")
                                .on_click(move || {
                                    let text = field.value_untracked();
                                    if text.trim().is_empty() {
                                        field.set_error("Enter a name");
                                        return;
                                    }
                                    match state.get_untracked() {
                                        Some(Naming::NewProject) => {
                                            requests(Request::CreateProject { name: text })
                                        }
                                        Some(Naming::Project { id, .. }) => {
                                            requests(Request::RenameProject { id, name: text })
                                        }
                                        Some(Naming::Label(l)) => requests(Request::UpdateLabel {
                                            id: l.id,
                                            name: text,
                                            color: color.get_untracked(),
                                        }),
                                        None => {}
                                    }
                                    close.set(false);
                                }),
                        ),
                )
                .into_view()
        },
        Vec::new,
    )
}
