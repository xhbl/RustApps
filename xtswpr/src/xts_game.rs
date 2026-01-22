use serde::{Deserialize, Serialize, Deserializer, Serializer};
use chrono::Local;
use directories::ProjectDirs;
use rand::prelude::*;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difficulty { 
    Beginner, 
    Intermediate, 
    Expert, 
    Custom(usize, usize, usize) 
}

impl Serialize for Difficulty {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for Difficulty {
    fn deserialize<D>(deserializer: D) -> Result<Difficulty, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            x if x == Difficulty::Beginner.name() => Ok(Difficulty::Beginner),
            x if x == Difficulty::Intermediate.name() => Ok(Difficulty::Intermediate),
            x if x == Difficulty::Expert.name() => Ok(Difficulty::Expert),
            x if x == Difficulty::Custom(0, 0, 0).name() => Ok(Difficulty::Custom(0, 0, 0)), // Will be set from custom_w/h/n
            _ => Err(serde::de::Error::custom("unknown difficulty")),
        }
    }
}

impl Difficulty {
    pub fn params(&self) -> (usize, usize, usize) {
        match self {
            Difficulty::Beginner => (9, 9, 10),
            Difficulty::Intermediate => (16, 16, 40),
            Difficulty::Expert => (30, 16, 99),
            Difficulty::Custom(w, h, n) => (*w, *h, *n),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Difficulty::Beginner => "Beginner",
            Difficulty::Intermediate => "Intermediate",
            Difficulty::Expert => "Expert",
            Difficulty::Custom(_, _, _) => "Custom",
        }
    }

    pub fn to_index(&self) -> usize {
        match self {
            Difficulty::Beginner => 0,
            Difficulty::Intermediate => 1,
            Difficulty::Expert => 2,
            Difficulty::Custom(_, _, _) => 3,
        }
    }

    pub fn from_index(i: usize, custom_w: usize, custom_h: usize, custom_n: usize) -> Difficulty {
        match i {
            0 => Difficulty::Beginner,
            1 => Difficulty::Intermediate,
            2 => Difficulty::Expert,
            _ => Difficulty::Custom(custom_w, custom_h, custom_n),
        }
    }

    pub fn names() -> [&'static str; 4] {
        [
            Difficulty::Beginner.name(),
            Difficulty::Intermediate.name(),
            Difficulty::Expert.name(),
            Difficulty::Custom(0, 0, 0).name(),
        ]
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Record { pub secs: u64, pub date: String }

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct Config { 
    pub difficulty: Difficulty,
    pub best_beginner: Option<Record>,
    pub best_intermediate: Option<Record>,
    pub best_expert: Option<Record>,
    pub custom_w: usize,
    pub custom_h: usize,
    pub custom_n: usize,
    pub use_question_marks: bool,
    pub ascii_icons: bool,
}

impl Default for Config {
    fn default() -> Self { Config { difficulty: Difficulty::Beginner, best_beginner: None, best_intermediate: None, best_expert: None, custom_w: 36, custom_h: 20, custom_n: 150, use_question_marks: true, ascii_icons: false } }
}

impl Config {
    pub fn get_record(&self, d: &Difficulty) -> Option<u64> {
        match d {
            Difficulty::Beginner => self.best_beginner.as_ref().map(|r| r.secs),
            Difficulty::Intermediate => self.best_intermediate.as_ref().map(|r| r.secs),
            Difficulty::Expert => self.best_expert.as_ref().map(|r| r.secs),
            Difficulty::Custom(_, _, _) => None,
        }
    }

    pub fn get_record_detail(&self, d: &Difficulty) -> Option<(u64,String)> {
        match d {
            Difficulty::Beginner => self.best_beginner.as_ref().map(|r| (r.secs, r.date.clone())),
            Difficulty::Intermediate => self.best_intermediate.as_ref().map(|r| (r.secs, r.date.clone())),
            Difficulty::Expert => self.best_expert.as_ref().map(|r| (r.secs, r.date.clone())),
            Difficulty::Custom(_, _, _) => None,
        }
    }

    pub fn set_record(&mut self, d: &Difficulty, secs: u64) {
        let date = Local::now().format("%Y/%m/%d").to_string();
        match d {
            Difficulty::Beginner => {
                if self.best_beginner.as_ref().map_or(true, |v| secs < v.secs) {
                    self.best_beginner = Some(Record { secs, date });
                }
            }
            Difficulty::Intermediate => {
                if self.best_intermediate.as_ref().map_or(true, |v| secs < v.secs) {
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

#[derive(Clone)]
pub struct Game {
    pub w: usize,
    pub h: usize,
    pub mines: usize,
    pub board: Vec<Cell>,
    pub revealed: Vec<bool>,
    pub flagged: Vec<u8>,
    pub cursor: (usize, usize),
    pub started: bool,
    pub start_time: Option<Instant>,
    pub elapsed: Duration,
    pub game_over: Option<bool>,
}

#[derive(Clone, Copy)]
pub struct Cell { pub mine: bool, pub adj: u8 }

impl Game {
    pub fn new(w: usize, h: usize, mines: usize) -> Self {
        let g = Game {
            w, h, mines,
            board: vec![Cell { mine: false, adj: 0 }; w*h],
            revealed: vec![false; w*h],
            flagged: vec![0u8; w*h],
            cursor: (0,0),
            started: false,
            start_time: None,
            elapsed: Duration::ZERO,
            game_over: None,
        };
        // Mines are placed after the first click to guarantee the first reveal is safe.
        g
    }

    pub fn index(&self, x: usize, y: usize) -> usize { y*self.w + x }

    fn place_mines(&mut self, avoid: Option<(usize,usize)>) {
        let mut rng = thread_rng();
        let n = self.w * self.h;
        // if we need to avoid a cell, ensure we have room for mines
        let mines = if avoid.is_some() { self.mines.min(n.saturating_sub(1)) } else { self.mines.min(n) };
        // clear board
        for i in 0..n { self.board[i] = Cell { mine: false, adj: 0 }; }
        let mut placed = 0;
        let avoid_idx = avoid.map(|(ax,ay)| self.index(ax,ay));
        while placed < mines {
            let i = rng.gen_range(0..n);
            if Some(i) == avoid_idx { continue; }
            if !self.board[i].mine {
                self.board[i].mine = true;
                placed += 1;
            }
        }
        // compute adjacency
        for y in 0..self.h {
            for x in 0..self.w {
                let mut adj = 0u8;
                for oy in y.saturating_sub(1)..=(y+1).min(self.h-1) {
                    for ox in x.saturating_sub(1)..=(x+1).min(self.w-1) {
                        if ox==x && oy==y { continue }
                        if self.board[self.index(ox,oy)].mine { adj += 1 }
                    }
                }
                let idx = self.index(x,y);
                self.board[idx].adj = adj;
            }
        }
    }

    pub fn reveal(&mut self, x: usize, y: usize) {
        // allow '?' (2) to be revealed like unopened cells; only block if flagged (1)
        if self.revealed[self.index(x,y)] || self.flagged[self.index(x,y)] == 1 { return }
        // On first reveal, place mines while avoiding the clicked cell so first click is never a mine.
        if !self.started {
            self.place_mines(Some((x,y)));
            self.started = true;
            self.start_time = Some(Instant::now());
        }
        let idx = self.index(x,y);
        self.revealed[idx] = true;
        if self.board[idx].mine {
            if let Some(t0) = self.start_time { self.elapsed = t0.elapsed(); }
            self.started = false;
            self.game_over = Some(false);
            return
        }
        if self.board[idx].adj == 0 {
            for oy in y.saturating_sub(1)..=(y+1).min(self.h-1) {
                for ox in x.saturating_sub(1)..=(x+1).min(self.w-1) {
                    if !(ox==x && oy==y) { if !self.revealed[self.index(ox,oy)] { self.reveal(ox,oy) } }
                }
            }
        }
        if self.check_win() {
            // Auto-flag any remaining mines when the player wins
            for i in 0..self.w*self.h {
                if self.board[i].mine {
                    self.flagged[i] = 1u8;
                }
            }
            if let Some(t0) = self.start_time { self.elapsed = t0.elapsed(); }
            self.started = false;
            self.game_over = Some(true);
        }
    }

    pub fn toggle_flag(&mut self, x: usize, y: usize) {
        let idx = self.index(x,y);
        if self.revealed[idx] { return }
        // cycle: 0 -> 1 (flag) -> 2 (question) -> 0
        self.flagged[idx] = match self.flagged[idx] {
            0 => 1,
            1 => 2,
            _ => 0,
        };
    }

    pub fn check_win(&self) -> bool {
        for i in 0..self.w*self.h {
            if !self.board[i].mine && !self.revealed[i] { return false }
        }
        true
    }

    pub fn remaining_mines(&self) -> isize {
        let flagged = self.flagged.iter().filter(|b| **b == 1u8).count();
        self.mines as isize - flagged as isize
    }

    pub fn step_cursor(&mut self, dx: isize, dy: isize) {
        let nx = (self.cursor.0 as isize + dx).clamp(0, (self.w-1) as isize) as usize;
        let ny = (self.cursor.1 as isize + dy).clamp(0, (self.h-1) as isize) as usize;
        self.cursor = (nx, ny);
    }

    pub fn reveal_all_mines(&mut self) {
        for i in 0..self.w*self.h {
            if self.board[i].mine { self.revealed[i] = true; }
        }
    }
}

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

pub fn load_or_create_config() -> Config {
    if let Some(path) = config_path() {
        if path.exists() {
            if let Ok(s) = fs::read_to_string(&path) {
                if let Ok(mut cfg) = toml::from_str::<Config>(&s) {
                    // If difficulty is Custom, restore it with the saved custom_w/h/n values
                    if matches!(cfg.difficulty, Difficulty::Custom(_, _, _)) {
                        cfg.difficulty = Difficulty::Custom(cfg.custom_w, cfg.custom_h, cfg.custom_n);
                    }
                    return cfg;
                }
            }
        }
        let cfg = Config::default();
        if let Ok(s) = toml::to_string(&cfg) {
            if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
            let _ = fs::write(&path, s);
        }
        return cfg;
    }
    Config::default()
}

pub fn save_config(cfg: &Config) {
    if let Some(path) = config_path() {
        if let Ok(s) = toml::to_string(cfg) {
            if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
            let _ = fs::write(&path, s);
        }
    }
}
