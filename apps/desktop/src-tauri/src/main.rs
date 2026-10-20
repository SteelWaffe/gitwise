// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // git and ssh start this same executable as their askpass helper; answer and exit before
    // any window is created.
    if gitwise_core::askpass::is_askpass_invocation() {
        std::process::exit(gitwise_core::askpass::run());
    }
    gitwise_lib::run()
}
