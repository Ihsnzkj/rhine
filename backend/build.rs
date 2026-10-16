fn main() {
    // 1.2 introduced the monitor signal used by this application, but 1.3
    // fixed a use-after-free when the compositor refuses a session lock.
    pkg_config::Config::new()
        .atleast_version("1.3")
        .probe("gtk4-layer-shell-0")
        .expect("gtk4-layer-shell >= 1.3 is required for safe lock refusal");
}
