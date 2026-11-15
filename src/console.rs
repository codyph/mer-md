//! Keep completion output visible when Windows opens a console for this process.

#[cfg(windows)]
pub fn pause_on_exit() {
    use std::io::{self, IsTerminal, Write};

    if std::env::var_os("CI").is_some() || !io::stdout().is_terminal() {
        return;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleProcessList(process_list: *mut u32, process_count: u32) -> u32;
    }

    let mut process_id = 0;
    // A one-element buffer is enough to distinguish our own console from a shell's.
    let count = unsafe { GetConsoleProcessList(&mut process_id, 1) };
    if count != 1 || process_id != std::process::id() {
        return;
    }

    let _ = writeln!(io::stdout(), "\nClosing in 3 seconds...");
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();
    std::thread::sleep(std::time::Duration::from_secs(3));
}

#[cfg(not(windows))]
pub fn pause_on_exit() {}
