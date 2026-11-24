sc.true
#import <syscall>

fn strlen(u8* s) -> u64 {
    u64 len = 0;
    u8* p = s;
    while (p != null) {
        u8 c = p[0];
        if (c == 0) {
            return(len);
        }
        len = len + 1;
        p = p + 1;
    }
    return(len);
}

fn strcmp(u8* s1, u8* s2) -> i64 {
    u64 i = 0;
    while (1) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        if (c1 == 0) {
            return(0);
        }
        i = i + 1;
    }
    return(0);
}

fn strncmp(u8* s1, u8* s2, u64 n) -> i64 {
    u64 i = 0;
    while (i < n) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        if (c1 == 0) {
            return(0);
        }
        i = i + 1;
    }
    return(0);
}

fn strcpy(u8* dest, u8* src) -> u8* {
    u8* original_dest = dest;
    u64 i = 0;
    while (1) {
        u8 c = src[i];
        dest[i] = c;
        if (c == 0) {
            return(original_dest);
        }
        i = i + 1;
    }
    return(original_dest);
}

fn strcat(u8* dest, u8* src) -> u8* {
    u8* original_dest = dest;
    u64 dest_len = strlen(dest);
    u64 i = 0;
    while (1) {
        u8 c = src[i];
        dest[dest_len + i] = c;
        if (c == 0) {
            return(original_dest);
        }
        i = i + 1;
    }
    return(original_dest);
}

fn memcpy(u8* dest, u8* src, u64 n) -> u8* {
    u64 d = (u64)dest;
    u64 s = (u64)src;
    u64 overlap = 0;
    if (d < s) {
        if (d + n > s) {
            overlap = 1;
        }
    } else {
        if (s + n > d) {
            overlap = 1;
        }
    }
    if (overlap == 1) {
        u64 i = 0;
        while (i < n) {
            dest[i] = src[i];
            i = i + 1;
        }
        return(dest);
    }
    u64 words = n / 8;
    u64* dp = (u64*)dest;
    u64* sp = (u64*)src;
    u64 w = 0;
    while (w < words) {
        dp[w] = sp[w];
        w = w + 1;
    }
    u64 tail = words * 8;
    while (tail < n) {
        dest[tail] = src[tail];
        tail = tail + 1;
    }
    return(dest);
}

fn memset(u8* s, u8 c, u64 n) -> u8* {
    u64 v = (u64)c;
    v = v | (v << 8);
    v = v | (v << 16);
    v = v | (v << 32);
    u64 words = n / 8;
    u64* p = (u64*)s;
    u64 w = 0;
    while (w < words) {
        p[w] = v;
        w = w + 1;
    }
    u64 tail = words * 8;
    while (tail < n) {
        s[tail] = c;
        tail = tail + 1;
    }
    return(s);
}

fn memmove(u8* dest, u8* src, u64 n) -> u8* {
    u64 d = (u64)dest;
    u64 s = (u64)src;
    if (d < s) {
        return(memcpy(dest, src, n));
    }
    if (d > s) {
        u64 i = n;
        while (i > 0) {
            i = i - 1;
            dest[i] = src[i];
        }
    }
    return(dest);
}

fn memcmp(u8* s1, u8* s2, u64 n) -> i64 {
    u64 i = 0;
    while (i < n) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        i = i + 1;
    }
    return(0);
}

fn atoi(u8* s) -> u64 {
    u64 i = 0;
    while (s[i] == 32 || s[i] == 9 || s[i] == 10 || s[i] == 13) {
        i = i + 1;
    }
    u64 res = 0;
    while (1) {
        u8 c = s[i];
        if (c < 48 || c > 57) {
            return(res);
        }
        u64 digit = (u64)(c - 48);
        u64 prev = res;
        res = res * 10 + digit;
        if (res < prev) {
            return(18446744073709551615);
        }
        i = i + 1;
    }
    return(res);
}

fn itoa(i64 num, u8* buf) -> u8* {
    u8* original_buf = buf;
    if (num == 0) {
        buf[0] = 48;
        buf[1] = 0;
        return(original_buf);
    }
    u64 is_negative = 0;
    u64 temp_num = 0;
    if (num < 0) {
        is_negative = 1;
        temp_num = (u64)(0 - num);
    } else {
        temp_num = (u64)num;
    }
    u8 rev_buf[32];
    u64 pos = 0;
    while (temp_num > 0) {
        u64 rem = temp_num % 10;
        rev_buf[pos] = (u8)(rem + 48);
        pos = pos + 1;
        temp_num = temp_num / 10;
    }
    if (is_negative == 1) {
        buf[0] = 45;
        buf = buf + 1;
    }
    while (pos > 0) {
        pos = pos - 1;
        buf[0] = rev_buf[pos];
        buf = buf + 1;
    }
    buf[0] = 0;
    return(original_buf);
}

fn strchr(u8* s, u8 c) -> u8* {
    u64 i = 0;
    while (1) {
        u8 ch = s[i];
        if (ch == c) {
            return(s + i);
        }
        if (ch == 0) {
            return(null);
        }
        i = i + 1;
    }
    return(null);
}

fn strrchr(u8* s, u8 c) -> u8* {
    u8* last = null;
    u64 i = 0;
    while (1) {
        u8 ch = s[i];
        if (ch == c) {
            last = s + i;
        }
        if (ch == 0) {
            return(last);
        }
        i = i + 1;
    }
    return(last);
}

fn strstr(u8* haystack, u8* needle) -> u8* {
    u64 nlen = strlen(needle);
    if (nlen == 0) {
        return(haystack);
    }
    u64 i = 0;
    while (haystack[i] != 0) {
        if (haystack[i] == needle[0]) {
            u64 j = 1;
            u64 matched = 1;
            while (j < nlen) {
                if (haystack[i + j] != needle[j]) {
                    matched = 0;
                    j = nlen;
                } else {
                    j = j + 1;
                }
            }
            if (matched == 1) {
                return(haystack + i);
            }
        }
        i = i + 1;
    }
    return(null);
}

fn strncpy(u8* dest, u8* src, u64 n) -> u8* {
    u64 i = 0;
    while (i < n) {
        u8 c = src[i];
        dest[i] = c;
        if (c == 0) {
            u64 j = i + 1;
            while (j < n) {
                dest[j] = 0;
                j = j + 1;
            }
            return(dest);
        }
        i = i + 1;
    }
    return(dest);
}

fn strcasecmp(u8* s1, u8* s2) -> i64 {
    u64 i = 0;
    while (1) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 >= 65) {
            if (c1 <= 90) {
                c1 = c1 + 32;
            }
        }
        if (c2 >= 65) {
            if (c2 <= 90) {
                c2 = c2 + 32;
            }
        }
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        if (c1 == 0) {
            return(0);
        }
        i = i + 1;
    }
    return(0);
}

fn utoa(u64 num, u8* buf) -> u8* {
    u8* original_buf = buf;
    if (num == 0) {
        buf[0] = 48;
        buf[1] = 0;
        return(original_buf);
    }
    u8 rev_buf[32];
    u64 pos = 0;
    while (num > 0) {
        u64 rem = num % 10;
        rev_buf[pos] = (u8)(rem + 48);
        pos = pos + 1;
        num = num / 10;
    }
    while (pos > 0) {
        pos = pos - 1;
        buf[0] = rev_buf[pos];
        buf = buf + 1;
    }
    buf[0] = 0;
    return(original_buf);
}

fn utoa_hex(u64 n, u8* buf, u64 width) -> u64 {
    u8 tmp[17];
    u64 pos = 16;
    u8 digits[17];
    strcpy(digits*adr, "0123456789abcdef");
    if (n == 0) {
        pos = pos - 1;
        tmp[pos] = 48;
    }
    while (n > 0) {
        pos = pos - 1;
        tmp[pos] = digits[n & 15];
        n = n >> 4;
    }
    u64 len = 16 - pos;
    if (width > 16) {
        width = 16;
    }
    u64 pad = 0;
    if (width > len) {
        pad = width - len;
    }
    u64 i = 0;
    while (i < pad) {
        buf[i] = 48;
        i = i + 1;
    }
    memcpy(buf + pad, tmp + pos, len);
    buf[pad + len] = 0;
    return(pad + len);
}

fn utoa_oct(u64 n, u8* buf, u64 width) -> u64 {
    u8 tmp[23];
    u64 pos = 22;
    if (n == 0) {
        pos = pos - 1;
        tmp[pos] = 48;
    }
    while (n > 0) {
        pos = pos - 1;
        tmp[pos] = (u8)(n % 8 + 48);
        n = n / 8;
    }
    u64 len = 22 - pos;
    if (width > 22) {
        width = 22;
    }
    u64 pad = 0;
    if (width > len) {
        pad = width - len;
    }
    u64 i = 0;
    while (i < pad) {
        buf[i] = 48;
        i = i + 1;
    }
    memcpy(buf + pad, tmp + pos, len);
    buf[pad + len] = 0;
    return(pad + len);
}

fn strtou64(u8* s, u64* out) -> u64 {
    if (s[0] == 0) {
        return(0);
    }
    u64 i = 0;
    u64 v = 0;
    while (s[i] != 0) {
        u8 c = s[i];
        if (c < 48) {
            return(0);
        }
        if (c > 57) {
            return(0);
        }
        u64 d = (u64)(c - 48);
        if (v > 1844674407370955161) {
            return(0);
        }
        v = v * 10;
        if (v > 18446744073709551615 - d) {
            return(0);
        }
        v = v + d;
        i = i + 1;
    }
    u64* o*o = out;
    o = v;
    return(1);
}

fn strtoi64(u8* s, i64* out) -> u64 {
    u64 i = 0;
    u64 neg = 0;
    if (s[0] == 45) {
        neg = 1;
        i = 1;
    } else {
        if (s[0] == 43) {
            i = 1;
        }
    }
    if (s[i] == 0) {
        return(0);
    }
    u64 v = 0;
    u64 limit = 9223372036854775807;
    if (neg == 1) {
        limit = 9223372036854775808;
    }
    while (s[i] != 0) {
        u8 c = s[i];
        if (c < 48) {
            return(0);
        }
        if (c > 57) {
            return(0);
        }
        u64 d = (u64)(c - 48);
        if (v > 922337203685477580) {
            return(0);
        }
        v = v * 10;
        if (v > limit - d) {
            return(0);
        }
        v = v + d;
        i = i + 1;
    }
    i64* o*o = out;
    if (neg == 1) {
        o = 0 - v;
    } else {
        o = v;
    }
    return(1);
}

fn strtosize(u8* s, u64* out) -> u64 {
    u64 i = 0;
    u64 v = 0;
    if (s[0] == 0) {
        return(0);
    }
    while (s[i] >= 48) {
        if (s[i] > 57) {
            break;
        }
        u64 d = (u64)(s[i] - 48);
        if (v > 1844674407370955161) {
            return(0);
        }
        v = v * 10;
        if (v > 18446744073709551615 - d) {
            return(0);
        }
        v = v + d;
        i = i + 1;
    }
    if (i == 0) {
        return(0);
    }
    u8 suf = s[i];
    u64 mult = 1;
    if (suf == 0) {
        u64* o*o = out;
        o = v;
        return(1);
    }
    if (suf == 99) {
        mult = 1;
        i = i + 1;
    } else if (suf == 119) {
        mult = 2;
        i = i + 1;
    } else if (suf == 98) {
        mult = 512;
        i = i + 1;
    } else if (suf == 75 || suf == 107) {
        mult = 1024;
        i = i + 1;
    } else if (suf == 77) {
        mult = 1048576;
        i = i + 1;
    } else if (suf == 71) {
        mult = 1073741824;
        i = i + 1;
    } else if (suf == 84) {
        mult = 1099511627776;
        i = i + 1;
    } else if (suf == 80) {
        mult = 1125899906842624;
        i = i + 1;
    } else if (suf == 69) {
        mult = 1152921504606846976;
        i = i + 1;
    } else {
        return(0);
    }
    if (s[i] == 105) {
        if (s[i + 1] != 66) {
            return(0);
        }
        i = i + 2;
    } else if (s[i] == 66) {
        if (mult == 1024) { mult = 1000; }
        else if (mult == 1048576) { mult = 1000000; }
        else if (mult == 1073741824) { mult = 1000000000; }
        else if (mult == 1099511627776) { mult = 1000000000000; }
        else if (mult == 1125899906842624) { mult = 1000000000000000; }
        else if (mult == 1152921504606846976) { mult = 1000000000000000000; }
        i = i + 1;
    }
    if (s[i] != 0) {
        return(0);
    }
    if (mult != 1) {
        if (v > 18446744073709551615 / mult) {
            return(0);
        }
    }
    u64* o*o = out;
    o = v * mult;
    return(1);
}
