use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEventKind, MouseButton};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Span, Spans, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Clear};
use ratatui::Terminal;
use std::error::Error;
use std::io;
use std::time::{Duration, Instant};

use crate::xts_color::WTMatch;
use crate::xts_game::{Game, Config, Difficulty, save_config};
use unicode_width::UnicodeWidthStr;

fn reset_ui_after_new_game(game: &mut Game, ui: &mut UiState) {
    ui.reset_after_new_game();
    ui.cursor_indicator = Some(game.cursor);
}

// Group runtime UI variables into a single structure to simplify passing them around
#[derive(Debug)]
struct UiState {
    left_press: Option<(usize,usize)>,
    _right_press: Option<(usize,usize)>,
    chord_active: Option<(usize,usize)>,
    // simulate key release timer: (start_instant, kind) where kind: 0=space,1=enter
    key_timer: Option<(Instant,u8)>,
    // runtime detection whether real key-release events are supported by the terminal
    supports_key_release: bool,
    // cursor indicator position (cell coords) for TUI
    cursor_indicator: Option<(usize,usize)>,
    flash_cell: Option<((usize,usize), Instant)>,
    clicked_index: Option<usize>,
    click_instant: Option<Instant>,
    hover_index: Option<usize>,
    modal_close_hovered: bool,
    modal_close_pressed: bool,
    modal_rect: Option<Rect>,
    modal_close_rect: Option<Rect>,
    showing_difficulty: bool,
    showing_about: bool,
    showing_options: bool,
    options_use_q: bool,
    options_ascii: bool,
    options_indicator: bool,
    options_use_q_rect: Option<Rect>,
    options_ascii_rect: Option<Rect>,
    options_indicator_rect: Option<Rect>,
    options_focus: Option<u8>,
    difficulty_hover: Option<usize>,
    showing_help: bool,
    showing_record: bool,
    showing_win: bool,
    showing_loss: bool,
    last_run_new_record: bool,
    exit_menu_item_down: bool,  // Track when exit menu item is pressed, wait for release
    exit_status_hovered: bool,
    custom_input_mode: Option<u8>,  // 0=width, 1=height, 2=mines; None=not in custom input
    custom_w_str: String,
    custom_h_str: String,
    custom_n_str: String,
    custom_error_msg: Option<String>,
    custom_w_rect: Option<Rect>,
    custom_h_rect: Option<Rect>,
    custom_n_rect: Option<Rect>,
    custom_invalid_field: Option<(u8, Instant)>,  // (field_index, flash_start_time) for error flashing
}

impl UiState {
    fn new() -> Self {
        UiState {
            left_press: None,
            _right_press: None,
            chord_active: None,
            flash_cell: None,
            clicked_index: None,
            click_instant: None,
            hover_index: None,
            modal_close_hovered: false,
            modal_close_pressed: false,
            modal_rect: None,
            modal_close_rect: None,
            showing_difficulty: false,
            showing_about: false,
            showing_options: false,
            options_use_q: false,
            options_ascii: false,
            options_indicator: false,
            options_use_q_rect: None,
            options_ascii_rect: None,
            options_indicator_rect: None,
            options_focus: None,
            difficulty_hover: None,
            showing_help: false,
            showing_record: false,
            showing_win: false,
            showing_loss: false,
            last_run_new_record: false,
            exit_menu_item_down: false,
            exit_status_hovered: false,
            custom_input_mode: None,
            custom_w_str: String::new(),
            custom_h_str: String::new(),
            custom_n_str: String::new(),
            custom_error_msg: None,
            custom_w_rect: None,
            custom_h_rect: None,
            custom_n_rect: None,
            custom_invalid_field: None,
            key_timer: None,
            supports_key_release: cfg!(windows),
            cursor_indicator: None,
        }
    }

    fn reset_after_new_game(&mut self) {
        self.last_run_new_record = false;
        self.left_press = None;
        self._right_press = None;
        self.chord_active = None;
        self.flash_cell = None;
        self.clicked_index = None;
        self.click_instant = None;
        self.hover_index = None;
        self.modal_close_hovered = false;
        self.modal_close_pressed = false;
        self.modal_rect = None;
        self.modal_close_rect = None;
        self.showing_difficulty = false;
        self.showing_about = false;
        self.showing_options = false;
        self.options_use_q = false;
        self.options_ascii = false;
        self.options_indicator = false;
        self.options_use_q_rect = None;
        self.options_ascii_rect = None;
        self.options_indicator_rect = None;
        self.options_focus = None;
        self.difficulty_hover = None;
        self.showing_help = false;
        self.showing_record = false;
        self.showing_win = false;
        self.showing_loss = false;
        self.exit_menu_item_down = false;
        self.custom_invalid_field = None;
        self.key_timer = None;
        self.supports_key_release = cfg!(windows);
        self.cursor_indicator = None;
    }
}

pub fn run(cfg: &mut Config) -> Result<(), Box<dyn Error>> {
    let (w,h,mines) = cfg.difficulty.params();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnableMouseCapture, terminal::EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut game = Game::new(w,h,mines);
    // grouped runtime UI state
    let mut ui = UiState::new();
    ui.cursor_indicator = Some(game.cursor);
    let mut menu_rect: Option<Rect> = None;
    let mut board_rect: Option<Rect> = None;
    let mut status_rect: Option<Rect> = None;
    // Centralized menu/key items (key, rest). Include Esc here so status can reuse it.
    let menu_items = [
        ("F1", "Help"),
        ("F2", "New"),
        ("F4", "Records"),
        ("F5", "Difficulty"),
        ("F7", "Options"),
        ("F9", "About"),
        ("Esc", "Exit"),
    ];
    let mut difficulty_selected: usize = cfg.difficulty.to_index();
    let mut exit_requested: bool = false;

    // Glyph computation helper: compute glyphs based on ascii_icons setting.
    let make_glyphs = |ascii: bool| {
        (
            (if ascii { "▪" } else { "■" }, Color::Gray.wtmatch()),
            (if ascii { "*" } else { "☼" }, Color::Black.wtmatch()),
            (if ascii { "F" } else { "⚑" }, Color::Red.wtmatch()),
            ("?", Color::Red.wtmatch()),
        )
    };

    // initialize glyphs once from current config
    let g_init = make_glyphs(cfg.ascii_icons);
    let mut glyph_unopened = g_init.0;
    let mut glyph_mine = g_init.1;
    let mut glyph_flag = g_init.2;
    let mut glyph_question = g_init.3;

    // Centralized glyph/color definitions are computed per-frame inside the draw closure
    // Background color for the minefield (change this variable to alter background)
    let board_bg = Color::DarkGray.wtmatch();
    // Cursor background color (centralized)
    let cursor_bg = Color::LightBlue.wtmatch();
    // Background color for neighbor highlight / reveal press
    let reveal_bg = Color::DarkGray.wtmatch();
    // Flash (warning) colors when chord fails
    let flash_bg = Color::Red.wtmatch();
    let flash_fg = Color::White.wtmatch();
    let flash_mod = Modifier::BOLD;
    // Menu / key label colors (centralized)
    let menu_key_fg = Color::Yellow.wtmatch();
    let menu_key_bg_hover = Color::LightBlue.wtmatch();
    let menu_key_bg_pressed = Color::Green.wtmatch();
    let menu_key_fg_pressed = Color::Black.wtmatch();
    // cursor indicator appearance
    let indicator_char = "▸";
    let indicator_fg = Color::Yellow.wtmatch();
    // Number colors for revealed cells 1..8
    let num_colors: [Color; 8] = [
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
        Color::Blue.wtmatch(),
    ];

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            let size = f.size();
            let min_twidth = 80u16;
            let min_theight = 24u16 + game.h.saturating_sub(16) as u16;
            // If terminal too small, render a centered warning and skip normal UI
            if size.width < min_twidth || size.height < min_theight {
                let warn_lines = vec![Spans::from(Span::raw("Terminal size too small.")), Spans::from(Span::raw(format!("Minimum required: {} x {}", min_twidth, min_theight)))];
                let warn = Paragraph::new(Text::from(warn_lines))
                    .block(Block::default().borders(Borders::ALL).title("Resize Terminal"))
                    .alignment(Alignment::Center);
                // clear screen and render warning centered
                f.render_widget(Clear, size);
                let w = 40u16.min(size.width.saturating_sub(2));
                let h = 5u16.min(size.height.saturating_sub(2));
                let area = center_rect(w, h, size);
                f.render_widget(warn, area);
                return;
            }

            // layout: top menu row, center board, bottom status
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(0)
                .constraints([Constraint::Length(3), Constraint::Min(6), Constraint::Length(3)].as_ref())
                .split(size);

            // menu row (per-item styled so hover/click mapping aligns with mouse offsets)
            let mut spans_vec: Vec<Span> = Vec::new();
            for (i, (label_key, label_rest)) in menu_items.iter().take(6).enumerate() {
                if i > 0 {
                    spans_vec.push(Span::raw("   "));
                }
                let (key_style, rest_style) = if Some(i) == ui.clicked_index {
                    (Style::default().bg(menu_key_bg_pressed).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD), Style::default().bg(menu_key_bg_pressed).fg(menu_key_fg_pressed))
                } else if Some(i) == ui.hover_index {
                    (Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD), Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed))
                } else {
                    (Style::default().fg(menu_key_fg).add_modifier(Modifier::BOLD), Style::default())
                };

                spans_vec.push(Span::styled(label_key.to_string(), key_style));
                spans_vec.push(Span::styled(format!(": {}", label_rest), rest_style));
            }
            // add one-space padding left and right inside the menu block
            spans_vec.insert(0, Span::raw(" "));
            spans_vec.push(Span::raw(" "));
            let menu = Paragraph::new(Spans::from(spans_vec)).block(Block::default().borders(Borders::ALL)).alignment(Alignment::Left);
            f.render_widget(menu, chunks[0]);
            menu_rect = Some(chunks[0]);

            // status row (left info + right-aligned Esc: Exit)
            let left_text = format!(" Mines: {}   Time: {}s ", game.remaining_mines(), if game.started { game.start_time.unwrap().elapsed().as_secs() } else { game.elapsed.as_secs() });
            let esc = menu_items.iter().find(|(k, _)| *k == "Esc").unwrap_or(&("Esc", "Exit"));
            let right_key = esc.0;
            let right_rest = esc.1;
            let inner_w = chunks[2].width.saturating_sub(2) as usize;
            let left_w = left_text.as_str().width();
            // account for the ": " we add when rendering the right-hand key/rest
            let right_w = right_key.width() + 2 + right_rest.width();
            let mid_spaces = if inner_w > left_w + right_w + 1 { inner_w - left_w - right_w - 1 } else { 1 };
            let mut status_spans: Vec<Span> = Vec::new();
            status_spans.push(Span::raw(left_text));
            status_spans.push(Span::raw(" ".repeat(mid_spaces)));
            let mut key_style = Style::default().fg(menu_key_fg).add_modifier(Modifier::BOLD);
            let mut rest_style = Style::default();
            if ui.exit_menu_item_down {
                key_style = Style::default().bg(menu_key_bg_pressed).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD);
                rest_style = Style::default().bg(menu_key_bg_pressed).fg(menu_key_fg_pressed);
            } else if ui.exit_status_hovered {
                key_style = Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD);
                rest_style = Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed);
            }
            status_spans.push(Span::styled(right_key.to_string(), key_style));
            status_spans.push(Span::styled(format!(": {}", right_rest), rest_style));
            status_spans.push(Span::raw(" "));
            let status = Paragraph::new(Text::from(Spans::from(status_spans)))
                .block(Block::default().borders(Borders::ALL))
                .alignment(Alignment::Left);
            f.render_widget(status, chunks[2]);
            status_rect = Some(chunks[2]);

            // glyphs are computed outside the main loop and updated when config changes

            // board area
            let board_area = centered_block(((game.w * 2) as u16) + 3, (game.h as u16) + 2, chunks[1]);
            board_rect = Some(board_area);
            let mut lines = vec![];
            for y in 0..game.h {
                let mut spans = vec![];
                for x in 0..game.w {
                    let idx = game.index(x,y);
                        let mut s = glyph_unopened.0.to_string();
                        let mut style = Style::default().fg(glyph_unopened.1).bg(board_bg);
                    if game.cursor == (x,y) { style = style.bg(cursor_bg); }
                    if game.revealed[idx] {
                            if game.board[idx].mine { s = glyph_mine.0.to_string(); style = style.fg(glyph_mine.1); }
                            else if game.board[idx].adj>0 { let n = (game.board[idx].adj as usize).saturating_sub(1); s = format!("{}", game.board[idx].adj); style = style.fg(num_colors[n]); }
                        else { s = " ".to_string(); }
                        } else if game.flagged[idx] == 1 { s = glyph_flag.0.to_string(); style = style.fg(glyph_flag.1); }
                        else if game.flagged[idx] == 2 { s = glyph_question.0.to_string(); style = style.fg(glyph_question.1); }
                    // highlight neighbors for active chord (both buttons pressed)
                    if let Some((ccx, ccy)) = ui.chord_active {
                        let xmin = ccx.saturating_sub(1);
                        let xmax = (ccx+1).min(game.w-1);
                        let ymin = ccy.saturating_sub(1);
                        let ymax = (ccy+1).min(game.h-1);
                        if x >= xmin && x <= xmax && y >= ymin && y <= ymax {
                            if !game.revealed[idx] && game.flagged[idx] != 1 {
                                style = style.bg(reveal_bg).fg(reveal_bg);
                            }
                        }
                    }
                    // highlight single-cell press (space or mouse down) using same chord color
                    if let Some((lx,ly)) = ui.left_press {
                        if x==lx && y==ly {
                            if !game.revealed[idx] && game.flagged[idx] != 1 {
                                style = style.bg(reveal_bg).fg(reveal_bg);
                            }
                        }
                    }
                    // apply flash style if this cell is flashing
                    if let Some(((fx,fy), t0)) = ui.flash_cell {
                        if fx==x && fy==y && t0.elapsed() < Duration::from_millis(350) {
                            style = style.bg(flash_bg).fg(flash_fg).add_modifier(flash_mod);
                        }
                    }
                    // render cursor indicator if enabled and mouse is over this cell
                    if cfg.show_indicator && ui.cursor_indicator == Some((x,y)) {
                        let indicator_style = style.fg(indicator_fg).add_modifier(Modifier::BOLD);
                        spans.push(Span::styled(indicator_char.to_string(), indicator_style));
                        spans.push(Span::styled(format!("{}", s), style));
                    } else {
                        spans.push(Span::styled(format!(" {}", s), style));
                    }
                }
                // append a one-character padding column so the right-side visual padding
                // uses the same background as the board
                spans.push(Span::styled(" ", Style::default().bg(board_bg)));
                lines.push(Spans::from(spans));
            }
            let paragraph = Paragraph::new(Text::from(lines)).block(Block::default().borders(Borders::ALL).title(cfg.difficulty.name()).title_alignment(Alignment::Center)).alignment(Alignment::Left);
            f.render_widget(paragraph, board_area);

            // modals
            ui.modal_close_rect = None;
            if ui.showing_difficulty {
                // If in custom input mode, show a larger dialog for input
                if ui.custom_input_mode.is_some() {
                    let mrect = centered_block(42, 10, size);
                    ui.modal_rect = Some(mrect);
                    f.render_widget(Clear, mrect);
                    f.render_widget(Block::default().borders(Borders::ALL).title(format!("{} {}", Difficulty::Custom(0,0,0).name(), menu_items[3].1)), mrect);
                    let inner = Rect::new(mrect.x + 1, mrect.y + 1, mrect.width.saturating_sub(2), mrect.height.saturating_sub(2));
                    
                    // Calculate max mines based on current W and H input
                    let w_val = ui.custom_w_str.trim().parse::<usize>().unwrap_or(0);
                    let h_val = ui.custom_h_str.trim().parse::<usize>().unwrap_or(0);
                    let max_mines = if w_val > 0 && h_val > 0 { ((w_val * h_val) as f64 * 0.926) as usize } else { 0 };
                    
                    let mut lines = vec![Spans::from(Span::raw(""))];
                    
                    // Use fixed label width for alignment (20 chars)
                    let label_width = 20usize;
                    
                    // Check if any field is in flash state (invalid input)
                    let is_flashing = if let Some((_, flash_time)) = ui.custom_invalid_field {
                        flash_time.elapsed() < Duration::from_millis(600)
                    } else {
                        false
                    };
                    
                    // Width row - label and input on same line
                    let w_style = if ui.custom_input_mode == Some(0) { Style::default().bg(Color::Yellow).fg(Color::Black) } else { Style::default().bg(Color::DarkGray) };
                    let w_label = format!("{:<width$}", "Width (9-36):", width = label_width);
                    let w_label_style = if is_flashing && ui.custom_invalid_field == Some((0, ui.custom_invalid_field.unwrap().1)) {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    lines.push(Spans::from(vec![
                        Span::raw(" "),
                        Span::styled(w_label, w_label_style),
                        Span::styled(format!("{:<3}", ui.custom_w_str), w_style),
                    ]));
                    
                    lines.push(Spans::from(Span::raw("")));
                    
                    // Height row - label and input on same line
                    let h_style = if ui.custom_input_mode == Some(1) { Style::default().bg(Color::Yellow).fg(Color::Black) } else { Style::default().bg(Color::DarkGray) };
                    let h_label = format!("{:<width$}", "Height (9-24):", width = label_width);
                    let h_label_style = if is_flashing && ui.custom_invalid_field == Some((1, ui.custom_invalid_field.unwrap().1)) {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    lines.push(Spans::from(vec![
                        Span::raw(" "),
                        Span::styled(h_label, h_label_style),
                        Span::styled(format!("{:<3}", ui.custom_h_str), h_style),
                    ]));
                    
                    lines.push(Spans::from(Span::raw("")));
                    
                    // Mines row - label shows actual max value and input on same line
                    let n_style = if ui.custom_input_mode == Some(2) { Style::default().bg(Color::Yellow).fg(Color::Black) } else { Style::default().bg(Color::DarkGray) };
                    let n_label = format!("{:<width$}", format!("Mines (10-{}):", max_mines), width = label_width);
                    let n_label_style = if is_flashing && ui.custom_invalid_field == Some((2, ui.custom_invalid_field.unwrap().1)) {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    lines.push(Spans::from(vec![
                        Span::raw(" "),
                        Span::styled(n_label, n_label_style),
                        Span::styled(format!("{:<3}", ui.custom_n_str), n_style),
                    ]));
                    
                    // Error message will be displayed just above OK button
                    
                    let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Left);
                    f.render_widget(p, inner);
                    
                    // Calculate input field rectangles for mouse click detection
                    // Row 1 = Width input, Row 3 = Height input, Row 5 = Mines input
                    let label_len = 20u16; // Fixed label width for alignment
                    let indent = 1u16; // One space indentation
                    ui.custom_w_rect = Some(Rect::new(inner.x + indent + label_len, inner.y + 1, 3, 1));
                    ui.custom_h_rect = Some(Rect::new(inner.x + indent + label_len, inner.y + 3, 3, 1));
                    ui.custom_n_rect = Some(Rect::new(inner.x + indent + label_len, inner.y + 5, 3, 1));
                } else {
                    ui.custom_w_rect = None;
                    ui.custom_h_rect = None;
                    ui.custom_n_rect = None;
                    // Normal difficulty selection
                    let mrect = centered_block(42, 10, size);
                    ui.modal_rect = Some(mrect);
                    f.render_widget(Clear, mrect);
                    f.render_widget(Block::default().borders(Borders::ALL).title(menu_items[3].1), mrect);
                    let inner = Rect::new(mrect.x + 1, mrect.y + 1, mrect.width.saturating_sub(2), mrect.height.saturating_sub(2));
                    let mut lines = vec![Spans::from(Span::raw(""))];
                    
                                    // Pre-defined difficulties
                                    // compute hovered/selected index for focus-based highlight
                                    let hover_index = ui.difficulty_hover.unwrap_or(difficulty_selected);
                                    for (i, d) in [Difficulty::Beginner, Difficulty::Intermediate, Difficulty::Expert].iter().enumerate() {
                                        // show star on the hovered item if present, otherwise on the selected item
                                        let mark = if i == hover_index { "*" } else { " " };
                                        let (ww, hh, mn) = d.params();
                                        let idx = format!(" {} ", i + 1);
                                        // Build name field using display width so wide characters align
                                        let name = d.name();
                                        let name_disp_w = name.width();
                                        let name_col_w = 14usize;
                                        let name_pad = name_col_w.saturating_sub(name_disp_w);
                                        let name_field = format!("{}{}", name, " ".repeat(name_pad));
                                        let suffix = format!(") {} {:>2}x{:<2}  {} mines", name_field, ww, hh, mn);
                                        let focus_style = Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD);
                                        if i == hover_index {
                                            let spans = Spans::from(vec![
                                                Span::raw(idx),
                                                Span::styled(mark, focus_style),
                                                Span::styled(suffix, focus_style),
                                            ]);
                                            lines.push(spans);
                                        } else {
                                            let mark_style = if i == difficulty_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                                            let spans = Spans::from(vec![
                                                Span::raw(idx),
                                                Span::styled(mark, mark_style),
                                                Span::raw(suffix),
                                            ]);
                                            lines.push(spans);
                                        }
                                    }
                    
                    // Custom difficulty option: support hover highlight and star on hover
                    let hover_index = ui.difficulty_hover.unwrap_or(difficulty_selected);
                    let mark = if hover_index == 3 { "*" } else { " " };
                    let idx = " 4 ";
                    let (cw, ch, cn) = (cfg.custom_w, cfg.custom_h, cfg.custom_n);
                    let name = Difficulty::names()[3];
                    let name_disp_w = name.width();
                    let name_col_w = 14usize;
                    let name_pad = name_col_w.saturating_sub(name_disp_w);
                    let name_field = format!("{}{}", name, " ".repeat(name_pad));
                    let suffix = format!(") {} {:>2}x{:<2}  {} mines", name_field, cw, ch, cn);
                    let focus_style = Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD);
                    if hover_index == 3 {
                        let spans = Spans::from(vec![
                            Span::raw(idx),
                            Span::styled(mark, focus_style),
                            Span::styled(suffix, focus_style),
                        ]);
                        lines.push(spans);
                    } else {
                        let mark_style = if difficulty_selected == 3 { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                        let spans = Spans::from(vec![
                            Span::raw(idx),
                            Span::styled(mark, mark_style),
                            Span::raw(suffix),
                        ]);
                        lines.push(spans);
                    }
                    
                    lines.push(Spans::from(Span::raw("")));
                    let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Left);
                    f.render_widget(p, inner);
                }
                
                // OK/Close button (OK in custom input mode, CLOSE for difficulty selection)
                let btn_w = if ui.custom_input_mode.is_some() { 5u16 } else { 9u16 };
                let mrect = ui.modal_rect.unwrap();
                let bx = mrect.x + (mrect.width.saturating_sub(btn_w)) / 2;
                let by = mrect.y + mrect.height.saturating_sub(2);  // Position button at last row before bottom border
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);

                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                
                let btn_text = if ui.custom_input_mode.is_some() { " OK " } else { " CLOSE " };
                let btn = Paragraph::new(Spans::from(Span::styled(btn_text, btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }
            if ui.showing_options {
                let mrect = centered_block(30,8, size);
                ui.modal_rect = Some(mrect);
                f.render_widget(Clear, mrect);
                f.render_widget(Block::default().borders(Borders::ALL).title(menu_items[4].1), mrect);
                let inner = Rect::new(mrect.x + 1, mrect.y + 1, mrect.width.saturating_sub(2), mrect.height.saturating_sub(2));
                let mut lines = vec![];
                let cb0 = if ui.options_indicator { "[x]" } else { "[ ]" };
                let cb1 = if ui.options_use_q { "[x]" } else { "[ ]" };
                let cb2 = if ui.options_ascii { "[x]" } else { "[ ]" };
                let focus0 = ui.options_focus == Some(0);
                let focus1 = ui.options_focus == Some(1);
                let focus2 = ui.options_focus == Some(2);
                let focus_style = Style::default().bg(menu_key_bg_hover).fg(menu_key_fg_pressed).add_modifier(Modifier::BOLD);
                lines.push(Spans::from(Span::raw("")));
                lines.push(Spans::from(vec![Span::raw(" "), if focus0 { Span::styled(format!("{} Show indicator", cb0), focus_style) } else { Span::raw(format!("{} Show indicator", cb0)) }]));
                lines.push(Spans::from(vec![Span::raw(" "), if focus1 { Span::styled(format!("{} Use ? marks", cb1), focus_style) } else { Span::raw(format!("{} Use ? marks", cb1)) }]));
                lines.push(Spans::from(vec![Span::raw(" "), if focus2 { Span::styled(format!("{} ASCII icons", cb2), focus_style) } else { Span::raw(format!("{} ASCII icons", cb2)) }]));
                let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Left);
                f.render_widget(p, inner);
                // checkbox rects for mouse interaction
                // Only make the clickable area cover the visible label text, not the whole line
                let label0 = format!("{} Show indicator", if ui.options_indicator { "[x]" } else { "[ ]" });
                let label1 = format!("{} Use ? marks", if ui.options_use_q { "[x]" } else { "[ ]" });
                let label2 = format!("{} Ascii icons", if ui.options_ascii { "[x]" } else { "[ ]" });
                let w0 = label0.width() as u16;
                let w1 = label1.width() as u16;
                let w2 = label2.width() as u16;
                ui.options_indicator_rect = Some(Rect::new(inner.x + 1, inner.y + 1, w0, 1));
                ui.options_use_q_rect = Some(Rect::new(inner.x + 1, inner.y + 2, w1, 1));
                ui.options_ascii_rect = Some(Rect::new(inner.x + 1, inner.y + 3, w2, 1));
                // OK button
                let btn_w = 5u16;
                let bx = inner.x + (inner.width.saturating_sub(btn_w)) / 2;
                let by = inner.y + inner.height.saturating_sub(1);
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);
                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let btn = Paragraph::new(Spans::from(Span::styled(" OK ", btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }

            if ui.showing_about {
                let mrect = centered_block(48,9, size);
                ui.modal_rect = Some(mrect);
                f.render_widget(Clear, mrect);
                f.render_widget(Block::default().borders(Borders::ALL).title(menu_items[5].1), mrect);
                let inner = Rect::new(mrect.x + 1, mrect.y + 1, mrect.width.saturating_sub(2), mrect.height.saturating_sub(2));
                let lines = vec![
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw(env!("CARGO_PKG_DESCRIPTION"))),
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw(format!("v{} by {}", env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_AUTHORS")))),
                ];
                let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
                f.render_widget(p, inner);
                // close button
                let btn_w = 9u16;
                let bx = inner.x + (inner.width.saturating_sub(btn_w)) / 2;
                let by = inner.y + inner.height.saturating_sub(1);
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);
                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let btn = Paragraph::new(Spans::from(Span::styled(" CLOSE ", btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }

            if ui.showing_help {
                let mrect = centered_block(50,11, size);
                ui.modal_rect = Some(mrect);
                f.render_widget(Clear, mrect);
                f.render_widget(Block::default().borders(Borders::ALL).title(menu_items[0].1), mrect);
                let inner = Rect::new(mrect.x + 1, mrect.y + 1, mrect.width.saturating_sub(2), mrect.height.saturating_sub(2));
                let help_lines = vec![
                    Spans::from(Span::raw("")),
                    Spans::from(Span::raw(" Controls:")),
                    Spans::from(Span::raw("  Mouse | Arrows    - move cursor")),
                    Spans::from(Span::raw("  L-Click | Space   - reveal")),
                    Spans::from(Span::raw("  R-Click | F       - toggle flag")),
                    Spans::from(Span::raw("  L+R-Click | Enter - chord (open neighbors)")),
                ];
                let p = Paragraph::new(Text::from(help_lines)).alignment(Alignment::Left);
                f.render_widget(p, inner);
                // close button
                let btn_w = 9u16;
                let bx = inner.x + (inner.width.saturating_sub(btn_w)) / 2;
                let by = inner.y + inner.height.saturating_sub(1);
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);
                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let btn = Paragraph::new(Spans::from(Span::styled(" CLOSE ", btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }

            if ui.showing_record {
                let rb = centered_block(40,10, size);
                ui.modal_rect = Some(rb);
                f.render_widget(Clear, rb);
                let mut rec_lines = vec![Spans::from(Span::raw("")), Spans::from(Span::raw(" Best time in seconds:"))];
                let labels = &Difficulty::names()[0..3];
                let label_max = labels.iter().map(|s| s.width()).max().unwrap_or(0);
                let time_w = 5usize; // allow up to 5 digits for time
                let r0 = cfg.get_record_detail(&Difficulty::Beginner);
                let r1 = cfg.get_record_detail(&Difficulty::Intermediate);
                let r2 = cfg.get_record_detail(&Difficulty::Expert);
                let make_line = |label: &str, rec: Option<(u64,String)>| {
                    let prefix = "  ";
                    let colon = ":";
                    // start with prefix + label + colon
                    let mut s = format!("{}{}{}", prefix, label, colon);
                    // pad so time column starts 2 spaces after the longest label (use display width)
                    let extra_label_pad = label_max.saturating_sub(label.width());
                    s.push_str(&" ".repeat(extra_label_pad));
                    s.push_str(&"  "); // two-space gap between longest-name and time
                    // time field
                    match rec {
                            Some((secs, date)) => {
                            let time_str = format!("{}", secs);
                            let time_w_actual = time_str.as_str().width();
                            let time_field = if time_w_actual > time_w {
                                time_str.chars().take(time_w).collect::<String>()
                            } else {
                                let pad = time_w.saturating_sub(time_w_actual);
                                format!("{}{}", " ".repeat(pad), time_str)
                            };
                            s.push_str(&time_field);
                            s.push_str("  "); // two-space gap between time and date
                            s.push_str(&date);
                            Spans::from(Span::raw(s))
                        }
                        None => {
                            let time_field = format!("{:>width$}", "-", width=time_w);
                            s.push_str(&time_field);
                            Spans::from(Span::raw(s))
                        }
                    }
                };
                rec_lines.push(make_line(labels[0], r0));
                rec_lines.push(make_line(labels[1], r1));
                rec_lines.push(make_line(labels[2], r2));
                let p = Paragraph::new(Text::from(rec_lines)).block(Block::default().borders(Borders::ALL).title(menu_items[2].1)).alignment(Alignment::Left);
                f.render_widget(p, rb);
                // close button
                let btn_w = 9u16;
                let bx = rb.x + (rb.width.saturating_sub(btn_w)) / 2;
                let by = rb.y + rb.height.saturating_sub(2);
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);
                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let btn = Paragraph::new(Spans::from(Span::styled(" CLOSE ", btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }

            if ui.showing_win {
                let wb = bottom_centered_block(40,8, size);
                ui.modal_rect = Some(wb);
                f.render_widget(Clear, wb);
                f.render_widget(Block::default().borders(Borders::ALL).title("Success"), wb);
                let inner = Rect::new(wb.x + 1, wb.y + 1, wb.width.saturating_sub(2), wb.height.saturating_sub(2));
                let t = if game.started { game.start_time.unwrap().elapsed().as_secs() } else { game.elapsed.as_secs() };
                // Use the last_run_new_record flag because the config may already
                // contain the saved value (making t == cfg value). We set this
                // flag when we write the new record above.
                // Don't show "New Record!" for Custom difficulty since it's not stored
                let is_custom = matches!(cfg.difficulty, Difficulty::Custom(_, _, _));
                let is_new = ui.last_run_new_record && !is_custom;
                let time_line = if is_new { format!("Time: {} seconds (New Record!)", t) } else { format!("Time: {} seconds", t) };
                let lines = vec![Spans::from(Span::raw("")), Spans::from(Span::raw("Mines Cleared — You Win!")), Spans::from(Span::raw(time_line)) ];
                let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
                f.render_widget(p, inner);
                // close button
                let btn_w = 9u16;
                let bx = inner.x + (inner.width.saturating_sub(btn_w)) / 2;
                let by = inner.y + inner.height.saturating_sub(1);
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);
                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let btn = Paragraph::new(Spans::from(Span::styled(" CLOSE ", btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }

            if ui.showing_loss {
                let lb = bottom_centered_block(44,8, size);
                ui.modal_rect = Some(lb);
                f.render_widget(Clear, lb);
                f.render_widget(Block::default().borders(Borders::ALL).title("Failure"), lb);
                let inner = Rect::new(lb.x + 1, lb.y + 1, lb.width.saturating_sub(2), lb.height.saturating_sub(2));
                let lines = vec![Spans::from(Span::raw("")), Spans::from(Span::raw("Mine Exploded — You Lose!")), Spans::from(Span::raw("Better luck next time."))];
                let p = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
                f.render_widget(p, inner);
                // close button
                let btn_w = 9u16;
                let bx = inner.x + (inner.width.saturating_sub(btn_w)) / 2;
                let by = inner.y + inner.height.saturating_sub(1);
                let btn_rect = Rect::new(bx, by, btn_w, 1);
                ui.modal_close_rect = Some(btn_rect);
                let mut btn_style = Style::default().bg(Color::Gray).fg(Color::Black).add_modifier(Modifier::BOLD);
                if ui.modal_close_pressed { btn_style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if ui.modal_close_hovered { btn_style = Style::default().bg(Color::White).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let btn = Paragraph::new(Spans::from(Span::styled(" CLOSE ", btn_style))).alignment(Alignment::Center).block(Block::default());
                f.render_widget(btn, btn_rect);
            }
        })?;

        // bind cursor indicator to current logical cursor each frame so it's always synced
        ui.cursor_indicator = Some(game.cursor);

        // If no modal was rendered this frame, ensure close button state is cleared
        if ui.modal_rect.is_none() {
            ui.modal_close_hovered = false;
            ui.modal_close_pressed = false;
        }

        let timeout = tick_rate.checked_sub(last_tick.elapsed()).unwrap_or_else(|| Duration::from_secs(0));
        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(KeyEvent{code, modifiers, kind, ..}) => {
                    match kind {
                        KeyEventKind::Press => {
                            if ui.showing_difficulty {
                                // Handle custom difficulty input mode
                                if ui.custom_input_mode.is_some() {
                                    match code {
                                        KeyCode::Char(c) if c.is_ascii_digit() => {
                                            match ui.custom_input_mode.unwrap() {
                                                0 => { // Width input
                                                    if ui.custom_w_str.len() < 2 {
                                                        ui.custom_w_str.push(c);
                                                    }
                                                    ui.custom_error_msg = None;
                                                }
                                                1 => { // Height input
                                                    if ui.custom_h_str.len() < 2 {
                                                        ui.custom_h_str.push(c);
                                                    }
                                                    ui.custom_error_msg = None;
                                                }
                                                2 => { // Mines input
                                                    if ui.custom_n_str.len() < 3 {
                                                        ui.custom_n_str.push(c);
                                                    }
                                                    ui.custom_error_msg = None;
                                                }
                                                _ => {}
                                            }
                                        }
                                        KeyCode::Backspace => {
                                            match ui.custom_input_mode.unwrap() {
                                                0 => { ui.custom_w_str.pop(); }
                                                1 => { ui.custom_h_str.pop(); }
                                                2 => { ui.custom_n_str.pop(); }
                                                _ => {}
                                            }
                                            ui.custom_error_msg = None;
                                        }
                                        KeyCode::Tab | KeyCode::Down => {
                                            // Move to next field
                                            if ui.custom_input_mode.unwrap() < 2 {
                                                ui.custom_input_mode = Some(ui.custom_input_mode.unwrap() + 1);
                                            } else {
                                                ui.custom_input_mode = Some(0);
                                            }
                                            ui.custom_error_msg = None;
                                        }
                                        KeyCode::BackTab | KeyCode::Up => {
                                            // Move to previous field
                                            if ui.custom_input_mode.unwrap() > 0 {
                                                ui.custom_input_mode = Some(ui.custom_input_mode.unwrap() - 1);
                                            } else {
                                                ui.custom_input_mode = Some(2);
                                            }
                                            ui.custom_error_msg = None;
                                        }
                                        KeyCode::Enter => {
                                            // Validate and apply custom difficulty
                                            let w_str = ui.custom_w_str.trim();
                                            let h_str = ui.custom_h_str.trim();
                                            let n_str = ui.custom_n_str.trim();
                                            
                                            if w_str.is_empty() || h_str.is_empty() || n_str.is_empty() {
                                                // Flash the first empty field
                                                if w_str.is_empty() {
                                                    ui.custom_invalid_field = Some((0, Instant::now()));
                                                } else if h_str.is_empty() {
                                                    ui.custom_invalid_field = Some((1, Instant::now()));
                                                } else {
                                                    ui.custom_invalid_field = Some((2, Instant::now()));
                                                }
                                            } else {
                                                let w = w_str.parse::<usize>().unwrap_or(0);
                                                let h = h_str.parse::<usize>().unwrap_or(0);
                                                let n = n_str.parse::<usize>().unwrap_or(0);
                                                
                                                let max_mines = ((w * h) as f64 * 0.926) as usize;
                                                
                                                if w < 9 || w > 36 {
                                                    ui.custom_invalid_field = Some((0, Instant::now()));
                                                } else if h < 9 || h > 24 {
                                                    ui.custom_invalid_field = Some((1, Instant::now()));
                                                } else if n < 10 || n > max_mines {
                                                    ui.custom_invalid_field = Some((2, Instant::now()));
                                                } else {
                                                    // Valid input, apply
                                                    cfg.custom_w = w;
                                                    cfg.custom_h = h;
                                                    cfg.custom_n = n;
                                                    cfg.difficulty = Difficulty::Custom(w, h, n);
                                                    save_config(&cfg);
                                                    game = Game::new(w, h, n);
                                                    reset_ui_after_new_game(&mut game, &mut ui);
                                                    ui.showing_difficulty = false;
                                                    ui.custom_input_mode = None;
                                                    ui.custom_w_str.clear();
                                                    ui.custom_h_str.clear();
                                                    ui.custom_n_str.clear();
                                                    ui.custom_error_msg = None;
                                                    ui.modal_rect = None;
                                                    ui.modal_close_rect = None;
                                                    ui.modal_close_pressed = false;
                                                }
                                            }
                                        }
                                        KeyCode::Esc => {
                                            ui.custom_input_mode = None;
                                            ui.custom_w_str.clear();
                                            ui.custom_h_str.clear();
                                            ui.custom_n_str.clear();
                                            ui.custom_error_msg = None;
                                            difficulty_selected = cfg.difficulty.to_index();
                                        }
                                        _ => {}
                                    }
                                } else {
                                    // Normal difficulty selection mode
                                    match code {
                                        KeyCode::Char('1') => {
                                            difficulty_selected = 0;
                                            cfg.difficulty = Difficulty::Beginner;
                                            save_config(&cfg);
                                            let (w,h,m) = cfg.difficulty.params();
                                            game = Game::new(w,h,m);
                                            reset_ui_after_new_game(&mut game, &mut ui);
                                            ui.showing_difficulty = false;
                                            ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                        }
                                        KeyCode::Char('2') => {
                                            difficulty_selected = 1;
                                            cfg.difficulty = Difficulty::Intermediate;
                                            save_config(&cfg);
                                            let (w,h,m) = cfg.difficulty.params();
                                            game = Game::new(w,h,m);
                                            reset_ui_after_new_game(&mut game, &mut ui);
                                            ui.showing_difficulty = false;
                                            ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                        }
                                        KeyCode::Char('3') => {
                                            difficulty_selected = 2;
                                            cfg.difficulty = Difficulty::Expert;
                                            save_config(&cfg);
                                            let (w,h,m) = cfg.difficulty.params();
                                            game = Game::new(w,h,m);
                                            reset_ui_after_new_game(&mut game, &mut ui);
                                                        ui.showing_difficulty = false;
                                            ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                        }
                                        KeyCode::Char('4') => {
                                            difficulty_selected = 3;
                                            ui.difficulty_hover = Some(3);
                                            ui.custom_input_mode = Some(0);
                                            ui.custom_w_str = format!("{}", cfg.custom_w);
                                            ui.custom_h_str = format!("{}", cfg.custom_h);
                                            ui.custom_n_str = format!("{}", cfg.custom_n);
                                            ui.custom_error_msg = None;
                                        }
                                        KeyCode::Up => {
                                            let base = ui.difficulty_hover.unwrap_or(difficulty_selected);
                                            let new_idx = if base == 0 { 3 } else { base - 1 };
                                            difficulty_selected = new_idx;
                                            ui.difficulty_hover = Some(new_idx);
                                        }
                                        KeyCode::Down => {
                                            let base = ui.difficulty_hover.unwrap_or(difficulty_selected);
                                            let new_idx = (base + 1) % 4;
                                            difficulty_selected = new_idx;
                                            ui.difficulty_hover = Some(new_idx);
                                        }
                                        KeyCode::Enter | KeyCode::Char(' ') => {
                                            if difficulty_selected == 3 {
                                                // Enter custom input mode
                                                ui.custom_input_mode = Some(0);
                                                ui.custom_w_str = format!("{}", cfg.custom_w);
                                                ui.custom_h_str = format!("{}", cfg.custom_h);
                                                ui.custom_n_str = format!("{}", cfg.custom_n);
                                                ui.custom_error_msg = None;
                                            } else {
                                                cfg.difficulty = Difficulty::from_index(difficulty_selected, cfg.custom_w, cfg.custom_h, cfg.custom_n);
                                                save_config(&cfg);
                                                let (w,h,m) = cfg.difficulty.params();
                                                game = Game::new(w,h,m);
                                                reset_ui_after_new_game(&mut game, &mut ui);
                                                    ui.showing_difficulty = false;
                                                ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                            }
                                        }
                                        KeyCode::Esc => { ui.showing_difficulty = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false }
                                        _ => {}
                                    }
                                }
                            } else if ui.showing_about {
                                match code { KeyCode::Esc => { ui.showing_about = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } _ => { ui.showing_about = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } }
                            } else if ui.showing_options {
                                match code {
                                    KeyCode::Esc => { ui.showing_options = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None; ui.options_focus = None },
                                    KeyCode::Enter => {
                                        cfg.show_indicator = ui.options_indicator;
                                        cfg.use_question_marks = ui.options_use_q;
                                        cfg.ascii_icons = ui.options_ascii;
                                        // update glyphs when ascii_icons changes
                                        let g = make_glyphs(cfg.ascii_icons);
                                        glyph_unopened = g.0;
                                        glyph_mine = g.1;
                                        glyph_flag = g.2;
                                        glyph_question = g.3;
                                        save_config(&cfg);
                                        ui.showing_options = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None; ui.options_focus = None
                                    }
                                    KeyCode::Up => {
                                        let f = ui.options_focus.unwrap_or(0);
                                        ui.options_focus = Some(if f == 0 { 2 } else { f - 1 });
                                    }
                                    KeyCode::Down => {
                                        let f = ui.options_focus.unwrap_or(0);
                                        ui.options_focus = Some((f + 1) % 3);
                                    }
                                    KeyCode::Char(' ') => {
                                        match ui.options_focus.unwrap_or(0) {
                                            0 => ui.options_indicator = !ui.options_indicator,
                                            1 => ui.options_use_q = !ui.options_use_q,
                                            2 => ui.options_ascii = !ui.options_ascii,
                                            _ => {}
                                        }
                                    }
                                    _ => {}
                                }
                            } else if ui.showing_help {
                                match code { KeyCode::Esc => { ui.showing_help = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } _ => { ui.showing_help = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } }
                            } else if ui.showing_record {
                                match code { KeyCode::Esc => { ui.showing_record = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } _ => { ui.showing_record = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } }
                            } else if ui.showing_win {
                                match code {
                                    KeyCode::Esc => {
                                        ui.showing_win = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None;
                                        let (ww,hh,mm) = cfg.difficulty.params();
                                        game = Game::new(ww, hh, mm);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                    }
                                    _ => {
                                        ui.showing_win = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None;
                                        let (ww,hh,mm) = cfg.difficulty.params();
                                        game = Game::new(ww, hh, mm);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                    }
                                }
                            } else if ui.showing_loss {
                                match code {
                                    KeyCode::Esc => {
                                        ui.showing_loss = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None;
                                        let (ww,hh,mm) = cfg.difficulty.params();
                                        game = Game::new(ww, hh, mm);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                    }
                                    _ => {
                                        ui.showing_loss = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None;
                                        let (ww,hh,mm) = cfg.difficulty.params();
                                        game = Game::new(ww, hh, mm);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                    }
                                }
                            } else {
                                // normal gameplay key-press handling
                                match code {
                                    KeyCode::Esc => { break }
                                    KeyCode::F(1) => { ui.showing_help = true }
                                    KeyCode::F(2) => { let (w,h,m) = cfg.difficulty.params(); game = Game::new(w,h,m); reset_ui_after_new_game(&mut game, &mut ui); }
                                    KeyCode::F(4) => { ui.showing_record = true }
                                        KeyCode::F(5) => { if !ui.showing_difficulty { difficulty_selected = cfg.difficulty.to_index(); } ui.showing_difficulty = !ui.showing_difficulty }
                                                                KeyCode::F(7) => { ui.options_use_q = cfg.use_question_marks; ui.options_ascii = cfg.ascii_icons; ui.options_indicator = cfg.show_indicator; ui.options_focus = Some(0); ui.showing_options = true }
                                    KeyCode::F(9) => { ui.showing_about = true }
                                    KeyCode::Char('o') if modifiers.contains(KeyModifiers::CONTROL) => { if !ui.showing_difficulty { difficulty_selected = cfg.difficulty.to_index(); } ui.showing_difficulty = !ui.showing_difficulty }
                                    KeyCode::Left => { game.step_cursor(-1,0); ui.cursor_indicator = Some(game.cursor); }
                                    KeyCode::Right => { game.step_cursor(1,0); ui.cursor_indicator = Some(game.cursor); }
                                    KeyCode::Up => { game.step_cursor(0,-1); ui.cursor_indicator = Some(game.cursor); }
                                    KeyCode::Down => { game.step_cursor(0,1); ui.cursor_indicator = Some(game.cursor); }
                                    KeyCode::Char(' ') => {
                                        // Space press: emulate left-button down at current cursor
                                        ui.left_press = Some(game.cursor);
                                        if !ui.supports_key_release { ui.key_timer = Some((Instant::now(), 0)); }
                                    }
                                    KeyCode::Enter => {
                                        // Enter press: emulate simultaneous left+right down (activate chord highlight)
                                        let c = game.cursor;
                                        ui.left_press = Some(c);
                                        ui._right_press = Some(c);
                                        ui.chord_active = Some(c);
                                        if !ui.supports_key_release { ui.key_timer = Some((Instant::now(), 1)); }
                                    }
                                    KeyCode::Char('f') | KeyCode::Char('F') => {
                                        let (cx,cy) = game.cursor;
                                        let idx = game.index(cx,cy);
                                        if !game.revealed[idx] {
                                            if cfg.use_question_marks {
                                                game.toggle_flag(cx,cy);
                                            } else {
                                                // toggle between 0 and 1 only
                                                if game.flagged[idx] == 1 { game.flagged[idx] = 0 } else { game.flagged[idx] = 1 }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        KeyEventKind::Release => {
                            // handle key releases for reveal / chord
                            if ui.showing_difficulty || ui.showing_about || ui.showing_options || ui.showing_help || ui.showing_record || ui.showing_win || ui.showing_loss {
                                // ignore releases in modals (they are handled on press)
                            } else {
                                match code {
                                    KeyCode::Char(' ') => {
                                        // Space release: if press started at same cursor, reveal
                                                if let Some((px,py)) = ui.left_press {
                                            let (cx,cy) = game.cursor;
                                            if px==cx && py==cy {
                                                let idx = game.index(cx,cy);
                                                if !game.revealed[idx] {
                                                    game.reveal(cx,cy);
                                                    if let Some(false) = game.game_over { game.reveal_all_mines(); ui.showing_loss = true; }
                                                    else if let Some(true) = game.game_over { ui.showing_win = true; }
                                                }
                                            }
                                        }
                                        ui.left_press = None;
                                        ui.key_timer = None;
                                        ui.supports_key_release = true;
                                    }
                                    KeyCode::Enter => {
                                        // Enter release: perform chord reveal if chord_active
                                            if let Some((ccx,ccy)) = ui.chord_active {
                                            let idx = game.index(ccx, ccy);
                                            if game.revealed[idx] {
                                                let adj = game.board[idx].adj as usize;
                                                let mut flagged = 0usize;
                                                let mut neighbors = vec![];
                                                for oy in ccy.saturating_sub(1)..=(ccy+1).min(game.h-1) {
                                                    for ox in ccx.saturating_sub(1)..=(ccx+1).min(game.w-1) {
                                                        if ox==ccx && oy==ccy { continue }
                                                        neighbors.push((ox,oy));
                                                    }
                                                }
                                                for (ox,oy) in &neighbors { if game.flagged[game.index(*ox,*oy)] == 1 { flagged += 1 } }
                                                if flagged != adj { ui.flash_cell = Some(((ccx,ccy), Instant::now())); }
                                                else {
                                                    let mut wrong_flag = false;
                                                    for (ox,oy) in &neighbors { let nidx = game.index(*ox,*oy); if game.flagged[nidx] == 1 && !game.board[nidx].mine { wrong_flag = true; break; } }
                                                    if wrong_flag {
                                                        game.reveal_all_mines();
                                                        if let Some(t0) = game.start_time { game.elapsed = t0.elapsed(); }
                                                        game.started = false;
                                                        game.game_over = Some(false);
                                                        ui.showing_loss = true;
                                                    }
                                                    else { for (ox,oy) in &neighbors { let nidx = game.index(*ox,*oy); if !game.revealed[nidx] && game.flagged[nidx] != 1 { game.reveal(*ox,*oy); } } if let Some(true) = game.game_over { ui.showing_win = true } }
                                                }
                                            }
                                            ui.chord_active = None; ui.left_press = None; ui._right_press = None;
                                        }
                                        ui.key_timer = None;
                                        ui.supports_key_release = true;
                                        }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::Mouse(me) => {
                    // if a modal is open, only respond to mouse events inside modal; otherwise handle menu
                    if let Some(mrect) = ui.modal_rect {
                        // check inside modal; if outside and click -> close modal; if inside and difficulty items -> handle item hover/click
                        match me.kind {
                            MouseEventKind::Moved => {
                                let inside = me.column >= mrect.x && me.column <= mrect.x + mrect.width.saturating_sub(1) && me.row >= mrect.y && me.row <= mrect.y + mrect.height.saturating_sub(1);
                                if !inside {
                                    // ignore hover outside modal
                                    ui.modal_close_hovered = false;
                                } else {
                                    // if over close button, set hovered
                                    if let Some(btn) = ui.modal_close_rect {
                                        let in_btn = me.column >= btn.x && me.column <= btn.x + btn.width.saturating_sub(1) && me.row >= btn.y && me.row <= btn.y + btn.height.saturating_sub(1);
                                        ui.modal_close_hovered = in_btn;
                                    } else {
                                        ui.modal_close_hovered = false;
                                    }
                                    // Always handle options hover when the options modal is shown
                                    if ui.showing_options {
                                        // Prefer per-rect detection (text width)
                                        if let Some(rect) = ui.options_indicator_rect {
                                            if me.column >= rect.x && me.column <= rect.x + rect.width.saturating_sub(1) && me.row >= rect.y && me.row <= rect.y + rect.height.saturating_sub(1) {
                                                ui.options_focus = Some(0);
                                            }
                                        }
                                        if let Some(rect) = ui.options_use_q_rect {
                                            if me.column >= rect.x && me.column <= rect.x + rect.width.saturating_sub(1) && me.row >= rect.y && me.row <= rect.y + rect.height.saturating_sub(1) {
                                                ui.options_focus = Some(1);
                                            }
                                        }
                                        if let Some(rect) = ui.options_ascii_rect {
                                            if me.column >= rect.x && me.column <= rect.x + rect.width.saturating_sub(1) && me.row >= rect.y && me.row <= rect.y + rect.height.saturating_sub(1) {
                                                ui.options_focus = Some(2);
                                            }
                                        }
                                        // Also allow hovering the whole line inside the modal to set focus
                                        if let Some(m) = ui.modal_rect {
                                            let inner = Rect::new(m.x + 1, m.y + 1, m.width.saturating_sub(2), m.height.saturating_sub(2));
                                            if me.column >= inner.x && me.column <= inner.x + inner.width.saturating_sub(1) && me.row >= inner.y && me.row <= inner.y + inner.height.saturating_sub(1) {
                                                let local_row = me.row as i32 - inner.y as i32; // 0-based
                                                match local_row {
                                                    1 => ui.options_focus = Some(0),
                                                    2 => ui.options_focus = Some(1),
                                                    3 => ui.options_focus = Some(2),
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    // if difficulty modal, update hovered option based on mouse row
                                    if ui.showing_difficulty && ui.custom_input_mode.is_none() {
                                        let local_row = me.row as i32 - (mrect.y as i32) - 1; // 0-based within content
                                        // content layout: 0:blank,1..4:difficulty items,5:blank
                                        if local_row >= 1 && local_row <= 4 {
                                            ui.difficulty_hover = Some((local_row - 1) as usize);
                                        } else {
                                            ui.difficulty_hover = None;
                                        }
                                    }
                                }
                            }
                            MouseEventKind::Down(MouseButton::Left) => {
                                let inside = me.column >= mrect.x && me.column <= mrect.x + mrect.width.saturating_sub(1) && me.row >= mrect.y && me.row <= mrect.y + mrect.height.saturating_sub(1);
                                if !inside {
                                    // ignore clicks outside modal; do not close
                                } else {
                                    // if click hits the CLOSE button rect, mark pressed
                                    if let Some(btn) = ui.modal_close_rect {
                                        let in_btn = me.column >= btn.x && me.column <= btn.x + btn.width.saturating_sub(1) && me.row >= btn.y && me.row <= btn.y + btn.height.saturating_sub(1);
                                        if in_btn {
                                            ui.modal_close_pressed = true;
                                            continue;
                                        }
                                    }
                                    // click inside modal: handle custom input mode or difficulty selection
                                    // Options modal click handling
                                    if ui.showing_options {
                                        if let Some(rect) = ui.options_indicator_rect {
                                            if me.column >= rect.x && me.column <= rect.x + rect.width.saturating_sub(1) && me.row >= rect.y && me.row <= rect.y + rect.height.saturating_sub(1) {
                                                ui.options_indicator = !ui.options_indicator;
                                                ui.options_focus = Some(0);
                                                continue;
                                            }
                                        }
                                        if let Some(rect) = ui.options_use_q_rect {
                                            if me.column >= rect.x && me.column <= rect.x + rect.width.saturating_sub(1) && me.row >= rect.y && me.row <= rect.y + rect.height.saturating_sub(1) {
                                                ui.options_use_q = !ui.options_use_q;
                                                ui.options_focus = Some(1);
                                                continue;
                                            }
                                        }
                                        if let Some(rect) = ui.options_ascii_rect {
                                            if me.column >= rect.x && me.column <= rect.x + rect.width.saturating_sub(1) && me.row >= rect.y && me.row <= rect.y + rect.height.saturating_sub(1) {
                                                ui.options_ascii = !ui.options_ascii;
                                                ui.options_focus = Some(2);
                                                continue;
                                            }
                                        }
                                    }

                                    if ui.showing_difficulty {
                                        // Handle custom input mode mouse clicks
                                        if ui.custom_input_mode.is_some() {
                                            // Check which input field was clicked
                                            if let Some(w_rect) = ui.custom_w_rect {
                                                if me.column >= w_rect.x && me.column <= w_rect.x + w_rect.width.saturating_sub(1) && me.row >= w_rect.y && me.row <= w_rect.y + w_rect.height.saturating_sub(1) {
                                                    ui.custom_input_mode = Some(0);
                                                    continue;
                                                }
                                            }
                                            if let Some(h_rect) = ui.custom_h_rect {
                                                if me.column >= h_rect.x && me.column <= h_rect.x + h_rect.width.saturating_sub(1) && me.row >= h_rect.y && me.row <= h_rect.y + h_rect.height.saturating_sub(1) {
                                                    ui.custom_input_mode = Some(1);
                                                    continue;
                                                }
                                            }
                                            if let Some(n_rect) = ui.custom_n_rect {
                                                if me.column >= n_rect.x && me.column <= n_rect.x + n_rect.width.saturating_sub(1) && me.row >= n_rect.y && me.row <= n_rect.y + n_rect.height.saturating_sub(1) {
                                                    ui.custom_input_mode = Some(2);
                                                    continue;
                                                }
                                            }
                                        } else {
                                            // Normal difficulty selection mode
                                            let local_row = me.row as i32 - (mrect.y as i32) - 1;
                                            if local_row >= 1 && local_row <= 4 {
                                                let idx = (local_row - 1) as usize;
                                                if idx <= 3 {
                                                    difficulty_selected = idx;
                                                    if idx == 3 {
                                                        // Enter custom input mode
                                                        ui.custom_input_mode = Some(0);
                                                        ui.custom_w_str = format!("{}", cfg.custom_w);
                                                        ui.custom_h_str = format!("{}", cfg.custom_h);
                                                        ui.custom_n_str = format!("{}", cfg.custom_n);
                                                        ui.custom_error_msg = None;
                                                    } else {
                                                        // apply selection immediately
                                                        cfg.difficulty = Difficulty::from_index(difficulty_selected, cfg.custom_w, cfg.custom_h, cfg.custom_n);
                                                        save_config(&cfg);
                                                        let (w,h,m) = cfg.difficulty.params();
                                                        game = Game::new(w,h,m);
                                                        reset_ui_after_new_game(&mut game, &mut ui);
                                                        ui.showing_difficulty = false;
                                                        // clear modal geometry so subsequent mouse events are handled by main UI
                                                        ui.modal_rect = None;
                                                        ui.modal_close_rect = None;
                                                        ui.modal_close_pressed = false;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            MouseEventKind::Up(_) => {
                                // if we had pressed the close/OK button, check release inside button
                                if ui.modal_close_pressed {
                                    if let Some(btn) = ui.modal_close_rect {
                                        let in_btn = me.column >= btn.x && me.column <= btn.x + btn.width.saturating_sub(1) && me.row >= btn.y && me.row <= btn.y + btn.height.saturating_sub(1);
                                            if in_btn {
                                            // Handle OK button in custom input mode (same as pressing Enter)
                                            if ui.custom_input_mode.is_some() {
                                                let w_str = ui.custom_w_str.trim();
                                                let h_str = ui.custom_h_str.trim();
                                                let n_str = ui.custom_n_str.trim();
                                                
                                                if w_str.is_empty() || h_str.is_empty() || n_str.is_empty() {
                                                    // Flash the first empty field
                                                    if w_str.is_empty() {
                                                        ui.custom_invalid_field = Some((0, Instant::now()));
                                                    } else if h_str.is_empty() {
                                                        ui.custom_invalid_field = Some((1, Instant::now()));
                                                    } else {
                                                        ui.custom_invalid_field = Some((2, Instant::now()));
                                                    }
                                                } else {
                                                    let w = w_str.parse::<usize>().unwrap_or(0);
                                                    let h = h_str.parse::<usize>().unwrap_or(0);
                                                    let n = n_str.parse::<usize>().unwrap_or(0);
                                                    
                                                    let max_mines = ((w * h) as f64 * 0.926) as usize;
                                                    
                                                    if w < 9 || w > 36 {
                                                        ui.custom_invalid_field = Some((0, Instant::now()));
                                                    } else if h < 9 || h > 24 {
                                                        ui.custom_invalid_field = Some((1, Instant::now()));
                                                    } else if n < 10 || n > max_mines {
                                                        ui.custom_invalid_field = Some((2, Instant::now()));
                                                    } else {
                                                        // Valid input, apply
                                                        cfg.custom_w = w;
                                                        cfg.custom_h = h;
                                                        cfg.custom_n = n;
                                                        cfg.difficulty = Difficulty::Custom(w, h, n);
                                                        save_config(&cfg);
                                                        game = Game::new(w, h, n);
                                                        reset_ui_after_new_game(&mut game, &mut ui);
                                                        ui.showing_difficulty = false;
                                                        ui.custom_input_mode = None;
                                                        ui.custom_w_str.clear();
                                                        ui.custom_h_str.clear();
                                                        ui.custom_n_str.clear();
                                                        ui.custom_error_msg = None;
                                                        ui.modal_rect = None;
                                                        ui.modal_close_rect = None;
                                                        ui.modal_close_pressed = false;
                                                    }
                                                }
                                            } else {
                                                // CLOSE/OK button in difficulty/other modals
                                                if ui.showing_options {
                                                    // apply option changes
                                                    cfg.show_indicator = ui.options_indicator;
                                                    cfg.use_question_marks = ui.options_use_q;
                                                        cfg.ascii_icons = ui.options_ascii;
                                                        // update glyphs when ascii_icons changes
                                                        let g = make_glyphs(cfg.ascii_icons);
                                                        glyph_unopened = g.0;
                                                        glyph_mine = g.1;
                                                        glyph_flag = g.2;
                                                        glyph_question = g.3;
                                                    save_config(&cfg);
                                                    ui.showing_options = false;
                                                    ui.modal_rect = None;
                                                    ui.modal_close_rect = None;
                                                    ui.hover_index = None;
                                                } else {
                                                    // CLOSE button in difficulty/other modals
                                                    let was_win = ui.showing_win;
                                                    let was_loss = ui.showing_loss;
                                                    ui.showing_difficulty = false;
                                                    ui.showing_about = false;
                                                    ui.showing_help = false;
                                                    ui.showing_record = false;
                                                    ui.showing_win = false;
                                                    ui.showing_loss = false;
                                                    // clear modal geometry immediately so following mouse events are not treated as inside modal
                                                    ui.modal_rect = None;
                                                    ui.modal_close_rect = None;
                                                    ui.hover_index = None;
                                                    if was_win || was_loss {
                                                        let (ww,hh,mm) = cfg.difficulty.params();
                                                        game = Game::new(ww, hh, mm);
                                                        reset_ui_after_new_game(&mut game, &mut ui);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    ui.modal_close_pressed = false;
                                }
                            }
                            MouseEventKind::Down(MouseButton::Right) => {
                                // Right-click in custom input mode: cancel and return to difficulty selection
                                if ui.custom_input_mode.is_some() {
                                    ui.custom_input_mode = None;
                                    ui.custom_w_str.clear();
                                    ui.custom_h_str.clear();
                                    ui.custom_n_str.clear();
                                    ui.custom_error_msg = None;
                                    difficulty_selected = cfg.difficulty.to_index();
                                } else {
                                    // Right-click anywhere in a modal should close it (like Esc)
                                    let was_win = ui.showing_win;
                                    let was_loss = ui.showing_loss;
                                    ui.showing_difficulty = false;
                                    ui.showing_about = false;
                                    ui.showing_options = false;
                                    ui.showing_help = false;
                                    ui.showing_record = false;
                                    ui.showing_win = false;
                                    ui.showing_loss = false;
                                    ui.modal_rect = None;
                                    ui.modal_close_rect = None;
                                    ui.modal_close_pressed = false;
                                    ui.hover_index = None;
                                    if was_win || was_loss {
                                        let (ww,hh,mm) = cfg.difficulty.params();
                                        game = Game::new(ww, hh, mm);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else {
                        // no modal: decide whether the mouse targets the menu or the board
                        let menu_handled = if let Some(rect) = menu_rect {
                            // detect whether mouse is over the menu row
                            let start_x = rect.x + 2; // account for one-space left padding inside menu
                            let y = rect.y + 1;
                            if me.row == y {
                                // handle menu hover/click
                                match me.kind {
                                    MouseEventKind::Moved => {
                                        let mut offset = start_x;
                                        let mut found: Option<usize> = None;
                                        for (i, (k, r)) in menu_items.iter().take(6).enumerate() {
                                            if i > 0 { offset += 3; }
                                            // account for the ": " we add when rendering (use display width)
                                            let full_len = (k.width() + 2 + r.width()) as u16;
                                            let end = offset + full_len - 1;
                                            if me.column >= offset && me.column <= end {
                                                found = Some(i);
                                                break;
                                            }
                                            offset = end + 1;
                                        }
                                        ui.hover_index = found;
                                        // when over menu, clear board indicator
                                        ui.cursor_indicator = None;
                                        true
                                    }
                                    MouseEventKind::Down(MouseButton::Left) => {
                                        let mut consumed = false;
                                        let mut offset = start_x;
                                        for (i, (k, r)) in menu_items.iter().take(6).enumerate() {
                                            if i > 0 { offset += 3; }
                                            // account for the ": " we add when rendering (use display width)
                                            let full_len = (k.width() + 2 + r.width()) as u16;
                                            let end = offset + full_len - 1;
                                            if me.column >= offset && me.column <= end {
                                                ui.clicked_index = Some(i);
                                                ui.click_instant = Some(Instant::now());
                                                match i {
                                                    0 => ui.showing_help = true,
                                                    1 => { let (w,h,m) = cfg.difficulty.params(); game = Game::new(w,h,m); reset_ui_after_new_game(&mut game, &mut ui); },
                                                    2 => ui.showing_record = true,
                                                    3 => { if !ui.showing_difficulty { difficulty_selected = cfg.difficulty.to_index(); } ui.showing_difficulty = true },
                                                    4 => { ui.options_use_q = cfg.use_question_marks; ui.options_ascii = cfg.ascii_icons; ui.options_indicator = cfg.show_indicator; ui.options_focus = Some(0); ui.showing_options = true },
                                                    5 => ui.showing_about = true,
                                                    _ => {}
                                                }
                                                consumed = true;
                                                break;
                                            }
                                            offset = end + 1;
                                        }
                                        consumed
                                    }
                                    MouseEventKind::Up(_) => {
                                        // Consume all Up events on menu row
                                        true
                                    }
                                    _ => false,
                                }
                            } else {
                                // mouse not on menu row -> clear hover
                                if let MouseEventKind::Moved = me.kind { ui.hover_index = None; }
                                false
                            }
                        } else { false };

                        if !menu_handled {
                            // handle status bar Esc: Exit mouse interactions (right-aligned label)
                            if let Some(srect) = status_rect {
                                let status_row = srect.y + 1;
                                if me.row == status_row {
                                    // compute positions matching rendering logic
                                    let left_text = format!(" Mines: {}   Time: {}s ", game.remaining_mines(), if game.started { game.start_time.unwrap().elapsed().as_secs() } else { game.elapsed.as_secs() });
                                    let right_label = "Esc: Exit";
                                    let inner_w = srect.width.saturating_sub(2) as usize;
                                    let left_w = left_text.as_str().width();
                                    let right_w = right_label.width();
                                    let mid_spaces = if inner_w > left_w + right_w + 1 { inner_w - left_w - right_w - 1 } else { 1 };
                                    let start_x = srect.x + 1 + left_w as u16 + mid_spaces as u16;
                                    let end_x = start_x + (right_w as u16).saturating_sub(1);
                                    match me.kind {
                                        MouseEventKind::Moved => {
                                            ui.exit_status_hovered = me.column >= start_x && me.column <= end_x;
                                            // do not consume movement here; allow board hover when not over status
                                        }
                                        MouseEventKind::Down(MouseButton::Left) => {
                                            if me.column >= start_x && me.column <= end_x {
                                                ui.exit_menu_item_down = true;
                                            }
                                        }
                                        MouseEventKind::Up(MouseButton::Left) => {
                                            if ui.exit_menu_item_down {
                                                ui.exit_menu_item_down = false;
                                                if me.column >= start_x && me.column <= end_x {
                                                    exit_requested = true;
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                } else {
                                    ui.exit_status_hovered = false;
                                }
                            }
                            if let Some(brect) = board_rect {
                                match me.kind {
                                    MouseEventKind::Moved => {
                                        let inner = Rect::new(brect.x + 1, brect.y + 1, brect.width.saturating_sub(2), brect.height.saturating_sub(2));
                                        let inside = me.column >= inner.x && me.column <= inner.x + inner.width.saturating_sub(1) && me.row >= inner.y && me.row <= inner.y + inner.height.saturating_sub(1);
                                        if inside {
                                            let local_x = me.column as i32 - inner.x as i32;
                                            let cx = (local_x / 2) as usize;
                                            let cy = (me.row - inner.y) as usize;
                                            if cx < game.w && cy < game.h {
                                                game.cursor = (cx, cy);
                                                ui.cursor_indicator = Some((cx, cy));
                                            }
                                        }
                                    }
                                    MouseEventKind::Down(MouseButton::Left) => {
                                        let inner = Rect::new(brect.x + 1, brect.y + 1, brect.width.saturating_sub(2), brect.height.saturating_sub(2));
                                        let inside = me.column >= inner.x && me.column <= inner.x + inner.width.saturating_sub(1) && me.row >= inner.y && me.row <= inner.y + inner.height.saturating_sub(1);
                                        if inside {
                                            let local_x = me.column as i32 - inner.x as i32;
                                            let cx = (local_x / 2) as usize;
                                            let cy = (me.row - inner.y) as usize;
                                            if cx < game.w && cy < game.h {
                                                if let Some((rx,ry)) = ui._right_press {
                                                    if rx==cx && ry==cy {
                                                        ui.chord_active = Some((cx, cy));
                                                    } else {
                                                        ui.left_press = Some((cx, cy));
                                                    }
                                                } else {
                                                    ui.left_press = Some((cx, cy));
                                                }
                                            }
                                        }
                                    }
                                    MouseEventKind::Up(MouseButton::Left) => {
                                        if let Some((ccx, ccy)) = ui.chord_active {
                                            let idx = game.index(ccx, ccy);
                                            if game.revealed[idx] {
                                                let adj = game.board[idx].adj as usize;
                                                let mut flagged = 0usize;
                                                let mut neighbors = vec![];
                                                for oy in ccy.saturating_sub(1)..=(ccy+1).min(game.h-1) {
                                                    for ox in ccx.saturating_sub(1)..=(ccx+1).min(game.w-1) {
                                                        if ox==ccx && oy==ccy { continue }
                                                        neighbors.push((ox,oy));
                                                    }
                                                }
                                                for (ox,oy) in &neighbors { if game.flagged[game.index(*ox,*oy)] == 1 { flagged += 1 } }
                                                if flagged != adj {
                                                    ui.flash_cell = Some(((ccx,ccy), Instant::now()));
                                                } else {
                                                    let mut wrong_flag = false;
                                                    for (ox,oy) in &neighbors {
                                                        let nidx = game.index(*ox,*oy);
                                                        if game.flagged[nidx] == 1 && !game.board[nidx].mine { wrong_flag = true; break; }
                                                    }
                                                    if wrong_flag {
                                                        game.reveal_all_mines();
                                                        if let Some(t0) = game.start_time { game.elapsed = t0.elapsed(); }
                                                        game.started = false;
                                                        game.game_over = Some(false);
                                                        ui.showing_loss = true;
                                                    }
                                                    else { for (ox,oy) in &neighbors { let nidx = game.index(*ox,*oy); if !game.revealed[nidx] && game.flagged[nidx] != 1 { game.reveal(*ox,*oy); } } if let Some(true) = game.game_over { ui.showing_win = true } }
                                                }
                                            }
                                            ui.chord_active = None;
                                            ui.left_press = None;
                                        } else {
                                            let inner = Rect::new(brect.x + 1, brect.y + 1, brect.width.saturating_sub(2), brect.height.saturating_sub(2));
                                            let inside = me.column >= inner.x && me.column <= inner.x + inner.width.saturating_sub(1) && me.row >= inner.y && me.row <= inner.y + inner.height.saturating_sub(1);
                                            if inside {
                                                let local_x = me.column as i32 - inner.x as i32;
                                                let cx = (local_x / 2) as usize;
                                                let cy = (me.row - inner.y) as usize;
                                                if cx < game.w && cy < game.h {
                                                    if let Some((px,py)) = ui.left_press {
                                                        if px==cx && py==cy {
                                                            let idx = game.index(cx, cy);
                                                            if !game.revealed[idx] {
                                                                game.reveal(cx,cy);
                                                                if let Some(false) = game.game_over {
                                                                    game.reveal_all_mines();
                                                                    ui.showing_loss = true;
                                                                } else if let Some(true) = game.game_over {
                                                                    ui.showing_win = true;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            ui.left_press = None;
                                        }
                                    }
                                    MouseEventKind::Down(MouseButton::Right) => {
                                        let inner = Rect::new(brect.x + 1, brect.y + 1, brect.width.saturating_sub(2), brect.height.saturating_sub(2));
                                        let inside = me.column >= inner.x && me.column <= inner.x + inner.width.saturating_sub(1) && me.row >= inner.y && me.row <= inner.y + inner.height.saturating_sub(1);
                                        if inside {
                                            let local_x = me.column as i32 - inner.x as i32;
                                            let cx = (local_x / 2) as usize;
                                            let cy = (me.row - inner.y) as usize;
                                            if cx < game.w && cy < game.h {
                                                if let Some((lx,ly)) = ui.left_press {
                                                    if lx==cx && ly==cy {
                                                        ui.chord_active = Some((cx,cy));
                                                    } else {
                                                        ui._right_press = Some((cx,cy));
                                                    }
                                                } else {
                                                    ui._right_press = Some((cx,cy));
                                                }
                                            }
                                        }
                                    }
                                    MouseEventKind::Up(MouseButton::Right) => {
                                        if let Some((ccx, ccy)) = ui.chord_active {
                                            let idx = game.index(ccx, ccy);
                                            if game.revealed[idx] {
                                                let adj = game.board[idx].adj as usize;
                                                let mut flagged = 0usize;
                                                let mut neighbors = vec![];
                                                for oy in ccy.saturating_sub(1)..=(ccy+1).min(game.h-1) {
                                                    for ox in ccx.saturating_sub(1)..=(ccx+1).min(game.w-1) {
                                                        if ox==ccx && oy==ccy { continue }
                                                        neighbors.push((ox,oy));
                                                    }
                                                }
                                                for (ox,oy) in &neighbors { if game.flagged[game.index(*ox,*oy)] == 1 { flagged += 1 } }
                                                if flagged != adj {
                                                    ui.flash_cell = Some(((ccx,ccy), Instant::now()));
                                                } else {
                                                    let mut wrong_flag = false;
                                                    for (ox,oy) in &neighbors {
                                                        let nidx = game.index(*ox,*oy);
                                                        if game.flagged[nidx] == 1 && !game.board[nidx].mine { wrong_flag = true; break; }
                                                    }
                                                    if wrong_flag {
                                                        game.reveal_all_mines();
                                                        if let Some(t0) = game.start_time { game.elapsed = t0.elapsed(); }
                                                        game.started = false;
                                                        game.game_over = Some(false);
                                                        ui.showing_loss = true;
                                                    }
                                                    else { for (ox,oy) in &neighbors { let nidx = game.index(*ox,*oy); if !game.revealed[nidx] && game.flagged[nidx] != 1 { game.reveal(*ox,*oy); } } if let Some(true) = game.game_over { ui.showing_win = true } }
                                                }
                                            }
                                            ui.chord_active = None;
                                            ui.left_press = None;
                                            ui._right_press = None;
                                        } else {
                                            let inner = Rect::new(brect.x + 1, brect.y + 1, brect.width.saturating_sub(2), brect.height.saturating_sub(2));
                                            let inside = me.column >= inner.x && me.column <= inner.x + inner.width.saturating_sub(1) && me.row >= inner.y && me.row <= inner.y + inner.height.saturating_sub(1);
                                            if inside {
                                                let local_x = me.column as i32 - inner.x as i32;
                                                let cx = (local_x / 2) as usize;
                                                let cy = (me.row - inner.y) as usize;
                                                if cx < game.w && cy < game.h {
                                                    if let Some((px,py)) = ui._right_press {
                                                        if px==cx && py==cy {
                                                            let idx = game.index(cx,cy);
                                                            if cfg.use_question_marks {
                                                                game.toggle_flag(cx,cy);
                                                            } else {
                                                                if game.flagged[idx] == 1 { game.flagged[idx] = 0 } else { game.flagged[idx] = 1 }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        ui._right_press = None;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            if exit_requested { break; }
        }

        // If player has won, update record for current difficulty
        // Don't record times for Custom difficulty since it's not persisted
        if let Some(true) = game.game_over {
            if game.elapsed.is_zero() == false {
                let secs = game.elapsed.as_secs();
                let difficulty = cfg.difficulty.clone();
                let is_custom = matches!(difficulty, Difficulty::Custom(_, _, _));
                if !is_custom {
                    let cur = cfg.get_record(&difficulty);
                    if cur.is_none() || secs < cur.unwrap() {
                        ui.last_run_new_record = true;
                        cfg.set_record(&difficulty, secs);
                        save_config(&cfg);
                    }
                }
            }
        }

        // handle simulated key release timer (100ms) for terminals that don't emit release events
        if let Some((t0, kind)) = ui.key_timer {
            if t0.elapsed() >= Duration::from_millis(100) {
                match kind {
                    0 => {
                        // simulate space release: reveal if press started at same cursor
                        if let Some((px,py)) = ui.left_press {
                            let (cx,cy) = game.cursor;
                            if px==cx && py==cy {
                                let idx = game.index(cx,cy);
                                if !game.revealed[idx] {
                                    game.reveal(cx,cy);
                                    if let Some(false) = game.game_over { game.reveal_all_mines(); ui.showing_loss = true; }
                                    else if let Some(true) = game.game_over { ui.showing_win = true; }
                                }
                            }
                        }
                        ui.left_press = None;
                    }
                    1 => {
                        // simulate enter release: perform chord reveal if chord_active
                        if let Some((ccx,ccy)) = ui.chord_active {
                            let idx = game.index(ccx, ccy);
                            if game.revealed[idx] {
                                let adj = game.board[idx].adj as usize;
                                let mut flagged = 0usize;
                                let mut neighbors = vec![];
                                for oy in ccy.saturating_sub(1)..=(ccy+1).min(game.h-1) {
                                    for ox in ccx.saturating_sub(1)..=(ccx+1).min(game.w-1) {
                                        if ox==ccx && oy==ccy { continue }
                                        neighbors.push((ox,oy));
                                    }
                                }
                                for (ox,oy) in &neighbors { if game.flagged[game.index(*ox,*oy)] == 1 { flagged += 1 } }
                                if flagged != adj { ui.flash_cell = Some(((ccx,ccy), Instant::now())); }
                                else {
                                    let mut wrong_flag = false;
                                    for (ox,oy) in &neighbors { let nidx = game.index(*ox,*oy); if game.flagged[nidx] == 1 && !game.board[nidx].mine { wrong_flag = true; break; } }
                                    if wrong_flag {
                                        game.reveal_all_mines();
                                        if let Some(t0) = game.start_time { game.elapsed = t0.elapsed(); }
                                        game.started = false;
                                        game.game_over = Some(false);
                                        ui.showing_loss = true;
                                    }
                                    else { for (ox,oy) in &neighbors { let nidx = game.index(*ox,*oy); if !game.revealed[nidx] && game.flagged[nidx] != 1 { game.reveal(*ox,*oy); } } if let Some(true) = game.game_over { ui.showing_win = true } }
                                }
                            }
                        }
                        ui.chord_active = None; ui.left_press = None; ui._right_press = None;
                    }
                    _ => {}
                }
                ui.key_timer = None;
            }
        }

        // clear click feedback after short duration
        if let Some(t0) = ui.click_instant {
            if t0.elapsed() > Duration::from_millis(200) {
                ui.clicked_index = None;
                ui.click_instant = None;
            }
        }

        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    // Save current difficulty before exiting
    save_config(&cfg);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), DisableMouseCapture, terminal::LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn center_rect(width: u16, height: u16, r: Rect) -> Rect {
    let x = r.x + (r.width.saturating_sub(width)) / 2;
    let y = r.y + (r.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn centered_block(w: u16, h: u16, r: Rect) -> Rect { center_rect(w, h, r) }

fn bottom_centered_block(width: u16, height: u16, r: Rect) -> Rect {
    let x = r.x + (r.width.saturating_sub(width)) / 2;
    let y = r.y + r.height.saturating_sub(height);
    Rect::new(x, y, width, height)
}