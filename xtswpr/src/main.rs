use std::error::Error;

mod xts_game;
mod xts_ui;
mod xts_color;
mod xts_lang;

use xts_game::load_or_create_config;
use xts_ui::run as run_ui;
use xts_lang::Lang;

fn main() -> Result<(), Box<dyn Error>> {
    let mut cfg = load_or_create_config();
    let mut lang = Lang::new(&cfg.language);
    run_ui(&mut cfg, &mut lang)
}
