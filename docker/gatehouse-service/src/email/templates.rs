//! The two emails gatehouse sends, in the five languages the UI speaks.
//!
//! Plain text is the source of truth; the HTML part is derived from it (one
//! `<p>` per paragraph, the link paragraph as an anchor), so the two cannot
//! drift apart. Everything substituted in is escaped for HTML.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Verification,
    PasswordReset,
}

impl Kind {
    pub const fn label(self) -> &'static str {
        match self {
            Kind::Verification => "verification",
            Kind::PasswordReset => "password-reset",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub subject: String,
    pub text: String,
    pub html: String,
}

struct Copy {
    lang: &'static str,
    subject: &'static str,
    /// `{username}` and `{link}` are filled in; the link is its own paragraph.
    body: &'static str,
}

const VERIFICATION: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Confirm your email address",
        body: "Hello {username},\n\nPlease confirm your email address by opening this link:\n\n{link}\n\nThe link is valid for 24 hours. If you did not create this account, you can ignore this message.\n",
    },
    Copy {
        lang: "pl",
        subject: "Potwierdź swój adres e-mail",
        body: "Cześć {username},\n\nPotwierdź swój adres e-mail, otwierając ten link:\n\n{link}\n\nLink jest ważny przez 24 godziny. Jeśli nie zakładasz konta, zignoruj tę wiadomość.\n",
    },
    Copy {
        lang: "de",
        subject: "Bestätigen Sie Ihre E-Mail-Adresse",
        body: "Hallo {username},\n\nbitte bestätigen Sie Ihre E-Mail-Adresse über diesen Link:\n\n{link}\n\nDer Link ist 24 Stunden gültig. Falls Sie dieses Konto nicht erstellt haben, können Sie diese Nachricht ignorieren.\n",
    },
    Copy {
        lang: "fr",
        subject: "Confirmez votre adresse e-mail",
        body: "Bonjour {username},\n\nVeuillez confirmer votre adresse e-mail en ouvrant ce lien :\n\n{link}\n\nLe lien est valable 24 heures. Si vous n'avez pas créé ce compte, vous pouvez ignorer ce message.\n",
    },
    Copy {
        lang: "es",
        subject: "Confirma tu dirección de correo electrónico",
        body: "Hola {username},\n\nConfirma tu dirección de correo electrónico abriendo este enlace:\n\n{link}\n\nEl enlace es válido durante 24 horas. Si no has creado esta cuenta, puedes ignorar este mensaje.\n",
    },
];

const PASSWORD_RESET: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Reset your password",
        body: "Hello {username},\n\nSomeone asked to reset the password for this account. To choose a new one, open this link:\n\n{link}\n\nThe link is valid for 1 hour and works once. If you did not ask for this, ignore this message - your password stays as it is.\n",
    },
    Copy {
        lang: "pl",
        subject: "Zresetuj swoje hasło",
        body: "Cześć {username},\n\nOtrzymaliśmy prośbę o zresetowanie hasła do tego konta. Aby ustawić nowe, otwórz ten link:\n\n{link}\n\nLink jest ważny przez 1 godzinę i działa tylko raz. Jeśli to nie Ty, zignoruj tę wiadomość - Twoje hasło pozostanie bez zmian.\n",
    },
    Copy {
        lang: "de",
        subject: "Passwort zurücksetzen",
        body: "Hallo {username},\n\njemand hat das Zurücksetzen des Passworts für dieses Konto angefordert. Um ein neues zu wählen, öffnen Sie diesen Link:\n\n{link}\n\nDer Link ist 1 Stunde gültig und funktioniert nur einmal. Falls Sie das nicht waren, ignorieren Sie diese Nachricht - Ihr Passwort bleibt unverändert.\n",
    },
    Copy {
        lang: "fr",
        subject: "Réinitialisez votre mot de passe",
        body: "Bonjour {username},\n\nQuelqu'un a demandé la réinitialisation du mot de passe de ce compte. Pour en choisir un nouveau, ouvrez ce lien :\n\n{link}\n\nLe lien est valable 1 heure et ne fonctionne qu'une fois. Si vous n'êtes pas à l'origine de cette demande, ignorez ce message : votre mot de passe reste inchangé.\n",
    },
    Copy {
        lang: "es",
        subject: "Restablece tu contraseña",
        body: "Hola {username},\n\nAlguien ha solicitado restablecer la contraseña de esta cuenta. Para elegir una nueva, abre este enlace:\n\n{link}\n\nEl enlace es válido durante 1 hora y solo funciona una vez. Si no lo has solicitado tú, ignora este mensaje: tu contraseña no cambiará.\n",
    },
];

/// `pl-PL`, `pl_PL`, `PL` and `pl` all mean Polish; anything unknown, or no
/// preference at all, means English.
pub fn language(locale: Option<&str>) -> &'static str {
    let tag = locale.unwrap_or("").trim();
    let primary = tag
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    ["en", "pl", "de", "fr", "es"]
        .into_iter()
        .find(|lang| *lang == primary)
        .unwrap_or("en")
}

pub fn render(kind: Kind, locale: Option<&str>, username: &str, link: &str) -> Rendered {
    let lang = language(locale);
    let copies = match kind {
        Kind::Verification => &VERIFICATION,
        Kind::PasswordReset => &PASSWORD_RESET,
    };
    let copy = copies.iter().find(|c| c.lang == lang).unwrap_or(&copies[0]);
    let text = fill(copy.body, username, link);
    let html = html(lang, &text, link);
    Rendered {
        subject: copy.subject.to_string(),
        text,
        html,
    }
}

/// One pass over the template, so a substituted value is never scanned again
/// (a username containing `{link}` stays a username).
fn fill(template: &str, username: &str, link: &str) -> String {
    let mut out = String::with_capacity(template.len() + link.len() + username.len());
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("{username}") {
            out.push_str(username);
            rest = after;
        } else if let Some(after) = tail.strip_prefix("{link}") {
            out.push_str(link);
            rest = after;
        } else {
            out.push('{');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

fn html(lang: &str, text: &str, link: &str) -> String {
    let mut out = format!(
        "<!doctype html>\n<html lang=\"{lang}\"><body style=\"font-family:sans-serif;line-height:1.5\">\n"
    );
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph == link {
            let href = escape(link);
            out.push_str(&format!("<p><a href=\"{href}\">{href}</a></p>\n"));
        } else {
            out.push_str(&format!(
                "<p>{}</p>\n",
                escape(paragraph).replace('\n', "<br>")
            ));
        }
    }
    out.push_str("</body></html>\n");
    out
}

fn escape(text: &str) -> String {
    text.chars()
        .fold(String::with_capacity(text.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&#39;"),
                c => out.push(c),
            }
            out
        })
}
