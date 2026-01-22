// Core game logic and configuration management
// Handles board generation, game state, records, and configuration persistence

use chrono::Local;
use directories::ProjectDirs;
use rand::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Difficulty presets and custom settings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difficulty {
    Beginner,                        // 9x9, 10 mines
    Intermediate,                    // 16x16, 40 mines
    Expert,                          // 30x16, 99 mines
    Custom(usize, usize, usize), // width, height, mines
}

impl Serialize for Difficulty {
    /// Serialize difficulty as a human-readable string (not an index)
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Difficulty {
    /// Deserialize difficulty from string name in config file
    fn deserialize<D>(deserializer: D) -> Result<Difficulty, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            x if x == Difficulty::Beginner.name() => Ok(Difficulty::Beginner),
            x if x == Difficulty::Intermediate.name() => Ok(Difficulty::Intermediate),
            x if x == Difficulty::Expert.name() => Ok(Difficulty::Expert),
            // Custom will be reconstructed from custom_w/h/n fields
            x if x == Difficulty::Custom(0, 0, 0).name() => Ok(Difficulty::Custom(0, 0, 0)),
            _ => Err(serde::de::Error::custom("unknown difficulty")),
        }
    }
}

impl Difficulty {
    /// Get game dimensions (width, height, mine count) for this difficulty
    pub fn params(&self) -> (usize, usize, usize) {
        match self {
            Difficulty::Beginner => (9, 9, 10),
            Difficulty::Intermediate => (16, 16, 40),
            Difficulty::Expert => (30, 16, 99),
            Difficulty::Custom(w, h, n) => (*w, *h, *n),
        }
    }

    /// Get the config file identifier for this difficulty
    /// Used for serialization - should remain stable across versions
    pub fn name(&self) -> &'static str {
        match self {
            Difficulty::Beginner => "Beginner",
            Difficulty::Intermediate => "Intermediate",
            Difficulty::Expert => "Expert",
            Difficulty::Custom(_, _, _) => "Custom",
        }
    }

    /// Convert difficulty to array index (0-3)
    pub fn to_index(&self) -> usize {
        match self {
            Difficulty::Beginner => 0,
            Difficulty::Intermediate => 1,
            Difficulty::Expert => 2,
            Difficulty::Custom(_, _, _) => 3,
        }
    }

    /// Create difficulty from array index and custom parameters
    pub fn from_index(i: usize, custom_w: usize, custom_h: usize, custom_n: usize) -> Difficulty {
        match i {
            0 => Difficulty::Beginner,
            1 => Difficulty::Intermediate,
            2 => Difficulty::Expert,
            _ => Difficulty::Custom(custom_w, custom_h, custom_n),
        }
    }
}

/// Record entry for best completion time
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Record {
    pub secs: u64,       // Completion time in seconds
    pub date: String,    // Date in ISO format (YYYY-MM-DD)
}

/// User configuration and game records
/// Persisted to disk as TOML
#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // Current difficulty setting
    pub difficulty: Difficulty,
    
    // Best time records for each preset difficulty
    pub best_beginner: Option<Record>,
    pub best_intermediate: Option<Record>,
    pub best_expert: Option<Record>,
    
    // Custom difficulty parameters
    pub custom_w: usize,
    pub custom_h: usize,
    pub custom_n: usize,
    
    // Game preferences
    pub use_question_marks: bool,  // Enable three-state flagging (none/flag/?)
    pub show_indicator: bool,       // Show cursor position indicator
    pub ascii_icons: bool,          // Use ASCII fallback icons
    pub language: String,           // Language code ("en" or "zh")
}

impl Default for Config {
    fn default() -> Self {
        // Auto-detect system language on first run
        let system_lang = sys_locale::get_locale().unwrap_or_else(|| "en".to_string());
        let lang = if system_lang.to_lowercase().starts_with("zh") {
            "zh".to_string()
        } else {
            "en".to_string()
        };

        Config {
            difficulty: Difficulty::Beginner,
            best_beginner: None,
            best_intermediate: None,
            best_expert: None,
            custom_w: 36,
            custom_h: 20,
            custom_n: 150,
            use_question_marks: false,
            show_indicator: false,
            ascii_icons: false,
            language: lang,
        }
    }
}

impl Config {
    /// Get the best time (seconds only) for a given difficulty
    /// Returns None for Custom difficulty
    pub fn get_record(&self, d: &Difficulty) -> Option<u64> {
        match d {
            Difficulty::Beginner => self.best_beginner.as_ref().map(|r| r.secs),
            Difficulty::Intermediate => self.best_intermediate.as_ref().map(|r| r.secs),
            Difficulty::Expert => self.best_expert.as_ref().map(|r| r.secs),
            Difficulty::Custom(_, _, _) => None,
        }
    }

    /// Get the best time and date for a given difficulty
    /// Returns None for Custom difficulty
    pub fn get_record_detail(&self, d: &Difficulty) -> Option<(u64, String)> {
        match d {
            Difficulty::Beginner => self
                .best_beginner
                .as_ref()
                .map(|r| (r.secs, r.date.clone())),
            Difficulty::Intermediate => self
                .best_intermediate
                .as_ref()
                .map(|r| (r.secs, r.date.clone())),
            Difficulty::Expert => self.best_expert.as_ref().map(|r| (r.secs, r.date.clone())),
            Difficulty::Custom(_, _, _) => None,
        }
    }

    /// Update the best time record if the new time is better
    /// Only records for preset difficulties (not Custom)
    pub fn set_record(&mut self, d: &Difficulty, secs: u64) {
        let date = Local::now().format("%Y-%m-%d").to_string();
        match d {
            Difficulty::Beginner => {
                if self.best_beginner.as_ref().map_or(true, |v| secs < v.secs) {
                    self.best_beginner = Some(Record { secs, date });
                }
            }
            Difficulty::Intermediate => {
                if self
                    .best_intermediate
                    .as_ref()
                    .map_or(true, |v| secs < v.secs)
                {
                    self.best_intermediate = Some(Record { secs, date });
                }
            }
            Difficulty::Expert => {
                if self.best_expert.as_ref().map_or(true, |v| secs < v.secs) {
                    self.best_expert = Some(Record { secs, date });
                }
            }
            Difficulty::Custom(_, _, _) => {
                // Do not record time for Custom difficulty
            }
        }
    }
}

/// Main game state
#[derive(Clone)]
pub struct Game {
    pub w: usize,            // Board width
    pub h: usize,            // Board height
    pub mines: usize,        // Total mine count
    pub board: Vec<Cell>,    // Board cells (mines + adjacency counts)
    pub revealed: Vec<bool>, // Cell reveal status
    pub flagged: Vec<u8>,    // Cell flag status (0=none, 1=flag, 2=question)
    pub cursor: (usize, usize), // Current cursor position
    pub started: bool,       // Has the game started (first reveal)
    pub start_time: Option<Instant>, // Timer start instant
    pub elapsed: Duration,   // Total elapsed time
    pub game_over: Option<bool>, // Game result (Some(true)=win, Some(false)=loss, None=ongoing)
}

/// A single cell on the minesweeper board
#[derive(Clone, Copy)]
pub struct Cell {
    pub mine: bool, // Contains a mine
    pub adj: u8,    // Adjacent mine count (0-8)
}

impl Game {
    /// Create a new game with the specified dimensions
    /// Board is initially empty (no mines placed yet)
    pub fn new(w: usize, h: usize, mines: usize) -> Self {
        let g = Game {
            w,
            h,
            mines,
            board: vec![
                Cell {
                    mine: false,
                    adj: 0
                };
                w * h
            ],
            revealed: vec![false; w * h],
            flagged: vec![0u8; w * h],
            cursor: (0, 0),
            started: false,
            start_time: None,
            elapsed: Duration::ZERO,
            game_over: None,
        };
        // Mines are placed on first reveal to guarantee safe first click
        g
    }

    /// Convert (x, y) coordinates to flat array index
    pub fn index(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    /// Randomly place mines on the board, avoiding a specific cell if provided
    /// Also calculates adjacency counts for all cells
    fn place_mines(&mut self, avoid: Option<(usize, usize)>) {
        let mut rng = thread_rng();
        let n = self.w * self.h;
        // if we need to avoid a cell, ensure we have room for mines
        let mines = if avoid.is_some() {
            self.mines.min(n.saturating_sub(1))
        } else {
            self.mines.min(n)
        };
        // clear board
        for i in 0..n {
            self.board[i] = Cell {
                mine: false,
                adj: 0,
            };
        }
        let mut placed = 0;
        let avoid_idx = avoid.map(|(ax, ay)| self.index(ax, ay));
        while placed < mines {
            let i = rng.gen_range(0..n);
            if Some(i) == avoid_idx {
                continue;
            }
            if !self.board[i].mine {
                self.board[i].mine = true;
                placed += 1;
            }
        }
        // compute adjacency
        for y in 0..self.h {
            for x in 0..self.w {
                let mut adj = 0u8;
                for oy in y.saturating_sub(1)..=(y + 1).min(self.h - 1) {
                    for ox in x.saturating_sub(1)..=(x + 1).min(self.w - 1) {
                        if ox == x && oy == y {
                            continue;
                        }
                        if self.board[self.index(ox, oy)].mine {
                            adj += 1
                        }
                    }
                }
                let idx = self.index(x, y);
                self.board[idx].adj = adj;
            }
        }
    }

    /// Reveal a cell at (x, y)
    /// - First reveal places mines and starts the timer
    /// - Auto-reveals neighbors if cell has no adjacent mines (flood fill)
    /// - Ends game on mine hit or win condition
    pub fn reveal(&mut self, x: usize, y: usize) {
        // Allow revealing cells marked with '?' but not flagged cells
        if self.revealed[self.index(x, y)] || self.flagged[self.index(x, y)] == 1 {
            return;
        }
        // On first reveal, place mines while avoiding this cell (safe first click)
        if !self.started {
            self.place_mines(Some((x, y)));
            self.started = true;
            self.start_time = Some(Instant::now());
        }
        let idx = self.index(x, y);
        self.revealed[idx] = true;
        if self.board[idx].mine {
            // Hit a mine - game over (loss)
            if let Some(t0) = self.start_time {
                self.elapsed = t0.elapsed();
            }
            self.started = false;
            self.game_over = Some(false);
            return;
        }
        // Flood fill: auto-reveal neighbors if this cell has no adjacent mines
        if self.board[idx].adj == 0 {
            for oy in y.saturating_sub(1)..=(y + 1).min(self.h - 1) {
                for ox in x.saturating_sub(1)..=(x + 1).min(self.w - 1) {
                    if !(ox == x && oy == y) {
                        if !self.revealed[self.index(ox, oy)] {
                            self.reveal(ox, oy)
                        }
                    }
                }
            }
        }
        if self.check_win() {
            // Auto-flag any remaining mines when the player wins
            for i in 0..self.w * self.h {
                if self.board[i].mine {
                    self.flagged[i] = 1u8;
                }
            }
            if let Some(t0) = self.start_time {
                self.elapsed = t0.elapsed();
            }
            self.started = false;
            self.game_over = Some(true);
        }
    }

    /// Toggle flag state for a cell
    /// - Two-state mode: none ↔ flag
    /// - Three-state mode: none → flag → question mark → none
    pub fn toggle_flag(&mut self, x: usize, y: usize, use_question_marks: bool) {
        let idx = self.index(x, y);
        if self.revealed[idx] {
            return;
        }
        if use_question_marks {
            // Cycle: 0 (none) → 1 (flag) → 2 (question) → 0
            self.flagged[idx] = match self.flagged[idx] {
                0 => 1,
                1 => 2,
                _ => 0,
            };
        } else {
            // Cycle: 0 (none) ↔ 1 (flag)
            self.flagged[idx] = if self.flagged[idx] == 1 { 0 } else { 1 };
        }
    }

    /// Check if all non-mine cells have been revealed (win condition)
    pub fn check_win(&self) -> bool {
        for i in 0..self.w * self.h {
            if !self.board[i].mine && !self.revealed[i] {
                return false;
            }
        }
        true
    }

    /// Get the mine counter display value (total mines - flagged cells)
    /// Can be negative if player places too many flags
    pub fn remaining_mines(&self) -> isize {
        let flagged = self.flagged.iter().filter(|b| **b == 1u8).count();
        self.mines as isize - flagged as isize
    }

    pub fn step_cursor(&mut self, dx: isize, dy: isize) {
        let nx = (self.cursor.0 as isize + dx).clamp(0, (self.w - 1) as isize) as usize;
        let ny = (self.cursor.1 as isize + dy).clamp(0, (self.h - 1) as isize) as usize;
        self.cursor = (nx, ny);
    }

    pub fn reveal_all_mines(&mut self) {
        for i in 0..self.w * self.h {
            if self.board[i].mine {
                self.revealed[i] = true;
            }
        }
    }
}

/// Get the configuration file path
/// Uses platform-specific config directory (e.g., ~/.config/xtswpr/xtswpr.toml on Linux)
/// Falls back to current directory if ProjectDirs is unavailable
pub fn config_path() -> Option<PathBuf> {
    // Use ProjectDirs so config is stored under a per-project config directory:
    // ProjectDirs::from("com","xhbl", exe_name) -> config_dir/<exe_name>.toml
    if let Ok(exe) = env::current_exe() {
        if let Some(name) = exe.file_stem().and_then(|s| s.to_str()) {
            if let Some(proj) = ProjectDirs::from("com", "xhbl", name) {
                let mut path = proj.config_dir().to_path_buf();
                path.push(format!("{}.toml", name));
                return Some(path);
            } else {
                // fallback to current directory
                if let Ok(mut path) = env::current_dir() {
                    path.push(format!("{}.toml", name));
                    return Some(path);
                }
            }
        }
    }
    None
}

/// Load configuration from disk, or create default if not found
/// Automatically reconstructs Custom difficulty parameters from saved values
pub fn load_or_create_config() -> Config {
    if let Some(path) = config_path() {
        if path.exists() {
            if let Ok(s) = fs::read_to_string(&path) {
                if let Ok(mut cfg) = toml::from_str::<Config>(&s) {
                    // If difficulty is Custom, restore it with the saved custom_w/h/n values
                    if matches!(cfg.difficulty, Difficulty::Custom(_, _, _)) {
                        cfg.difficulty =
                            Difficulty::Custom(cfg.custom_w, cfg.custom_h, cfg.custom_n);
                    }
                    return cfg;
                }
            }
        }
        let cfg = Config::default();
        if let Ok(s) = toml::to_string(&cfg) {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, s);
        }
        return cfg;
    }
    Config::default()
}

/// Save configuration to disk as TOML
pub fn save_config(cfg: &Config) {
    if let Some(path) = config_path() {
        if let Ok(s) = toml::to_string(cfg) {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, s);
        }
    }
}
