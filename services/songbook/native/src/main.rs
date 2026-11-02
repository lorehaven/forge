//! `songbook_native`: reads requests from stdin, one JSON line each, and writes the answers to stdout (see `proto`). Each
//! request is answered on a thread of its own, so a slow one does not hold the others up.

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

use songbook_native::Brain;
use songbook_native::store::Store;

fn main() {
    let brain = Arc::new(Brain::new(Store::standard()));
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let mut running = Vec::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let (brain, out) = (Arc::clone(&brain), Arc::clone(&out));
        running.push(std::thread::spawn(move || {
            let answer = brain.handle(&line);
            if let Ok(mut out) = out.lock() {
                let _ = writeln!(out, "{answer}");
                let _ = out.flush();
            }
        }));
    }
    // The shell closed its end: answer what is still being asked.
    for r in running {
        let _ = r.join();
    }
}
