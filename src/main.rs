mod display;
// Not wired into the display yet; the tests exercise it.
#[allow(dead_code, unused_imports)]
mod physics;

fn main() {
    if let Err(err) = display::run() {
        eprintln!("display error: {err}");
        std::process::exit(1);
    }
}
