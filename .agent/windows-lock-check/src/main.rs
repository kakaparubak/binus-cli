#[path = "../../../spikes/001-auth/src/windows_lock.rs"]
mod windows_lock;
fn main() {
    let _ = windows_lock::acquire();
}

#[cfg(test)]
#[test]
fn native_module_tests_compile_for_windows_target() {}
