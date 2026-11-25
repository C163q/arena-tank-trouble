#[cfg(target_os = "linux")]
fn main() -> std::process::ExitCode {
    fff_session_timer::timer_main()
}

#[cfg(not(target_os = "linux"))]
fn main() -> std::process::ExitCode {
    eprintln!("fff-timer requires Linux");
    std::process::ExitCode::from(2)
}
