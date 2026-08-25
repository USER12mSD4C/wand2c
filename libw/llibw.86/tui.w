sc.true
#import <syscall>
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
    i64 scroll_row;
    i64 scroll_col;
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
    u64 resize_pending;
    u64 widget_id_counter;
}

sect.tui_g
    u64 state_ptr = 0;
EOS

fn tui_malloc(u64 size) -> u64 {
    u8* p = mloc(0, size);
    return(u64 p);
}

fn tui_free(u64 ptr) {
    if (ptr != 0) {
        u8* p = ptr;
        mfree(p);
    }
}

fn tui_realloc(u64 old_ptr, u64 old_size, u64 new_size) -> u64 {
    u64 new_ptr = tui_malloc(new_size);
    if (new_ptr == 0) {
        return(0);
    }
    if (old_ptr != 0 && old_size > 0) {
        u8* src = old_ptr;
        u8* dst = new_ptr;
        u64 copy_size = old_size;
        if (copy_size > new_size) {
            copy_size = new_size;
        }
        memcpy(dst, src, copy_size);
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
    sys_ioctl(1, 21523, ws_buf*adr);
    u16* p_row*i = ws_buf*adr;
    u16 r = p_row;
    u16* p_col*i = (ws_buf*adr) + 2;
    u16 c = p_col;
    out_rows = (i64)r;
    out_cols = (i64)c;
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

fn tui_buf_draw_str(u64 buf, i64 cols, i64 x, i64 y, u8* str, u8 fg, u8 bg, u8 attr) {
    u8* p*i = str;
    i64 cx = x;
    u8 c = p;
    while (c != 0) {
        tui_buf_draw_char(buf, cols, cx, y, c, fg, bg, attr);
        cx = cx + 1;
        p = p + 1;
        c = p;
    }
}

fn tui_emit_cursor(TuiState* s, i64 row, i64 col) {
    tui_out_str(s, "\x1b[");
    tui_out_num(s, row + 1);
    tui_out_byte(s, 59);
    tui_out_num(s, col + 1);
    tui_out_byte(s, 72);
}

fn tui_emit_color(TuiState* s, u8 fg, u8 bg, u8 attr) {
    tui_out_str(s, "\x1b[0");
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
    sys_ioctl(0, 21523, s->termios_backup*adr);
    u8 tc_buf[60];
    sys_ioctl(0, 21523, tc_buf*adr);
    u32* lflag_ptr = tc_buf*adr + 12;
    u32 lflag = lflag_ptr[0];
    lflag = lflag & 0xFFFFFFF0;
    lflag = lflag & 0xFFFFFBFF;
    lflag_ptr[0] = lflag;
    u8* cc_ptr = tc_buf*adr + 20;
    cc_ptr[6] = 1;
    cc_ptr[5] = 0;
    sys_ioctl(0, 21524, tc_buf*adr);
    s->raw_term_set = 1;
}

fn tui_restore_term(TuiState* s) {
    if (s->raw_term_set != 0) {
        sys_ioctl(0, 21524, s->termios_backup*adr);
        s->raw_term_set = 0;
    }
}

fn tui_enable_mouse(TuiState* s) {
    if (s->mouse_enabled != 0) {
        return;
    }
    tui_out_str(s, "\x1b[?1000h");
    tui_out_str(s, "\x1b[?1002h");
    tui_out_str(s, "\x1b[?1006h");
    tui_out_flush(s);
    s->mouse_enabled = 1;
}

fn tui_disable_mouse(TuiState* s) {
    if (s->mouse_enabled == 0) {
        return;
    }
    tui_out_str(s, "\x1b[?1000l");
    tui_out_str(s, "\x1b[?1002l");
    tui_out_str(s, "\x1b[?1006l");
    tui_out_flush(s);
    s->mouse_enabled = 0;
}

fn tui_hide_cursor(TuiState* s) {
    tui_out_str(s, "\x1b[?25l");
    tui_out_flush(s);
}

fn tui_show_cursor(TuiState* s) {
    tui_out_str(s, "\x1b[?25h");
    tui_out_flush(s);
}

fn tui_clear_screen(TuiState* s) {
    tui_out_str(s, "\x1b[2J\x1b[H");
    tui_out_flush(s);
}

fn tui_install_sigwinch() {
    SignalHandler handler;
    signal_init_handler(handler*adr);
    signal_install(handler*adr, 28);
}

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

fn tui_parse_input(TuiState* s, u64* out_type*o, u64* out_a*o, u64* out_b*o, u64* out_c*o) -> u64 {
    i64 b = tui_read_byte();
    if (b < 0) {
        out_type = TUI_EVT_NONE;
        return(0);
    }
    if (b == 27) {
        i64 b2 = tui_read_byte_timeout();
        if (b2 < 0) {
            out_type = TUI_EVT_KEY;
            out_a = 27;
            return(1);
        }
        if (b2 == 91) {
            i64 b3 = tui_read_byte_timeout();
            if (b3 < 0) {
                out_type = TUI_EVT_KEY;
                out_a = 27;
                return(1);
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

fn tui_textview_set_text(u64 widget, u8* text) {
    if (widget == 0) {
        return;
    }
    TuiWidget* w = widget;
    if (w->widget_type != TUI_TYPE_TEXTVIEW) {
        return;
    }
    TextViewData* td = w->data;
    u64* arr = td->lines;
    u64 i = 0;
    while (i < td->line_count) {
        if (arr[i] != 0) {
            tui_free(arr[i]);
        }
        i = i + 1;
    }
    td->line_count = 0;
    td->scroll_row = 0;
    u8* p*i = text;
    u64 line_start = 0;
    u64 pos = 0;
    u8 c = p;
    while (c != 0) {
        if (c == 10) {
            u64 line_len = pos - line_start;
            u64 line_buf = tui_malloc(line_len + 1);
            if (line_buf != 0) {
                u8* dst = line_buf;
                u8* src = text + line_start;
                u64 k = 0;
                while (k < line_len) {
                    dst[k] = src[k];
                    k = k + 1;
                }
                dst[line_len] = 0;
            }
            u64* lines_arr = td->lines;
            if (td->line_count < td->line_capacity) {
                lines_arr[td->line_count] = line_buf;
                td->line_count = td->line_count + 1;
            } else {
                tui_textview_add_line(td, text + line_start);
            }
            line_start = pos + 1;
        }
        pos = pos + 1;
        p = p + 1;
        c = p;
    }
    if (pos > line_start) {
        tui_textview_add_line(td, text + line_start);
    }
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
    tui_textview_add_line(td, line);
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
    u64* arr = td->lines;
    u64 i = 0;
    while (i < td->line_count) {
        if (arr[i] != 0) {
            tui_free(arr[i]);
        }
        i = i + 1;
    }
    td->line_count = 0;
    td->scroll_row = 0;
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
    if (style == TUI_BORDER_DOUBLE) {
        out[0] = 201; out[1] = 187; out[2] = 200; out[3] = 188;
        out[4] = 205; out[5] = 205; out[6] = 186; out[7] = 186;
        return;
    }
    if (style == TUI_BORDER_ROUNDED) {
        out[0] = 218; out[1] = 191; out[2] = 192; out[3] = 217;
        out[4] = 196; out[5] = 196; out[6] = 179; out[7] = 179;
        return;
    }
    if (style == TUI_BORDER_HEAVY) {
        out[0] = 218; out[1] = 191; out[2] = 192; out[3] = 217;
        out[4] = 196; out[5] = 196; out[6] = 179; out[7] = 179;
        return;
    }
    out[0] = 218; out[1] = 191; out[2] = 192; out[3] = 217;
    out[4] = 196; out[5] = 196; out[6] = 179; out[7] = 179;
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
    u64 i = 0;
    while (i < vis_w && (scroll + i) < td->buf_len) {
        u8 ch = text[scroll + i];
        tui_buf_draw_char(buf, cols, w->x + (i64)i, w->y, ch, fg, bg, attr);
        i = i + 1;
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

fn tui_draw_textview(u64 buf, i64 cols, TuiWidget* w) {
    TextViewData* td = w->data;
    u8 fg = w->fg;
    u8 bg = w->bg;
    u8 attr = w->attr;
    u32 fill = tui_cell_pack(32, fg, bg, attr);
    tui_buf_fill(buf, cols, w->x, w->y, w->w, w->h, fill);
    i64 visible_rows = w->h;
    i64 r = 0;
    while (r < visible_rows) {
        i64 line_idx = td->scroll_row + r;
        if (line_idx >= 0 && line_idx < (i64)td->line_count) {
            u64* lines = td->lines;
            u64 line_ptr = lines[line_idx];
            u8* text = line_ptr;
            if (text != null) {
                u8* p*i = text + td->scroll_col;
                i64 c = 0;
                u8 ch = p;
                while (ch != 0 && c < w->w) {
                    tui_buf_draw_char(buf, cols, w->x + c, w->y + r, ch, fg, bg, attr);
                    c = c + 1;
                    p = p + 1;
                    ch = p;
                }
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
    tui_enable_mouse(s);
    tui_hide_cursor(s);
    tui_clear_screen(s);
    tui_install_sigwinch();
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
    tui_restore_term(s);
    tui_out_str(s, "\x1b[2J\x1b[H");
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
    if (r < 1) {
        r = 24;
    }
    if (c < 1) {
        c = 80;
    }
    if (r == s->rows && c == s->cols) {
        return;
    }
    s->cols = c;
    s->rows = r;
    if (s->front_buf != 0) {
        tui_free_buf(s->front_buf);
    }
    if (s->back_buf != 0) {
        tui_free_buf(s->back_buf);
    }
    s->front_buf = tui_alloc_buf(c, r);
    s->back_buf = tui_alloc_buf(c, r);
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
    if (s->resize_pending != 0) {
        s->resize_pending = 0;
        tui_handle_resize(s);
        out_type = TUI_EVT_RESIZE;
        out_a = (u64)s->cols;
        out_b = (u64)s->rows;
        out_c = 0;
        return(1);
    }
    return(tui_parse_input(s, out_type, out_a, out_b, out_c));
}

fn tui_process_event(TuiState* s, u64 evt_type, u64 evt_a, u64 evt_b, u64 evt_c) {
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

fn tui_render(TuiState* s) {
    tui_layout(s);
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
            tui_process_event(s, evt_type, evt_a, evt_b, evt_c);
            tui_render(s);
        } else {
            sys_nanosleep(null, null);
        }
    }
}

export fn tui_render_frame() {
    TuiState* s = tui_get_state();
    if (s == null) {
        return;
    }
    if (s->initialized == 0) {
        return;
    }
    tui_render(s);
}
