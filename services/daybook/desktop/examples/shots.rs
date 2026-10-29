//! Draws the screens offscreen, with sample tasks, to PNG files: `cargo run -p daybook-desktop --example shots -- <dir>`.
//! Nothing is opened and no service is contacted.

use std::rc::Rc;

use daybook_app::{Daybook, Setup, daybook};
use daybook_core::calendar::{all_day_time, at_local};
use daybook_core::{Data, Label, ListRef, Msg, Project, Recurrence, Task, Unit};
use quench_core::time::{Duration, Time};
use quench_layout::LayoutBox;
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

struct Page {
    r: Rc<NativeRenderer>,
    canvas: Canvas,
    boxes: Rc<LayoutBox>,
}

impl Page {
    fn draw(&mut self) {
        self.boxes = self.r.render(&mut self.canvas);
    }

    fn tap(&mut self, id: &str) {
        let node = find(&self.r, self.r.root(), id).unwrap_or_else(|| panic!("no element {id}"));
        let (x, y) = locate(&self.r, node, self.canvas.width(), self.canvas.height())
            .unwrap_or_else(|| panic!("{id} is not on the screen"));
        self.r.pointer_down(x, y);
        self.r.pointer_up(x, y);
        // Let the animations that bring a dialog in run their course.
        std::thread::sleep(std::time::Duration::from_millis(350));
        self.r.tick();
        self.r
            .advance_animations(std::time::Duration::from_millis(350));
        self.r.tick();
        self.draw();
    }

    fn save(&mut self, dir: &str, name: &str) {
        // The canvas is blue, green, red, alpha.
        let mut rgba = self.canvas.bytes().to_vec();
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let png = quench_image::encode_png(
            u32::try_from(self.canvas.width()).unwrap(),
            u32::try_from(self.canvas.height()).unwrap(),
            &rgba,
        );
        std::fs::write(format!("{dir}/{name}.png"), png).unwrap();
    }
}

/// What a drawn page needs kept alive: the page, the app's inbox, and its mount.
type Drawn = (Page, Rc<dyn Fn(Msg)>, quench_ui::Mount<NativeRenderer>);

fn page(width: usize, height: usize, has_session: bool) -> Drawn {
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
    renderer.add_stylesheet(daybook_app::STYLE);
    renderer.set_viewport(width as f32, height as f32);
    let zone = Zone::named("Europe/Warsaw").unwrap();
    let Daybook { view, deliver } = daybook(
        Rc::new(|_| {}),
        Setup {
            ask_server: true,
            server: "https://ennor.ddns.net".into(),
            list: ListRef::Inbox,
            has_session,
            zone,
            clock: Rc::new(Time::now),
            toast_ms: 0,
        },
    );
    let mounted = mount(&renderer, &renderer.root(), view);
    let mut canvas = Canvas::new(width, height);
    let boxes = renderer.render(&mut canvas);
    (
        Page {
            r: renderer,
            canvas,
            boxes,
        },
        deliver,
        mounted,
    )
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("a directory for the PNG files");
    std::fs::create_dir_all(&dir).unwrap();
    let zone = Zone::named("Europe/Warsaw").unwrap();
    let now = Time::now() + Duration::seconds(0);

    let (mut p, _d, _m) = page(420, 860, false);
    p.save(&dir, "1-sign-in");

    let (mut p, deliver, _m) = page(420, 860, true);
    deliver(Msg::Data(Box::new(sample(&zone, now))));
    p.draw();
    p.save(&dir, "2-inbox");
    p.tap("task-b");
    p.save(&dir, "3-form");
    p.tap("due");
    p.save(&dir, "4-date");
    p.tap("date-ok");
    p.save(&dir, "4b-kind");
    p.tap("choose-time");
    p.save(&dir, "4c-time");
    p.tap("time-ok");
    p.tap("repeat");
    p.tap("repeat-custom");
    p.save(&dir, "4d-repeat");

    let (mut p, deliver, _m) = page(420, 860, true);
    deliver(Msg::Data(Box::new(sample(&zone, now))));
    p.draw();
    p.tap("menu");
    p.save(&dir, "5-lists");

    let (mut p, deliver, _m) = page(1100, 700, true);
    deliver(Msg::Data(Box::new(sample(&zone, now))));
    p.draw();
    p.save(&dir, "6-wide");
}
