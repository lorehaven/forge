//! The picker.
//!
//! For the case the flags are clumsy at: deciding what you need by looking at
//! what is already up, then starting it. `foreman start conveyor` is the same
//! thing in one line once you know what you want.

use anyhow::Result;
use quench_cli::prelude::{DIM, GREEN, RESET, print_box_banner};
use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

use crate::commands;
use crate::estate::Estate;
use crate::services;
use crate::ui;

pub struct Picker<'a> {
    estate: &'a Estate,
    picked: Vec<String>,
}

impl<'a> Picker<'a> {
    pub fn new(estate: &'a Estate) -> Self {
        Self {
            estate,
            picked: Vec::new(),
        }
    }

    /// Seeded from the command line, so `foreman repl conveyor` opens with the
    /// obvious thing already ticked.
    pub fn seed(&mut self, names: &[String]) -> Result<()> {
        if !names.is_empty() {
            self.picked = self.estate.resolve_names(names)?;
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        let mut editor = DefaultEditor::new()?;

        print_box_banner(
            &format!("{} picker", self.estate.config.project.name),
            "pick services, then `up`",
        );
        ui::info("picker", "`help` for the rest, `quit` to leave");
        self.list()?;

        loop {
            let prompt = if self.picked.is_empty() {
                "foreman> ".to_string()
            } else {
                format!("foreman ({})> ", self.picked.join(","))
            };

            let line = match editor.readline(&prompt) {
                Ok(line) => line,
                // Ctrl-C and Ctrl-D. Leaving without starting anything is a
                // valid answer.
                Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
                Err(err) => return Err(err.into()),
            };

            let words: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            let Some(command) = words.first() else {
                continue;
            };
            let _ = editor.add_history_entry(line.trim());

            if matches!(command.as_str(), "quit" | "exit" | "q") {
                break;
            }

            // A failing command should cost you the command, not the session.
            if let Err(error) = self.dispatch(command, &words[1..]) {
                ui::error("error", error.to_string());
            }
        }

        ui::info(
            "picker",
            "left the picker; `foreman status` shows what is up",
        );
        Ok(())
    }

    fn dispatch(&mut self, command: &str, args: &[String]) -> Result<()> {
        match command {
            "help" | "h" | "?" => help(),
            "list" | "ls" | "l" => self.list()?,
            "all" => {
                self.picked = self.estate.service_names();
                self.list()?;
            }
            "none" | "clear" => {
                self.picked.clear();
                self.list()?;
            }
            "running" => {
                self.picked = services::running_services(self.estate);
                self.list()?;
            }
            "up" | "start" => {
                if self.picked.is_empty() {
                    ui::warn("picker", "nothing selected - `all` for the whole estate");
                } else {
                    let picked = self.picked.clone();
                    commands::start(self.estate, &picked)?;
                    // Dependencies may have joined the selection on the way in;
                    // showing them keeps `down` from being a surprise.
                    self.picked = self.estate.with_dependencies(&picked)?;
                    self.list()?;
                }
            }
            "down" | "stop" => {
                if self.picked.is_empty() {
                    ui::warn("picker", "nothing selected");
                } else {
                    services::stop(self.estate, &self.picked)?;
                    services::report_strays(self.estate)?;
                    self.list()?;
                }
            }
            "status" => {
                commands::status(self.estate)?;
                ui::blank();
            }
            "logs" => match args.first() {
                // Interrupting `tail -f` is how you get back here, so the
                // picker must not take that as a reason to exit.
                Some(name) => commands::logs(self.estate, name)?,
                None => ui::warn("picker", "usage: logs <service>"),
            },
            "db" => commands::db(self.estate)?,
            "reset" => commands::reset(self.estate)?,
            "run" => match args.first() {
                Some(task) => commands::run_task(self.estate, task, &self.picked)?,
                None => ui::warn("picker", "usage: run <task>"),
            },
            _ => {
                // Anything else is a selection: several at a time is fine.
                for token in std::iter::once(&command.to_string()).chain(args) {
                    self.toggle(token)?;
                }
                self.list()?;
            }
        }

        Ok(())
    }

    fn toggle(&mut self, token: &str) -> Result<()> {
        let names = self.estate.service_names();

        // A number is an index into the list as printed, which is the order the
        // config file gives.
        let name = match token.parse::<usize>() {
            Ok(index) if index >= 1 && index <= names.len() => names[index - 1].clone(),
            Ok(index) => {
                ui::warn("picker", format!("no service {index}"));
                return Ok(());
            }
            Err(_) => token.to_string(),
        };

        if !names.contains(&name) {
            ui::warn("picker", format!("unknown service '{name}'"));
            return Ok(());
        }

        if self.picked.contains(&name) {
            self.picked.retain(|picked| *picked != name);
            ui::info("picker", format!("{name} off"));
        } else {
            self.picked.push(name.clone());
            self.picked = self.estate.in_table_order(&self.picked);
            ui::ok("picker", format!("{name} on"));
        }

        Ok(())
    }

    fn list(&self) -> Result<()> {
        ui::blank();
        for (index, name) in self.estate.service_names().iter().enumerate() {
            let mark = if self.picked.contains(name) {
                format!("{GREEN}[x]{RESET}")
            } else {
                "[ ]".to_string()
            };
            let state = match services::is_running(self.estate, name) {
                true => {
                    let service = self.estate.resolve(name)?;
                    format!("{GREEN}running on {}{RESET}", service.port)
                }
                false => format!("{DIM}stopped{RESET}"),
            };
            println!("  {mark} {}) {name:<12} {state}", index + 1);
        }
        ui::blank();
        Ok(())
    }
}

fn help() {
    println!("  <n> | <name>   toggle one (several at a time is fine: 1 5, sage conveyor)");
    println!("  all | none     select every service, or clear the selection");
    println!("  running        select whatever is up right now");
    println!("  up             start the selection, with what it depends on");
    println!("  down           stop the selection");
    println!("  status         what is up, and on which port");
    println!("  logs <name>    follow one service's log until you interrupt it");
    println!("  db | reset     install the schema catalog, or drop it and rebuild");
    println!("  run <task>     run a task from the config");
    println!("  list           the services again");
    println!("  quit           leave; anything started keeps running");
    println!();
}
