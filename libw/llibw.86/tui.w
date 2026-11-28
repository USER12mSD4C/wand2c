sc.true
#import <syscall>
#import <syscall_structs>
#import <string>
#import <keyboard>
#import <signal>

const TUI_TYPE_PANEL = 1;
const TUI_TYPE_LABEL = 2;
const TUI_TYPE_TEXTBOX = 3;
const TUI_TYPE_LIST = 4;
const TUI_TYPE_TEXTVIEW = 5;
const TUI_TYPE_BUTTON = 6;

const TUI_EVT_NONE = 0;
const TUI_EVT_KEY = 1;
const TUI_EVT_MOUSE_CLICK = 2;
const TUI_EVT_MOUSE_RELEASE = 3;
const TUI_EVT_MOUSE_MOVE = 4;
const TUI_EVT_MOUSE_WHEEL = 5;
const TUI_EVT_RESIZE = 6;
const TUI_EVT_FOCUS_IN = 7;
const TUI_EVT_FOCUS_OUT = 8;
const TUI_EVT_PASTE = 9;

const TUI_ATTR_BOLD = 1;
const TUI_ATTR_DIM = 2;
const TUI_ATTR_ITALIC = 4;
const TUI_ATTR_UNDERLINE = 8;
const TUI_ATTR_REVERSE = 16;

const TUI_BORDER_NONE = 0;
const TUI_BORDER_SINGLE = 1;
const TUI_BORDER_DOUBLE = 2;
const TUI_BORDER_ROUNDED = 3;
const TUI_BORDER_HEAVY = 4;

const TUI_MOUSE_LEFT = 0;
const TUI_MOUSE_MIDDLE = 1;
const TUI_MOUSE_RIGHT = 2;
const TUI_MOUSE_WHEEL_UP = 64;
const TUI_MOUSE_WHEEL_DOWN = 65;

// Per-line style marker for textview lines (see tui_textview_add_line_styled).
const TUI_LINE_STYLE = 255;

const TUI_CLR_BLACK = 0;
const TUI_CLR_RED = 1;
const TUI_CLR_GREEN = 2;
const TUI_CLR_YELLOW = 3;
const TUI_CLR_BLUE = 4;
const TUI_CLR_MAGENTA = 5;
const TUI_CLR_CYAN = 6;
const TUI_CLR_WHITE = 7;
const TUI_CLR_BRIGHT_BLACK = 8;
const TUI_CLR_BRIGHT_RED = 9;
const TUI_CLR_BRIGHT_GREEN = 10;
const TUI_CLR_BRIGHT_YELLOW = 11;
const TUI_CLR_BRIGHT_BLUE = 12;
const TUI_CLR_BRIGHT_MAGENTA = 13;
const TUI_CLR_BRIGHT_CYAN = 14;
const TUI_CLR_BRIGHT_WHITE = 15;

// Bounds for the cell buffers. A pty reports u16 dimensions, so an extreme
// resize could otherwise ask for a 16 GB allocation. The TUI caps its managed
// area at these bounds; resize clears the whole screen, but only the capped
// area is repainted afterwards.
const TUI_MAX_COLS = 512;
const TUI_MAX_ROWS = 200;

struct TuiCell {
    u8 ch;
    u8 fg;
    u8 bg;
    u8 attr;
}

struct TuiAnchor {
    u8 left;
    u8 top;
    u8 right;
    u8 bottom;
    i64 left_off;
    i64 top_off;
    i64 right_off;
    i64 bottom_off;
}

struct TuiWidget {
    u64 widget_type;
    i64 x;
    i64 y;
    i64 w;
    i64 h;
    u8 visible;
    u8 dirty;
    u8 focusable;
    u8 focused;
    u8 fg;
    u8 bg;
    u8 attr;
    u64 id;
    u64 userdata;
    u64 on_click;
    u64 on_key;
    TuiAnchor anchor;
    u64 parent;
    u64 children;
    u64 child_count;
    u64 child_capacity;
    u64 data;
}

struct PanelData {
    u8* title;
    u8 border_style;
}

struct LabelData {
    u8* text;
}

struct TextBoxData {
    u8* buf;
    u64 buf_len;
    u64 max_len;
    u64 cursor_pos;
    u64 scroll_off;
    u8* placeholder;
}

struct ListData {
    u64 items;
    u64 item_count;
    u64 item_capacity;
    i64 selected;
    i64 scroll_off;
}

struct TextViewData {
    u64 lines;
    u64 line_count;
    u64 line_capacity;
    // Original (unwrapped) records. Stored so a resize can rewrap the log at
    // the new width instead of clipping old lines.
    u64 sources;
    u64 source_count;
    u64 source_capacity;
    i64 scroll_row;
    i64 scroll_col;
    i64 wrap_width;
}

// One source record:
//   [TuiSource][tag bytes][0][folded text][0][raw UTF-8 text][0]
// kind 0 = plain wrapped text, kind 1 = tagged block (accent tag on its own
// line, indented body), kind 2 = one display line per append_line call.
// Rewrap uses the stored folded text; dump uses the raw text.
struct TuiSource {
    u8 kind;
    u8 fg;
    u8 bg;
    u8 attr;
    u8 tag_fg;
    u8 tag_attr;
    u8 tag_len;
    u8 reserved;
    u64 folded_len;
    u64 raw_len;
}

struct ButtonData {
    u8* text;
    u8 pressed;
}

struct TuiMouseState {
    i64 x;
    i64 y;
    u8 left_down;
    u8 middle_down;
    u8 right_down;
}

struct TuiState {
    u64 front_buf;
    u64 back_buf;
    i64 cols;
    i64 rows;
    u8 initialized;
    u8 exit_requested;
    u64 root;
    u64 focused;
    u64 out_buf;
    u64 out_pos;
    u64 out_capacity;
    TuiMouseState mouse;
    u64 mouse_enabled;
    u64 raw_term_set;
    u8 termios_backup[60];
    // Set by tui_handle_resize and consumed by tui_poll_event. render_frame
    // may apply a resize first; the flag keeps one event for the application.
    u64 resize_pending;
    u64 widget_id_counter;
    // Bracketed paste: we never enable mouse capture, but we do enable paste
    // so a multi-line paste pasted from another app is not split into commands.
    u8 paste_buf[16384];
    u64 paste_len;
    u64 paste_truncated;
}

sect.tui_g
    u64 state_ptr = 0;
EOS

// The stdlib arena allocator relocates itself when it grows (mem_grow moves
// the arena), which would invalidate every widget pointer mid-session. TUI
// allocations therefore use their own private page mappings: each block is
// independent and stable for the whole session.
fn tui_malloc(u64 size) -> u64 {
    if (size == 0) { size = 16; }
    u64 payload = size;
    u64 rem = payload % 16;
    if (rem != 0) { payload = payload + 16 - rem; }
    u64 total = payload + 16;
    u64 pages = total / 4096;
    if (total % 4096 != 0) { pages = pages + 1; }
    u64 map_len = pages * 4096;
    u64 base = sys_mmap(0, map_len, PROT_READ | PROT_WRITE,
                        MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (syscall_error(base) == 1) { return(0); }
    u64* header = base;
    header[0] = size;
    header[1] = map_len;
    return(base + 16);
}

fn tui_free(u64 ptr) {
    if (ptr == 0) { return; }
    u64 base = ptr - 16;
    u64* header = base;
    sys_munmap(base, header[1]);
}

fn tui_realloc(u64 old_ptr, u64 old_size, u64 new_size) -> u64 {
    u64 new_ptr = tui_malloc(new_size);
    if (new_ptr == 0) {
        return(0);
    }
    if (old_ptr != 0) {
        if (old_size > 0) {
            u8* src = old_ptr;
            u8* dst = new_ptr;
            u64 copy_size = old_size;
            if (copy_size > new_size) {
                copy_size = new_size;
            }
            memcpy(dst, src, copy_size);
        }
        tui_free(old_ptr);
    }
    return(new_ptr);
}

fn tui_get_state() -> TuiState* {
    u64 p = tui_g:state_ptr;
    TuiState* s = p;
    return(s);
}

fn tui_color_rgb(u8 r, u8 g, u8 b) -> u8 {
    u8 ri = r / 51;
    u8 gi = g / 51;
    u8 bi = b / 51;
    return(16 + (ri * 36) + (gi * 6) + bi);
}

fn tui_color_index(u8 idx) -> u8 {
    return(idx);
}

fn tui_get_cols() -> i64 {
    TuiState* s = tui_get_state();
    return(s->cols);
}

fn tui_get_rows() -> i64 {
    TuiState* s = tui_get_state();
    return(s->rows);
}

fn tui_out_reset(TuiState* s) {
    s->out_pos = 0;
}

fn tui_out_byte(TuiState* s, u8 b) {
    if (s->out_pos >= s->out_capacity) {
        u64 new_cap = s->out_capacity * 2;
        if (new_cap == 0) {
            new_cap = 4096;
        }
        u64 new_buf = tui_realloc(s->out_buf, s->out_capacity, new_cap);
        if (new_buf == 0) {
            return;
        }
        s->out_buf = new_buf;
        s->out_capacity = new_cap;
    }
    u8* buf = s->out_buf;
    buf[s->out_pos] = b;
    s->out_pos = s->out_pos + 1;
}

fn tui_out_str(TuiState* s, u8* str) {
    u8* p*i = str;
    u8 c = p;
    while (c != 0) {
        tui_out_byte(s, c);
        p = p + 1;
        c = p;
    }
}

fn tui_out_num(TuiState* s, i64 num) {
    if (num < 0) {
        tui_out_byte(s, 45);
        num = 0 - num;
    }
    if (num == 0) {
        tui_out_byte(s, 48);
        return;
    }
    u8 tmp[20];
    i64 pos = 19;
    u64 n = num;
    while (n > 0) {
        tmp[pos] = (n % 10) + 48;
        n = n / 10;
        pos = pos - 1;
    }
    pos = pos + 1;
    while (pos < 20) {
        tui_out_byte(s, tmp[pos]);
        pos = pos + 1;
    }
}

fn tui_out_flush(TuiState* s) {
    if (s->out_pos > 0) {
        u8* buf = s->out_buf;
        sys_write(1, buf, s->out_pos);
        s->out_pos = 0;
    }
}

fn tui_get_terminal_size(i64* out_rows*o, i64* out_cols*o) {
    u8 ws_buf[8];
    ws_buf[0] = 0;
    ws_buf[1] = 0;
    ws_buf[2] = 0;
    ws_buf[3] = 0;
    u64 rc = sys_ioctl(1, 21523, ws_buf*adr);
    if (syscall_error(rc) == 1) {
        // No tty: callers supply the 24x80 fallback instead of a 1x1 screen.
        out_rows = 0;
        out_cols = 0;
        return;
    }
    u16* p_row*i = ws_buf*adr;
    u16 r = p_row;
    u16* p_col*i = (ws_buf*adr) + 2;
    u16 c = p_col;
    i64 rows = (i64)r;
    i64 cols = (i64)c;
    if (rows < 1) { rows = 1; }
    if (cols < 1) { cols = 1; }
    if (rows > TUI_MAX_ROWS) { rows = TUI_MAX_ROWS; }
    if (cols > TUI_MAX_COLS) { cols = TUI_MAX_COLS; }
    out_rows = rows;
    out_cols = cols;
}

fn tui_cell_pack(u8 ch, u8 fg, u8 bg, u8 attr) -> u32 {
    u32 v = (u32)ch;
    v = v | ((u32)fg << 8);
    v = v | ((u32)bg << 16);
    v = v | ((u32)attr << 24);
    return(v);
}

fn tui_cell_ch(u32 cell) -> u8 {
    return((u8)(cell & 255));
}

fn tui_cell_fg(u32 cell) -> u8 {
    return((u8)((cell >> 8) & 255));
}

fn tui_cell_bg(u32 cell) -> u8 {
    return((u8)((cell >> 16) & 255));
}

fn tui_cell_attr(u32 cell) -> u8 {
    return((u8)((cell >> 24) & 255));
}

fn tui_alloc_buf(i64 cols, i64 rows) -> u64 {
    if (cols < 1 || rows < 1) { return(0); }
    if (cols > TUI_MAX_COLS || rows > TUI_MAX_ROWS) { return(0); }
    u64 total = (u64)(cols * rows) * 4;
    u64 buf = tui_malloc(total);
    if (buf == 0) {
        return(0);
    }
    u32* p = buf;
    u32 default_cell = tui_cell_pack(32, 15, 0, 0);
    u64 count = (u64)(cols * rows);
    u64 i = 0;
    while (i < count) {
        p[i] = default_cell;
        i = i + 1;
    }
    return(buf);
}

fn tui_free_buf(u64 buf) {
    tui_free(buf);
}

fn tui_buf_get(u64 buf, i64 cols, i64 x, i64 y) -> u32 {
    u32* p = buf;
    u64 idx = (u64)(y * cols + x);
    return(p[idx]);
}

fn tui_buf_set(u64 buf, i64 cols, i64 x, i64 y, u32 cell) {
    u32* p = buf;
    u64 idx = (u64)(y * cols + x);
    p[idx] = cell;
}

fn tui_buf_fill(u64 buf, i64 cols, i64 x, i64 y, i64 w, i64 h, u32 cell) {
    i64 r = y;
    while (r < y + h) {
        i64 c = x;
        while (c < x + w) {
            if (r >= 0) {
                if (r < tui_get_rows()) {
                    if (c >= 0) {
                        if (c < tui_get_cols()) {
                            tui_buf_set(buf, cols, c, r, cell);
                        }
                    }
                }
            }
            c = c + 1;
        }
        r = r + 1;
    }
}

fn tui_buf_draw_char(u64 buf, i64 cols, i64 x, i64 y, u8 ch, u8 fg, u8 bg, u8 attr) {
    if (y >= 0) {
        if (y < tui_get_rows()) {
            if (x >= 0) {
                if (x < tui_get_cols()) {
                    u32 cell = tui_cell_pack(ch, fg, bg, attr);
                    tui_buf_set(buf, cols, x, y, cell);
                }
            }
        }
    }
}

// The cell buffer stores one byte per cell, so raw UTF-8 bytes would occupy
// several cells while the terminal renders one glyph. Decode to one ASCII
// cell per codepoint instead; this keeps columns aligned on UTF-8 terminals.
// A malformed sequence is consumed one byte at a time, never past the NUL.
fn tui_utf8_seq_len(u8* p) -> u64 {
    u8 c = p[0];
    if (c < 128) { return(1); }
    u64 n = 1;
    if (c >= 248) { return(1); }
    if (c >= 240) { n = 4; }
    else if (c >= 224) { n = 3; }
    else if (c >= 192) { n = 2; }
    u64 k = 1;
    while (k < n) {
        if ((p[k] & 192) != 128) { return(1); }
        k = k + 1;
    }
    return(n);
}

fn tui_utf8_ascii(u8* p, u64 n) -> u8 {
    u64 cp = 0;
    if (n == 1) {
        if (p[0] >= 32 && p[0] < 127) { return(p[0]); }
        if (p[0] == 9) { return(32); }
        return(46);
    }
    if (n == 2) {
        cp = ((u64)(p[0] & 31) << 6) | (u64)(p[1] & 63);
    } else if (n == 3) {
        cp = ((u64)(p[0] & 15) << 12) | ((u64)(p[1] & 63) << 6) | (u64)(p[2] & 63);
    } else {
        cp = ((u64)(p[0] & 7) << 18) | ((u64)(p[1] & 63) << 12) |
             ((u64)(p[2] & 63) << 6) | (u64)(p[3] & 63);
    }
    if (cp == 0x2500 || cp == 0x2501 || cp == 0x2013 || cp == 0x2014) { return(45); }
    if (cp == 0x2502 || cp == 0x2503) { return(124); }
    if (cp >= 0x250c && cp <= 0x254b) { return(43); }
    if (cp == 0x276f || cp == 0x25b8 || cp == 0x2192) { return(62); }
    if (cp == 0x2190) { return(60); }
    if (cp == 0x2191) { return(94); }
    if (cp == 0x2193) { return(118); }
    if (cp == 0x25c6 || cp == 0x2022 || cp == 0x25cf || cp == 0x2605) { return(42); }
    if (cp == 0x2713 || cp == 0x2714) { return(118); }
    if (cp == 0x2717 || cp == 0x2718) { return(120); }
    if (cp == 0x2018 || cp == 0x2019) { return(39); }
    if (cp == 0x201c || cp == 0x201d) { return(34); }
    if (cp == 0x2139) { return(105); }
    if (cp == 0x00b7 || cp == 0x2026) { return(46); }
    return(46);
}

// Convert a UTF-8 string to exactly one byte per displayed cell. Newlines
// survive so the wrapper can split logical lines afterwards.
fn tui_ascii_copy(u8* dst, u8* src, u64 max) -> u64 {
    u64 n = 0;
    while (src[0] != 0 && n + 1 < max) {
        if (src[0] == 10 || src[0] == 13) {
            dst[n] = 10;
            n = n + 1;
            src = src + 1;
            continue;
        }
        u64 len = tui_utf8_seq_len(src);
        dst[n] = tui_utf8_ascii(src, len);
        n = n + 1;
        u64 k = 0;
        while (k < len) {
            src = src + 1;
            k = k + 1;
        }
    }
    dst[n] = 0;
    return(n);
}

fn tui_buf_draw_span(u64 buf, i64 cols, i64 x, i64 y, u8* p, i64 max_cells, u8 fg, u8 bg, u8 attr) -> i64 {
    i64 cx = 0;
    while (cx < max_cells) {
        u8 c = p[0];
        if (c == 0) { return(cx); }
        u64 n = tui_utf8_seq_len(p);
        tui_buf_draw_char(buf, cols, x + cx, y, tui_utf8_ascii(p, n), fg, bg, attr);
        cx = cx + 1;
        u64 k = 0;
        while (k < n) {
            p = p + 1;
            k = k + 1;
        }
    }
    return(cx);
}

fn tui_buf_draw_str(u64 buf, i64 cols, i64 x, i64 y, u8* str, u8 fg, u8 bg, u8 attr) {
    tui_buf_draw_span(buf, cols, x, y, str, 4096, fg, bg, attr);
}

fn tui_emit_cursor(TuiState* s, i64 row, i64 col) {
    tui_out_str(s, "\033[");
    tui_out_num(s, row + 1);
    tui_out_byte(s, 59);
    tui_out_num(s, col + 1);
    tui_out_byte(s, 72);
}

fn tui_emit_color(TuiState* s, u8 fg, u8 bg, u8 attr) {
    tui_out_str(s, "\033[0");
    if ((attr & 1) != 0) {
        tui_out_str(s, ";1");
    }
    if ((attr & 2) != 0) {
        tui_out_str(s, ";2");
    }
    if ((attr & 4) != 0) {
        tui_out_str(s, ";3");
    }
    if ((attr & 8) != 0) {
        tui_out_str(s, ";4");
    }
    if ((attr & 16) != 0) {
        tui_out_str(s, ";7");
    }
    if (fg < 16) {
        if (fg < 8) {
            tui_out_byte(s, 59);
            tui_out_num(s, 30 + (i64)fg);
        } else {
            tui_out_byte(s, 59);
            tui_out_num(s, 82 + (i64)(fg - 8));
        }
    } else {
        tui_out_str(s, ";38;5;");
        tui_out_num(s, (i64)fg);
    }
    if (bg < 16) {
        if (bg < 8) {
            tui_out_byte(s, 59);
            tui_out_num(s, 40 + (i64)bg);
        } else {
            tui_out_byte(s, 59);
            tui_out_num(s, 92 + (i64)(bg - 8));
        }
    } else {
        tui_out_str(s, ";48;5;");
        tui_out_num(s, (i64)bg);
    }
    tui_out_byte(s, 109);
}

fn tui_render_diff(TuiState* s) {
    u32* front = s->front_buf;
    u32* back = s->back_buf;
    u64 total = (u64)(s->cols * s->rows);
    u8 cur_fg = 255;
    u8 cur_bg = 255;
    u8 cur_attr = 255;
    i64 cur_row = -1;
    i64 cur_col = -1;
    u64 i = 0;
    while (i < total) {
        u32 fc = front[i];
        u32 bc = back[i];
        if (fc != bc) {
            back[i] = fc;
            i64 r = (i64)(i / (u64)s->cols);
            i64 c = (i64)(i % (u64)s->cols);
            u8 ch = tui_cell_ch(fc);
            u8 fg = tui_cell_fg(fc);
            u8 bg = tui_cell_bg(fc);
            u8 attr = tui_cell_attr(fc);
            if (r != cur_row || c != cur_col) {
                tui_emit_cursor(s, r, c);
                cur_row = r;
                cur_col = c;
            }
            if (fg != cur_fg || bg != cur_bg || attr != cur_attr) {
                tui_emit_color(s, fg, bg, attr);
                cur_fg = fg;
                cur_bg = bg;
                cur_attr = attr;
            }
            tui_out_byte(s, ch);
            cur_col = cur_col + 1;
            if (cur_col >= s->cols) {
                cur_row = -1;
                cur_col = -1;
            }
        }
        i = i + 1;
    }
}

fn tui_set_raw_mode(TuiState* s) {
    if (s->raw_term_set != 0) {
        return;
    }
    sys_ioctl(0, 0x5401, s->termios_backup*adr);
    u8 tc_buf[60];
    sys_ioctl(0, 0x5401, tc_buf*adr);
    u32* lflag_ptr = tc_buf*adr + 12;
    u32 lflag = lflag_ptr[0];
    lflag = lflag & 0xFFFFFFF0;
    lflag = lflag & 0xFFFF7FFF;
    lflag_ptr[0] = lflag;
    u8* cc_ptr = tc_buf*adr + 17;
    cc_ptr[6] = 1;
    cc_ptr[5] = 0;
    sys_ioctl(0, 0x5402, tc_buf*adr);
    s->raw_term_set = 1;
}

fn tui_restore_term(TuiState* s) {
    if (s->raw_term_set != 0) {
        sys_ioctl(0, 0x5402, s->termios_backup*adr);
        s->raw_term_set = 0;
    }
}

fn tui_enable_mouse(TuiState* s) {
    if (s->mouse_enabled != 0) {
        return;
    }
    tui_out_str(s, "\033[?1000h");
    tui_out_str(s, "\033[?1002h");
    tui_out_str(s, "\033[?1006h");
    tui_out_flush(s);
    s->mouse_enabled = 1;
}

fn tui_disable_mouse(TuiState* s) {
    if (s->mouse_enabled == 0) {
        return;
    }
    tui_out_str(s, "\033[?1000l");
    tui_out_str(s, "\033[?1002l");
    tui_out_str(s, "\033[?1006l");
    tui_out_flush(s);
    s->mouse_enabled = 0;
}

fn tui_hide_cursor(TuiState* s) {
    tui_out_str(s, "\033[?25l");
    tui_out_flush(s);
}

fn tui_show_cursor(TuiState* s) {
    tui_out_str(s, "\033[?25h");
    tui_out_flush(s);
}

fn tui_clear_screen(TuiState* s) {
    tui_out_str(s, "\033[2J\033[H");
    tui_out_flush(s);
}

// No SIGWINCH handler: a signal delivered while blocked in read() can corrupt
// the interrupted syscall frame on this runtime. The event loop polls stdin
// with a short timeout instead and notices terminal size changes there.

fn tui_char_available() -> u64 {
    u32 count = 0;
    sys_ioctl(0, 21531, count*adr);
    return((u64)count);
}

fn tui_read_byte() -> i64 {
    u8 b = 0;
    i64 n = (i64)sys_read(0, b*adr, 1);
    if (n <= 0) {
        return(-1);
    }
    return((i64)b);
}

fn tui_read_byte_timeout() -> i64 {
    u32 count = 0;
    sys_ioctl(0, 21531, count*adr);
    if (count == 0) {
        return(-1);
    }
    return(tui_read_byte());
}

// Wait briefly for the next input byte. This keeps the event loop responsive
// to terminal resizes without relying on an asynchronous signal handler.
fn tui_read_byte_wait() -> i64 {
    pollfd fds;
    fds.fd = 0;
    fds.events = 1;
    fds.revents = 0;
    i64 n = (i64)sys_poll(fds*adr, 1, 50);
    if (n <= 0) {
        return(-1);
    }
    return(tui_read_byte());
}

// Terminals may write ESC and '[' in separate batches. A short wait here keeps
// a bare ESC responsive while tolerating ordinary scheduling gaps.
fn tui_read_byte_esc() -> i64 {
    pollfd fds;
    fds.fd = 0;
    fds.events = 1;
    fds.revents = 0;
    i64 n = (i64)sys_poll(fds*adr, 1, 250);
    if (n <= 0) {
        return(-1);
    }
    return(tui_read_byte());
}

// Bracketed paste must not be split by a short input gap. Wait much longer
// than tui_read_byte_wait(), but still give up if the terminal never sends the
// closing marker.
fn tui_read_byte_paste() -> i64 {
    pollfd fds;
    fds.fd = 0;
    fds.events = 1;
    fds.revents = 0;
    i64 n = (i64)sys_poll(fds*adr, 1, 1000);
    if (n <= 0) {
        return(-1);
    }
    return(tui_read_byte());
}

fn tui_parse_sgr_mouse(TuiState* s, i64* out_type*o, i64* out_btn*o, i64* out_x*o, i64* out_y*o) -> u64 {
    i64 btn = 0;
    i64 mx = 0;
    i64 my = 0;
    i64 phase = 0;
    i64 val = 0;
    u64 running = 1;
    while (running == 1) {
        i64 b = tui_read_byte_timeout();
        if (b < 0) {
            return(0);
        }
        if (b >= 48 && b <= 57) {
            val = val * 10 + (b - 48);
        } else if (b == 59) {
            if (phase == 0) {
                btn = val;
            } else if (phase == 1) {
                mx = val - 1;
            }
            val = 0;
            phase = phase + 1;
        } else if (b == 77) {
            my = val - 1;
            if (btn >= 64 && btn <= 65) {
                out_type = TUI_EVT_MOUSE_WHEEL;
                out_btn = btn;
                out_x = mx;
                out_y = my;
                return(1);
            }
            if ((btn & 32) != 0) {
                out_type = TUI_EVT_MOUSE_MOVE;
                out_btn = btn & 3;
                out_x = mx;
                out_y = my;
                return(1);
            }
            out_type = TUI_EVT_MOUSE_CLICK;
            out_btn = btn & 3;
            out_x = mx;
            out_y = my;
            return(1);
        } else if (b == 109) {
            my = val - 1;
            out_type = TUI_EVT_MOUSE_RELEASE;
            out_btn = btn & 3;
            out_x = mx;
            out_y = my;
            return(1);
        } else {
            return(0);
        }
    }
    return(0);
}

fn tui_paste_end_marker(u8* pat) -> u64 {
    if (pat[0] != 27 || pat[1] != 91) { return(0); }
    if (pat[2] != 50 || pat[3] != 48 || pat[4] != 49 || pat[5] != 126) { return(0); }
    return(1);
}

// The opening ESC[200~ was already consumed. Collect until ESC[201~ (or EOF),
// keeping only the pasted bytes. A paste never becomes keystrokes/commands.
fn tui_read_paste(TuiState* s) -> u64 {
    u8 pat[6];
    u64 i = 0;
    while (i < 6) {
        pat[i] = 0;
        i = i + 1;
    }
    s->paste_len = 0;
    s->paste_truncated = 0;
    u64 filled = 0;
    while (1) {
        i64 b = tui_read_byte_paste();
        if (b < 0) {
            s->paste_buf[s->paste_len] = 0;
            return(0);
        }
        i = 0;
        while (i < 5) {
            pat[i] = pat[i + 1];
            i = i + 1;
        }
        pat[5] = (u8)b;
        if (filled < 6) { filled = filled + 1; }
        // Hold back the last five bytes: they might be the start of ESC[201~.
        if (filled == 6 && tui_paste_end_marker(pat*adr) == 1) {
            s->paste_buf[s->paste_len] = 0;
            return(1);
        }
        if (filled == 6 && s->paste_len < 16383) {
            s->paste_buf[s->paste_len] = pat[0];
            s->paste_len = s->paste_len + 1;
        } else if (filled == 6) {
            s->paste_truncated = 1;
        }
    }
    return(0);
}

fn tui_parse_input(TuiState* s, u64* out_type*o, u64* out_a*o, u64* out_b*o, u64* out_c*o) -> u64 {
    i64 b = tui_read_byte_wait();
    if (b < 0) {
        out_type = TUI_EVT_NONE;
        return(0);
    }
    if (b == 27) {
        i64 b2 = tui_read_byte_esc();
        if (b2 < 0) {
            out_type = TUI_EVT_KEY;
            out_a = 27;
            return(1);
        }
        if (b2 == 91) {
            // Once ESC[ is seen, allow slow/chunked escape sequences. They are
            // written by terminals in separate batches too.
            i64 b3 = tui_read_byte_paste();
            if (b3 < 0) {
                out_type = TUI_EVT_KEY;
                out_a = 27;
                return(1);
            }
            if (b3 == 50) {
                // ESC[200~ : bracketed paste start.
                i64 b4 = tui_read_byte_paste();
                if (b4 == 48) {
                    i64 b5 = tui_read_byte_paste();
                    if (b5 == 48) {
                        i64 b6 = tui_read_byte_paste();
                        if (b6 == 126) {
                            if (tui_read_paste(s) == 1) {
                                out_type = TUI_EVT_PASTE;
                                out_a = s->paste_len;
                                out_b = s->paste_truncated;
                                out_c = 0;
                                return(1);
                            }
                            out_type = TUI_EVT_NONE;
                            return(0);
                        }
                    }
                }
            }
            if (b3 == 60) {
                i64 evt_type = 0;
                i64 evt_btn = 0;
                i64 evt_x = 0;
                i64 evt_y = 0;
                u64 ok = tui_parse_sgr_mouse(s, evt_type*adr, evt_btn*adr, evt_x*adr, evt_y*adr);
                if (ok == 1) {
                    out_type = (u64)evt_type;
                    out_a = (u64)evt_btn;
                    out_b = (u64)evt_x;
                    out_c = (u64)evt_y;
                    return(1);
                }
                out_type = TUI_EVT_NONE;
                return(0);
            }
            if (b3 == 65) {
                out_type = TUI_EVT_KEY;
                out_a = keys:arrow_up;
                return(1);
            }
            if (b3 == 66) {
                out_type = TUI_EVT_KEY;
                out_a = keys:arrow_down;
                return(1);
            }
            if (b3 == 67) {
                out_type = TUI_EVT_KEY;
                out_a = keys:arrow_right;
                return(1);
            }
            if (b3 == 68) {
                out_type = TUI_EVT_KEY;
                out_a = keys:arrow_left;
                return(1);
            }
            if (b3 == 72) {
                out_type = TUI_EVT_KEY;
                out_a = keys:key_home;
                return(1);
            }
            if (b3 == 70) {
                out_type = TUI_EVT_KEY;
                out_a = keys:key_end;
                return(1);
            }
            if (b3 >= 48 && b3 <= 57) {
                i64 b4 = tui_read_byte_timeout();
                if (b4 == 126) {
                    if (b3 == 51) {
                        out_type = TUI_EVT_KEY;
                        out_a = keys:key_delete;
                        return(1);
                    }
                    if (b3 == 53) {
                        out_type = TUI_EVT_KEY;
                        out_a = keys:page_up;
                        return(1);
                    }
                    if (b3 == 54) {
                        out_type = TUI_EVT_KEY;
                        out_a = keys:page_down;
                        return(1);
                    }
                }
            }
        }
        out_type = TUI_EVT_KEY;
        out_a = 27;
        return(1);
    }
    out_type = TUI_EVT_KEY;
    out_a = (u64)b;
    return(1);
}

fn tui_widget_new(u64 wtype, i64 x, i64 y, i64 w, i64 h) -> u64 {
    u64 ptr = tui_malloc(sizeof(TuiWidget));
    if (ptr == 0) {
        return(0);
    }
    TuiWidget* widget = ptr;
    memset(widget, 0, sizeof(TuiWidget));
    widget->widget_type = wtype;
    widget->x = x;
    widget->y = y;
    widget->w = w;
    widget->h = h;
    widget->visible = 1;
    widget->dirty = 1;
    widget->focusable = 0;
    widget->focused = 0;
    widget->fg = TUI_CLR_WHITE;
    widget->bg = TUI_CLR_BLACK;
    widget->attr = 0;
    TuiState* s = tui_get_state();
    s->widget_id_counter = s->widget_id_counter + 1;
    widget->id = s->widget_id_counter;
    widget->anchor.left = 1;
    widget->anchor.top = 1;
    widget->anchor.right = 0;
    widget->anchor.bottom = 0;
    widget->anchor.left_off = x;
    widget->anchor.top_off = y;
    widget->anchor.right_off = 0;
    widget->anchor.bottom_off = 0;
    return(ptr);
}

fn tui_widget_add_child(u64 parent, u64 child) {
    if (parent == 0 || child == 0) {
        return;
    }
    TuiWidget* pw = parent;
    TuiWidget* cw = child;
    cw->parent = parent;
    if (pw->child_count >= pw->child_capacity) {
        u64 new_cap = pw->child_capacity * 2;
        if (new_cap == 0) {
            new_cap = 8;
        }
        u64 old_size = pw->child_capacity * 8;
        u64 new_size = new_cap * 8;
        u64 new_children = tui_realloc(pw->children, old_size, new_size);
        if (new_children == 0) {
            return;
        }
        pw->children = new_children;
        pw->child_capacity = new_cap;
    }
    u64* arr = pw->children;
    arr[pw->child_count] = child;
    pw->child_count = pw->child_count + 1;
    tui_widget_mark_dirty(parent);
}

fn tui_widget_remove_child(u64 parent, u64 child) {
    if (parent == 0 || child == 0) {
        return;
    }
    TuiWidget* pw = parent;
    u64* arr = pw->children;
    u64 i = 0;
    u64 found = 0;
    while (i < pw->child_count) {
        if (found == 1) {
            arr[i - 1] = arr[i];
        } else if (arr[i] == child) {
            found = 1;
        }
        i = i + 1;
    }
    if (found == 1) {
        pw->child_count = pw->child_count - 1;
        TuiWidget* cw = child;
        cw->parent = 0;
        tui_widget_mark_dirty(parent);
    }
}

fn tui_widget_set_visible(u64 widget, u8 visible) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->visible = visible;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_anchor(u64 widget, u8 left, u8 top, u8 right, u8 bottom) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->anchor.left = left;
    w->anchor.top = top;
    w->anchor.right = right;
    w->anchor.bottom = bottom;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_anchor_offsets(u64 widget, i64 l, i64 t, i64 r, i64 b) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->anchor.left_off = l;
    w->anchor.top_off = t;
    w->anchor.right_off = r;
    w->anchor.bottom_off = b;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_colors(u64 widget, u8 fg, u8 bg) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->fg = fg;
    w->bg = bg;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_attr(u64 widget, u8 attr) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->attr = attr;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_pos(u64 widget, i64 x, i64 y) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->x = x;
    w->y = y;
    w->anchor.left_off = x;
    w->anchor.top_off = y;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_size(u64 widget, i64 w_val, i64 h_val) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->w = w_val;
    w->h = h_val;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_set_focusable(u64 widget, u8 focusable) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->focusable = focusable;
}

fn tui_widget_set_on_click(u64 widget, u64 callback) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->on_click = callback;
}

fn tui_widget_set_on_key(u64 widget, u64 callback) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->on_key = callback;
}

fn tui_widget_get_type(u64 widget) -> u64 {
    if (widget == 0) {
        return(0);
    }
    TuiWidget* w = widget;
    return(w->widget_type);
}

fn tui_widget_set_id(u64 widget, u64 id) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->id = id;
}

fn tui_widget_get_id(u64 widget) -> u64 {
    if (widget == 0) {
        return(0);
    }
    TuiWidget* w = widget;
    return(w->id);
}

fn tui_widget_set_userdata(u64 widget, u64 data) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->userdata = data;
}

fn tui_widget_get_userdata(u64 widget) -> u64 {
    if (widget == 0) {
        return(0);
    }
    TuiWidget* w = widget;
    return(w->userdata);
}

fn tui_widget_mark_dirty(u64 widget) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    w->dirty = 1;
}

fn tui_focus_widget(u64 widget) {
    TuiState* s = tui_get_state();
    if (s->focused != 0) {
        TuiWidget* old_w = s->focused;
        old_w->focused = 0;
        tui_widget_mark_dirty(s->focused);
    }
    s->focused = widget;
    if (widget != 0) {
        TuiWidget* w = widget;
        w->focused = 1;
        tui_widget_mark_dirty(widget);
    }
}

fn tui_get_focused_widget() -> u64 {
    TuiState* s = tui_get_state();
    return(s->focused);
}

fn tui_panel_new(u8* title, i64 x, i64 y, i64 w, i64 h) -> u64 {
    u64 widget = tui_widget_new(TUI_TYPE_PANEL, x, y, w, h);
    if (widget == 0) {
        return(0);
    }
    TuiWidget* wd = widget;
    u64 data = tui_malloc(sizeof(PanelData));
    if (data == 0) {
        tui_free(widget);
        return(0);
    }
    PanelData* pd = data;
    memset(pd, 0, sizeof(PanelData));
    u64 title_len = strlen(title);
    u64 title_buf = tui_malloc(title_len + 1);
    if (title_buf != 0) {
        u8* dst = title_buf;
        strcpy(dst, title);
    }
    pd->title = title_buf;
    pd->border_style = TUI_BORDER_SINGLE;
    wd->data = data;
    wd->fg = TUI_CLR_WHITE;
    wd->bg = TUI_CLR_BLACK;
    return(widget);
}

fn tui_panel_set_title(u64 widget, u8* title) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_PANEL) {
        return;
    }
    PanelData* pd = w->data;
    if (pd->title != 0) {
        tui_free(pd->title);
    }
    u64 title_len = strlen(title);
    u64 title_buf = tui_malloc(title_len + 1);
    if (title_buf != 0) {
        u8* dst = title_buf;
        strcpy(dst, title);
    }
    pd->title = title_buf;
    tui_widget_mark_dirty(widget);
}

fn tui_panel_set_border(u64 widget, u8 border_style) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_PANEL) {
        return;
    }
    PanelData* pd = w->data;
    pd->border_style = border_style;
    tui_widget_mark_dirty(widget);
}

fn tui_label_new(u8* text, i64 x, i64 y, i64 w) -> u64 {
    u64 widget = tui_widget_new(TUI_TYPE_LABEL, x, y, w, 1);
    if (widget == 0) {
        return(0);
    }
    TuiWidget* wd = widget;
    u64 data = tui_malloc(sizeof(LabelData));
    if (data == 0) {
        tui_free(widget);
        return(0);
    }
    LabelData* ld = data;
    memset(ld, 0, sizeof(LabelData));
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    ld->text = text_buf;
    wd->data = data;
    return(widget);
}

fn tui_label_set_text(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LABEL) {
        return;
    }
    LabelData* ld = w->data;
    if (ld->text != 0) {
        tui_free(ld->text);
    }
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    ld->text = text_buf;
    tui_widget_mark_dirty(widget);
}

fn tui_label_get_text(u64 widget) -> u8* {
    if (widget == 0) {
        return(null);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LABEL) {
        return(null);
    }
    LabelData* ld = w->data;
    return(ld->text);
}

fn tui_textbox_new(i64 x, i64 y, i64 w, u64 max_len) -> u64 {
    u64 widget = tui_widget_new(TUI_TYPE_TEXTBOX, x, y, w, 1);
    if (widget == 0) {
        return(0);
    }
    TuiWidget* wd = widget;
    u64 data = tui_malloc(sizeof(TextBoxData));
    if (data == 0) {
        tui_free(widget);
        return(0);
    }
    TextBoxData* td = data;
    memset(td, 0, sizeof(TextBoxData));
    td->max_len = max_len;
    td->buf_len = 0;
    td->cursor_pos = 0;
    td->scroll_off = 0;
    u64 buf = tui_malloc(max_len + 1);
    if (buf != 0) {
        u8* p = buf;
        p[0] = 0;
    }
    td->buf = buf;
    wd->data = data;
    wd->focusable = 1;
    wd->fg = TUI_CLR_WHITE;
    wd->bg = TUI_CLR_BLUE;
    return(widget);
}

fn tui_textbox_get_text(u64 widget) -> u8* {
    if (widget == 0) {
        return(null);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTBOX) {
        return(null);
    }
    TextBoxData* td = w->data;
    return(td->buf);
}

fn tui_textbox_set_text(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTBOX) {
        return;
    }
    TextBoxData* td = w->data;
    u64 len = strlen(text);
    if (len > td->max_len) {
        len = td->max_len;
    }
    u8* dst = td->buf;
    u8* src*i = text;
    u64 i = 0;
    while (i < len) {
        dst[i] = src[i];
        i = i + 1;
    }
    dst[len] = 0;
    td->buf_len = len;
    td->cursor_pos = len;
    tui_widget_mark_dirty(widget);
}

fn tui_textbox_set_placeholder(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTBOX) {
        return;
    }
    TextBoxData* td = w->data;
    if (td->placeholder != 0) {
        tui_free(td->placeholder);
    }
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    td->placeholder = text_buf;
    tui_widget_mark_dirty(widget);
}

fn tui_list_new(i64 x, i64 y, i64 w, i64 h) -> u64 {
    u64 widget = tui_widget_new(TUI_TYPE_LIST, x, y, w, h);
    if (widget == 0) {
        return(0);
    }
    TuiWidget* wd = widget;
    u64 data = tui_malloc(sizeof(ListData));
    if (data == 0) {
        tui_free(widget);
        return(0);
    }
    ListData* ld = data;
    memset(ld, 0, sizeof(ListData));
    ld->selected = -1;
    ld->scroll_off = 0;
    wd->data = data;
    wd->focusable = 1;
    return(widget);
}

fn tui_list_add_item(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return;
    }
    ListData* ld = w->data;
    if (ld->item_count >= ld->item_capacity) {
        u64 new_cap = ld->item_capacity * 2;
        if (new_cap == 0) {
            new_cap = 16;
        }
        u64 old_size = ld->item_capacity * 8;
        u64 new_size = new_cap * 8;
        u64 new_items = tui_realloc(ld->items, old_size, new_size);
        if (new_items == 0) {
            return;
        }
        ld->items = new_items;
        ld->item_capacity = new_cap;
    }
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    u64* arr = ld->items;
    arr[ld->item_count] = text_buf;
    ld->item_count = ld->item_count + 1;
    if (ld->selected < 0) {
        ld->selected = 0;
    }
    tui_widget_mark_dirty(widget);
}

fn tui_list_get_selected(u64 widget) -> i64 {
    if (widget == 0) {
        return(-1);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return(-1);
    }
    ListData* ld = w->data;
    return(ld->selected);
}

fn tui_list_set_selected(u64 widget, i64 idx) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return;
    }
    ListData* ld = w->data;
    if (idx < 0) {
        idx = 0;
    }
    if (idx >= (i64)ld->item_count) {
        idx = (i64)ld->item_count - 1;
    }
    ld->selected = idx;
    tui_widget_mark_dirty(widget);
}

fn tui_list_get_item(u64 widget, i64 idx) -> u8* {
    if (widget == 0) {
        return(null);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return(null);
    }
    ListData* ld = w->data;
    if (idx < 0 || idx >= (i64)ld->item_count) {
        return(null);
    }
    u64* arr = ld->items;
    u64 ptr = arr[idx];
    u8* text = ptr;
    return(text);
}

fn tui_list_get_count(u64 widget) -> i64 {
    if (widget == 0) {
        return(0);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return(0);
    }
    ListData* ld = w->data;
    return((i64)ld->item_count);
}

fn tui_list_clear(u64 widget) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return;
    }
    ListData* ld = w->data;
    u64* arr = ld->items;
    u64 i = 0;
    while (i < ld->item_count) {
        if (arr[i] != 0) {
            tui_free(arr[i]);
        }
        i = i + 1;
    }
    ld->item_count = 0;
    ld->selected = -1;
    ld->scroll_off = 0;
    tui_widget_mark_dirty(widget);
}

fn tui_list_remove_item(u64 widget, i64 idx) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_LIST) {
        return;
    }
    ListData* ld = w->data;
    if (idx < 0 || idx >= (i64)ld->item_count) {
        return;
    }
    u64* arr = ld->items;
    if (arr[idx] != 0) {
        tui_free(arr[idx]);
    }
    u64 i = (u64)idx;
    while (i < ld->item_count - 1) {
        arr[i] = arr[i + 1];
        i = i + 1;
    }
    ld->item_count = ld->item_count - 1;
    if (ld->selected >= (i64)ld->item_count) {
        ld->selected = (i64)ld->item_count - 1;
    }
    tui_widget_mark_dirty(widget);
}

fn tui_textview_new(i64 x, i64 y, i64 w, i64 h) -> u64 {
    u64 widget = tui_widget_new(TUI_TYPE_TEXTVIEW, x, y, w, h);
    if (widget == 0) {
        return(0);
    }
    TuiWidget* wd = widget;
    u64 data = tui_malloc(sizeof(TextViewData));
    if (data == 0) {
        tui_free(widget);
        return(0);
    }
    TextViewData* td = data;
    memset(td, 0, sizeof(TextViewData));
    td->scroll_row = 0;
    td->scroll_col = 0;
    td->wrap_width = -1;
    wd->data = data;
    wd->focusable = 1;
    return(widget);
}

fn tui_textview_add_line(TextViewData* td, u8* text) {
    if (td->line_count >= td->line_capacity) {
        u64 new_cap = td->line_capacity * 2;
        if (new_cap == 0) {
            new_cap = 32;
        }
        u64 old_size = td->line_capacity * 8;
        u64 new_size = new_cap * 8;
        u64 new_lines = tui_realloc(td->lines, old_size, new_size);
        if (new_lines == 0) {
            return;
        }
        td->lines = new_lines;
        td->line_capacity = new_cap;
    }
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    u64* arr = td->lines;
    arr[td->line_count] = text_buf;
    td->line_count = td->line_count + 1;
}

// A styled line is stored as [255][fg][bg][attr][text...]. The first byte
// doubles as a marker because normal log text never starts with 255.
fn tui_textview_add_line_styled(TextViewData* td, u8* text, u8 fg, u8 bg, u8 attr) {
    if (td->line_count >= td->line_capacity) {
        u64 new_cap = td->line_capacity * 2;
        if (new_cap == 0) {
            new_cap = 32;
        }
        u64 old_size = td->line_capacity * 8;
        u64 new_size = new_cap * 8;
        u64 new_lines = tui_realloc(td->lines, old_size, new_size);
        if (new_lines == 0) {
            return;
        }
        td->lines = new_lines;
        td->line_capacity = new_cap;
    }
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 5);
    if (text_buf != 0) {
        u8* dst = text_buf;
        dst[0] = TUI_LINE_STYLE;
        dst[1] = fg;
        dst[2] = bg;
        dst[3] = attr;
        strcpy(dst + 4, text);
    }
    u64* arr = td->lines;
    arr[td->line_count] = text_buf;
    td->line_count = td->line_count + 1;
}

// ---- source records -------------------------------------------------------
// Log records are kept unwrapped so a resize can reflow them at the new
// terminal width. Display lines are always derived from these records.

fn tui_textview_clear_display(TextViewData* td) {
    u64* arr = td->lines;
    u64 i = 0;
    while (i < td->line_count) {
        if (arr[i] != 0) { tui_free(arr[i]); }
        i = i + 1;
    }
    td->line_count = 0;
    td->scroll_row = 0;
    td->scroll_col = 0;
}

fn tui_textview_add_source(TextViewData* td, u8 kind, u8* tag, u8* text,
                           u8 fg, u8 bg, u8 attr, u8 tag_fg, u8 tag_attr) -> u64 {
    if (tag == null) { tag = ""; }
    if (text == null) { text = ""; }
    // Truncate tags on a UTF-8 boundary so a multibyte tag is never split.
    u64 tag_full = strlen(tag);
    u64 tag_len = 0;
    while (tag_len < tag_full && tag_len < 12) {
        u64 seq = tui_utf8_seq_len(tag + tag_len);
        if (tag_len + seq > 12) { break; }
        tag_len = tag_len + seq;
    }
    u64 raw_len = strlen(text);
    if (td->source_count >= td->source_capacity) {
        u64 new_cap = td->source_capacity * 2;
        if (new_cap == 0) { new_cap = 32; }
        u64 old_size = td->source_capacity * 8;
        u64 new_size = new_cap * 8;
        u64 new_arr = tui_realloc(td->sources, old_size, new_size);
        if (new_arr == 0) { return(0); }
        td->sources = new_arr;
        td->source_capacity = new_cap;
    }
    u64 total = sizeof(TuiSource) + tag_len + 1 + raw_len + 1 + raw_len + 1;
    u64 ptr = tui_malloc(total);
    if (ptr == 0) { return(0); }
    TuiSource* src = ptr;
    memset(src, 0, sizeof(TuiSource));
    src->kind = kind;
    src->fg = fg;
    src->bg = bg;
    src->attr = attr;
    src->tag_fg = tag_fg;
    src->tag_attr = tag_attr;
    src->tag_len = (u8)tag_len;
    u8* base = ptr;
    u8* dst = base + sizeof(TuiSource);
    u64 i = 0;
    while (i < tag_len) {
        dst[i] = tag[i];
        i = i + 1;
    }
    dst[tag_len] = 0;
    dst = dst + tag_len + 1;
    u64 folded_len = tui_ascii_copy(dst, text, raw_len + 1);
    src->folded_len = folded_len;
    src->raw_len = raw_len;
    u8* raw = dst + folded_len + 1;
    i = 0;
    while (i < raw_len) {
        raw[i] = text[i];
        i = i + 1;
    }
    raw[raw_len] = 0;
    u64* arr = td->sources;
    arr[td->source_count] = ptr;
    td->source_count = td->source_count + 1;
    return(ptr);
}

fn tui_wrap_segment(TextViewData* td, u8* text, u64 start, u64 end,
                    i64 limit, u8 fg, u8 bg, u8 attr, i64 indent) {
    u8 line[600];
    u64 i = 0;
    while (i < (u64)indent && i < 20) {
        line[i] = 32;
        i = i + 1;
    }
    if (start == end) {
        line[i] = 0;
        tui_textview_add_line_styled(td, line, fg, bg, attr);
        return;
    }
    u64 pos = start;
    while (pos < end) {
        u64 take = (u64)limit;
        if (pos + take > end) { take = end - pos; }
        if (pos + take < end && take > 1) {
            u64 j = pos + take - 1;
            while (j > pos && text[j] != 32) { j = j - 1; }
            if (j > pos && text[j] == 32) { take = j - pos; }
        }
        if (take == 0) { take = 1; }
        u64 k = 0;
        while (k < take) {
            line[i] = text[pos + k];
            i = i + 1;
            k = k + 1;
        }
        line[i] = 0;
        tui_textview_add_line_styled(td, line, fg, bg, attr);
        pos = pos + take;
        while (pos < end && text[pos] == 32) { pos = pos + 1; }
        i = (u64)indent;
    }
}

fn tui_textview_wrap_source(TextViewData* td, u64 src_ptr, i64 width) {
    if (src_ptr == 0) { return; }
    TuiSource* src = src_ptr;
    u8* base = src_ptr;
    u8* text = base + sizeof(TuiSource) + (u64)src->tag_len + 1;
    if (src->kind == 2) {
        // append_line promised exactly one display line per call. Keep that
        // contract while still retaining a source record for rewrap.
        u8 line[600];
        u64 i = 0;
        while (i < 599 && text[i] != 0) {
            u8 c = text[i];
            if (c == 10 || c == 13) { c = 32; }
            line[i] = c;
            i = i + 1;
        }
        line[i] = 0;
        tui_textview_add_line_styled(td, line, src->fg, src->bg, src->attr);
        return;
    }
    i64 indent = 0;
    if (src->kind == 1 && src->tag_len > 0) {
        u8 tag[16];
        u64 i = 0;
        while (i < (u64)src->tag_len && i < 15) {
            tag[i] = base[sizeof(TuiSource) + i];
            i = i + 1;
        }
        tag[i] = 0;
        tui_textview_add_line_styled(td, tag, src->tag_fg, src->bg, src->tag_attr);
        indent = (i64)src->tag_len + 1;
        if (indent > 10) { indent = 10; }
    }
    i64 limit = width - indent - 1;
    if (limit < 4) { limit = 4; }
    if (limit > 512) { limit = 512; }
    u64 start = 0;
    u64 pos = 0;
    while (1) {
        u8 c = text[pos];
        if (c == 0) {
            if (pos > start) { tui_wrap_segment(td, text, start, pos, limit, src->fg, src->bg, src->attr, indent); }
            return;
        }
        if (c == 10 || c == 13) {
            tui_wrap_segment(td, text, start, pos, limit, src->fg, src->bg, src->attr, indent);
            start = pos + 1;
        }
        pos = pos + 1;
    }
}

fn tui_textview_rewrap_all(TextViewData* td, i64 width) {
    u64* arr = td->sources;
    u64 i = 0;
    while (i < td->source_count) {
        tui_textview_wrap_source(td, arr[i], width);
        i = i + 1;
    }
}

fn tui_textview_set_text(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) {
        return;
    }
    TextViewData* td = w->data;
    tui_textview_clear_display(td);
    u64* sources = td->sources;
    u64 i = 0;
    while (i < td->source_count) {
        if (sources[i] != 0) { tui_free(sources[i]); }
        i = i + 1;
    }
    td->source_count = 0;
    tui_textview_add_source(td, 0, "", text, w->fg, w->bg, w->attr, w->fg, w->attr);
    tui_textview_rewrap_all(td, w->w);
    td->wrap_width = w->w;
    tui_widget_mark_dirty(widget);
}

fn tui_textview_append_line(u64 widget, u8* line) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) {
        return;
    }
    TextViewData* td = w->data;
    // Keep append_line records in the source list too. Otherwise a resize
    // rewrap would silently drop every line added through this API. Kind 2
    // keeps the old one-display-line-per-call contract while surviving rewrap.
    u64 src = tui_textview_add_source(td, 2, "", line, w->fg, w->bg, w->attr,
                                      w->fg, w->attr);
    tui_textview_wrap_source(td, src, w->w);
    td->wrap_width = w->w;
    tui_widget_mark_dirty(widget);
}

fn tui_textview_clear(u64 widget) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) {
        return;
    }
    TextViewData* td = w->data;
    tui_textview_clear_display(td);
    u64* sources = td->sources;
    u64 i = 0;
    while (i < td->source_count) {
        if (sources[i] != 0) { tui_free(sources[i]); }
        i = i + 1;
    }
    td->source_count = 0;
    tui_widget_mark_dirty(widget);
}

fn tui_textview_get_line_count(u64 widget) -> i64 {
    if (widget == 0) {
        return(0);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) {
        return(0);
    }
    TextViewData* td = w->data;
    return((i64)td->line_count);
}

export fn tui_textview_get_scroll_row(u64 widget) -> i64 {
    if (widget == 0) {
        return(0);
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) {
        return(0);
    }
    TextViewData* td = w->data;
    return(td->scroll_row);
}

fn tui_button_new(u8* text, i64 x, i64 y, i64 w) -> u64 {
    u64 widget = tui_widget_new(TUI_TYPE_BUTTON, x, y, w, 1);
    if (widget == 0) {
        return(0);
    }
    TuiWidget* wd = widget;
    u64 data = tui_malloc(sizeof(ButtonData));
    if (data == 0) {
        tui_free(widget);
        return(0);
    }
    ButtonData* bd = data;
    memset(bd, 0, sizeof(ButtonData));
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    bd->text = text_buf;
    bd->pressed = 0;
    wd->data = data;
    wd->focusable = 1;
    wd->fg = TUI_CLR_BLACK;
    wd->bg = TUI_CLR_WHITE;
    return(widget);
}

fn tui_button_set_text(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_BUTTON) {
        return;
    }
    ButtonData* bd = w->data;
    if (bd->text != 0) {
        tui_free(bd->text);
    }
    u64 text_len = strlen(text);
    u64 text_buf = tui_malloc(text_len + 1);
    if (text_buf != 0) {
        u8* dst = text_buf;
        strcpy(dst, text);
    }
    bd->text = text_buf;
    tui_widget_mark_dirty(widget);
}

fn tui_widget_destroy(u64 widget) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->parent != 0) {
        tui_widget_remove_child(w->parent, widget);
    }
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        tui_widget_destroy(children[i]);
        i = i + 1;
    }
    if (w->children != 0) {
        tui_free(w->children);
    }
    if (w->data != 0) {
        if (w->widget_type == TUI_TYPE_PANEL) {
            PanelData* pd = w->data;
            if (pd->title != 0) {
                tui_free(pd->title);
            }
        }
        if (w->widget_type == TUI_TYPE_LABEL) {
            LabelData* ld = w->data;
            if (ld->text != 0) {
                tui_free(ld->text);
            }
        }
        if (w->widget_type == TUI_TYPE_TEXTBOX) {
            TextBoxData* td = w->data;
            if (td->buf != 0) {
                tui_free(td->buf);
            }
            if (td->placeholder != 0) {
                tui_free(td->placeholder);
            }
        }
        if (w->widget_type == TUI_TYPE_LIST) {
            ListData* ld = w->data;
            u64* items = ld->items;
            u64 k = 0;
            while (k < ld->item_count) {
                if (items[k] != 0) {
                    tui_free(items[k]);
                }
                k = k + 1;
            }
            if (ld->items != 0) {
                tui_free(ld->items);
            }
        }
        if (w->widget_type == TUI_TYPE_TEXTVIEW) {
            TextViewData* td = w->data;
            u64* lines = td->lines;
            u64 k = 0;
            while (k < td->line_count) {
                if (lines[k] != 0) {
                    tui_free(lines[k]);
                }
                k = k + 1;
            }
            if (td->lines != 0) {
                tui_free(td->lines);
            }
            u64* sources = td->sources;
            k = 0;
            while (k < td->source_count) {
                if (sources[k] != 0) {
                    tui_free(sources[k]);
                }
                k = k + 1;
            }
            if (td->sources != 0) {
                tui_free(td->sources);
            }
        }
        if (w->widget_type == TUI_TYPE_BUTTON) {
            ButtonData* bd = w->data;
            if (bd->text != 0) {
                tui_free(bd->text);
            }
        }
        tui_free(w->data);
    }
    TuiState* s = tui_get_state();
    if (s->focused == widget) {
        s->focused = 0;
    }
    tui_free(widget);
}

fn tui_get_border_chars(u8 style, u8* out*o) {
    // ASCII borders keep one cell per byte on every terminal encoding.
    if (style == TUI_BORDER_DOUBLE) {
        out[0] = 43; out[1] = 43; out[2] = 43; out[3] = 43;
        out[4] = 61; out[5] = 61; out[6] = 124; out[7] = 124;
        return;
    }
    if (style == TUI_BORDER_HEAVY) {
        out[0] = 35; out[1] = 35; out[2] = 35; out[3] = 35;
        out[4] = 35; out[5] = 35; out[6] = 35; out[7] = 35;
        return;
    }
    out[0] = 43; out[1] = 43; out[2] = 43; out[3] = 43;
    out[4] = 45; out[5] = 45; out[6] = 124; out[7] = 124;
}

fn tui_draw_panel(u64 buf, i64 cols, TuiWidget* w) {
    PanelData* pd = w->data;
    i64 x = w->x;
    i64 y = w->y;
    i64 pw = w->w;
    i64 ph = w->h;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    if (pd->border_style == TUI_BORDER_NONE) {
        u32 fill = tui_cell_pack(32, fg, bg, attr);
        tui_buf_fill(buf, cols, x, y, pw, ph, fill);
        return;
    }
    u8 border_chars[8];
    tui_get_border_chars(pd->border_style, border_chars*adr);
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, x, y, pw, ph, fill);
    i64 c = 1;
    while (c < pw - 1) {
        tui_buf_draw_char(buf, cols, x + c, y, border_chars[4], fg, bg, attr);
        tui_buf_draw_char(buf, cols, x + c, y + ph - 1, border_chars[5], fg, bg, attr);
        c = c + 1;
    }
    i64 r = 1;
    while (r < ph - 1) {
        tui_buf_draw_char(buf, cols, x, y + r, border_chars[6], fg, bg, attr);
        tui_buf_draw_char(buf, cols, x + pw - 1, y + r, border_chars[7], fg, bg, attr);
        r = r + 1;
    }
    tui_buf_draw_char(buf, cols, x, y, border_chars[0], fg, bg, attr);
    tui_buf_draw_char(buf, cols, x + pw - 1, y, border_chars[1], fg, bg, attr);
    tui_buf_draw_char(buf, cols, x, y + ph - 1, border_chars[2], fg, bg, attr);
    tui_buf_draw_char(buf, cols, x + pw - 1, y + ph - 1, border_chars[3], fg, bg, attr);
    if (pd->title != 0) {
        u8* title = pd->title;
        u64 title_len = strlen(title);
        if (title_len > 0 && pw > 4) {
            u64 max_title = (u64)(pw - 4);
            if (title_len > max_title) {
                title_len = max_title;
            }
            i64 title_x = x + 2;
            u64 i = 0;
            u8* tp*i = title;
            while (i < title_len) {
                tui_buf_draw_char(buf, cols, title_x + (i64)i, y, tp[i], fg, bg, attr | TUI_ATTR_BOLD);
                i = i + 1;
            }
        }
    }
}

fn tui_draw_label(u64 buf, i64 cols, TuiWidget* w) {
    LabelData* ld = w->data;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, w->x, w->y, w->w, 1, fill);
    if (ld->text != 0) {
        tui_buf_draw_str(buf, cols, w->x, w->y, ld->text, fg, bg, attr);
    }
}

fn tui_draw_textbox(u64 buf, i64 cols, TuiWidget* w) {
    TextBoxData* td = w->data;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, w->x, w->y, w->w, 1, fill);
    if (td->buf_len == 0 && td->placeholder != 0) {
        u8 dim_attr = attr | TUI_ATTR_DIM;
        tui_buf_draw_str(buf, cols, w->x, w->y, td->placeholder, fg, bg, dim_attr);
        return;
    }
    u8* text = td->buf;
    u64 vis_w = (u64)w->w;
    u64 scroll = td->scroll_off;
    if (td->buf_len > scroll) {
        tui_buf_draw_span(buf, cols, w->x, w->y, text + scroll,
                          (i64)vis_w, fg, bg, attr);
    }
    if (w->focused == 1) {
        i64 cursor_col = w->x + (i64)(td->cursor_pos - scroll);
        if (cursor_col >= w->x && cursor_col < w->x + w->w) {
            tui_buf_draw_char(buf, cols, cursor_col, w->y, 95, fg, bg, attr | TUI_ATTR_UNDERLINE);
        }
    }
}

fn tui_draw_list(u64 buf, i64 cols, TuiWidget* w) {
    ListData* ld = w->data;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, w->x, w->y, w->w, w->h, fill);
    i64 visible_rows = w->h;
    if (ld->selected >= 0 && ld->selected < ld->scroll_off) {
        ld->scroll_off = ld->selected;
    }
    if (ld->selected >= ld->scroll_off + visible_rows) {
        ld->scroll_off = ld->selected - visible_rows + 1;
    }
    i64 r = 0;
    while (r < visible_rows) {
        i64 item_idx = ld->scroll_off + r;
        if (item_idx >= 0 && item_idx < (i64)ld->item_count) {
            u64* items = ld->items;
            u64 item_ptr = items[item_idx];
            u8* text = item_ptr;
            u8 item_fg = fg;
            u8 item_bg = bg;
            u8 item_attr = attr;
            if (item_idx == ld->selected) {
                if (w->focused == 1) {
                    item_fg = bg;
                    item_bg = fg;
                    item_attr = attr | TUI_ATTR_BOLD;
                } else {
                    item_attr = attr | TUI_ATTR_REVERSE;
                }
            }
            u32 row_fill = tui_cell_pack(32, item_fg, item_bg, item_attr);
            tui_buf_fill(buf, cols, w->x, w->y + r, w->w, 1, row_fill);
            if (text != null) {
                u8* p*i = text;
                i64 c = 0;
                u8 ch = p;
                while (ch != 0 && c < w->w) {
                    tui_buf_draw_char(buf, cols, w->x + c, w->y + r, ch, item_fg, item_bg, item_attr);
                    c = c + 1;
                    p = p + 1;
                    ch = p;
                }
            }
        }
        r = r + 1;
    }
}

fn tui_textview_line_len(u8* text) -> i64 {
    if (text == null) { return(0); }
    if (text[0] == TUI_LINE_STYLE) { text = text + 4; }
    i64 n = 0;
    while (text[n] != 0 && n < 4096) { n = n + 1; }
    return(n);
}

fn tui_draw_textview(u64 buf, i64 cols, TuiWidget* w) {
    TextViewData* td = w->data;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, w->x, w->y, w->w, w->h, fill);
    i64 visible_rows = w->h;
    if (visible_rows < 1) { return; }
    if (td->scroll_row < 0) { td->scroll_row = 0; }
    i64 max_row = (i64)td->line_count - 1;
    if (max_row < 0) { max_row = 0; }
    if (td->scroll_row > max_row) { td->scroll_row = max_row; }
    // Clamp horizontal scrolling to the longest visible line. Without this,
    // arrow-right could walk the pointer past the NUL of a short line.
    i64 max_col = 0;
    u64* lines = td->lines;
    i64 r = 0;
    while (r < visible_rows) {
        i64 idx = td->scroll_row + r;
        if (idx >= 0 && idx < (i64)td->line_count) {
            i64 len = tui_textview_line_len(lines[idx]);
            if (len > max_col) { max_col = len; }
        }
        r = r + 1;
    }
    if (td->scroll_col < 0) { td->scroll_col = 0; }
    if (td->scroll_col > max_col) { td->scroll_col = max_col; }
    r = 0;
    while (r < visible_rows) {
        i64 line_idx = td->scroll_row + r;
        if (line_idx >= 0 && line_idx < (i64)td->line_count) {
            u64 line_ptr = lines[line_idx];
            u8* text = line_ptr;
            if (text != null) {
                u8 line_fg = fg;
                u8 line_bg = bg;
                u8 line_attr = attr;
                u8* span = text;
                if (text[0] == TUI_LINE_STYLE) {
                    line_fg = text[1];
                    line_bg = text[2];
                    line_attr = text[3];
                    span = text + 4;
                }
                i64 skip = td->scroll_col;
                i64 len = tui_textview_line_len(text);
                if (skip > len) { skip = len; }
                tui_buf_draw_span(buf, cols, w->x, w->y + r, span + skip,
                                  w->w, line_fg, line_bg, line_attr);
            }
        }
        r = r + 1;
    }
}

fn tui_draw_button(u64 buf, i64 cols, TuiWidget* w) {
    ButtonData* bd = w->data;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    if (bd->pressed == 1) {
        u8 tmp = fg;
        fg = bg;
        bg = tmp;
    }
    if (w->focused == 1) {
        attr = attr | TUI_ATTR_BOLD;
    }
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, w->x, w->y, w->w, 1, fill);
    if (bd->text != 0) {
        u64 text_len = strlen(bd->text);
        i64 text_x = w->x + (w->w - (i64)text_len) / 2;
        if (text_x < w->x) {
            text_x = w->x;
        }
        tui_buf_draw_str(buf, cols, text_x, w->y, bd->text, fg, bg, attr);
    }
}

fn tui_draw_widget(u64 buf, i64 cols, u64 widget_ptr) {
    if (widget_ptr == 0) {
        return;
    }
    TuiWidget* w = widget_ptr;
    if (w->visible == 0) {
        return;
    }
    if (w->widget_type == TUI_TYPE_PANEL) {
        tui_draw_panel(buf, cols, w);
    } else if (w->widget_type == TUI_TYPE_LABEL) {
        tui_draw_label(buf, cols, w);
    } else if (w->widget_type == TUI_TYPE_TEXTBOX) {
        tui_draw_textbox(buf, cols, w);
    } else if (w->widget_type == TUI_TYPE_LIST) {
        tui_draw_list(buf, cols, w);
    } else if (w->widget_type == TUI_TYPE_TEXTVIEW) {
        tui_draw_textview(buf, cols, w);
    } else if (w->widget_type == TUI_TYPE_BUTTON) {
        tui_draw_button(buf, cols, w);
    }
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        tui_draw_widget(buf, cols, children[i]);
        i = i + 1;
    }
    w->dirty = 0;
}

fn tui_apply_anchor(TuiWidget* w, i64 parent_x, i64 parent_y, i64 parent_w, i64 parent_h) {
    if (w->anchor.left == 1 && w->anchor.right == 1) {
        w->x = parent_x + w->anchor.left_off;
        w->w = parent_w - w->anchor.left_off - w->anchor.right_off;
        if (w->w < 1) {
            w->w = 1;
        }
    } else if (w->anchor.left == 1) {
        w->x = parent_x + w->anchor.left_off;
    } else if (w->anchor.right == 1) {
        w->x = parent_x + parent_w - w->anchor.right_off - w->w;
    }
    if (w->anchor.top == 1 && w->anchor.bottom == 1) {
        w->y = parent_y + w->anchor.top_off;
        w->h = parent_h - w->anchor.top_off - w->anchor.bottom_off;
        if (w->h < 1) {
            w->h = 1;
        }
    } else if (w->anchor.top == 1) {
        w->y = parent_y + w->anchor.top_off;
    } else if (w->anchor.bottom == 1) {
        w->y = parent_y + parent_h - w->anchor.bottom_off - w->h;
    }
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        TuiWidget* child = children[i];
        tui_apply_anchor(child, w->x, w->y, w->w, w->h);
        i = i + 1;
    }
}

fn tui_layout(TuiState* s) {
    if (s->root == 0) {
        return;
    }
    TuiWidget* root = s->root;
    root->x = 0;
    root->y = 0;
    root->w = s->cols;
    root->h = s->rows;
    u64* children = root->children;
    u64 i = 0;
    while (i < root->child_count) {
        TuiWidget* child = children[i];
        tui_apply_anchor(child, 0, 0, s->cols, s->rows);
        i = i + 1;
    }
}

fn tui_find_widget_at(u64 widget_ptr, i64 mx, i64 my) -> u64 {
    if (widget_ptr == 0) {
        return(0);
    }
    TuiWidget* w = widget_ptr;
    if (w->visible == 0) {
        return(0);
    }
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        u64 found = tui_find_widget_at(children[i], mx, my);
        if (found != 0) {
            return(found);
        }
        i = i + 1;
    }
    if (mx >= w->x && mx < w->x + w->w && my >= w->y && my < w->y + w->h) {
        return(widget_ptr);
    }
    return(0);
}

fn tui_dispatch_key(u64 widget_ptr, u64 key) -> u64 {
    if (widget_ptr == 0) {
        return(0);
    }
    TuiWidget* w = widget_ptr;
    if (w->widget_type == TUI_TYPE_TEXTBOX) {
        TextBoxData* td = w->data;
        if (key == keys:key_backspace || key == 127 || key == 8) {
            if (td->cursor_pos > 0) {
                u8* buf = td->buf;
                u64 i = td->cursor_pos - 1;
                while (i < td->buf_len) {
                    buf[i] = buf[i + 1];
                    i = i + 1;
                }
                td->buf_len = td->buf_len - 1;
                td->cursor_pos = td->cursor_pos - 1;
                if (td->cursor_pos < td->scroll_off) {
                    td->scroll_off = td->cursor_pos;
                }
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:key_delete || key == 1004) {
            if (td->cursor_pos < td->buf_len) {
                u8* buf = td->buf;
                u64 i = td->cursor_pos;
                while (i < td->buf_len) {
                    buf[i] = buf[i + 1];
                    i = i + 1;
                }
                td->buf_len = td->buf_len - 1;
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:arrow_left) {
            if (td->cursor_pos > 0) {
                td->cursor_pos = td->cursor_pos - 1;
                if (td->cursor_pos < td->scroll_off) {
                    td->scroll_off = td->cursor_pos;
                }
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:arrow_right) {
            if (td->cursor_pos < td->buf_len) {
                td->cursor_pos = td->cursor_pos + 1;
                u64 vis_w = (u64)w->w;
                if (td->cursor_pos >= td->scroll_off + vis_w) {
                    td->scroll_off = td->cursor_pos - vis_w + 1;
                }
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:key_home) {
            td->cursor_pos = 0;
            td->scroll_off = 0;
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:key_end) {
            td->cursor_pos = td->buf_len;
            u64 vis_w = (u64)w->w;
            if (td->cursor_pos >= td->scroll_off + vis_w) {
                td->scroll_off = td->cursor_pos - vis_w + 1;
            }
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key >= 32 && key < 127) {
            if (td->buf_len < td->max_len) {
                u8* buf = td->buf;
                u64 i = td->buf_len;
                while (i > td->cursor_pos) {
                    buf[i] = buf[i - 1];
                    i = i - 1;
                }
                buf[td->cursor_pos] = (u8)key;
                td->buf_len = td->buf_len + 1;
                buf[td->buf_len] = 0;
                td->cursor_pos = td->cursor_pos + 1;
                u64 vis_w = (u64)w->w;
                if (td->cursor_pos >= td->scroll_off + vis_w) {
                    td->scroll_off = td->cursor_pos - vis_w + 1;
                }
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        return(0);
    }
    if (w->widget_type == TUI_TYPE_LIST) {
        ListData* ld = w->data;
        if (key == keys:arrow_up) {
            if (ld->selected > 0) {
                ld->selected = ld->selected - 1;
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:arrow_down) {
            if (ld->selected < (i64)ld->item_count - 1) {
                ld->selected = ld->selected + 1;
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:page_up) {
            ld->selected = ld->selected - w->h;
            if (ld->selected < 0) {
                ld->selected = 0;
            }
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:page_down) {
            ld->selected = ld->selected + w->h;
            if (ld->selected >= (i64)ld->item_count) {
                ld->selected = (i64)ld->item_count - 1;
            }
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:key_home) {
            ld->selected = 0;
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:key_end) {
            ld->selected = (i64)ld->item_count - 1;
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        return(0);
    }
    if (w->widget_type == TUI_TYPE_TEXTVIEW) {
        TextViewData* td = w->data;
        if (key == keys:arrow_up) {
            if (td->scroll_row > 0) {
                td->scroll_row = td->scroll_row - 1;
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:arrow_down) {
            td->scroll_row = td->scroll_row + 1;
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:arrow_left) {
            if (td->scroll_col > 0) {
                td->scroll_col = td->scroll_col - 1;
                tui_widget_mark_dirty(widget_ptr);
            }
            return(1);
        }
        if (key == keys:arrow_right) {
            td->scroll_col = td->scroll_col + 1;
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:page_up) {
            td->scroll_row = td->scroll_row - w->h;
            if (td->scroll_row < 0) {
                td->scroll_row = 0;
            }
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        if (key == keys:page_down) {
            td->scroll_row = td->scroll_row + w->h;
            tui_widget_mark_dirty(widget_ptr);
            return(1);
        }
        return(0);
    }
    return(0);
}

fn tui_find_next_focus(u64 root, u64 current) -> u64 {
    if (root == 0) {
        return(0);
    }
    TuiWidget* w = root;
    if (w->focusable == 1 && root != current) {
        return(root);
    }
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        u64 found = tui_find_next_focus(children[i], current);
        if (found != 0) {
            return(found);
        }
        i = i + 1;
    }
    return(0);
}

fn tui_find_prev_focus(u64 root, u64 current) -> u64 {
    if (root == 0) {
        return(0);
    }
    u64 last = 0;
    TuiWidget* w = root;
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        u64 found = tui_find_prev_focus(children[i], current);
        if (found != 0) {
            if (found == current) {
                return(last);
            }
            last = found;
        }
        i = i + 1;
    }
    if (w->focusable == 1) {
        if (root == current) {
            return(last);
        }
        last = root;
    }
    return(last);
}

export fn tui_is_ready() -> u64 {
    TuiState* s = tui_get_state();
    if (s == null) { return(0); }
    return(s->initialized);
}

fn tui_init() {
    u64 state = tui_malloc(sizeof(TuiState));
    if (state == 0) {
        return;
    }
    TuiState* s = state;
    memset(s, 0, sizeof(TuiState));
    tui_g:state_ptr = state;
    i64 r = 24;
    i64 c = 80;
    tui_get_terminal_size(r*adr, c*adr);
    if (r < 1) {
        r = 24;
    }
    if (c < 1) {
        c = 80;
    }
    s->cols = c;
    s->rows = r;
    s->front_buf = tui_alloc_buf(c, r);
    s->back_buf = tui_alloc_buf(c, r);
    s->out_capacity = 8192;
    s->out_buf = tui_malloc(s->out_capacity);
    s->out_pos = 0;
    s->initialized = 1;
    s->exit_requested = 0;
    s->root = 0;
    s->focused = 0;
    s->widget_id_counter = 0;
    tui_set_raw_mode(s);
    // Disable autowrap so a write to the last cell of the last row can never
    // scroll the screen during a diff repaint. Restored on shutdown.
    tui_out_str(s, "\033[?7l");
    // Keep the alternate screen (the caller's scrollback survives), but never
    // capture the mouse: drag-selection must remain a terminal feature so the
    // user can copy text out of the session normally.
    tui_out_str(s, "\033[?1049h");
    // Multi-line paste must arrive as one event, not as a stream of Returns.
    tui_out_str(s, "\033[?2004h");
    tui_out_flush(s);
    tui_hide_cursor(s);
    tui_clear_screen(s);
}

fn tui_shutdown() {
    TuiState* s = tui_get_state();
    if (s == null) {
        return;
    }
    if (s->initialized == 0) {
        return;
    }
    tui_show_cursor(s);
    tui_disable_mouse(s);
    tui_out_str(s, "\033[?2004l");
    tui_out_str(s, "\033[2J\033[H\033[?1049l");
    tui_out_str(s, "\033[?7h");
    tui_restore_term(s);
    tui_out_flush(s);
    if (s->front_buf != 0) {
        tui_free_buf(s->front_buf);
    }
    if (s->back_buf != 0) {
        tui_free_buf(s->back_buf);
    }
    if (s->out_buf != 0) {
        tui_free(s->out_buf);
    }
    s->initialized = 0;
}

fn tui_request_exit() {
    TuiState* s = tui_get_state();
    if (s != null) {
        s->exit_requested = 1;
    }
}

fn tui_handle_resize(TuiState* s) {
    i64 r = 24;
    i64 c = 80;
    tui_get_terminal_size(r*adr, c*adr);
    if (r == 0 || c == 0) {
        return;
    }
    if (r == s->rows && c == s->cols) {
        return;
    }
    // Allocate before freeing: an allocation failure must leave the old
    // buffers and dimensions intact instead of drawing into a null buffer.
    u64 new_front = tui_alloc_buf(c, r);
    u64 new_back = tui_alloc_buf(c, r);
    if (new_front == 0 || new_back == 0) {
        if (new_front != 0) { tui_free_buf(new_front); }
        if (new_back != 0) { tui_free_buf(new_back); }
        return;
    }
    if (s->front_buf != 0) { tui_free_buf(s->front_buf); }
    if (s->back_buf != 0) { tui_free_buf(s->back_buf); }
    s->front_buf = new_front;
    s->back_buf = new_back;
    s->cols = c;
    s->rows = r;
    // render_frame may apply the resize before poll_event sees it. Keep an
    // explicit pending flag so the application still receives one RESIZE event.
    s->resize_pending = 1;
    tui_clear_screen(s);
    if (s->root != 0) {
        TuiWidget* root = s->root;
        root->dirty = 1;
    }
}

fn tui_poll_event(u64 out_type*o, u64 out_a*o, u64 out_b*o, u64 out_c*o) -> u64 {
    TuiState* s = tui_get_state();
    if (s == null || s->initialized == 0) {
        out_type = TUI_EVT_NONE;
        return(0);
    }
    tui_handle_resize(s);
    if (s->resize_pending != 0) {
        s->resize_pending = 0;
        out_type = TUI_EVT_RESIZE;
        out_a = (u64)s->cols;
        out_b = (u64)s->rows;
        out_c = 0;
        return(1);
    }
    return(tui_parse_input(s, out_type, out_a, out_b, out_c));
}

// Feed a bracketed paste to the focused widget. Newlines and tabs become
// spaces so a single-line textbox cannot be derailed by a multi-line paste.
fn tui_dispatch_paste(u64 widget_ptr) {
    if (widget_ptr == 0) { return; }
    TuiState* s = tui_get_state();
    if (s == null) { return; }
    u8* text = s->paste_buf;
    u64 i = 0;
    while (i < s->paste_len) {
        u64 ch = text[i];
        if (ch == 10 || ch == 13 || ch == 9) { ch = 32; }
        if (ch >= 32 && ch < 127) {
            tui_dispatch_key(widget_ptr, ch);
        }
        i = i + 1;
    }
}

fn tui_process_event_impl(TuiState* s, u64 evt_type, u64 evt_a, u64 evt_b, u64 evt_c) {
    if (evt_type == TUI_EVT_PASTE) {
        if (s->focused != 0) { tui_dispatch_paste(s->focused); }
        return;
    }
    if (evt_type == TUI_EVT_KEY) {
        if (evt_a == 9) {
            u64 next = tui_find_next_focus(s->root, s->focused);
            if (next != 0) {
                tui_focus_widget(next);
            }
            return;
        }
        if (s->focused != 0) {
            tui_dispatch_key(s->focused, evt_a);
        }
        return;
    }
    if (evt_type == TUI_EVT_MOUSE_CLICK) {
        u64 btn = evt_a;
        i64 mx = (i64)evt_b;
        i64 my = (i64)evt_c;
        s->mouse.x = mx;
        s->mouse.y = my;
        if (btn == TUI_MOUSE_LEFT) {
            s->mouse.left_down = 1;
        }
        if (btn == TUI_MOUSE_MIDDLE) {
            s->mouse.middle_down = 1;
        }
        if (btn == TUI_MOUSE_RIGHT) {
            s->mouse.right_down = 1;
        }
        u64 clicked = tui_find_widget_at(s->root, mx, my);
        if (clicked != 0) {
            TuiWidget* cw = clicked;
            if (cw->focusable == 1) {
                tui_focus_widget(clicked);
            }
            if (cw->widget_type == TUI_TYPE_LIST) {
                ListData* ld = cw->data;
                i64 rel_y = my - cw->y;
                i64 item_idx = ld->scroll_off + rel_y;
                if (item_idx >= 0 && item_idx < (i64)ld->item_count) {
                    ld->selected = item_idx;
                    tui_widget_mark_dirty(clicked);
                }
            }
            if (cw->widget_type == TUI_TYPE_BUTTON) {
                ButtonData* bd = cw->data;
                bd->pressed = 1;
                tui_widget_mark_dirty(clicked);
            }
        }
        return;
    }
    if (evt_type == TUI_EVT_MOUSE_RELEASE) {
        u64 btn = evt_a;
        if (btn == TUI_MOUSE_LEFT) {
            s->mouse.left_down = 0;
        }
        if (btn == TUI_MOUSE_MIDDLE) {
            s->mouse.middle_down = 0;
        }
        if (btn == TUI_MOUSE_RIGHT) {
            s->mouse.right_down = 0;
        }
        u64 clicked = tui_find_widget_at(s->root, (i64)evt_b, (i64)evt_c);
        if (clicked != 0) {
            TuiWidget* cw = clicked;
            if (cw->widget_type == TUI_TYPE_BUTTON) {
                ButtonData* bd = cw->data;
                if (bd->pressed == 1) {
                    bd->pressed = 0;
                    tui_widget_mark_dirty(clicked);
                }
            }
        }
        return;
    }
    if (evt_type == TUI_EVT_MOUSE_WHEEL) {
        u64 btn = evt_a;
        i64 mx = (i64)evt_b;
        i64 my = (i64)evt_c;
        u64 target = tui_find_widget_at(s->root, mx, my);
        if (target != 0) {
            TuiWidget* tw = target;
            if (tw->widget_type == TUI_TYPE_LIST) {
                ListData* ld = tw->data;
                if (btn == TUI_MOUSE_WHEEL_UP) {
                    if (ld->selected > 0) {
                        ld->selected = ld->selected - 1;
                        tui_widget_mark_dirty(target);
                    }
                }
                if (btn == TUI_MOUSE_WHEEL_DOWN) {
                    if (ld->selected < (i64)ld->item_count - 1) {
                        ld->selected = ld->selected + 1;
                        tui_widget_mark_dirty(target);
                    }
                }
            }
            if (tw->widget_type == TUI_TYPE_TEXTVIEW) {
                TextViewData* td = tw->data;
                if (btn == TUI_MOUSE_WHEEL_UP) {
                    if (td->scroll_row > 0) {
                        td->scroll_row = td->scroll_row - 3;
                        if (td->scroll_row < 0) {
                            td->scroll_row = 0;
                        }
                        tui_widget_mark_dirty(target);
                    }
                }
                if (btn == TUI_MOUSE_WHEEL_DOWN) {
                    td->scroll_row = td->scroll_row + 3;
                    tui_widget_mark_dirty(target);
                }
            }
        }
        return;
    }
    if (evt_type == TUI_EVT_MOUSE_MOVE) {
        s->mouse.x = (i64)evt_b;
        s->mouse.y = (i64)evt_c;
        return;
    }
}

// Dispatch to widgets using the current TUI state. Applications that own the
// event loop can call this after tui_poll_event().
export fn tui_process_event(u64 evt_type, u64 evt_a, u64 evt_b, u64 evt_c) {
    TuiState* s = tui_get_state();
    if (s == null || s->initialized == 0) { return; }
    tui_process_event_impl(s, evt_type, evt_a, evt_b, evt_c);
}

// Layout applies anchors first, then reflows textviews whose width changed.
// This keeps source records authoritative without making the app handle every
// resize itself.
fn tui_reflow_widget(u64 widget_ptr) {
    if (widget_ptr == 0) { return; }
    TuiWidget* w = widget_ptr;
    if (w->widget_type == TUI_TYPE_TEXTVIEW && w->w > 0) {
        TextViewData* td = w->data;
        if (td->wrap_width != w->w) {
            tui_textview_rewrap_impl(widget_ptr, w->w, 1);
        }
    }
    u64* children = w->children;
    u64 i = 0;
    while (i < w->child_count) {
        tui_reflow_widget(children[i]);
        i = i + 1;
    }
}

fn tui_render(TuiState* s) {
    tui_layout(s);
    if (s->root != 0) { tui_reflow_widget(s->root); }
    u32 default_cell = tui_cell_pack(32, TUI_CLR_WHITE, TUI_CLR_BLACK, 0);
    u64 total = (u64)(s->cols * s->rows);
    u32* front = s->front_buf;
    u64 i = 0;
    while (i < total) {
        front[i] = default_cell;
        i = i + 1;
    }
    if (s->root != 0) {
        tui_draw_widget(s->front_buf, s->cols, s->root);
    }
    tui_out_reset(s);
    tui_render_diff(s);
    tui_out_flush(s);
}

fn tui_run(u64 root) {
    TuiState* s = tui_get_state();
    if (s == null || s->initialized == 0) {
        return;
    }
    s->root = root;
    if (root != 0) {
        TuiWidget* rw = root;
        rw->x = 0;
        rw->y = 0;
        rw->w = s->cols;
        rw->h = s->rows;
        tui_focus_widget(tui_find_next_focus(root, 0));
    }
    tui_render(s);
    while (s->exit_requested == 0) {
        u64 evt_type = 0;
        u64 evt_a = 0;
        u64 evt_b = 0;
        u64 evt_c = 0;
        u64 got = tui_poll_event(evt_type*adr, evt_a*adr, evt_b*adr, evt_c*adr);
        if (got == 1) {
            tui_process_event_impl(s, evt_type, evt_a, evt_b, evt_c);
            tui_render(s);
        } else {
            sys_nanosleep(null, null);
        }
    }
}

// Append a record to a textview, preserving every line and reflowing long
// lines at the widget width. Blank lines are kept; nothing is truncated.
fn tui_append_wrapped_impl(u64 widget, u8* text, i64 width, u8 fg, u8 bg, u8 attr) {
    if (widget == 0 || text == null) { return; }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) { return; }
    TextViewData* td = w->data;
    u64 src = tui_textview_add_source(td, 0, "", text, fg, bg, attr, fg, attr);
    tui_textview_wrap_source(td, src, width);
    td->wrap_width = width;
    tui_widget_mark_dirty(widget);
}

export fn tui_textview_append_wrapped(u64 widget, u8* text, i64 width) {
    if (widget == 0) { return; }
    TuiWidget* w = widget;
    tui_append_wrapped_impl(widget, text, width, w->fg, w->bg, w->attr);
}

export fn tui_textview_append_wrapped_styled(u64 widget, u8* text, i64 width, u8 fg, u8 bg, u8 attr) {
    tui_append_wrapped_impl(widget, text, width, fg, bg, attr);
}

// A tagged block: the tag gets its own accent line, the body is indented under
// it. Used for user / assistant / tool message groups.
export fn tui_textview_append_block(u64 widget, u8* tag, u8* text, i64 width,
                                    u8 tag_fg, u8 body_fg, u8 bg,
                                    u8 tag_attr, u8 body_attr) {
    if (widget == 0 || text == null) { return; }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) { return; }
    TextViewData* td = w->data;
    u64 src = tui_textview_add_source(td, 1, tag, text, body_fg, bg, body_attr,
                                      tag_fg, tag_attr);
    tui_textview_wrap_source(td, src, width);
    td->wrap_width = width;
    tui_widget_mark_dirty(widget);
}

// Reflow every stored record. Scroll position is preserved unless the view
// was already pinned to the bottom, in which case it stays pinned there.
fn tui_textview_rewrap_impl(u64 widget, i64 width, u8 preserve_scroll) {
    if (widget == 0) { return; }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) { return; }
    TextViewData* td = w->data;
    i64 old_scroll = td->scroll_row;
    i64 old_col = td->scroll_col;
    i64 old_count = td->line_count;
    i64 view_h = w->h;
    u8 at_bottom = 0;
    if (old_count <= view_h || old_scroll + view_h >= old_count) {
        at_bottom = 1;
    }
    tui_textview_clear_display(td);
    if (width < 8) { width = 20; }
    tui_textview_rewrap_all(td, width);
    td->wrap_width = width;
    if (preserve_scroll == 1) {
        if (at_bottom == 1) {
            td->scroll_row = td->line_count - view_h;
            if (td->scroll_row < 0) { td->scroll_row = 0; }
        } else {
            td->scroll_row = old_scroll;
            i64 max_top = td->line_count - view_h;
            if (max_top < 0) { max_top = 0; }
            if (td->scroll_row > max_top) { td->scroll_row = max_top; }
        }
        td->scroll_col = old_col;
    }
    tui_widget_mark_dirty(widget);
}

// Reflow every stored record after a terminal resize.
export fn tui_textview_rewrap(u64 widget, i64 width) {
    tui_textview_rewrap_impl(widget, width, 1);
}

// Write bytes verbatim, except CRLF is emitted as a single newline so a dump
// does not grow blank lines when the source used Windows line endings.
fn tui_write_transcript(u8* text, u64 len) {
    u64 start = 0;
    u64 i = 0;
    while (i < len) {
        if (text[i] == 13 && i + 1 < len && text[i + 1] == 10) {
            u64 n = i - start;
            if (n > 0) { sys_write(1, text + start, n); }
            start = i + 1;
        }
        i = i + 1;
    }
    u64 tail = len - start;
    if (tail > 0) { sys_write(1, text + start, tail); }
}

// Write the original (unwrapped) transcript to stdout after the terminal has
// been restored, so the whole session is available in normal scrollback for
// copy/paste. UTF-8 and CRLF in the source records are preserved. No ANSI
// styling is emitted here.
export fn tui_textview_dump(u64 widget) {
    if (widget == 0) { return; }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) { return; }
    TextViewData* td = w->data;
    u64* arr = td->sources;
    u64 i = 0;
    while (i < td->source_count) {
        u8* base = arr[i];
        TuiSource* src = arr[i];
        u64 tag_len = (u64)src->tag_len;
        u8* raw = base + sizeof(TuiSource) + tag_len + 1 + src->folded_len + 1;
        if (src->kind == 1 && tag_len > 0) {
            tui_write_transcript(base + sizeof(TuiSource), tag_len);
            u8 colon[2];
            colon[0] = 58;
            colon[1] = 32;
            sys_write(1, colon*adr, 2);
        }
        tui_write_transcript(raw, src->raw_len);
        u8 nl = 10;
        sys_write(1, nl*adr, 1);
        i = i + 1;
    }
}

// Points to an internal buffer owned by the TUI. It stays valid until the next
// paste. The length is passed as out_a of TUI_EVT_PASTE; out_b reports whether
// the paste exceeded the buffer and was truncated.
export fn tui_get_paste_text() -> u8* {
    TuiState* s = tui_get_state();
    if (s == null) { return(null); }
    return(s->paste_buf);
}

export fn tui_textview_scroll_bottom(u64 widget) {
    if (widget == 0) { return; }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) { return; }
    TextViewData* td = w->data;
    i64 top = (i64)td->line_count - w->h;
    if (top < 0) { top = 0; }
    td->scroll_row = top;
    tui_widget_mark_dirty(widget);
}

// Allows a synchronous application to own the event loop instead of calling
// tui_run(). Widgets still receive the exact same key events.
export fn tui_set_root(u64 root) {
    TuiState* s = tui_get_state();
    if (s == null) { return; }
    s->root = root;
    if (root != 0) {
        TuiWidget* rw = root;
        rw->x = 0;
        rw->y = 0;
        rw->w = s->cols;
        rw->h = s->rows;
        tui_focus_widget(tui_find_next_focus(root, 0));
    }
    tui_layout(s);
    tui_widget_mark_dirty(root);
}

export fn tui_send_key(u64 widget, u64 key) {
    if (widget == 0) { return; }
    tui_dispatch_key(widget, key);
}

export fn tui_render_frame() {
    TuiState* s = tui_get_state();
    if (s == null) {
        return;
    }
    if (s->initialized == 0) {
        return;
    }
    tui_handle_resize(s);
    tui_render(s);
}
