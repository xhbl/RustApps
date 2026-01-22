// Entry point for the Minesweeper TUI application
// Initializes configuration, language settings, and launches the main UI

use std::error::Error;

// Module declarations
mod xts_color; // Cross-platform color matching utilities
mod xts_game;  // Core game logic and configuration
mod xts_lang;  // Multi-language string resources
mod xts_ui;    // Terminal UI rendering and event handling

use xts_game::load_or_create_config;
use xts_lang::Lang;
use xts_ui::run as run_ui;

fn main() -> Result<(), Box<dyn Error>> {
    // Load or create user configuration (difficulty, preferences, records)
    let mut cfg = load_or_create_config();
    
    // Initialize language resources based on saved or system language
    let mut lang = Lang::new(&cfg.language);
    
    // Launch the main UI loop
    run_ui(&mut cfg, &mut lang)
}
