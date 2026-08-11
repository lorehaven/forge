use crate::config::Job;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;

pub fn dry_run(job: &Job) -> Result<bool, Box<dyn std::error::Error>> {
    println!("Executing dry-run");

    let mut cmd = Command::new("rsync");
    cmd.arg("-avz")
        .arg("--dry-run")
        .arg("--itemize-changes")
        .arg("--delete");

    for skip in &job.skip {
        cmd.arg(format!("--exclude={}", skip));
    }

    cmd.arg(format!("{}/", job.src));
    cmd.arg(format!("{}/", job.dest));

    let lines = run_command(&mut cmd)?;

    let mut count = 0;

    for line in lines {
        match classify_line(&line) {
            Some(Change::Delete(path)) => {
                println!("DELETE  {path}");
                count += 1;
            }
            Some(Change::Create(path)) => {
                if let Some(path) = path {
                    println!("CREATE {path}");
                }
                count += 1;
            }
            Some(Change::Modify(path)) => {
                if let Some(path) = path {
                    println!("MODIFY {path}");
                }
                count += 1;
            }
            None => {}
        }
    }

    println!("{}: {} total changes", job.desc, count);

    Ok(count > 0)
}

/// One `--itemize-changes` line's meaning, or `None` for a line dry-run
/// doesn't report as a change (a directory entering sync unchanged, a
/// progress line, etc).
#[derive(Debug, PartialEq, Eq)]
enum Change {
    Delete(String),
    /// `None` when the line had no trailing path to show - still a change to
    /// count, just nothing to print.
    Create(Option<String>),
    Modify(Option<String>),
}

fn classify_line(line: &str) -> Option<Change> {
    if line.starts_with("*deleting") {
        return Some(Change::Delete(line.replace("*deleting ", "")));
    }
    if line.starts_with(">f+++++++++") || line.starts_with("<f+++++++++") || line.starts_with("cd+")
    {
        return Some(Change::Create(
            line.split_once(' ').map(|(_, p)| p.to_string()),
        ));
    }
    if line.starts_with(">f") {
        return Some(Change::Modify(
            line.split_once(' ').map(|(_, p)| p.to_string()),
        ));
    }
    None
}

pub fn update(job: &Job) -> Result<(), Box<dyn std::error::Error>> {
    println!("Executing update");

    let mut cmd = Command::new("rsync");
    cmd.arg("-avzu").arg("--progress");

    if job.delete {
        cmd.arg("--delete");
    }

    for skip in &job.skip {
        cmd.arg(format!("--exclude={}", skip));
    }

    cmd.arg(format!("{}/", job.src));
    cmd.arg(format!("{}/", job.dest));

    run_command_async(&mut cmd)?;

    Ok(())
}

fn run_command(command: &mut Command) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = process.stdout.take().unwrap();
    let stderr = process.stderr.take().unwrap();

    let stdout_handle = std::thread::spawn(move || {
        BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .map(|line| line.trim().to_string())
            .filter(|file_name| !file_name.ends_with('/'))
            .collect::<Vec<String>>()
    });

    let stderr_handle = std::thread::spawn(move || {
        BufReader::new(stderr)
            .lines()
            .map_while(Result::ok)
            .collect::<Vec<String>>()
    });

    let status = process.wait()?;
    let result = stdout_handle.join().unwrap();
    let stderr_lines = stderr_handle.join().unwrap();

    if !status.success() {
        return Err(format!("rsync exited with {status}: {}", stderr_lines.join(" | ")).into());
    }

    Ok(result)
}

pub fn run_command_async(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    fn thread_spawn(
        child: impl Read + Send + 'static,
        sender: std::sync::mpsc::Sender<String>,
    ) -> JoinHandle<()> {
        std::thread::spawn(move || {
            let reader_err = BufReader::new(child);
            for line in reader_err.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        })
    }

    let mut process = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = process.stdout.take().unwrap();
    let stderr = process.stderr.take().unwrap();

    let (stdout_tx, stdout_rx) = std::sync::mpsc::channel();
    let (stderr_tx, stderr_rx) = std::sync::mpsc::channel();

    let stdout_handle = thread_spawn(stdout, stdout_tx);
    let stderr_handle = thread_spawn(stderr, stderr_tx);

    let stdout_thread = std::thread::spawn(move || {
        for line in stdout_rx {
            eprintln!("{line}");
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        for line in stderr_rx {
            eprintln!("{line}");
        }
    });

    let status = process.wait()?;

    stdout_handle.join().unwrap();
    stderr_handle.join().unwrap();
    stdout_thread.join().unwrap();
    stderr_thread.join().unwrap();

    if !status.success() {
        return Err(format!("rsync exited with {status}").into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_line_recognizes_a_deletion() {
        let change = classify_line("*deleting old-file.txt");
        assert_eq!(change, Some(Change::Delete("old-file.txt".to_string())));
    }

    #[test]
    fn classify_line_recognizes_a_new_file() {
        let change = classify_line(">f+++++++++ new-file.txt");
        assert_eq!(
            change,
            Some(Change::Create(Some("new-file.txt".to_string())))
        );
    }

    #[test]
    fn classify_line_recognizes_a_new_file_received_from_the_other_side() {
        let change = classify_line("<f+++++++++ new-file.txt");
        assert_eq!(
            change,
            Some(Change::Create(Some("new-file.txt".to_string())))
        );
    }

    #[test]
    fn classify_line_recognizes_a_new_directory() {
        let change = classify_line("cd+++++++++ new-dir/");
        assert_eq!(change, Some(Change::Create(Some("new-dir/".to_string()))));
    }

    #[test]
    fn classify_line_recognizes_a_modified_file() {
        let change = classify_line(">f.st...... existing-file.txt");
        assert_eq!(
            change,
            Some(Change::Modify(Some("existing-file.txt".to_string())))
        );
    }

    #[test]
    fn classify_line_counts_a_change_even_without_a_parseable_path() {
        // No space in the line, so there's nothing after the itemize flags to
        // print - but rsync still reported a change, so it must still count.
        let change = classify_line(">f+++++++++");
        assert_eq!(change, Some(Change::Create(None)));
    }

    #[test]
    fn classify_line_ignores_unrelated_output() {
        assert_eq!(classify_line("sending incremental file list"), None);
        assert_eq!(classify_line(""), None);
        assert_eq!(classify_line("total size is 1,234"), None);
    }
}
