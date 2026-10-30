---
title: Testing
parent: gatehouse-client
grand_parent: Libraries
nav_order: 3
---

`tests/unit.rs` starts a stand-in server on a local TCP port (it records each request as `Seen` and answers from a handler) that numbers its tokens and accepts any refresh token that is not marked spent. Cases: bad logins say why (the error variants above); the password is sent once and `Debug` does not print tokens; a token with life left is not refreshed; a token at the end of its life is renewed and the new refresh token is kept at once; a new refresh token is kept even when the request fails; a refused token is renewed once and the request repeated; a request refused twice is the server's answer, not a loop; a token another program kept is used before a refresh and after a refusal; a refused refresh leaves the hooks in place; an absent server is a network error. Run: `cargo test -p gatehouse-client`.
