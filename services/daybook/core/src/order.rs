//! Which tasks a view shows and in what order.

use std::cmp::Ordering;

use quench_tz::Zone;

use crate::calendar::due_sort_key;
use crate::model::{ListRef, Task};

/// Open tasks by due date (an all-day task before the timed ones of its day; those with no date last), then by their own
/// order and creation; completed ones after them by last change, newest first.
#[must_use]
pub fn order_tasks(tasks: &[Task], zone: &Zone) -> Vec<Task> {
    let mut open: Vec<&Task> = tasks.iter().filter(|t| !t.is_completed).collect();
    open.sort_by(|a, b| compare_open(a, b, zone));
    let mut done: Vec<&Task> = tasks.iter().filter(|t| t.is_completed).collect();
    done.sort_by_key(|t| std::cmp::Reverse(t.updated_at));
    open.into_iter().chain(done).cloned().collect()
}

fn compare_open(a: &Task, b: &Task, zone: &Zone) -> Ordering {
    let key = |t: &Task| t.due_at.map(|d| due_sort_key(d, t.due_all_day, zone));
    match (key(a), key(b)) {
        (Some(x), Some(y)) if x != y => return x.cmp(&y),
        (Some(_), None) => return Ordering::Less,
        (None, Some(_)) => return Ordering::Greater,
        _ => {}
    }
    a.sort_order
        .cmp(&b.sort_order)
        .then(a.created_at.cmp(&b.created_at))
}

/// The open tasks of one view, ordered (completed ones are not shown).
#[must_use]
pub fn view(tasks: &[Task], list: &ListRef, zone: &Zone) -> Vec<Task> {
    let shown: Vec<Task> = tasks
        .iter()
        .filter(|t| list.contains(t) && !t.is_completed)
        .cloned()
        .collect();
    order_tasks(&shown, zone)
}

/// How many open tasks a view holds.
#[must_use]
pub fn open_count(tasks: &[Task], list: &ListRef) -> usize {
    tasks
        .iter()
        .filter(|t| !t.is_completed && list.contains(t))
        .count()
}
