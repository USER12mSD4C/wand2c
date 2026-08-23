sc.true
#import <syscall>
#import <io>
#import <mem>
#import <string>

sect.tui_state
    u8* screen_buf = null;
    u8* prev_buf = null;
    u64 cols = 80;
    u64 rows = 24;
EOS

fn get_terminal_size(u64* out_rows*o, u64* out_cols*o) {
    u8[8] ws_buf;
    sys_ioctl(1, 21523, ws_buf*adr);

    // Считываем u16 ws_row и ws_col из буфера
    u16* p_row*i = ws_buf*adr;
    u16 r = p_row;

    u16* p_col*i = (ws_buf*adr) + 2;
    u16 c = p_col;

    out_rows = r;
    out_cols = c;
}

fn tui_clear_physical() {
    print_char(27);
    print_string("[2J");
    print_char(27);
    print_string("[H");
}

fn tui_move_cursor_physical(u64 row, u64 col) {
    print_char(27);
    print_string("[");
    print_number(row);
    print_char(59); // ';'
    print_number(col);
    print_string("H");
}

fn tui_init() {
    u64 r = 24;
    u64 c = 80;
    get_terminal_size(r*adr, c*adr);

    tui_state:rows = r;
    tui_state:cols = c;
    u64 total_cells = r * c;

    tui_state:screen_buf = malloc(total_cells);
    tui_state:prev_buf = malloc(total_cells);

    memset(tui_state:screen_buf, 32, total_cells);
    memset(tui_state:prev_buf, 32, total_cells);

    tui_clear_physical();
}

fn tui_clear() {
    u64 total_cells = tui_state:rows * tui_state:cols;
    memset(tui_state:screen_buf, 32, total_cells);
}

fn tui_draw_char(u64 r, u64 c, u8 ch) {
    if (r < tui_state:rows) {
        if (c < tui_state:cols) {
            u64 idx = (r * tui_state:cols) + c;
            u8* buf = tui_state:screen_buf;
            buf[idx] = ch;
        }
    }
}

fn tui_draw_string(u64 r, u64 c, u8* s) {
    u64 len = strlen(s);
    u64 i = 0;
    while (i < len) {
        u8* p*i = s + i;
        u8 ch = p;
        tui_draw_char(r, c + i, ch);
        i = i + 1;
    }
}

fn tui_set_cursor(u64 r, u64 c) {
    tui_move_cursor_physical(r + 1, c + 1);
}

fn tui_render() {
    u64 r = 0;
    u64 max_rows = tui_state:rows;
    u64 max_cols = tui_state:cols;
    u8* screen = tui_state:screen_buf;
    u8* prev = tui_state:prev_buf;

    u64 last_cursor_r = 99999;
    u64 last_cursor_c = 99999;

    while (r < max_rows) {
        u64 c = 0;
        while (c < max_cols) {
            u64 idx = (r * max_cols) + c;
            u8 screen_ch = screen[idx];
            u8 prev_ch = prev[idx];

            if (screen_ch != prev_ch) {
                prev[idx] = screen_ch;

                if (r != last_cursor_r) {
                    tui_move_cursor_physical(r + 1, c + 1);
                    last_cursor_r = r;
                    last_cursor_c = c;
                } else {
                    if (c != last_cursor_c) {
                        tui_move_cursor_physical(r + 1, c + 1);
                        last_cursor_r = r;
                        last_cursor_c = c;
                    }
                }

                print_char(screen_ch);

                last_cursor_c = last_cursor_c + 1;
                if (last_cursor_c == max_cols) {
                    last_cursor_r = 99999;
                }
            }
            c = c + 1;
        }
        r = r + 1;
    }
}
