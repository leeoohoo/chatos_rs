fn main() {
    if chatos_windows_react_lib::run_ssh_askpass_helper_if_requested() {
        return;
    }
    chatos_windows_react_lib::run();
}
