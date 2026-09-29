//! The notifications other services may ask gatehouse to send.
//!
//! A service names one of these and supplies its variables; the wording, the
//! translations and who may receive it are decided here. So a service that is
//! compromised - or just buggy - cannot put its own text under this domain, and
//! every message carries the same footer and unsubscribe link.

use crate::email::{Rendered, fill_with, language, render_html};
use std::collections::BTreeMap;

/// A variable's longest value.
pub const MAX_VALUE_LEN: usize = 300;
/// A link variable's longest value.
pub const MAX_URL_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    /// Plain text on one line.
    Text,
    /// A link into this estate: it must start with gatehouse's own public origin.
    Url,
}

#[derive(Debug, Clone, Copy)]
pub struct Var {
    pub name: &'static str,
    pub kind: VarKind,
}

struct Copy {
    lang: &'static str,
    subject: &'static str,
    /// Placeholders: `{username}`, `{unsubscribe}` and the template's own variables.
    body: &'static str,
}

pub struct Template {
    /// `service.thing.event`, e.g. `conveyor.run.failed`.
    pub id: &'static str,
    /// The service the message is about - what the account page groups by.
    pub service: &'static str,
    /// Whether someone who never chose is subscribed.
    pub default_on: bool,
    /// Every one is required, and nothing else is accepted.
    pub vars: &'static [Var],
    copies: [Copy; 5],
}

const RUN_VARS: &[Var] = &[
    Var {
        name: "project",
        kind: VarKind::Text,
    },
    Var {
        name: "run",
        kind: VarKind::Text,
    },
    Var {
        name: "ref",
        kind: VarKind::Text,
    },
    Var {
        name: "url",
        kind: VarKind::Url,
    },
];

pub static TEMPLATES: [Template; 3] = [
    Template {
        id: "conveyor.run.failed",
        service: "conveyor",
        default_on: true,
        vars: RUN_VARS,
        copies: [
            Copy {
                lang: "en",
                subject: "Run failed: {project}",
                body: "Hello {username},\n\nA run failed in {project}.\n\nRun: {run}\nRef: {ref}\n\n{url}\n\nYou get this because you subscribed to failed-run notifications. To stop them, open:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "pl",
                subject: "Nieudany przebieg: {project}",
                body: "Cześć {username},\n\nPrzebieg w projekcie {project} zakończył się niepowodzeniem.\n\nPrzebieg: {run}\nRef: {ref}\n\n{url}\n\nOtrzymujesz tę wiadomość, bo subskrybujesz powiadomienia o nieudanych przebiegach. Aby je wyłączyć, otwórz:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "de",
                subject: "Lauf fehlgeschlagen: {project}",
                body: "Hallo {username},\n\nein Lauf in {project} ist fehlgeschlagen.\n\nLauf: {run}\nRef: {ref}\n\n{url}\n\nSie erhalten diese Nachricht, weil Sie Benachrichtigungen über fehlgeschlagene Läufe abonniert haben. Zum Abbestellen öffnen Sie:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "fr",
                subject: "Exécution échouée : {project}",
                body: "Bonjour {username},\n\nUne exécution a échoué dans {project}.\n\nExécution : {run}\nRéf : {ref}\n\n{url}\n\nVous recevez ce message car vous êtes abonné aux notifications d'exécutions échouées. Pour vous désabonner, ouvrez :\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "es",
                subject: "Ejecución fallida: {project}",
                body: "Hola {username},\n\nUna ejecución ha fallado en {project}.\n\nEjecución: {run}\nRef: {ref}\n\n{url}\n\nRecibes este mensaje porque te suscribiste a las notificaciones de ejecuciones fallidas. Para darte de baja, abre:\n\n{unsubscribe}\n",
            },
        ],
    },
    Template {
        id: "conveyor.run.succeeded",
        service: "conveyor",
        default_on: false,
        vars: RUN_VARS,
        copies: [
            Copy {
                lang: "en",
                subject: "Run passed: {project}",
                body: "Hello {username},\n\nA run passed in {project}.\n\nRun: {run}\nRef: {ref}\n\n{url}\n\nYou get this because you subscribed to passed-run notifications. To stop them, open:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "pl",
                subject: "Udany przebieg: {project}",
                body: "Cześć {username},\n\nPrzebieg w projekcie {project} zakończył się powodzeniem.\n\nPrzebieg: {run}\nRef: {ref}\n\n{url}\n\nOtrzymujesz tę wiadomość, bo subskrybujesz powiadomienia o udanych przebiegach. Aby je wyłączyć, otwórz:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "de",
                subject: "Lauf erfolgreich: {project}",
                body: "Hallo {username},\n\nein Lauf in {project} war erfolgreich.\n\nLauf: {run}\nRef: {ref}\n\n{url}\n\nSie erhalten diese Nachricht, weil Sie Benachrichtigungen über erfolgreiche Läufe abonniert haben. Zum Abbestellen öffnen Sie:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "fr",
                subject: "Exécution réussie : {project}",
                body: "Bonjour {username},\n\nUne exécution a réussi dans {project}.\n\nExécution : {run}\nRéf : {ref}\n\n{url}\n\nVous recevez ce message car vous êtes abonné aux notifications d'exécutions réussies. Pour vous désabonner, ouvrez :\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "es",
                subject: "Ejecución correcta: {project}",
                body: "Hola {username},\n\nUna ejecución ha terminado correctamente en {project}.\n\nEjecución: {run}\nRef: {ref}\n\n{url}\n\nRecibes este mensaje porque te suscribiste a las notificaciones de ejecuciones correctas. Para darte de baja, abre:\n\n{unsubscribe}\n",
            },
        ],
    },
    Template {
        id: "conveyor.run.recovered",
        service: "conveyor",
        default_on: false,
        vars: RUN_VARS,
        copies: [
            Copy {
                lang: "en",
                subject: "Run recovered: {project}",
                body: "Hello {username},\n\nThe first run to pass in {project} after a failure.\n\nRun: {run}\nRef: {ref}\n\n{url}\n\nYou get this because you subscribed to recovery notifications. To stop them, open:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "pl",
                subject: "Przebieg naprawiony: {project}",
                body: "Cześć {username},\n\nPierwszy udany przebieg w projekcie {project} po niepowodzeniu.\n\nPrzebieg: {run}\nRef: {ref}\n\n{url}\n\nOtrzymujesz tę wiadomość, bo subskrybujesz powiadomienia o naprawionych przebiegach. Aby je wyłączyć, otwórz:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "de",
                subject: "Lauf wieder erfolgreich: {project}",
                body: "Hallo {username},\n\nder erste erfolgreiche Lauf in {project} nach einem Fehler.\n\nLauf: {run}\nRef: {ref}\n\n{url}\n\nSie erhalten diese Nachricht, weil Sie Benachrichtigungen über behobene Läufe abonniert haben. Zum Abbestellen öffnen Sie:\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "fr",
                subject: "Exécution rétablie : {project}",
                body: "Bonjour {username},\n\nPremière exécution réussie dans {project} après un échec.\n\nExécution : {run}\nRéf : {ref}\n\n{url}\n\nVous recevez ce message car vous êtes abonné aux notifications de rétablissement. Pour vous désabonner, ouvrez :\n\n{unsubscribe}\n",
            },
            Copy {
                lang: "es",
                subject: "Ejecución recuperada: {project}",
                body: "Hola {username},\n\nLa primera ejecución correcta en {project} tras un fallo.\n\nEjecución: {run}\nRef: {ref}\n\n{url}\n\nRecibes este mensaje porque te suscribiste a las notificaciones de recuperación. Para darte de baja, abre:\n\n{unsubscribe}\n",
            },
        ],
    },
];

pub fn all() -> &'static [Template] {
    &TEMPLATES
}

pub fn find(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|template| template.id == id)
}

impl Template {
    /// The translation key of this kind's name on the account page.
    pub fn label_key(&self) -> String {
        format!("ui_notification_{}", self.id.replace('.', "_"))
    }

    /// Checks `vars` against what this template declares: exactly those names,
    /// each a single line of reasonable length, and links that stay inside the
    /// estate (`allowed_origin` is gatehouse's own public origin).
    pub fn validate(
        &self,
        vars: &BTreeMap<String, String>,
        allowed_origin: &str,
    ) -> Result<(), String> {
        for name in vars.keys() {
            if !self.vars.iter().any(|v| v.name == name) {
                return Err(format!(
                    "{} does not take a variable named {name:?}",
                    self.id
                ));
            }
        }
        for var in self.vars {
            let value = vars
                .get(var.name)
                .ok_or_else(|| format!("{} needs the variable {:?}", self.id, var.name))?;
            if value.trim().is_empty() {
                return Err(format!("the variable {:?} is empty", var.name));
            }
            if value.chars().any(char::is_control) {
                return Err(format!(
                    "the variable {:?} contains a line break or control character",
                    var.name
                ));
            }
            let limit = if var.kind == VarKind::Url {
                MAX_URL_LEN
            } else {
                MAX_VALUE_LEN
            };
            if value.chars().count() > limit {
                return Err(format!(
                    "the variable {:?} is longer than {limit} characters",
                    var.name
                ));
            }
            if var.kind == VarKind::Url {
                let inside = value
                    .strip_prefix(allowed_origin)
                    .is_some_and(|rest| rest.starts_with('/'));
                if !inside || value.contains(char::is_whitespace) {
                    return Err(format!(
                        "the variable {:?} must be a link under {allowed_origin}/",
                        var.name
                    ));
                }
            }
        }
        Ok(())
    }

    /// The message in `locale` (English when unknown). Validate `vars` first.
    pub fn render(
        &self,
        locale: Option<&str>,
        username: &str,
        vars: &[(&str, &str)],
        unsubscribe: &str,
    ) -> Rendered {
        let lang = language(locale);
        let copy = self
            .copies
            .iter()
            .find(|c| c.lang == lang)
            .unwrap_or(&self.copies[0]);
        let lookup = |name: &str| -> Option<&str> {
            match name {
                "username" => Some(username),
                "unsubscribe" => Some(unsubscribe),
                other => vars.iter().find(|(k, _)| *k == other).map(|(_, v)| *v),
            }
        };
        let subject = fill_with(copy.subject, lookup);
        let text = fill_with(copy.body, lookup);
        let mut links: Vec<&str> = self
            .vars
            .iter()
            .filter(|v| v.kind == VarKind::Url)
            .filter_map(|v| lookup(v.name))
            .collect();
        links.push(unsubscribe);
        let html = render_html(lang, &text, &links);
        Rendered {
            subject,
            text,
            html,
        }
    }
}
