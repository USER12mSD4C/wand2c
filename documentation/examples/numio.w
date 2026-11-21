sc.true
#import <io>

fn show(u64 num) {
    u8 buf[32];
    u64 pos = 30;
    buf[31] = 0;

    u64 temp = num;
    if (temp == 0) {
        buf[30] = 48;
    } else {
        while (temp > 0) {
            u8 digit = (temp % 10) + 48;
            buf[pos] = digit;
            pos = pos - 1;
            temp = temp / 10;
        }
        pos = pos + 1;
    }

    u8* start*i = buf*adr;
    print_string(start + pos);
}
