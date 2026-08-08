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
        if line.starts_with("*deleting") {
            println!("DELETE  {}", line.replace("*deleting ", ""));
            count += 1;
        } else if line.starts_with(">f+++++++++")
            || line.starts_with("<f+++++++++")
            || line.starts_with("cd+")
        {
            if let Some(path) = line.split_once(' ').map(|(_, p)| p) {
                println!("CREATE {}", path);
            }
            count += 1;
        } else if line.starts_with(">f") {
            if let Some(path) = line.split_once(' ').map(|(_, p)| p) {
                println!("MODIFY {}", path);
            }
            count += 1;
        }
    }

    println!("{}: {} total changes", job.desc, count);

    Ok(count > 0)
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

    let stderr_handle =
        std::thread::spawn(move || BufReader::new(stderr).lines().map_while(Result::ok).collect::<Vec<String>>());

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
