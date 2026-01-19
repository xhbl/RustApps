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

use crate::xts_game::{Game, Config, Difficulty, save_config};

fn reset_ui_after_new_game(_game: &mut Game, ui: &mut UiState) {
    ui.reset_after_new_game();
}

// Group runtime UI variables into a single structure to simplify passing them around
#[derive(Debug)]
struct UiState {
    left_press: Option<(usize,usize)>,
    _right_press: Option<(usize,usize)>,
    chord_active: Option<(usize,usize)>,
    flash_cell: Option<((usize,usize), Instant)>,
    clicked_index: Option<usize>,
    click_instant: Option<Instant>,
    hover_index: Option<usize>,
    modal_close_hovered: bool,
    modal_close_pressed: bool,
    modal_rect: Option<Rect>,
    modal_close_rect: Option<Rect>,
    showing_options: bool,
    showing_about: bool,
    showing_help: bool,
    showing_record: bool,
    showing_win: bool,
    showing_loss: bool,
    last_run_new_record: bool,
    exit_menu_item_down: bool,  // Track when exit menu item is pressed, wait for release
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
            showing_options: false,
            showing_about: false,
            showing_help: false,
            showing_record: false,
            showing_win: false,
            showing_loss: false,
            last_run_new_record: false,
            exit_menu_item_down: false,
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
        self.showing_options = false;
        self.showing_about = false;
        self.showing_help = false;
        self.showing_record = false;
        self.showing_win = false;
        self.showing_loss = false;
        self.exit_menu_item_down = false;
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
    let mut menu_rect: Option<Rect> = None;
    let mut board_rect: Option<Rect> = None;
    let menu_labels = ["F1: Help","F2: New","F4: Records","F5: Difficulty","F9: About","Esc: Exit"];
    let mut options_selected: usize = cfg.difficulty.to_index();
    let mut exit_requested: bool = false;

    // Centralized glyph definitions: change characters/colors here to alter appearance globally
    let glyph_unopened = ("■", Color::White);
    let glyph_mine = ("☼", Color::Black);
    let glyph_flag = ("⚑", Color::Red);
    let glyph_question = ("?", Color::Red);
    // Background color for the minefield (change this variable to alter background)
    let board_bg = Color::DarkGray;
    // Cursor background color (centralized)
    let cursor_bg = Color::LightBlue;
    // Background color for neighbor highlight when chord is active
    let chord_bg = Color::LightBlue;
    // Flash (warning) colors when chord fails
    let flash_bg = Color::Red;
    let flash_fg = Color::White;
    let flash_mod = Modifier::BOLD;
    // Number colors for revealed cells 1..8
    let num_colors: [Color; 8] = [
        Color::Blue,
        Color::Blue,
        Color::Blue,
        Color::Blue,
        Color::Blue,
        Color::Blue,
        Color::Blue,
        Color::Blue,
    ];

    let tick_rate = Duration::from_millis(200);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| {
            let size = f.size();
            // If terminal too small, render a centered warning and skip normal UI
            if size.width < 80 || size.height < 24 {
                let warn_lines = vec![Spans::from(Span::raw("Terminal size too small.")), Spans::from(Span::raw("Minimum required: 80 x 24"))];
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
                .margin(1)
                .constraints([Constraint::Length(3), Constraint::Min(6), Constraint::Length(3)].as_ref())
                .split(size);

            // menu row (per-item styled so hover/click mapping aligns with mouse offsets)
            let mut spans_vec: Vec<Span> = Vec::new();
            for (i, lbl) in menu_labels.iter().enumerate() {
                if i > 0 {
                    spans_vec.push(Span::raw("   "));
                }
                let (label_key, label_rest) = match *lbl {
                    "F1: Help" => ("F1", ": Help"),
                    "F2: New" => ("F2", ": New"),
                    "F4: Records" => ("F4", ": Records"),
                    "F5: Difficulty" => ("F5", ": Difficulty"),
                    "F9: About" => ("F9", ": About"),
                    "Esc: Exit" => ("Esc", ": Exit"),
                    _ => (*lbl, ""),
                };
                let (key_style, rest_style) = if Some(i) == ui.clicked_index {
                    (Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD), Style::default().bg(Color::Green).fg(Color::Black))
                } else if Some(i) == ui.hover_index {
                    (Style::default().bg(Color::Blue).fg(Color::Black).add_modifier(Modifier::BOLD), Style::default().bg(Color::Blue).fg(Color::Black))
                } else {
                    (Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD), Style::default())
                };

                spans_vec.push(Span::styled(label_key.to_string(), key_style));
                spans_vec.push(Span::styled(label_rest.to_string(), rest_style));
            }
            // add one-space padding left and right inside the menu block
            spans_vec.insert(0, Span::raw(" "));
            spans_vec.push(Span::raw(" "));
            let menu = Paragraph::new(Spans::from(spans_vec)).block(Block::default().borders(Borders::ALL)).alignment(Alignment::Left);
            f.render_widget(menu, chunks[0]);
            menu_rect = Some(chunks[0]);

            

            // status row
            let status_text = format!(" Mines: {}   Time: {}s ", game.remaining_mines(), if game.started { game.start_time.unwrap().elapsed().as_secs() } else { game.elapsed.as_secs() });
            let status = Paragraph::new(Text::from(Spans::from(Span::raw(status_text))))
                .block(Block::default().borders(Borders::ALL))
                .alignment(Alignment::Left);
            f.render_widget(status, chunks[2]);

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
                        if !(ccx==x && ccy==y) {
                            let xmin = ccx.saturating_sub(1);
                            let xmax = (ccx+1).min(game.w-1);
                            let ymin = ccy.saturating_sub(1);
                            let ymax = (ccy+1).min(game.h-1);
                                    if x >= xmin && x <= xmax && y >= ymin && y <= ymax {
                                if !game.revealed[idx] && game.flagged[idx] != 1 {
                                    style = style.bg(chord_bg);
                                }
                            }
                        }
                    }
                    // apply flash style if this cell is flashing
                    if let Some(((fx,fy), t0)) = ui.flash_cell {
                        if fx==x && fy==y && t0.elapsed() < Duration::from_millis(350) {
                            style = style.bg(flash_bg).fg(flash_fg).add_modifier(flash_mod);
                        }
                    }
                    spans.push(Span::styled(format!(" {}", s), style));
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
            if ui.showing_options {
                let mrect = centered_block(40,8, size);
                ui.modal_rect = Some(mrect);
                f.render_widget(Clear, mrect);
                f.render_widget(Block::default().borders(Borders::ALL).title("Difficulty"), mrect);
                let inner = Rect::new(mrect.x + 1, mrect.y + 1, mrect.width.saturating_sub(2), mrect.height.saturating_sub(2));
                let mut lines = vec![Spans::from(Span::raw(""))];
                for (i, d) in [Difficulty::Beginner, Difficulty::Intermediate, Difficulty::Expert].iter().enumerate() {
                    let mark = if i==options_selected { "*" } else { " " };
                    let (ww,hh,mn) = d.params();
                    let idx = format!(" {} ", i+1);
                    let suffix = format!(") {:<14} {:>2}x{:<2}  {} mines", d.name(), ww, hh, mn);
                    let mark_style = if i==options_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
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

            if ui.showing_about {
                let mrect = centered_block(48,8, size);
                ui.modal_rect = Some(mrect);
                f.render_widget(Clear, mrect);
                f.render_widget(Block::default().borders(Borders::ALL).title("About"), mrect);
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
                let mrect = centered_block(50,10, size);
                ui.modal_rect = Some(mrect);
                f.render_widget(Clear, mrect);
                f.render_widget(Block::default().borders(Borders::ALL).title("Help"), mrect);
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
                let rb = centered_block(40,9, size);
                ui.modal_rect = Some(rb);
                f.render_widget(Clear, rb);
                let mut rec_lines = vec![Spans::from(Span::raw("")), Spans::from(Span::raw(" Best time in seconds:"))];
                let labels = ["Beginner", "Intermediate", "Expert"];
                let label_max = labels.iter().map(|s| s.len()).max().unwrap_or(0);
                let time_w = 5usize; // allow up to 5 digits for time
                let r0 = cfg.get_record_detail(Difficulty::Beginner);
                let r1 = cfg.get_record_detail(Difficulty::Intermediate);
                let r2 = cfg.get_record_detail(Difficulty::Expert);
                let make_line = |label: &str, rec: Option<(u64,String)>| {
                    let prefix = "  ";
                    let colon = ":";
                    // start with prefix + label + colon
                    let mut s = format!("{}{}{}", prefix, label, colon);
                    // pad so time column starts 2 spaces after the longest label
                    let extra_label_pad = label_max.saturating_sub(label.len());
                    s.push_str(&" ".repeat(extra_label_pad));
                    s.push_str(&"  "); // two-space gap between longest-name and time
                    // time field
                    match rec {
                        Some((secs, date)) => {
                            let time_str = format!("{}", secs);
                            let time_field = if time_str.len() > time_w { time_str.chars().take(time_w).collect::<String>() } else { format!("{:>width$}", time_str, width=time_w) };
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
                rec_lines.push(make_line("Beginner", r0));
                rec_lines.push(make_line("Intermediate", r1));
                rec_lines.push(make_line("Expert", r2));
                let p = Paragraph::new(Text::from(rec_lines)).block(Block::default().borders(Borders::ALL).title("Records")).alignment(Alignment::Left);
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
                let wb = bottom_centered_block(40,7, size);
                ui.modal_rect = Some(wb);
                f.render_widget(Clear, wb);
                f.render_widget(Block::default().borders(Borders::ALL).title("Success"), wb);
                let inner = Rect::new(wb.x + 1, wb.y + 1, wb.width.saturating_sub(2), wb.height.saturating_sub(2));
                let t = if game.started { game.start_time.unwrap().elapsed().as_secs() } else { game.elapsed.as_secs() };
                // Use the last_run_new_record flag because the config may already
                // contain the saved value (making t == cfg value). We set this
                // flag when we write the new record above.
                let is_new = ui.last_run_new_record;
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
                let lb = bottom_centered_block(44,7, size);
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
                            if ui.showing_options {
                                match code {
                                    KeyCode::Char('1') => {
                                        options_selected = 0;
                                        cfg.difficulty = Difficulty::Beginner;
                                        save_config(&cfg);
                                        let (w,h,m) = cfg.difficulty.params();
                                        game = Game::new(w,h,m);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                        ui.showing_options = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                    }
                                    KeyCode::Char('2') => {
                                        options_selected = 1;
                                        cfg.difficulty = Difficulty::Intermediate;
                                        save_config(&cfg);
                                        let (w,h,m) = cfg.difficulty.params();
                                        game = Game::new(w,h,m);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                        ui.showing_options = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                    }
                                    KeyCode::Char('3') => {
                                        options_selected = 2;
                                        cfg.difficulty = Difficulty::Expert;
                                        save_config(&cfg);
                                        let (w,h,m) = cfg.difficulty.params();
                                        game = Game::new(w,h,m);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                        ui.showing_options = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                    }
                                    KeyCode::Up => { if options_selected == 0 { options_selected = 2 } else { options_selected -= 1 } }
                                    KeyCode::Down => { options_selected = (options_selected + 1) % 3 }
                                    KeyCode::Enter | KeyCode::Char(' ') => {
                                        cfg.difficulty = Difficulty::from_index(options_selected);
                                        save_config(&cfg);
                                        let (w,h,m) = cfg.difficulty.params();
                                        game = Game::new(w,h,m);
                                        reset_ui_after_new_game(&mut game, &mut ui);
                                        ui.showing_options = false;
                                        ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false;
                                    }
                                    KeyCode::Esc => { ui.showing_options = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false }
                                    _ => {}
                                }
                            } else if ui.showing_about {
                                match code { KeyCode::Esc => { ui.showing_about = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } _ => { ui.showing_about = false; ui.modal_rect = None; ui.modal_close_rect = None; ui.modal_close_pressed = false; ui.hover_index = None } }
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
                                    KeyCode::F(5) => { if !ui.showing_options { options_selected = cfg.difficulty.to_index(); } ui.showing_options = !ui.showing_options }
                                    KeyCode::F(9) => { ui.showing_about = true }
                                    KeyCode::Char('o') if modifiers.contains(KeyModifiers::CONTROL) => { if !ui.showing_options { options_selected = cfg.difficulty.to_index(); } ui.showing_options = !ui.showing_options }
                                    KeyCode::Left => { game.step_cursor(-1,0) }
                                    KeyCode::Right => { game.step_cursor(1,0) }
                                    KeyCode::Up => { game.step_cursor(0,-1) }
                                    KeyCode::Down => { game.step_cursor(0,1) }
                                    KeyCode::Char(' ') => {
                                        // Space press: emulate left-button down at current cursor
                                        ui.left_press = Some(game.cursor);
                                    }
                                    KeyCode::Enter => {
                                        // Enter press: emulate simultaneous left+right down (activate chord highlight)
                                        let c = game.cursor;
                                        ui.left_press = Some(c);
                                        ui._right_press = Some(c);
                                        ui.chord_active = Some(c);
                                    }
                                    KeyCode::Char('f') | KeyCode::Char('F') => { game.toggle_flag(game.cursor.0, game.cursor.1) }
                                    _ => {}
                                }
                            }
                        }
                        KeyEventKind::Release => {
                            // handle key releases for reveal / chord
                            if ui.showing_options || ui.showing_about || ui.showing_help || ui.showing_record || ui.showing_win || ui.showing_loss {
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
                        // check inside modal; if outside and click -> close modal; if inside and Options -> handle item hover/click
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
                                    // if options modal, update hovered option based on mouse row
                                    if ui.showing_options {
                                        let local_row = me.row as i32 - (mrect.y as i32) - 1; // 0-based within content
                                        // content layout: 0:blank,1..3:options,4:blank
                                        if local_row >= 1 && local_row <= 3 {
                                            options_selected = (local_row - 1) as usize;
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
                                    // click inside modal: if options, determine which line and apply selection
                                    if ui.showing_options {
                                        let local_row = me.row as i32 - (mrect.y as i32) - 1;
                                        if local_row >= 1 && local_row <= 3 {
                                            let idx = (local_row - 1) as usize;
                                            options_selected = idx;
                                            // apply selection immediately
                                            cfg.difficulty = Difficulty::from_index(options_selected);
                                            save_config(&cfg);
                                            let (w,h,m) = cfg.difficulty.params();
                                            game = Game::new(w,h,m);
                                            reset_ui_after_new_game(&mut game, &mut ui);
                                            ui.showing_options = false;
                                            // clear modal geometry so subsequent mouse events are handled by main UI
                                            ui.modal_rect = None;
                                            ui.modal_close_rect = None;
                                            ui.modal_close_pressed = false;
                                        }
                                    } else {
                                        // other modals: clicking inside currently just keeps them open
                                    }
                                }
                            }
                            MouseEventKind::Up(_) => {
                                // if we had pressed the close button, check release inside button to close
                                if ui.modal_close_pressed {
                                    if let Some(btn) = ui.modal_close_rect {
                                        let in_btn = me.column >= btn.x && me.column <= btn.x + btn.width.saturating_sub(1) && me.row >= btn.y && me.row <= btn.y + btn.height.saturating_sub(1);
                                            if in_btn {
                                            let was_win = ui.showing_win;
                                            let was_loss = ui.showing_loss;
                                            ui.showing_options = false;
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
                                    ui.modal_close_pressed = false;
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
                                        for (i, lbl) in menu_labels.iter().enumerate() {
                                            if i > 0 { offset += 3; }
                                            let full_len = (*lbl).len() as u16;
                                            let end = offset + full_len - 1;
                                            if me.column >= offset && me.column <= end {
                                                found = Some(i);
                                                break;
                                            }
                                            offset = end + 1;
                                        }
                                        ui.hover_index = found;
                                        true
                                    }
                                    MouseEventKind::Down(MouseButton::Left) => {
                                        let mut consumed = false;
                                        let mut offset = start_x;
                                        for (i, lbl) in menu_labels.iter().enumerate() {
                                            if i > 0 { offset += 3; }
                                            let full_len = (*lbl).len() as u16;
                                            let end = offset + full_len - 1;
                                            if me.column >= offset && me.column <= end {
                                                ui.clicked_index = Some(i);
                                                ui.click_instant = Some(Instant::now());
                                                match i {
                                                    0 => ui.showing_help = true,
                                                    1 => { let (w,h,m) = cfg.difficulty.params(); game = Game::new(w,h,m); reset_ui_after_new_game(&mut game, &mut ui); },
                                                    2 => ui.showing_record = true,
                                                    3 => { if !ui.showing_options { options_selected = cfg.difficulty.to_index(); } ui.showing_options = true },
                                                    4 => ui.showing_about = true,
                                                    5 => {
                                                        // Mark that exit button is pressed, will exit on Up event
                                                        ui.exit_menu_item_down = true;
                                                    },
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
                                        // If exit button was pressed, actually exit now after the Up event
                                        if ui.exit_menu_item_down {
                                            ui.exit_menu_item_down = false;
                                            exit_requested = true;
                                        }
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
                                                            game.toggle_flag(cx,cy);
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
        if let Some(true) = game.game_over {
            if game.elapsed.is_zero() == false {
                let secs = game.elapsed.as_secs();
                let cur = cfg.get_record(cfg.difficulty);
                if cur.is_none() || secs < cur.unwrap() {
                    ui.last_run_new_record = true;
                    cfg.set_record(cfg.difficulty, secs);
                    save_config(&cfg);
                }
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