sc.true

#import <syscall>
#import <string>
#import <io>

fn is_leap(u64 y) -> u64 {
    if (y % 4 != 0) { return(0); }
    if (y % 100 != 0) { return(1); }
    if (y % 400 == 0) { return(1); }
    return(0);
}

fn days_in_month(u64 y, u64 m) -> u64 {
    if (m == 1 || m == 3 || m == 5 || m == 7 || m == 8 || m == 10 || m == 12) { return(31); }
    if (m == 4 || m == 6 || m == 9 || m == 11) { return(30); }
    if (is_leap(y) == 1) { return(29); }
    return(28);
}

fn gmtime_sec(u64 t, u64* year*o, u64* mon*o, u64* day*o, u64* hour*o, u64* min*o, u64* sec*o) {
    sec = t % 60;
    u64 tm = t / 60;
    min = tm % 60;
    u64 th = tm / 60;
    hour = th % 24;
    u64 days = th / 24; // days since 1970-01-01

    u64 y = 1970;
    while (1) {
        u64 diy = 365;
        if (is_leap(y) == 1) { diy = 366; }
        if (days >= diy) { days = days - diy; y = y + 1; }
        else { break; }
    }
    u64 m = 1;
    while (m < 12) {
        u64 dim = days_in_month(y, m);
        if (days >= dim) { days = days - dim; m = m + 1; }
        else { break; }
    }
    year = y;
    mon = m;
    day = days + 1;
}

fn fmt2(u64 v, u8* out) {
    out[0] = (u8)(48 + v / 10);
    out[1] = (u8)(48 + v % 10);
    out[2] = 0;
}

fn fmt_ls_time(u64 t, u8* out) {
    // "MMM DD HH:MM" or "MMM DD  YYYY" - use UTC for now; better than raw epoch
    u64 y = 0; u64 mo = 0; u64 d = 0; u64 h = 0; u64 mi = 0; u64 s = 0;
    gmtime_sec(t, y*adr, mo*adr, d*adr, h*adr, mi*adr, s*adr);
    u8* months = "JanFebMarAprMayJunJulAugSepOctNovDec";
    u64 mi3 = (mo - 1) * 3;
    out[0] = months[mi3];
    out[1] = months[mi3 + 1];
    out[2] = months[mi3 + 2];
    out[3] = 32;
    out[4] = (u8)(48 + d / 10);
    out[5] = (u8)(48 + d % 10);
    out[6] = 32;
    // if within 6 months, show HH:MM else year
    u64 now_s[2];
    sys_gettimeofday(now_s*adr, null);
    u64 now = now_s[0];
    u64 diff = 0;
    if (now > t) { diff = now - t; } else { diff = t - now; }
    if (diff > 15552000) { // ~180 days
        out[7] = 32;
        u8 yb[8];
        utoa(y, yb*adr);
        u64 yl = strlen(yb*adr);
        // right align in 5 chars
        u64 pad = 0;
        if (yl < 5) { pad = 5 - yl; }
        u64 k = 0;
        while (k < pad) { out[7 + k] = 32; k = k + 1; }
        memcpy(out + 7 + pad, yb*adr, yl);
        out[7 + pad + yl] = 0;
    } else {
        out[7] = (u8)(48 + h / 10);
        out[8] = (u8)(48 + h % 10);
        out[9] = 58;
        out[10] = (u8)(48 + mi / 10);
        out[11] = (u8)(48 + mi % 10);
        out[12] = 0;
    }
}
