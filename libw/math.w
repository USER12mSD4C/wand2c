sc.true
#import <syscall>

sect.math_const
    f64 PI = 3.141592653589793;
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
    // Ряд Тейлора для sin(x)
    f64 x2 = x * x;
    f64 term1 = x;
    f64 term3 = (x * x2) / 6.0;
    f64 term5 = (term3 * x2) / 20.0;
    f64 term7 = (term5 * x2) / 42.0;
    return(term1 - term3 + term5 - term7);
}

fn cos(f64 x) -> f64 {
    // Ряд Тейлора для cos(x)
    f64 x2 = x * x;
    f64 term2 = x2 / 2.0;
    f64 term4 = (term2 * x2) / 12.0;
    f64 term6 = (term4 * x2) / 30.0;
    return(1.0 - term2 + term4 - term6);
}

fn tan(f64 x) -> f64 {
    return(sin(x) / cos(x));
}

fn print_float(f64 x) {
    if (x < 0.0) {
        print_char(45); // '-'
        x = 0.0 - x;
    }

    // Получаем целую часть явным приведением типов f64 -> i64
    i64 ipart = (i64)x;

    // Получаем дробную часть и переводим в целое число (6 знаков)
    f64 fpart = x - (f64)ipart;
    i64 fpart_int = (i64)(fpart * 1000000.0);

    print_number(ipart);
    print_char(46); // '.'

    // Выводим ведущие нули дробной части
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
        print_char(48); // '0'
    }
    if (fpart_int > 0) {
        print_number(fpart_int);
    }
}
