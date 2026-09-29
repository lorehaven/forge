//! Every email gatehouse sends, in the five languages the UI speaks.
//!
//! Plain text is the source of truth; the HTML part is derived from it (one
//! `<p>` per paragraph, a paragraph that is exactly the link becomes an anchor),
//! so the two cannot drift apart. Everything substituted in is escaped for HTML.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Confirm an address after registering.
    Verification,
    PasswordReset,
    /// An administrator created the account: choose a password, confirm the address.
    Invite,
    /// To the NEW address: confirm you want it on the account.
    EmailChange,
    /// To the OLD address, after the change went through.
    EmailChanged,
    PasswordChanged,
    MfaEnabled,
    MfaDisabled,
    /// A message another service asked gatehouse to send - see `crate::notify`.
    Notification,
}

impl Kind {
    pub const fn label(self) -> &'static str {
        match self {
            Kind::Verification => "verification",
            Kind::PasswordReset => "password-reset",
            Kind::Invite => "invite",
            Kind::EmailChange => "email-change",
            Kind::EmailChanged => "email-changed",
            Kind::PasswordChanged => "password-changed",
            Kind::MfaEnabled => "mfa-enabled",
            Kind::MfaDisabled => "mfa-disabled",
            Kind::Notification => "notification",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub subject: String,
    pub text: String,
    pub html: String,
}

/// What a template can refer to: `{username}`, `{link}`, `{new_email}`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vars<'a> {
    pub username: &'a str,
    pub link: &'a str,
    pub new_email: &'a str,
}

struct Copy {
    lang: &'static str,
    subject: &'static str,
    /// A link, when there is one, is its own paragraph.
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

const INVITE: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "You have been invited",
        body: "Hello {username},\n\nAn account has been created for you. Choose your password and confirm your email address by opening this link:\n\n{link}\n\nThe link is valid for 7 days and works once. If you were not expecting this, you can ignore this message.\n",
    },
    Copy {
        lang: "pl",
        subject: "Zaproszenie do konta",
        body: "Cześć {username},\n\nZostało dla Ciebie utworzone konto. Ustaw hasło i potwierdź swój adres e-mail, otwierając ten link:\n\n{link}\n\nLink jest ważny przez 7 dni i działa tylko raz. Jeśli nie oczekujesz tej wiadomości, zignoruj ją.\n",
    },
    Copy {
        lang: "de",
        subject: "Einladung zu Ihrem Konto",
        body: "Hallo {username},\n\nfür Sie wurde ein Konto angelegt. Wählen Sie Ihr Passwort und bestätigen Sie Ihre E-Mail-Adresse über diesen Link:\n\n{link}\n\nDer Link ist 7 Tage gültig und funktioniert nur einmal. Falls Sie diese Nachricht nicht erwartet haben, können Sie sie ignorieren.\n",
    },
    Copy {
        lang: "fr",
        subject: "Invitation à votre compte",
        body: "Bonjour {username},\n\nUn compte a été créé pour vous. Choisissez votre mot de passe et confirmez votre adresse e-mail en ouvrant ce lien :\n\n{link}\n\nLe lien est valable 7 jours et ne fonctionne qu'une fois. Si vous n'attendiez pas ce message, vous pouvez l'ignorer.\n",
    },
    Copy {
        lang: "es",
        subject: "Invitación a tu cuenta",
        body: "Hola {username},\n\nSe ha creado una cuenta para ti. Elige tu contraseña y confirma tu dirección de correo electrónico abriendo este enlace:\n\n{link}\n\nEl enlace es válido durante 7 días y solo funciona una vez. Si no esperabas este mensaje, puedes ignorarlo.\n",
    },
];

const EMAIL_CHANGE: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Confirm your new email address",
        body: "Hello {username},\n\nThis address was given as the new email address for your account. Confirm the change by opening this link:\n\n{link}\n\nThe link is valid for 24 hours. If you did not ask for this, ignore this message - the address on your account stays as it is.\n",
    },
    Copy {
        lang: "pl",
        subject: "Potwierdź nowy adres e-mail",
        body: "Cześć {username},\n\nTen adres został wskazany jako nowy adres e-mail Twojego konta. Potwierdź zmianę, otwierając ten link:\n\n{link}\n\nLink jest ważny przez 24 godziny. Jeśli to nie Ty, zignoruj tę wiadomość - adres konta pozostanie bez zmian.\n",
    },
    Copy {
        lang: "de",
        subject: "Neue E-Mail-Adresse bestätigen",
        body: "Hallo {username},\n\ndiese Adresse wurde als neue E-Mail-Adresse für Ihr Konto angegeben. Bestätigen Sie die Änderung über diesen Link:\n\n{link}\n\nDer Link ist 24 Stunden gültig. Falls Sie das nicht waren, ignorieren Sie diese Nachricht - die Adresse Ihres Kontos bleibt unverändert.\n",
    },
    Copy {
        lang: "fr",
        subject: "Confirmez votre nouvelle adresse e-mail",
        body: "Bonjour {username},\n\nCette adresse a été indiquée comme nouvelle adresse e-mail de votre compte. Confirmez le changement en ouvrant ce lien :\n\n{link}\n\nLe lien est valable 24 heures. Si vous n'êtes pas à l'origine de cette demande, ignorez ce message : l'adresse de votre compte reste inchangée.\n",
    },
    Copy {
        lang: "es",
        subject: "Confirma tu nueva dirección de correo electrónico",
        body: "Hola {username},\n\nSe ha indicado esta dirección como nueva dirección de correo electrónico de tu cuenta. Confirma el cambio abriendo este enlace:\n\n{link}\n\nEl enlace es válido durante 24 horas. Si no lo has solicitado tú, ignora este mensaje: la dirección de tu cuenta no cambiará.\n",
    },
];

const EMAIL_CHANGED: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Your email address was changed",
        body: "Hello {username},\n\nThe email address of your account was changed to {new_email}. This message goes to the old address as a precaution.\n\nIf this was not you, change your password now and tell an administrator.\n",
    },
    Copy {
        lang: "pl",
        subject: "Zmieniono Twój adres e-mail",
        body: "Cześć {username},\n\nAdres e-mail Twojego konta został zmieniony na {new_email}. Ta wiadomość trafia na poprzedni adres dla bezpieczeństwa.\n\nJeśli to nie Ty, zmień hasło i powiadom administratora.\n",
    },
    Copy {
        lang: "de",
        subject: "Ihre E-Mail-Adresse wurde geändert",
        body: "Hallo {username},\n\ndie E-Mail-Adresse Ihres Kontos wurde auf {new_email} geändert. Diese Nachricht geht vorsorglich an die alte Adresse.\n\nFalls Sie das nicht waren, ändern Sie jetzt Ihr Passwort und informieren Sie einen Administrator.\n",
    },
    Copy {
        lang: "fr",
        subject: "Votre adresse e-mail a été modifiée",
        body: "Bonjour {username},\n\nL'adresse e-mail de votre compte a été remplacée par {new_email}. Ce message est envoyé à l'ancienne adresse par précaution.\n\nSi ce n'était pas vous, changez votre mot de passe maintenant et prévenez un administrateur.\n",
    },
    Copy {
        lang: "es",
        subject: "Se ha cambiado tu dirección de correo electrónico",
        body: "Hola {username},\n\nLa dirección de correo electrónico de tu cuenta se ha cambiado a {new_email}. Este mensaje se envía a la dirección anterior por precaución.\n\nSi no has sido tú, cambia tu contraseña ahora y avisa a un administrador.\n",
    },
];

const PASSWORD_CHANGED: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Your password was changed",
        body: "Hello {username},\n\nThe password of your account was just changed.\n\nIf this was not you, reset your password right away and tell an administrator.\n",
    },
    Copy {
        lang: "pl",
        subject: "Twoje hasło zostało zmienione",
        body: "Cześć {username},\n\nHasło do Twojego konta zostało właśnie zmienione.\n\nJeśli to nie Ty, natychmiast zresetuj hasło i powiadom administratora.\n",
    },
    Copy {
        lang: "de",
        subject: "Ihr Passwort wurde geändert",
        body: "Hallo {username},\n\ndas Passwort Ihres Kontos wurde soeben geändert.\n\nFalls Sie das nicht waren, setzen Sie Ihr Passwort sofort zurück und informieren Sie einen Administrator.\n",
    },
    Copy {
        lang: "fr",
        subject: "Votre mot de passe a été modifié",
        body: "Bonjour {username},\n\nLe mot de passe de votre compte vient d'être modifié.\n\nSi ce n'était pas vous, réinitialisez immédiatement votre mot de passe et prévenez un administrateur.\n",
    },
    Copy {
        lang: "es",
        subject: "Tu contraseña ha sido cambiada",
        body: "Hola {username},\n\nLa contraseña de tu cuenta se acaba de cambiar.\n\nSi no has sido tú, restablece tu contraseña de inmediato y avisa a un administrador.\n",
    },
];

const MFA_ENABLED: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Two-factor authentication was turned on",
        body: "Hello {username},\n\nTwo-factor authentication was turned on for your account.\n\nIf this was not you, reset your password right away and tell an administrator.\n",
    },
    Copy {
        lang: "pl",
        subject: "Włączono uwierzytelnianie dwuskładnikowe",
        body: "Cześć {username},\n\nDla Twojego konta włączono uwierzytelnianie dwuskładnikowe.\n\nJeśli to nie Ty, natychmiast zresetuj hasło i powiadom administratora.\n",
    },
    Copy {
        lang: "de",
        subject: "Zwei-Faktor-Authentifizierung wurde aktiviert",
        body: "Hallo {username},\n\nfür Ihr Konto wurde die Zwei-Faktor-Authentifizierung aktiviert.\n\nFalls Sie das nicht waren, setzen Sie Ihr Passwort sofort zurück und informieren Sie einen Administrator.\n",
    },
    Copy {
        lang: "fr",
        subject: "L'authentification à deux facteurs a été activée",
        body: "Bonjour {username},\n\nL'authentification à deux facteurs a été activée pour votre compte.\n\nSi ce n'était pas vous, réinitialisez immédiatement votre mot de passe et prévenez un administrateur.\n",
    },
    Copy {
        lang: "es",
        subject: "Se ha activado la autenticación en dos pasos",
        body: "Hola {username},\n\nSe ha activado la autenticación en dos pasos en tu cuenta.\n\nSi no has sido tú, restablece tu contraseña de inmediato y avisa a un administrador.\n",
    },
];

const MFA_DISABLED: [Copy; 5] = [
    Copy {
        lang: "en",
        subject: "Two-factor authentication was turned off",
        body: "Hello {username},\n\nTwo-factor authentication was turned off for your account.\n\nIf this was not you, reset your password right away and tell an administrator.\n",
    },
    Copy {
        lang: "pl",
        subject: "Wyłączono uwierzytelnianie dwuskładnikowe",
        body: "Cześć {username},\n\nDla Twojego konta wyłączono uwierzytelnianie dwuskładnikowe.\n\nJeśli to nie Ty, natychmiast zresetuj hasło i powiadom administratora.\n",
    },
    Copy {
        lang: "de",
        subject: "Zwei-Faktor-Authentifizierung wurde deaktiviert",
        body: "Hallo {username},\n\nfür Ihr Konto wurde die Zwei-Faktor-Authentifizierung deaktiviert.\n\nFalls Sie das nicht waren, setzen Sie Ihr Passwort sofort zurück und informieren Sie einen Administrator.\n",
    },
    Copy {
        lang: "fr",
        subject: "L'authentification à deux facteurs a été désactivée",
        body: "Bonjour {username},\n\nL'authentification à deux facteurs a été désactivée pour votre compte.\n\nSi ce n'était pas vous, réinitialisez immédiatement votre mot de passe et prévenez un administrateur.\n",
    },
    Copy {
        lang: "es",
        subject: "Se ha desactivado la autenticación en dos pasos",
        body: "Hola {username},\n\nSe ha desactivado la autenticación en dos pasos en tu cuenta.\n\nSi no has sido tú, restablece tu contraseña de inmediato y avisa a un administrador.\n",
    },
];

fn copies(kind: Kind) -> &'static [Copy; 5] {
    match kind {
        Kind::Verification => &VERIFICATION,
        Kind::PasswordReset => &PASSWORD_RESET,
        Kind::Invite => &INVITE,
        Kind::EmailChange => &EMAIL_CHANGE,
        Kind::EmailChanged => &EMAIL_CHANGED,
        Kind::PasswordChanged => &PASSWORD_CHANGED,
        Kind::MfaEnabled => &MFA_ENABLED,
        Kind::MfaDisabled => &MFA_DISABLED,
        // Notifications carry their own text (`crate::notify`), never this table.
        Kind::Notification => &VERIFICATION,
    }
}

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

/// The common case: a message with a username and a link.
pub fn render(kind: Kind, locale: Option<&str>, username: &str, link: &str) -> Rendered {
    render_with(
        kind,
        locale,
        &Vars {
            username,
            link,
            new_email: "",
        },
    )
}

pub fn render_with(kind: Kind, locale: Option<&str>, vars: &Vars<'_>) -> Rendered {
    let lang = language(locale);
    let all = copies(kind);
    let copy = all.iter().find(|c| c.lang == lang).unwrap_or(&all[0]);
    let text = fill(copy.body, vars);
    let links: Vec<&str> = [vars.link].into_iter().filter(|l| !l.is_empty()).collect();
    let html = html(lang, &text, &links);
    Rendered {
        subject: copy.subject.to_string(),
        text,
        html,
    }
}

/// One pass over the template, so a substituted value is never scanned again
/// (a username containing `{link}` stays a username).
fn fill(template: &str, vars: &Vars<'_>) -> String {
    fill_with(template, |name| match name {
        "username" => Some(vars.username),
        "link" => Some(vars.link),
        "new_email" => Some(vars.new_email),
        _ => None,
    })
}

/// The same single pass with any set of names: `{name}` becomes `lookup(name)`,
/// and a brace that names nothing known is left as it is.
pub fn fill_with<'v>(template: &str, lookup: impl Fn(&str) -> Option<&'v str>) -> String {
    let mut out = String::with_capacity(template.len() + 64);
    let mut rest = template;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let named = tail
            .find('}')
            .map(|end| (&tail[1..end], &tail[end + 1..]))
            .and_then(|(name, after)| lookup(name).map(|value| (value, after)));
        match named {
            Some((value, after)) => {
                out.push_str(value);
                rest = after;
            }
            None => {
                out.push('{');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The HTML twin of `text`: a paragraph that is exactly one of `links` becomes an
/// anchor, every other paragraph is escaped text.
pub fn html(lang: &str, text: &str, links: &[&str]) -> String {
    let mut out = format!(
        "<!doctype html>\n<html lang=\"{lang}\"><body style=\"font-family:sans-serif;line-height:1.5\">\n"
    );
    for paragraph in text.split("\n\n") {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if !paragraph.is_empty() && links.contains(&paragraph) {
            let href = escape(paragraph);
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
