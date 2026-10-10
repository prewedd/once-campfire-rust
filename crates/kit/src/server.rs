//! Process-level serving helpers: graceful shutdown on SIGINT/SIGTERM, and the open-file limit
//! many sockets need. The server itself is `front`.

/// Resolves on Ctrl-C or SIGTERM (what `kamal`/Docker send on stop).
pub async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

/// Raises the soft limit on open files to the hard limit, and returns the new limit. Every
/// WebSocket is a file descriptor, and containers commonly start processes with a soft limit
/// (65,536 in Docker's default, 1,024 elsewhere) far below the hard one, which would cap the
/// number of connected clients however little memory each takes.
#[cfg(unix)]
#[allow(unsafe_code, reason = "getrlimit and setrlimit have no safe wrapper in std")]
pub fn raise_open_file_limit() -> Option<u64> {
    let mut limit = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
    // SAFETY: getrlimit/setrlimit read and write one `rlimit` through a valid pointer.
    unsafe {
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return None;
        }
        if limit.rlim_cur < limit.rlim_max {
            let raised = libc::rlimit { rlim_cur: limit.rlim_max, rlim_max: limit.rlim_max };
            if libc::setrlimit(libc::RLIMIT_NOFILE, &raised) == 0 {
                return Some(raised.rlim_cur);
            }
        }
    }
    Some(limit.rlim_cur)
}

#[cfg(not(unix))]
pub fn raise_open_file_limit() -> Option<u64> {
    None
}

#[cfg(all(test, unix))]
#[allow(unsafe_code, reason = "reads the limit back with getrlimit")]
mod open_file_limit_tests {
    #[test]
    fn raises_the_soft_limit_to_the_hard_one() {
        let limit = super::raise_open_file_limit().expect("getrlimit works");
        let mut now = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        // SAFETY: getrlimit writes one `rlimit` through a valid pointer.
        assert_eq!(unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut now) }, 0);
        assert_eq!(now.rlim_cur, now.rlim_max);
        assert_eq!(limit, now.rlim_cur);
    }
}
