use gatehouse_service::RateLimiter;
use gatehouse_service::ratelimit::{ClientIp, Limit, Verdict, policy};
use quench_cache::CacheStore;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A limiter over a store, with a clock the test moves by hand.
fn limiter_at(start: u64) -> (RateLimiter, Arc<AtomicU64>) {
    let now = Arc::new(AtomicU64::new(start));
    let clock = now.clone();
    let limiter = RateLimiter::with_clock(
        CacheStore::in_memory(),
        Arc::new(move || clock.load(Ordering::SeqCst)),
    );
    (limiter, now)
}

const THREE_PER_HOUR: Limit = Limit::per_hour(3);

#[tokio::test]
async fn hits_up_to_the_limit_pass_and_the_next_is_refused() {
    let (limiter, _) = limiter_at(1_000_000);
    for n in 1..=3 {
        assert!(
            limiter
                .check("t", "alice", THREE_PER_HOUR)
                .await
                .is_allowed(),
            "hit {n}"
        );
    }
    assert!(matches!(
        limiter.check("t", "alice", THREE_PER_HOUR).await,
        Verdict::Limited { .. }
    ));
}

#[tokio::test]
async fn retry_after_counts_down_to_the_end_of_the_window() {
    // 3600-second windows: 999_000 is 1800s past the boundary at 997_200.
    let (limiter, now) = limiter_at(999_000);
    for _ in 0..3 {
        limiter.check("t", "a", THREE_PER_HOUR).await;
    }
    let Verdict::Limited { retry_after_secs } = limiter.check("t", "a", THREE_PER_HOUR).await
    else {
        panic!("expected limited");
    };
    assert_eq!(retry_after_secs, 1_800);
    now.store(999_000 + 900, Ordering::SeqCst);
    let Verdict::Limited { retry_after_secs } = limiter.check("t", "a", THREE_PER_HOUR).await
    else {
        panic!("still limited");
    };
    assert_eq!(retry_after_secs, 900);
}

#[tokio::test]
async fn a_new_window_starts_fresh() {
    let (limiter, now) = limiter_at(1_000_000);
    for _ in 0..4 {
        limiter.check("t", "a", THREE_PER_HOUR).await;
    }
    assert!(!limiter.check("t", "a", THREE_PER_HOUR).await.is_allowed());
    now.fetch_add(3_600, Ordering::SeqCst);
    assert!(limiter.check("t", "a", THREE_PER_HOUR).await.is_allowed());
}

#[tokio::test]
async fn subjects_and_scopes_are_independent() {
    let (limiter, _) = limiter_at(1_000_000);
    for _ in 0..4 {
        limiter.check("reset", "alice", THREE_PER_HOUR).await;
    }
    assert!(
        !limiter
            .check("reset", "alice", THREE_PER_HOUR)
            .await
            .is_allowed()
    );
    assert!(
        limiter
            .check("reset", "bob", THREE_PER_HOUR)
            .await
            .is_allowed()
    );
    assert!(
        limiter
            .check("resend", "alice", THREE_PER_HOUR)
            .await
            .is_allowed()
    );
}

#[tokio::test]
async fn subjects_are_compared_ignoring_case_and_surrounding_space() {
    let (limiter, _) = limiter_at(1_000_000);
    for subject in ["Alice", "alice", " ALICE ", "aLiCe"] {
        limiter.check("t", subject, THREE_PER_HOUR).await;
    }
    assert!(
        !limiter
            .check("t", "alice", THREE_PER_HOUR)
            .await
            .is_allowed()
    );
}

#[tokio::test]
async fn hammering_while_blocked_does_not_extend_the_block_past_the_window() {
    let (limiter, now) = limiter_at(1_000_000);
    for _ in 0..500 {
        limiter.check("t", "a", THREE_PER_HOUR).await;
    }
    now.fetch_add(3_600, Ordering::SeqCst);
    assert!(limiter.check("t", "a", THREE_PER_HOUR).await.is_allowed());
}

#[tokio::test]
async fn a_parallel_burst_never_lets_more_than_the_limit_through() {
    let limiter = Arc::new(limiter_at(1_000_000).0);
    let mut tasks = Vec::new();
    for _ in 0..200 {
        let limiter = limiter.clone();
        tasks.push(tokio::spawn(async move {
            limiter
                .check("burst", "victim@example.com", Limit::per_hour(5))
                .await
        }));
    }
    let mut allowed = 0;
    for task in tasks {
        if task.await.unwrap().is_allowed() {
            allowed += 1;
        }
    }
    assert!(
        allowed <= 5,
        "{allowed} of 200 parallel requests were allowed"
    );
    assert!(allowed >= 1, "the limiter must not refuse everything");
}

#[tokio::test]
async fn extremely_long_or_odd_subjects_are_fine() {
    let (limiter, _) = limiter_at(1_000_000);
    let long = "x".repeat(100_000);
    for odd in [long.as_str(), "", "a:b:c", "line\nbreak", "zoë\0", "*"] {
        assert!(
            limiter.check("t", odd, THREE_PER_HOUR).await.is_allowed(),
            "{odd:.10?}"
        );
    }
}

#[tokio::test]
async fn check_all_needs_every_limit_to_pass() {
    let (limiter, _) = limiter_at(1_000_000);
    let checks = |ip: &'static str, user: &'static str| {
        [
            ("ip", ip, Limit::per_hour(10)),
            ("user", user, Limit::per_hour(2)),
        ]
    };
    assert!(
        limiter
            .check_all(&checks("1.1.1.1", "alice"))
            .await
            .is_allowed()
    );
    assert!(
        limiter
            .check_all(&checks("1.1.1.1", "alice"))
            .await
            .is_allowed()
    );
    // Third for alice trips the per-user limit although the address is fine.
    assert!(
        !limiter
            .check_all(&checks("1.1.1.1", "alice"))
            .await
            .is_allowed()
    );
    // Another user from the same address is unaffected.
    assert!(
        limiter
            .check_all(&checks("1.1.1.1", "bob"))
            .await
            .is_allowed()
    );
    // Same user from elsewhere is still limited.
    assert!(
        !limiter
            .check_all(&checks("2.2.2.2", "alice"))
            .await
            .is_allowed()
    );
}

#[tokio::test]
async fn check_all_reports_the_longest_wait() {
    let (limiter, _) = limiter_at(1_000_000);
    let both = [
        ("minute", "a", Limit::per_minute(1)),
        ("hour", "a", Limit::per_hour(1)),
    ];
    limiter.check_all(&both).await;
    let Verdict::Limited { retry_after_secs } = limiter.check_all(&both).await else {
        panic!("expected limited");
    };
    assert!(
        retry_after_secs > 60,
        "waits for the hour, not the minute: {retry_after_secs}"
    );
}

#[tokio::test]
async fn policy_limits_are_sane() {
    // These protect a 300-a-day mail quota; a typo that made one enormous
    // (or zero) should be noticed.
    for limit in [
        policy::REGISTER_IP,
        policy::REGISTER_EMAIL,
        policy::RESET_IP,
        policy::RESET_USER,
        policy::RESEND_IP,
        policy::RESEND_USER,
        policy::RESEND_COOLDOWN,
    ] {
        assert!((1..=50).contains(&limit.max), "{limit:?}");
        assert!(limit.window_secs >= 60);
    }
}

// -- ClientIp -------------------------------------------------------

#[test]
fn client_ip_prefers_x_real_ip() {
    let ip = ClientIp::from_headers(Some("203.0.113.9"), Some("198.51.100.1, 10.0.0.1"));
    assert_eq!(ip.0, "203.0.113.9");
}

#[test]
fn client_ip_falls_back_to_the_last_forwarded_entry() {
    // Earlier entries are what the client claimed; the last is the proxy's.
    let ip = ClientIp::from_headers(None, Some("6.6.6.6, 198.51.100.7"));
    assert_eq!(ip.0, "198.51.100.7");
}

#[test]
fn client_ip_accepts_ipv6_and_normalises_it() {
    let ip = ClientIp::from_headers(Some("2001:0db8:0000:0000:0000:0000:0000:0001"), None);
    assert_eq!(ip.0, "2001:db8::1");
}

#[test]
fn client_ip_without_a_usable_header_is_the_shared_unknown_bucket() {
    for (real, fwd) in [
        (None, None),
        (Some(""), None),
        (Some("not-an-ip"), None),
        (Some("1.2.3.4:8080"), None),
        (None, Some("garbage, more-garbage")),
        (Some("1.1.1.1\r\nX-Evil: 1"), None),
    ] {
        assert_eq!(
            ClientIp::from_headers(real, fwd).0,
            ClientIp::UNKNOWN,
            "{real:?} {fwd:?}"
        );
    }
}

// A compile-time check: the default must stay under the relay's 300 a day.
const _: () = assert!(policy::DEFAULT_DAILY_MAIL < 300);
