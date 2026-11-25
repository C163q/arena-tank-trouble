#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    fff_session_timer::query_main()
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("fff-timer-query requires Linux");
    std::process::ExitCode::from(2)
}
