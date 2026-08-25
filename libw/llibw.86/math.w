sc.true
#import <syscall>

sect.math_const
    f64 PI = 3.141592653589793;
    f64 TWO_PI = 6.283185307179586;
    f64 HALF_PI = 1.570796326794896;
EOS

fn abs(f64 x) -> f64 {
    if (x < 0.0) {
        return(0.0 - x);
    }
    return(x);
}

fn sqrt(f64 x) -> f64 {
    if (x <= 0.0) {
        return(0.0);
    }
    f64 res = x / 2.0;
    for (u64 i = 0; i < 20; i = i + 1) {
        res = (res + (x / res)) / 2.0;
    }
    return(res);
}

fn sin(f64 x) -> f64 {
    f64 reduced = x;
    while (reduced > math_const:PI) {
        reduced = reduced - math_const:TWO_PI;
    }
    while (reduced < (0.0 - math_const:PI)) {
        reduced = reduced + math_const:TWO_PI;
    }
    f64 x2 = reduced * reduced;
    f64 term1 = reduced;
    f64 term3 = (reduced * x2) / 6.0;
    f64 term5 = (term3 * x2) / 20.0;
    f64 term7 = (term5 * x2) / 42.0;
    f64 term9 = (term7 * x2) / 72.0;
    return(term1 - term3 + term5 - term7 + term9);
}

fn cos(f64 x) -> f64 {
    return(sin(x + math_const:HALF_PI));
}

fn tan(f64 x) -> f64 {
    f64 c = cos(x);
    if (c == 0.0) {
        return(0.0);
    }
    return(sin(x) / c);
}

fn print_float(f64 x) {
    if (x < 0.0) {
        print_char(45);
        x = 0.0 - x;
    }
    i64 ipart = (i64)x;
    f64 fpart = x - (f64)ipart;
    i64 fpart_int = (i64)(fpart * 1000000.0);
    print_number(ipart);
    print_char(46);
    i64 temp = fpart_int;
    i64 padding_zeros = 0;
    if (temp == 0) {
        padding_zeros = 5;
    } else {
        if (temp < 10) { padding_zeros = 5; }
        else {
            if (temp < 100) { padding_zeros = 4; }
            else {
                if (temp < 1000) { padding_zeros = 3; }
                else {
                    if (temp < 10000) { padding_zeros = 2; }
                    else {
                        if (temp < 100000) { padding_zeros = 1; }
                    }
                }
            }
        }
    }
    for (u64 i = 0; i < padding_zeros; i = i + 1) {
        print_char(48);
    }
    if (fpart_int > 0) {
        print_number(fpart_int);
    }
}
