//! How long a frame and a change take: `cargo run --release -p daybook-desktop --example bench [-- style.css]`.
//! Draws the inbox with many tasks at phone size, offscreen; nothing is opened and no service is contacted.

use std::rc::Rc;

use daybook_app::{Daybook, Setup, daybook};
use daybook_core::calendar::{all_day_time, at_local};
use daybook_core::{Data, Label, ListRef, Msg, Project, Recurrence, Task, Unit};
use quench_core::time::Time;
use quench_raster::Canvas;
use quench_style::theme::Theme;
use quench_tz::Zone;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

fn task(id: &str, content: &str) -> Task {
    Task {
        id: id.into(),
        project_id: None,
        content: content.into(),
        description: None,
        is_completed: false,
        sort_order: 0,
        due_at: None,
        due_all_day: false,
        remind_at: None,
        label_ids: vec![],
        recurrence: None,
        created_at: Time::from_unix(1),
        updated_at: Time::from_unix(1),
    }
}

fn sample(zone: &Zone, now: Time) -> Data {
    let today = zone.civil_at(now).date();
    let day = |n: i64| today.add_days(n).unwrap();
    let mut a = task("a", "Pay the rent");
    a.due_at = Some(all_day_time(day(-1)));
    a.due_all_day = true;
    let mut b = task("b", "Call the dentist");
    b.due_at = at_local(day(0), 15, 30, zone);
    let mut c = task("c", "Water the plants");
    c.due_at = Some(all_day_time(day(0)));
    c.due_all_day = true;
    c.recurrence = Some(Recurrence::every(Unit::Day, zone.name()));
    c.label_ids = vec!["l1".into()];
    let mut d = task("d", "Buy a birthday present");
    d.due_at = Some(all_day_time(day(2)));
    d.due_all_day = true;
    let mut e = task("e", "Book the holiday");
    e.due_at = Some(all_day_time(day(6)));
    e.due_all_day = true;
    let mut f = task("f", "Renew the passport");
    f.due_at = Some(all_day_time(day(40)));
    f.due_all_day = true;
    Data {
        projects: vec![
            Project {
                id: "p1".into(),
                name: "Home".into(),
                sort_order: 0,
            },
            Project {
                id: "p2".into(),
                name: "Work".into(),
                sort_order: 1,
            },
        ],
        labels: vec![Label {
            id: "l1".into(),
            name: "chores".into(),
            color: "#22c55e".into(),
        }],
        tasks: vec![a, b, c, d, e, f, task("g", "Think about the garden")],
    }
}

fn many(zone: &Zone, now: Time, n: usize) -> Data {
    let mut data = sample(zone, now);
    let template = data.tasks.clone();
    data.tasks = (0..n)
        .map(|i| {
            let mut t = template[i % template.len()].clone();
            t.id = format!("t{i}");
            t.content = format!("{} {i}", t.content);
            t
        })
        .collect();
    data
}

fn renderer(css: &str) -> Rc<NativeRenderer> {
    let renderer = Rc::new(NativeRenderer::new());
    for (scheme, theme) in [("dark", Theme::DefaultDark), ("light", Theme::DefaultLight)] {
        let rules = Theme::theme(theme);
        renderer.add_stylesheet(
            &[
                "@media (prefers-color-scheme: ",
                scheme,
                ") {\n",
                &rules,
                "\n}",
            ]
            .concat(),
        );
    }
    for sheet in [
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    ] {
        renderer.add_stylesheet(&sheet);
    }
    renderer.add_stylesheet(css);
    renderer
}

fn find(
    r: &NativeRenderer,
    node: quench_ui_native::NodeId,
    id: &str,
) -> Option<quench_ui_native::NodeId> {
    if r.attr(node, "id").as_deref() == Some(id) {
        return Some(node);
    }
    r.children(node).into_iter().find_map(|c| find(r, c, id))
}

fn descendants(
    r: &NativeRenderer,
    node: quench_ui_native::NodeId,
    out: &mut Vec<quench_ui_native::NodeId>,
) {
    out.push(node);
    for c in r.children(node) {
        descendants(r, c, out);
    }
}

/// Where on the screen the element is, found by asking what is at each point.
fn locate(
    r: &NativeRenderer,
    node: quench_ui_native::NodeId,
    width: usize,
    height: usize,
) -> Option<(f32, f32)> {
    let mut inside = Vec::new();
    descendants(r, node, &mut inside);
    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            if r.element_at(x as f32, y as f32)
                .is_some_and(|n| inside.contains(&n))
            {
                return Some((x as f32 + 2.0, y as f32 + 2.0));
            }
        }
    }
    None
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn run(label: &str, css: &str, tasks: usize) {
    let (width, height) = (1080, 2400);
    let r = renderer(css);
    r.set_viewport(width as f32, height as f32);
    let zone = Zone::named("Europe/Warsaw").unwrap();
    let now = Time::now();
    let Daybook { view, deliver } = daybook(
        Rc::new(|_| {}),
        Setup {
            ask_server: true,
            server: "https://ennor.ddns.net".into(),
            list: ListRef::Inbox,
            has_session: true,
            zone: zone.clone(),
            clock: Rc::new(Time::now),
            toast_ms: 0,
        },
    );
    let _mounted = mount(&r, &r.root(), view);
    let mut canvas = Canvas::new(width, height);
    let mut data = many(&zone, now, tasks);
    let t = std::time::Instant::now();
    deliver(Msg::Data(Box::new(data.clone())));
    let first = t.elapsed();
    r.render(&mut canvas);
    // A frame with nothing changed.
    let n = 40;
    // The best of a few: the machine is shared, the cost of a frame is what it takes when nothing else runs.
    let frame = (0..n)
        .map(|_| {
            let t = std::time::Instant::now();
            r.render(&mut canvas);
            ms(t.elapsed())
        })
        .fold(f64::MAX, f64::min);
    // One task checked off: new data in, then a frame.
    let (mut deliver_total, mut frame_total) = (0.0, 0.0);
    for i in 0..10_usize {
        let at = i % tasks;
        data.tasks[at].is_completed = !data.tasks[at].is_completed;
        let t = std::time::Instant::now();
        deliver(Msg::Data(Box::new(data.clone())));
        deliver_total += ms(t.elapsed());
        let t = std::time::Instant::now();
        r.render(&mut canvas);
        frame_total += ms(t.elapsed());
    }
    // The task form open: a dialog over the list, the frames of its coming in and a frame at rest.
    deliver(Msg::Data(Box::new(many(&zone, now, tasks))));
    r.render(&mut canvas);
    let task = find(&r, r.root(), "task-t3").expect("task-t3");
    let (x, y) = locate(&r, task, width, height).expect("on screen");
    r.pointer_down(x, y);
    r.pointer_up(x, y);
    r.tick();
    r.advance_animations(std::time::Duration::from_millis(350));
    r.tick();
    r.render(&mut canvas);
    let dialog = (0..n)
        .map(|_| {
            let t = std::time::Instant::now();
            r.render(&mut canvas);
            ms(t.elapsed())
        })
        .fold(f64::MAX, f64::min);
    println!(
        "{label:>14} {tasks:>4} tasks: first data {:6.1} ms | frame {frame:6.1} ms | change: update {:6.1} ms + frame {:6.1} ms | dialog frame {dialog:6.1} ms",
        ms(first),
        deliver_total / 10.0,
        frame_total / 10.0
    );
}

fn main() {
    let custom = std::env::args()
        .nth(1)
        .map(|p| std::fs::read_to_string(p).unwrap());
    for tasks in [7, 30, 60] {
        run("style.css", daybook_app::STYLE, tasks);
        if let Some(css) = &custom {
            run("custom", css, tasks);
        }
    }
}
