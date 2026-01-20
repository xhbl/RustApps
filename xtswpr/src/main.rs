use std::error::Error;

mod xts_game;
mod xts_ui;
mod xts_color;

use xts_game::load_or_create_config;
use xts_ui::run as run_ui;

fn main() -> Result<(), Box<dyn Error>> {
    let mut cfg = load_or_create_config();
    run_ui(&mut cfg)
}
