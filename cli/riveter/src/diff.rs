//! A line diff in the unified format, for `riveter diff`.
//!
//! What `kubectl diff` gets from the system's `diff -u`, done here so that comparing a manifest with the cluster needs no
//! program but riveter. The two texts are YAML of one object each, a few hundred lines, so the common start and end are cut
//! off first and the middle is compared with Myers' algorithm.

/// Lines of unchanged text shown around a change.
const CONTEXT: usize = 3;

/// More lines than this, and the middle is shown as wholly replaced rather than searched for what stayed: the search keeps a
/// trace that grows with both the length and the number of changes.
const SEARCH_LIMIT: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    /// The same line, at these positions of the old and the new text.
    Same(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// The unified diff of `old` to `new`, headed by the two names; `None` when the texts are the same.
#[must_use]
pub fn unified(old: &str, new: &str, old_name: &str, new_name: &str) -> Option<String> {
    if old == new {
        return None;
    }
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let ops = script(&a, &b);
    if ops.iter().all(|op| matches!(op, Op::Same(..))) {
        return None;
    }

    let mut out = format!("--- {old_name}\n+++ {new_name}\n");
    for range in hunks(&ops) {
        out.push_str(&render(&ops, &range, &a, &b));
    }
    Some(out)
}

/// The edits that turn `a` into `b`, in order.
fn script(a: &[&str], b: &[&str]) -> Vec<Op> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a_mid, b_mid) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);

    let mut ops: Vec<Op> = (0..prefix).map(|i| Op::Same(i, i)).collect();
    for op in middle(a_mid, b_mid) {
        ops.push(match op {
            Op::Same(i, j) => Op::Same(i + prefix, j + prefix),
            Op::Delete(i) => Op::Delete(i + prefix),
            Op::Insert(j) => Op::Insert(j + prefix),
        });
    }
    for k in 0..suffix {
        ops.push(Op::Same(a.len() - suffix + k, b.len() - suffix + k));
    }
    ops
}

/// Myers' O(ND) search for the shortest edit script between two slices that share no first or last line.
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    reason = "the diagonals are signed offsets into arrays that are sized to hold them, named as in the paper"
)]
fn middle(a: &[&str], b: &[&str]) -> Vec<Op> {
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return (0..m).map(Op::Insert).collect();
    }
    if m == 0 {
        return (0..n).map(Op::Delete).collect();
    }
    if n + m > SEARCH_LIMIT {
        return (0..n)
            .map(Op::Delete)
            .chain((0..m).map(Op::Insert))
            .collect();
    }

    let max = (n + m) as isize;
    let offset = max;
    let mut v = vec![0_isize; 2 * max as usize + 2];
    let mut trace: Vec<Vec<isize>> = Vec::new();

    'search: for d in 0..=max {
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            let index = (k + offset) as usize;
            let mut x = if k == -d || (k != d && v[index - 1] < v[index + 1]) {
                v[index + 1]
            } else {
                v[index - 1] + 1
            };
            let mut y = x - k;
            while (x as usize) < n && (y as usize) < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[index] = x;
            if x as usize >= n && y as usize >= m {
                break 'search;
            }
            k += 2;
        }
    }

    // Back from the end to the start, one round at a time.
    let mut ops = Vec::new();
    let (mut x, mut y) = (n as isize, m as isize);
    for d in (0..trace.len() as isize).rev() {
        let before = &trace[d as usize];
        let k = x - y;
        let previous = if k == -d
            || (k != d && before[(k - 1 + offset) as usize] < before[(k + 1 + offset) as usize])
        {
            k + 1
        } else {
            k - 1
        };
        let previous_x = before[(previous + offset) as usize];
        let previous_y = previous_x - previous;
        while x > previous_x && y > previous_y {
            x -= 1;
            y -= 1;
            ops.push(Op::Same(x as usize, y as usize));
        }
        if d > 0 {
            if x == previous_x {
                y -= 1;
                ops.push(Op::Insert(y as usize));
            } else {
                x -= 1;
                ops.push(Op::Delete(x as usize));
            }
        }
        x = previous_x;
        y = previous_y;
    }
    ops.reverse();
    ops
}

/// The ranges of the script that are changes with their context, joined where the context of two meets.
fn hunks(ops: &[Op]) -> Vec<std::ops::Range<usize>> {
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for (at, _) in ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Same(..)))
    {
        let start = at.saturating_sub(CONTEXT);
        let end = (at + CONTEXT + 1).min(ops.len());
        match ranges.last_mut() {
            Some(last) if start <= last.end => last.end = last.end.max(end),
            _ => ranges.push(start..end),
        }
    }
    ranges
}

/// How many lines of the old and of the new text a stretch of the script covers.
fn counts(ops: &[Op]) -> (usize, usize) {
    ops.iter().fold((0, 0), |(old, new), op| match op {
        Op::Same(..) => (old + 1, new + 1),
        Op::Delete(_) => (old + 1, new),
        Op::Insert(_) => (old, new + 1),
    })
}

/// The hunk at `range` as text: its `@@` line, then each line with its marker.
fn render(ops: &[Op], range: &std::ops::Range<usize>, a: &[&str], b: &[&str]) -> String {
    let hunk = &ops[range.clone()];
    let (before_old, before_new) = counts(&ops[..range.start]);
    let (old_len, new_len) = counts(hunk);
    // An empty side is named by the line before it, as `diff -u` does.
    let first = |len: usize, before: usize| if len == 0 { before } else { before + 1 };

    let mut out = format!(
        "@@ -{},{old_len} +{},{new_len} @@\n",
        first(old_len, before_old),
        first(new_len, before_new)
    );
    for op in hunk {
        match *op {
            Op::Same(i, _) => {
                out.push(' ');
                out.push_str(a[i]);
            }
            Op::Delete(i) => {
                out.push('-');
                out.push_str(a[i]);
            }
            Op::Insert(j) => {
                out.push('+');
                out.push_str(b[j]);
            }
        }
        out.push('\n');
    }
    out
}
