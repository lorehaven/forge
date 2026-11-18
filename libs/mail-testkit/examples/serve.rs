//! Fake provider servers with some mail in them, for looking at Mailroom in a browser:
//!
//! ```text
//! cargo run -p mail-testkit --example serve
//! ```
//!
//! IMAP on 12993 and SMTP on 12525 (TLS, the fixture certificate for `localhost`), one person: `me@provider.example` with the
//! password `bdd-mailbox-app-password`. The relay login `noreply@bdd.example` / `bdd-relay-password` works too.

use mail_testkit::{ImapOptions, ImapServer, SmtpOptions, SmtpServer};

const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

#[tokio::main]
async fn main() {
    let imap = ImapServer::start_on(12993, ImapOptions::default()).await;
    let user = "me@provider.example";
    imap.add_account(user, "bdd-mailbox-app-password");
    for (name, role) in [
        ("Sent", "\\Sent"),
        ("Drafts", "\\Drafts"),
        ("Trash", "\\Trash"),
        ("Junk", "\\Junk"),
        ("Archive", "\\Archive"),
    ] {
        imap.add_folder(user, name, &[role]);
    }
    let plain = |from: &str, subject: &str, body: &str, flags: &[&str]| {
        let raw = format!(
            "From: {from}\r\nTo: Me <{user}>\r\nSubject: {subject}\r\nDate: Tue, 29 Sep 2026 12:00:00 +0000\r\nMessage-ID: <{}@provider.example>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{body}\r\n",
            subject.replace(' ', "-")
        );
        imap.deliver(user, "INBOX", raw.as_bytes(), flags);
    };
    plain(
        "Alice Example <alice@example.org>",
        "Lunch on friday?",
        "Hi,\r\n\r\nare you free for lunch on friday?\r\n\r\nAlice",
        &[],
    );
    plain(
        "Bob <bob@example.org>",
        "Zażółć gęślą jaźń",
        "Polish text: zażółć gęślą jaźń.\r\n",
        &["\\Seen"],
    );
    plain(
        "billing@shop.example",
        "Your invoice",
        "Invoice 42 is attached to your account.\r\n",
        &["\\Flagged"],
    );
    let html = format!(
        "From: Newsletter <news@shop.example>\r\nTo: {user}\r\nSubject: Weekly offers\r\nDate: Tue, 29 Sep 2026 10:00:00 +0000\r\nMessage-ID: <offers@shop.example>\r\nList-Unsubscribe: <https://shop.example/unsubscribe?id=1>\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=\"m\"\r\n\r\n--m\r\nContent-Type: multipart/related; boundary=\"r\"\r\n\r\n--r\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<h2 style=\"color:#a33\">Weekly offers</h2><p onclick=\"steal()\">Save <b>20%</b> this week.</p><img src=\"cid:logo\" width=\"40\"><p><img src=\"https://tracker.example/pixel.gif\" width=\"1\" height=\"1\"></p><p><a href=\"https://shop.example/offers\">See the offers</a> <a href=\"javascript:alert(1)\">bad link</a></p><script>alert('xss')</script>\r\n--r\r\nContent-Type: image/png\r\nContent-ID: <logo>\r\nContent-Disposition: inline\r\nContent-Transfer-Encoding: base64\r\n\r\n{PNG}\r\n--r--\r\n--m\r\nContent-Type: application/pdf; name=\"catalogue.pdf\"\r\nContent-Disposition: attachment; filename=\"catalogue.pdf\"\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQK\r\n--m--\r\n"
    );
    imap.deliver(user, "INBOX", html.as_bytes(), &[]);

    let smtp = SmtpServer::start_on(12525, SmtpOptions::default()).await;
    smtp.add_user("noreply@bdd.example", "bdd-relay-password");
    smtp.add_user(user, "bdd-mailbox-app-password");
    println!("fake provider ready: IMAP 12993, SMTP 12525, user {user}");
    println!("Ctrl-C to stop");
    // Print what is sent so a person clicking around can see it.
    let mut shown = 0;
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let all = smtp.received();
        for mail in all.iter().skip(shown) {
            println!(
                "--- SMTP received: {} -> {:?} (logged in as {:?})\n{}",
                mail.from,
                mail.to,
                mail.auth_user,
                mail.text()
            );
        }
        shown = all.len();
    }
}
