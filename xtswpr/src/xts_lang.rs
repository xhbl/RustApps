// Multi-language support module
// Provides localized UI strings for English and Chinese with an extensible design

#[derive(Clone)]
pub struct Assets {
    // Menu items
    pub menu_help: &'static str,
    pub menu_new: &'static str,
    pub menu_records: &'static str,
    pub menu_difficulty: &'static str,
    pub menu_options: &'static str,
    pub menu_about: &'static str,
    pub menu_exit: &'static str,

    // Difficulty names
    pub diff_beginner: &'static str,
    pub diff_intermediate: &'static str,
    pub diff_expert: &'static str,
    pub diff_custom: &'static str,

    // Difficulty modal
    pub diff_width_label: &'static str,
    pub diff_height_label: &'static str,
    pub diff_mines_label_fmt: &'static str, // "Mines (10-{}):"
    pub diff_mines_ncnt: &'static str,      // "mines" / "个雷"

    // Options modal
    pub opt_show_indicator: &'static str,
    pub opt_use_question: &'static str,
    pub opt_ascii_icons: &'static str,
    pub opt_language: &'static str,

    // Help modal
    pub help_controls: &'static str,
    pub help_move: &'static str,
    pub help_reveal: &'static str,
    pub help_flag: &'static str,
    pub help_chord: &'static str,

    // Records modal
    pub rec_best_time: &'static str,
    pub rec_no_record: &'static str,

    // Win/Loss modals
    pub win_title: &'static str,
    pub win_message: &'static str,
    pub win_time_fmt: &'static str,        // "Time: {} seconds"
    pub win_time_record_fmt: &'static str, // "Time: {} seconds (New Record!)"

    pub loss_title: &'static str,
    pub loss_message: &'static str,
    pub loss_better_luck: &'static str,

    // About modal
    pub about_description: &'static str,
    pub about_version_fmt: &'static str, // "v{} by {}"

    // Status bar
    pub status_mines_fmt: &'static str, // " Mines: {}   Time: {}s "

    // Buttons
    pub btn_ok: &'static str,
    pub btn_close: &'static str,

    // Terminal size messages
    pub tsmsg_line1: &'static str,
    pub tsmsg_line2: &'static str,
    pub tsmsg_title: &'static str,

    // Language names for selection
    pub lang_english: &'static str,
    pub lang_chinese: &'static str,
}

/// Returns English language assets
pub fn english_assets() -> Assets {
    Assets {
        // Menu items
        menu_help: "Help",
        menu_new: "New",
        menu_records: "Records",
        menu_difficulty: "Difficulty",
        menu_options: "Options",
        menu_about: "About",
        menu_exit: "Exit",

        // Difficulty names
        diff_beginner: "Beginner",
        diff_intermediate: "Intermediate",
        diff_expert: "Expert",
        diff_custom: "Custom",

        // Difficulty modal
        diff_width_label: "Width (9-36):",
        diff_height_label: "Height (9-24):",
        diff_mines_label_fmt: "Mines (10-{}):",
        diff_mines_ncnt: "mines",

        // Options modal
        opt_show_indicator: "Show indicator",
        opt_use_question: "Use ? marks",
        opt_ascii_icons: "ASCII icons",
        opt_language: "Language",

        // Help modal
        help_controls: " Controls:",
        help_move: "  Mouse | Arrows    - move cursor",
        help_reveal: "  L-Click | Space   - reveal",
        help_flag: "  R-Click | F       - toggle flag",
        help_chord: "  L+R-Click | Enter - chord (open neighbors)",

        // Records modal
        rec_best_time: " Best time in seconds:",
        rec_no_record: "-",

        // Win/Loss modals
        win_title: "Success",
        win_message: "Mines Cleared — You Win!",
        win_time_fmt: "Time: {} seconds",
        win_time_record_fmt: "Time: {} seconds (New Record!)",

        loss_title: "Failure",
        loss_message: "Mine Exploded — You Lose!",
        loss_better_luck: "Better luck next time.",

        // About modal
        about_description: "A terminal-based classic Minesweeper game",
        about_version_fmt: "v{} by {}",

        // Status bar
        status_mines_fmt: " Mines: {}   Time: {} seconds ",

        // Buttons
        btn_ok: " OK ",
        btn_close: " CLOSE ",

        // terminal size messages
        tsmsg_line1: "Terminal layout too small",
        tsmsg_line2: "Minimum size required: {} x {}",
        tsmsg_title: "Resize needed",

        // Language names
        lang_english: "English",
        lang_chinese: "中文",
    }
}

/// Returns Chinese language assets
pub fn chinese_assets() -> Assets {
    Assets {
        // Menu items
        menu_help: "帮助",
        menu_new: "新游戏",
        menu_records: "纪录",
        menu_difficulty: "难度",
        menu_options: "选项",
        menu_about: "关于",
        menu_exit: "退出",

        // Difficulty names
        diff_beginner: "初级",
        diff_intermediate: "中级",
        diff_expert: "高级",
        diff_custom: "自定义",

        // Difficulty modal
        diff_width_label: "宽度 (9-36):",
        diff_height_label: "高度 (9-24):",
        diff_mines_label_fmt: "地雷 (10-{}):",
        diff_mines_ncnt: "个雷",

        // Options modal
        opt_show_indicator: "显示游标",
        opt_use_question: "使用问号",
        opt_ascii_icons: "ASCII图标",
        opt_language: "语言",

        // Help modal
        help_controls: " 操作说明：",
        help_move: "  鼠标 | 方向键     - 移动光标",
        help_reveal: "  左键 | 空格       - 翻开",
        help_flag: "  右键 | F          - 标记/取消",
        help_chord: "  双键 | 回车       - 组合排雷（开邻近格子）",

        // Records modal
        rec_best_time: " 最佳时间（秒）：",
        rec_no_record: "-",

        // Win/Loss modals
        win_title: "成功",
        win_message: "地雷已清除 — 你赢了！",
        win_time_fmt: "用时：{} 秒",
        win_time_record_fmt: "用时：{} 秒（新纪录！）",

        loss_title: "失败",
        loss_message: "地雷爆炸 — 你输了！",
        loss_better_luck: "祝下次好运。",

        // About modal
        about_description: "一款基于终端的经典扫雷游戏",
        about_version_fmt: "v{} 作者 {}",

        // Status bar
        status_mines_fmt: " 地雷：{}   时间：{} 秒 ",

        // Buttons
        btn_ok: " 确定 ",
        btn_close: " 关闭 ",

        // terminal size messages
        tsmsg_line1: "终端屏幕布局过小",
        tsmsg_line2: "最小需要尺寸：{} x {}",
        tsmsg_title: "需要调整大小",

        // Language names
        lang_english: "English",
        lang_chinese: "中文",
    }
}

/// Main language manager struct
/// Holds the current language code and active string assets
pub struct Lang {
    pub current_lang: String,
    pub assets: Assets,
}

impl Lang {
    /// Creates a new Lang instance from a language code
    /// Normalizes input (e.g., "zh-CN" → "zh") and defaults to English for unsupported languages
    pub fn new(lang_code: &str) -> Self {
        let normalized = lang_code.to_lowercase();
        let code = if normalized.starts_with("zh") {
            "zh"
        } else {
            "en"
        };

        Lang {
            current_lang: code.to_string(),
            assets: if code == "zh" {
                chinese_assets()
            } else {
                english_assets()
            },
        }
    }

    /// Switches the current language and reloads all string assets
    /// Used when the user changes language in the options menu
    pub fn switch_to(&mut self, lang_code: &str) {
        let normalized = lang_code.to_lowercase();
        let code = if normalized.starts_with("zh") {
            "zh"
        } else {
            "en"
        };

        self.current_lang = code.to_string();
        self.assets = if code == "zh" {
            chinese_assets()
        } else {
            english_assets()
        };
    }

    /// Get localized difficulty name by index
    /// Index mapping: 0=Beginner, 1=Intermediate, 2=Expert, 3=Custom
    pub fn diff_name(&self, index: usize) -> &'static str {
        match index {
            0 => self.assets.diff_beginner,
            1 => self.assets.diff_intermediate,
            2 => self.assets.diff_expert,
            3 => self.assets.diff_custom,
            _ => self.assets.diff_custom,
        }
    }

    /// Get all difficulty names as an array
    /// Returns [Beginner, Intermediate, Expert, Custom] in the current language
    pub fn diff_names(&self) -> [&'static str; 4] {
        [
            self.assets.diff_beginner,
            self.assets.diff_intermediate,
            self.assets.diff_expert,
            self.assets.diff_custom,
        ]
    }

    /// Format an ISO date (YYYY-MM-DD) according to the current language
    /// English: MM/DD/YYYY (e.g., "01/22/2026")
    /// Chinese: YYYY年MM月DD日 (e.g., "2026年01月22日")
    pub fn format_date(&self, iso_date: &str) -> String {
        let parts: Vec<&str> = iso_date.split('-').collect();
        if parts.len() != 3 {
            return iso_date.to_string();
        }

        if self.current_lang == "zh" {
            format!("{}年{}月{}日", parts[0], parts[1], parts[2])
        } else {
            format!("{}/{}/{}", parts[1], parts[2], parts[0])
        }
    }
}
